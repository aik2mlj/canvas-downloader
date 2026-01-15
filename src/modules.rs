use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::api::get_pages;
use crate::canvas::{ModuleItemResult, ModuleResult, ProcessOptions};
use crate::files::{filter_files, process_file_id};
use crate::pages::process_page_body;
use crate::utils::{create_folder_if_not_exist_or_ignored, get_raw_json_path, prettify_json};

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
            Ok(ModuleResult::Ok(modules)) => {
                if !modules.is_empty() && !has_modules {
                    // Create modules folder only when we have actual modules
                    let modules_path = path.join("modules");
                    if !create_folder_if_not_exist_or_ignored(&modules_path, &options)? {
                        continue;
                    }
                    modules_folder_path = Some(modules_path.clone());
                    has_modules = true;

                    // Create modules.json file
                    if let Some(module_json) = get_raw_json_path(
                        &path,
                        "modules.json",
                        &options.base_path,
                        options.save_json,
                    )? {
                        let mut module_file = std::fs::File::create(module_json.clone())
                            .with_context(|| {
                                format!("Unable to create file for {:?}", module_json)
                            })?;
                        let pretty_json =
                            prettify_json(&module_body).unwrap_or(module_body.clone());
                        module_file
                            .write_all(pretty_json.as_bytes())
                            .with_context(|| {
                                format!("Unable to write to file for {:?}", module_json)
                            })?;
                    }
                }

                for module in modules {
                    if let Some(ref modules_path) = modules_folder_path {
                        let module_path =
                            modules_path.join(sanitize_filename::sanitize(&module.name));
                        if !create_folder_if_not_exist_or_ignored(&module_path, &options)? {
                            continue;
                        }

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

        if let Some(items_json) = get_raw_json_path(
            &path,
            "module_items.json",
            &options.base_path,
            options.save_json,
        )? {
            let mut items_file = std::fs::File::create(items_json.clone())
                .with_context(|| format!("Unable to create file for {:?}", items_json))?;

            let pretty_json = prettify_json(&items_body).unwrap_or(items_body.clone());
            items_file
                .write_all(pretty_json.as_bytes())
                .with_context(|| format!("Unable to write to file for {:?}", items_json))?;
        }

        let items_result = serde_json::from_str::<ModuleItemResult>(&items_body);

        match items_result {
            Ok(ModuleItemResult::Ok(items)) => {
                let mut files_to_process = Vec::new();

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
                                        files_to_process.push(file);
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
                            if let Some(full_page_url) = item.url {
                                let item_path = path.join(sanitize_filename::sanitize(&item.title));
                                if !create_folder_if_not_exist_or_ignored(&item_path, &options)? {
                                    continue;
                                }

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
                                tracing::debug!(
                                    "Module item {} references assignment {}",
                                    item.title,
                                    content_id
                                );
                            }
                        }
                        "Discussion" => {
                            if let Some(content_id) = item.content_id {
                                tracing::debug!(
                                    "Module item {} references discussion {}",
                                    item.title,
                                    content_id
                                );
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
                            if !create_folder_if_not_exist_or_ignored(&subheader_path, &options)? {
                                continue;
                            }
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

                // Filter and add all collected files to download queue in one batch
                if !files_to_process.is_empty() {
                    let filtered_files = filter_files(&options, &path, files_to_process);
                    if !filtered_files.is_empty() {
                        let mut lock = options.files_to_download.lock().await;
                        lock.extend(filtered_files);
                    }
                }
            }

            Ok(ModuleItemResult::Err { status }) => {
                tracing::error!(
                    "Failed to access module items at link:{url}, path:{path:?}, status:{status}"
                );
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
