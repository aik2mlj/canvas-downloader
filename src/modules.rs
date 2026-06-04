use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use anyhow::{Context, Result};

use crate::api::get_pages;
use crate::canvas::{File, ModuleItemResult, ModuleResult, ProcessOptions};
use crate::files::{filter_files, process_file_id};
use crate::pages::process_page_body;
use crate::utils::{create_folder_if_not_exist_or_ignored, get_raw_json_path, prettify_json};

fn write_url_shortcut(section_path: &Path, title: &str, url: &str) {
    let url_file = section_path.join(format!("{}.url", sanitize_filename::sanitize(title)));
    if let Ok(mut file) = std::fs::File::create(&url_file) {
        let _ = writeln!(file, "[InternetShortcut]");
        let _ = writeln!(file, "URL={}", url);
    }
}

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

    if has_modules {
        tracing::debug!(
            "📦 Modules synced for {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        options.n_modules.fetch_add(1, Ordering::Relaxed);
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
                // Items in a Canvas module are returned as a flat list; a
                // `SubHeader` item starts a section that owns every following
                // item until the next `SubHeader`. `current_section` is the
                // destination folder for the section we're currently in:
                // `Some(path)` for items before any subheader, `Some(sub)`
                // while inside a subheader, or `None` if the active subheader
                // folder is ignored (skip its contents too).
                let mut current_section: Option<PathBuf> = Some(path.clone());
                let mut files_to_process: Vec<(PathBuf, File)> = Vec::new();

                for item in items {
                    match item.item_type.as_str() {
                        "File" => {
                            let Some(section_path) = current_section.as_ref() else {
                                continue;
                            };
                            if let Some(content_id) = item.content_id {
                                let file_url = format!(
                                    "{}/api/v1/files/{}",
                                    options.canvas_url.trim_end_matches('/'),
                                    content_id
                                );

                                match process_file_id(
                                    (file_url, section_path.clone()),
                                    options.clone(),
                                )
                                .await
                                {
                                    Ok(file) => {
                                        files_to_process.push((section_path.clone(), file));
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
                            let Some(section_path) = current_section.as_ref() else {
                                continue;
                            };
                            if let Some(full_page_url) = item.url {
                                let item_path =
                                    section_path.join(sanitize_filename::sanitize(&item.title));
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
                            let Some(section_path) = current_section.as_ref() else {
                                continue;
                            };
                            if let Some(external_url) = &item.external_url {
                                write_url_shortcut(section_path, &item.title, external_url);
                            }
                        }
                        "Quiz" => {
                            let Some(section_path) = current_section.as_ref() else {
                                continue;
                            };
                            if let Some(quiz_url) = item.html_url.as_deref().or(item.url.as_deref())
                            {
                                write_url_shortcut(section_path, &item.title, quiz_url);
                            }
                        }
                        "SubHeader" => {
                            // SubHeader starts a new section. Subheader folders
                            // are siblings under the module folder, not nested
                            // inside the previous section.
                            let subheader_path =
                                path.join(sanitize_filename::sanitize(&item.title));
                            if !create_folder_if_not_exist_or_ignored(&subheader_path, &options)? {
                                current_section = None;
                                continue;
                            }
                            current_section = Some(subheader_path);
                        }
                        _ => {
                            tracing::debug!(
                                "Unsupported module item type '{}' for item '{}'",
                                item.item_type,
                                item.title
                            );
                        }
                    }
                }

                // Group queued files by destination section, then filter each
                // group against its own folder before extending the global
                // download queue in one lock acquisition.
                if !files_to_process.is_empty() {
                    let mut by_section: HashMap<PathBuf, Vec<File>> = HashMap::new();
                    for (section_path, file) in files_to_process {
                        by_section.entry(section_path).or_default().push(file);
                    }
                    let mut all_filtered: Vec<File> = Vec::new();
                    for (section_path, files) in by_section {
                        all_filtered.extend(filter_files(&options, &section_path, files));
                    }
                    if !all_filtered.is_empty() {
                        let mut lock = options.files_to_download.lock().await;
                        lock.extend(all_filtered);
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
