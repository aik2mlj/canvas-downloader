use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::api::{get_canvas_api, get_pages};
use crate::canvas::{PageBody, PageResult, ProcessOptions};
use crate::html::process_html_links;
use crate::utils::{create_folder_if_not_exist, prettify_json};

pub async fn process_pages(
    (url, path): (String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let pages_url = format!("{}pages", url);
    let pages = get_pages(pages_url, &options).await?;

    let mut has_pages = false;
    let mut pages_folder_path = None;

    for pg in pages {
        let uri = pg.url().to_string();
        let page_body = pg.text().await?;

        let page_result = serde_json::from_str::<PageResult>(&page_body);

        match page_result {
            Ok(PageResult::Ok(pages)) | Ok(PageResult::Direct(pages)) => {
                if !pages.is_empty() && !has_pages {
                    // Create pages folder only when we have actual pages
                    let pages_path = path.join("pages");
                    create_folder_if_not_exist(&pages_path)?;
                    pages_folder_path = Some(pages_path.clone());
                    has_pages = true;

                    // Create pages.json file
                    let pages_json_path = pages_path.join("pages.json");
                    let mut pages_file = std::fs::File::create(pages_json_path.clone())
                        .with_context(|| {
                            format!("Unable to create file for {:?}", pages_json_path)
                        })?;
                    let pretty_json = prettify_json(&page_body).unwrap_or(page_body.clone());
                    pages_file
                        .write_all(pretty_json.as_bytes())
                        .with_context(|| {
                            format!("Could not write to file {:?}", pages_json_path)
                        })?;
                }

                for page in pages {
                    if let Some(ref pages_path) = pages_folder_path {
                        let page_url = format!("{}pages/{}", url, page.url);
                        let page_file_path = pages_path.join(page.url.clone());
                        create_folder_if_not_exist(&page_file_path)?;
                        fork!(
                            process_page_body,
                            (page_url, page.url, page_file_path),
                            (String, String, PathBuf),
                            options.clone()
                        )
                    }
                }
            }

            Ok(PageResult::Err { status }) => {
                tracing::debug!("No pages found for url {} (status: {})", uri, status);
            }

            Ok(PageResult::Empty(_)) => {
                tracing::debug!("No pages found for url {} (empty response)", uri);
            }

            Err(e) => {
                tracing::debug!("No pages found for url {} (error: {})", uri, e);
            }
        };
    }

    Ok(())
}

pub async fn process_page_body(
    (url, title, path): (String, String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let page_resp = get_canvas_api(url.clone(), &options).await?;

    let page_file_path = path.join(format!("{}.json", title));
    let mut page_file = std::fs::File::create(page_file_path.clone())
        .with_context(|| format!("Unable to create file for {:?}", page_file_path))?;

    let page_resp_text = page_resp.text().await?;
    let pretty_json = prettify_json(&page_resp_text).unwrap_or(page_resp_text.clone());
    page_file
        .write_all(pretty_json.as_bytes())
        .with_context(|| format!("Could not write to file {:?}", page_file_path))?;

    let page_body_result = serde_json::from_str::<PageBody>(&page_resp_text);
    match page_body_result {
        Result::Ok(page_body) => {
            let page_html = format!(
                "<html><head><title>{}</title></head><body>{}</body></html>",
                page_body.title, page_body.body
            );

            let page_html_path = path.join(format!("{}.html", page_body.url));
            let mut page_html_file = std::fs::File::create(page_html_path.clone())
                .with_context(|| format!("Unable to create file for {:?}", page_html_path))?;

            page_html_file
                .write_all(page_html.as_bytes())
                .with_context(|| format!("Could not write to file {:?}", page_html_path))?;

            fork!(
                process_html_links,
                (page_html, path),
                (String, PathBuf),
                options.clone()
            )
        }
        Result::Err(e) => {
            tracing::error!(
                "Error when parsing page body at link:{url}, path:{page_file_path:?}\n{e:?}",
            );
        }
    }
    Ok(())
}
