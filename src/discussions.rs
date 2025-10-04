use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::api::{get_canvas_api, get_pages};
use crate::canvas::{DiscussionResult, DiscussionView, ProcessOptions};
use crate::files::filter_files;
use crate::html::process_html_links;
use crate::utils::{create_folder_if_not_exist, prettify_json};

pub async fn process_discussions(
    (url, announcement, path): (String, bool, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let discussion_url = format!("{}discussion_topics{}", url, if announcement { "?only_announcements=true" } else { "" });
    let pages = get_pages(discussion_url, &options).await?;

    let mut has_discussions = false;
    let mut discussions_folder_path = None;

    for pg in pages {
        let uri = pg.url().to_string();
        let page_body = pg.text().await?;

        let discussion_result = serde_json::from_str::<DiscussionResult>(&page_body);

        match discussion_result {
            Ok(DiscussionResult::Ok(discussions)) => {
                if !discussions.is_empty() && !has_discussions {
                    // Create discussions or announcements folder only when we have actual discussions
                    let folder_name = if announcement { "announcements" } else { "discussions" };
                    let folder_path = path.join(folder_name);
                    create_folder_if_not_exist(&folder_path)?;
                    discussions_folder_path = Some(folder_path.clone());
                    has_discussions = true;

                    // Create discussions.json file
                    let discussion_path = folder_path.join("discussions.json");
                    let mut discussion_file = std::fs::File::create(discussion_path.clone())
                        .with_context(|| format!("Unable to create file for {:?}", discussion_path))?;
                    let pretty_json = prettify_json(&page_body).unwrap_or(page_body.clone());
                    discussion_file
                        .write_all(pretty_json.as_bytes())
                        .with_context(|| format!("Unable to write to file for {:?}", discussion_path))?;
                }

                for discussion in discussions {
                    if let Some(ref folder_path) = discussions_folder_path {
                        // download attachments
                        let discussion_folder_path = folder_path.join(format!("{}_{}", discussion.id, sanitize_filename::sanitize(discussion.title)));
                        create_folder_if_not_exist(&discussion_folder_path)?;

                        let files = discussion.attachments
                            .into_iter()
                            .map(|mut f| {
                                f.display_name = format!("{}_{}", f.id, &f.display_name);
                                f
                            })
                            .collect();
                        {
                            let mut filtered_files = filter_files(&options, &discussion_folder_path, files);
                            let mut lock = options.files_to_download.lock().await;
                            lock.append(&mut filtered_files);
                        }

                        fork!(
                            process_html_links,
                            (discussion.message, discussion_folder_path.clone()),
                            (String, PathBuf),
                            options.clone()
                        );
                        let view_url = format!("{}discussion_topics/{}/view", url, discussion.id);
                        fork!(
                            process_discussion_view,
                            (view_url, discussion_folder_path),
                            (String, PathBuf),
                            options.clone()
                        )
                    }
                }
            }
            Ok(DiscussionResult::Err { status }) => {
                eprintln!(
                    "Failed to access discussions at link:{uri}, path:{path:?}, status:{status}",
                );
            }
            Err(e) => {
                eprintln!("Error when getting discussions at link:{uri}, path:{path:?}\n{e:?}",);
            }
        }
    }
    Ok(())
}

async fn process_discussion_view(
    (url, path): (String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let resp = get_canvas_api(url.clone(), &options).await?;
    let discussion_view_body = resp.text().await?;

    let discussion_view_json = path.join("discussion.json");
    let mut discussion_view_file = std::fs::File::create(discussion_view_json.clone())
        .with_context(|| format!("Unable to create file for {:?}", discussion_view_json))?;

    let pretty_json = prettify_json(&discussion_view_body).unwrap_or(discussion_view_body.clone());
    discussion_view_file
        .write_all(pretty_json.as_bytes())
        .with_context(|| format!("Unable to write to file for {:?}", discussion_view_json))?;

    let discussion_view_result = serde_json::from_str::<DiscussionView>(&discussion_view_body);
    let mut attachments_all = Vec::new();
    match discussion_view_result {
        Result::Ok(discussion_view) => {
            for view in discussion_view.view {
                if let Some(message) = view.message {
                    fork!(
                        process_html_links,
                        (message, path.clone()),
                        (String, PathBuf),
                        options.clone()
                    )
                }
                if let Some(mut attachments) = view.attachments {
                    attachments_all.append(&mut attachments);
                }
                if let Some(attachment) = view.attachment {
                    attachments_all.push(attachment);
                }
            }
        }
        Result::Err(e) => {
            eprintln!("Error when getting submissions at link:{url}, path:{path:?}\n{e:?}",);
        }
    }

    let files = attachments_all
        .into_iter()
        .map(|mut f| {
            f.display_name = format!("{}_{}", f.id, &f.display_name);
            f
        })
        .collect();
    let mut filtered_files = filter_files(&options, &path, files);
    let mut lock = options.files_to_download.lock().await;
    lock.append(&mut filtered_files);

    Ok(())
}
