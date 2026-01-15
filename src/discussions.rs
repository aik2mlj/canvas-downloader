use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::api::{get_canvas_api, get_pages};
use crate::canvas::{Discussion, DiscussionResult, DiscussionView, File, ProcessOptions};
use crate::files::filter_files;
use crate::html::process_html_links;
use crate::utils::{create_folder_if_not_exist_or_ignored, get_raw_json_path, prettify_json};

pub async fn process_discussions(
    (url, announcement, path): (String, bool, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let discussion_url = format!(
        "{}discussion_topics{}",
        url,
        if announcement {
            "?only_announcements=true"
        } else {
            ""
        }
    );
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
                    let folder_name = if announcement {
                        "announcements"
                    } else {
                        "discussions"
                    };
                    let folder_path = path.join(folder_name);
                    if !create_folder_if_not_exist_or_ignored(&folder_path, &options)? {
                        continue;
                    }
                    discussions_folder_path = Some(folder_path.clone());
                    has_discussions = true;

                    // Create discussions.json file
                    if let Some(discussions_json_path) = get_raw_json_path(
                        &path,
                        &format!("{folder_name}.json"),
                        &options.base_path,
                        options.save_json,
                    )? {
                        let mut discussions_json_file =
                            std::fs::File::create(discussions_json_path.clone()).with_context(
                                || format!("Unable to create file for {:?}", discussions_json_path),
                            )?;
                        let pretty_json = prettify_json(&page_body).unwrap_or(page_body.clone());
                        discussions_json_file
                            .write_all(pretty_json.as_bytes())
                            .with_context(|| {
                                format!("Unable to write to file for {:?}", discussions_json_path)
                            })?;
                    }
                }

                for discussion in discussions {
                    if let Some(ref folder_path) = discussions_folder_path {
                        // download attachments (TODO: not sure if this is needed)
                        let discussion_folder_path =
                            folder_path.join(sanitize_filename::sanitize(&discussion.title));

                        let files: Vec<File> = discussion
                            .attachments
                            .clone()
                            .into_iter()
                            .map(|mut f| {
                                f.display_name = format!("{}_{}", f.id, &f.display_name);
                                f
                            })
                            .collect();
                        let mut filtered_files =
                            filter_files(&options, &discussion_folder_path, files);
                        if !filtered_files.is_empty() {
                            // create folder for discussion if there are files to download
                            create_folder_if_not_exist_or_ignored(
                                &discussion_folder_path,
                                &options,
                            )?;
                            // add files to download list
                            let mut lock = options.files_to_download.lock().await;
                            lock.append(&mut filtered_files);
                        }

                        fork!(
                            process_html_links,
                            (
                                discussion.message.clone(),
                                folder_path.clone(),
                                discussion.title.clone()
                            ),
                            (String, PathBuf, String),
                            options.clone()
                        );
                        let view_url = format!("{}discussion_topics/{}/view", url, discussion.id);
                        fork!(
                            process_discussion_view,
                            (view_url, folder_path.clone(), discussion),
                            (String, PathBuf, Discussion),
                            options.clone()
                        )
                    }
                }
            }
            Ok(DiscussionResult::Err { status }) => {
                tracing::debug!(
                    "Failed to access discussions at link:{uri}, path:{path:?}, status:{status}",
                );
            }
            Err(e) => {
                tracing::debug!(
                    "Error when getting discussions at link:{uri}, path:{path:?}\n{e:?}",
                );
            }
        }
    }
    Ok(())
}

