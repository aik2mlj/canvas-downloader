use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::api::get_pages;
use crate::canvas::{ModuleItemResult, ModuleResult, ProcessOptions};
use crate::files::{filter_files, process_file_id};
use crate::pages::process_page_body;
use crate::utils::{create_folder_if_not_exist, prettify_json};

pub async fn process_modules(
    (url, path): (String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let modules_url = format!("{}modules", url);
    let pages = get_pages(modules_url, &options).await?;

    let mut has_modules = false;
    let mut modules_folder_path = None;

    for page in pages {
        let module_body = page.text().await?;
        let module_result = serde_json::from_str::<ModuleResult>(&module_body);

        match module_result {
            Ok(ModuleResult::Ok(modules)) | Ok(ModuleResult::Direct(modules)) => {
                if !modules.is_empty() && !has_modules {
                    // Create modules folder only when we have actual modules
                    let modules_path = path.join("modules");
                    create_folder_if_not_exist(&modules_path)?;
                    modules_folder_path = Some(modules_path.clone());
                    has_modules = true;

                    // Create modules.json file
                    let module_json = modules_path.join("modules.json");
                    let mut module_file = std::fs::File::create(module_json.clone())
                        .with_context(|| format!("Unable to create file for {:?}", module_json))?;
                    let pretty_json = prettify_json(&module_body).unwrap_or(module_body.clone());
                    module_file
                        .write_all(pretty_json.as_bytes())
                        .with_context(|| {
                            format!("Unable to write to file for {:?}", module_json)
                        })?;
                }

                for module in modules {
                    if let Some(ref modules_path) = modules_folder_path {
                        let module_path =
                            modules_path.join(sanitize_filename::sanitize(&module.name));
                        create_folder_if_not_exist(&module_path)?;

                        fork!(
                            process_module_items,
                            (module.items_url, module_path),
                            (String, PathBuf),
                            options.clone()
                        );
                    }
                }
            }

            Ok(ModuleResult::Err { status }) => {
                tracing::error!("No modules found for url {} status: {}", url, status);
            }

            Ok(ModuleResult::Empty(_)) => {
                tracing::error!("No modules found for url {} (empty response)", url);
            }

            Err(e) => {
                tracing::error!("No modules found for url {} error: {}", url, e);
            }
        };
    }

    Ok(())
}

async fn process_module_items(
    (url, path): (String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let pages = get_pages(url.clone(), &options).await?;

    for page in pages {
        let items_body = page.text().await?;
        let items_json = path.join("module_items.json");
        let mut items_file = std::fs::File::create(items_json.clone())
            .with_context(|| format!("Unable to create file for {:?}", items_json))?;

        let pretty_json = prettify_json(&items_body).unwrap_or(items_body.clone());
        items_file
            .write_all(pretty_json.as_bytes())
            .with_context(|| format!("Unable to write to file for {:?}", items_json))?;

        let items_result = serde_json::from_str::<ModuleItemResult>(&items_body);

        match items_result {
            Ok(ModuleItemResult::Ok(items)) | Ok(ModuleItemResult::Direct(items)) => {
                for item in items {
                    match item.item_type.as_str() {
                        "File" => {
                            if let Some(content_id) = item.content_id {
                                let file_url = format!(
                                    "{}/api/v1/files/{}",
                                    options.canvas_url.trim_end_matches('/'),
                                    content_id
                                );

                                match process_file_id((file_url, path.clone()), options.clone())
                                    .await
                                {
                                    Ok(file) => {
                                        // Use filter_files to apply standard filtering logic
                                        let filtered = filter_files(&options, &path, vec![file]);

                                        if !filtered.is_empty() {
                                            let mut lock = options.files_to_download.lock().await;
                                            lock.push(filtered.into_iter().next().unwrap());
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Error processing module file {}: {:?}",
                                            content_id,
                                            e
                                        );
                                    }
                                }
                            }
                        }
                        "Page" => {
                            if let Some(page_url) = &item.page_url {
                                let full_page_url = format!(
                                    "{}pages/{}",
                                    url.replace("/modules/", "/").replace("/items", ""),
                                    page_url
                                );
                                let item_path = path.join(sanitize_filename::sanitize(&item.title));
                                create_folder_if_not_exist(&item_path)?;

                                fork!(
                                    process_page_body,
                                    (full_page_url, item.title, item_path),
                                    (String, String, PathBuf),
                                    options.clone()
                                );
                            }
                        }
                        "Assignment" => {
                            if let Some(content_id) = item.content_id {
                                tracing::error!("Module item {} references assignment {}, consider downloading assignments separately",
                                         item.title, content_id);
                            }
                        }
                        "Discussion" => {
                            if let Some(content_id) = item.content_id {
                                tracing::error!("Module item {} references discussion {}, consider downloading discussions separately",
                                         item.title, content_id);
                            }
                        }
                        "ExternalUrl" => {
                            if let Some(external_url) = &item.external_url {
                                let url_file = path.join(format!(
                                    "{}.url",
                                    sanitize_filename::sanitize(&item.title)
                                ));
                                if let Ok(mut file) = std::fs::File::create(&url_file) {
                                    let _ = writeln!(file, "[InternetShortcut]");
                                    let _ = writeln!(file, "URL={}", external_url);
                                }
                            }
                        }
                        "SubHeader" => {
                            // SubHeaders are just organizational - create a folder
                            let subheader_path =
                                path.join(sanitize_filename::sanitize(&item.title));
                            create_folder_if_not_exist(&subheader_path)?;
                        }
                        _ => {
                            tracing::error!(
                                "Unsupported module item type '{}' for item '{}'",
                                item.item_type,
                                item.title
                            );
                        }
                    }
                }
            }

            Ok(ModuleItemResult::Err { status }) => {
                tracing::error!(
                    "Failed to access module items at link:{url}, path:{path:?}, status:{status}"
                );
            }

            Ok(ModuleItemResult::Empty(_)) => {
                tracing::error!("No module items found for url {} (empty response)", url);
            }

            Err(e) => {
                tracing::error!(
                    "Error when getting module items at link:{url}, path:{path:?}\n{e:?}"
                );
            }
        }
    }

    Ok(())
}