fn generate_discussion_html(
    discussion: &Discussion,
    comments: &[crate::canvas::Comments],
) -> String {
    let mut html = String::new();

    html.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    html.push_str("    <meta charset=\"UTF-8\">\n");
    html.push_str(&format!(
        "    <title>{}</title>\n",
        html_escape(&discussion.title)
    ));
    html.push_str(r#"    <style>
        body { font-family: system-ui, -apple-system, "Segoe UI", Arial, sans-serif; font-size: 16px; line-height: 1.5; max-width: 900px; margin: 20px auto; padding: 0 20px; }

        .discussion-post { background: #f9f9f9; border-left: 4px solid #4CAF50; padding: 20px; margin-bottom: 30px; }
        .discussion-title { font-size: 1.5rem; font-weight: 600; margin-bottom: 10px; }
        .discussion-meta { color: #666; font-size: 0.875rem; margin-bottom: 10px; font-weight: 500; }

        .discussion-message,
        .comment-message { font-size: 0.95rem; }

        .discussion-message p,
        .comment-message p { margin: 0; }

        .discussion-message p + p,
        .comment-message p + p { margin-top: 0.75em; }

        .comments-section { margin-top: 30px; }
        .comments-header { font-size: 1.25rem; font-weight: 600; margin-bottom: 15px; border-bottom: 2px solid #ddd; padding-bottom: 10px; }

        .comment { background: #fff; border: 1px solid #ddd; padding: 15px; margin-bottom: 15px; border-radius: 6px; }
        .comment-meta { color: #666; font-size: 0.875rem; margin-bottom: 10px; font-weight: 500; }
    </style>
"#);
    html.push_str("</head>\n<body>\n");

    // Main discussion post
    html.push_str("    <div class=\"discussion-post\">\n");
    html.push_str(&format!(
        "        <div class=\"discussion-title\">{}</div>\n",
        html_escape(&discussion.title)
    ));
    html.push_str("        <div class=\"discussion-meta\">\n");

    if let Some(ref author) = discussion.author {
        if let Some(ref display_name) = author.display_name {
            html.push_str(&format!("            {}", html_escape(display_name)));
        }
    }

    if let Some(ref posted_at) = discussion.posted_at {
        html.push_str(&format!(" | {}", html_escape(posted_at)));
    }

    html.push_str("\n        </div>\n");
    html.push_str(&format!(
        "        <div class=\"discussion-message\">{}</div>\n",
        &discussion.message
    ));
    html.push_str("    </div>\n");

    // Comments section
    if !comments.is_empty() {
        html.push_str("    <div class=\"comments-section\">\n");
        html.push_str(&format!(
            "        <div class=\"comments-header\">Comments ({})</div>\n",
            comments.len()
        ));

        for comment in comments {
            if let Some(ref message) = comment.message {
                html.push_str("        <div class=\"comment\">\n");
                html.push_str("            <div class=\"comment-meta\">\n");

                if let Some(ref user_name) = comment.user_name {
                    html.push_str(&format!("                {}", html_escape(user_name)));
                }

                if let Some(ref created_at) = comment.created_at {
                    html.push_str(&format!(" | {}", html_escape(created_at)));
                }

                html.push_str("\n            </div>\n");
                html.push_str(&format!(
                    "            <div class=\"comment-message\">{}</div>\n",
                    message
                ));
                html.push_str("        </div>\n");
            }
        }

        html.push_str("    </div>\n");
    }

    html.push_str("</body>\n</html>");
    html
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

async fn process_discussion_view(
    (url, path, discussion): (String, PathBuf, Discussion),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let resp = get_canvas_api(url.clone(), &options).await?;
    let discussion_view_body = resp.text().await?;

    let discussion_name = sanitize_filename::sanitize(&discussion.title);
    if let Some(discussion_view_json) = get_raw_json_path(
        &path,
        &format!("{discussion_name}.json"),
        &options.base_path,
        options.save_json,
    )? {
        let mut discussion_view_file = std::fs::File::create(discussion_view_json.clone())
            .with_context(|| format!("Unable to create file for {:?}", discussion_view_json))?;

        let pretty_json =
            prettify_json(&discussion_view_body).unwrap_or(discussion_view_body.clone());
        discussion_view_file
            .write_all(pretty_json.as_bytes())
            .with_context(|| format!("Unable to write to file for {:?}", discussion_view_json))?;
    }

    let discussion_view_result = serde_json::from_str::<DiscussionView>(&discussion_view_body);
    let mut attachments_all = Vec::new();
    let mut comments = Vec::new();

    match discussion_view_result {
        Result::Ok(discussion_view) => {
            // Create a mapping from user_id to display_name
            let user_map: HashMap<u32, String> = discussion_view
                .participants
                .iter()
                .map(|p| (p.id, p.display_name.clone()))
                .collect();

            for mut view in discussion_view.view {
                // Map user_id to display_name
                if let Some(user_id) = view.user_id {
                    if let Some(display_name) = user_map.get(&user_id) {
                        view.user_name = Some(display_name.clone());
                    }
                }

                comments.push(view.clone());

                if let Some(message) = view.message {
                    fork!(
                        process_html_links,
                        (message, path.clone(), discussion_name.clone()),
                        (String, PathBuf, String),
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

            // Generate HTML file with discussion and comments
            let html_content = generate_discussion_html(&discussion, &comments);
            let html_path = path.join(format!("{discussion_name}.html"));
            let mut html_file = std::fs::File::create(html_path.clone())
                .with_context(|| format!("Unable to create file for {:?}", html_path))?;
            html_file
                .write_all(html_content.as_bytes())
                .with_context(|| format!("Could not write to file {:?}", html_path))?;
        }
        Result::Err(e) => {
            tracing::error!(
                "Error when getting discussion views at link:{url}, path:{path:?}\n{e:?}",
            );
        }
    }

    let files = attachments_all
        .into_iter()
        .map(|mut f| {
            f.display_name = format!("{}_{}", f.id, &f.display_name);
            f
        })
        .collect();
    let discussion_folder_path = path.join(discussion_name);
    let mut filtered_files = filter_files(&options, &discussion_folder_path, files);
    if !filtered_files.is_empty() {
        // create folder for discussion if there are files to download
        create_folder_if_not_exist_or_ignored(&discussion_folder_path, &options)?;

        let mut lock = options.files_to_download.lock().await;
        lock.append(&mut filtered_files);
    }

    Ok(())
}
