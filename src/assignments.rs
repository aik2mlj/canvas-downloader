use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use anyhow::{Context, Result};

use crate::api::{get_canvas_api, get_pages};
use crate::canvas::{Assignment, AssignmentResult, ProcessOptions, Submission};
use crate::files::filter_files;
use crate::html::process_html_links;
use crate::utils::{create_folder_if_not_exist_or_ignored, get_raw_json_path, prettify_json};

pub async fn process_assignments(
    (url, path): (String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let assignments_url = format!(
        "{}assignments?include[]=submission&include[]=assignment_visibility&include[]=all_dates&include[]=overrides&include[]=observed_users&include[]=can_edit&include[]=score_statistics",
        url
    );
    let pages = get_pages(assignments_url, &options).await?;

    let mut has_assignments = false;
    let mut assignments_folder_path = None;

    for pg in pages {
        let uri = pg.url().to_string();
        let page_body = pg.text().await?;

        let assignment_result = serde_json::from_str::<AssignmentResult>(&page_body);

        match assignment_result {
            Ok(AssignmentResult::Ok(assignments)) => {
                if !assignments.is_empty() && !has_assignments {
                    // Create assignments folder only when we have actual assignments
                    let folder_path = path.join("assignments");
                    if !create_folder_if_not_exist_or_ignored(&folder_path, &options)? {
                        continue;
                    }
                    assignments_folder_path = Some(folder_path.clone());
                    has_assignments = true;

                    // Create assignments.json file
                    if let Some(assignments_json_path) = get_raw_json_path(
                        &path,
                        "assignments.json",
                        &options.base_path,
                        options.save_json,
                    )? {
                        let mut assignments_json_file =
                            std::fs::File::create(assignments_json_path.clone()).with_context(
                                || format!("Unable to create file for {:?}", assignments_json_path),
                            )?;
                        let pretty_json = prettify_json(&page_body).unwrap_or(page_body.clone());
                        assignments_json_file
                            .write_all(pretty_json.as_bytes())
                            .with_context(|| {
                                format!("Unable to write to file for {:?}", assignments_json_path)
                            })?;
                    }
                }

                for assignment in assignments {
                    if let Some(ref folder_path) = assignments_folder_path {
                        // let assignment_path = path.join(sanitize_filename::sanitize(&assignment.name));
                        let submissions_url =
                            format!("{}assignments/{}/submissions/", url, assignment.id);
                        fork!(
                            process_submissions,
                            (submissions_url, folder_path.clone(), assignment.clone()),
                            (String, PathBuf, Assignment),
                            options.clone()
                        );
                        if let Some(desc) = assignment.description {
                            fork!(
                                process_html_links,
                                (desc, folder_path.clone(), assignment.name.clone()),
                                (String, PathBuf, String),
                                options.clone()
                            );
                        }
                    }
                }
            }
            Ok(AssignmentResult::Err { status }) => {
                tracing::error!(
                    "Failed to access assignments at link:{uri}, path:{path:?}, status:{status}",
                );
            }
            Err(e) => {
                tracing::error!(
                    "Error when getting assignments at link:{uri}, path:{path:?}\n{e:?}",
                );
            }
        }
    }

    if has_assignments {
        options.n_assignments.fetch_add(1, Ordering::Relaxed);
    }

    Ok(())
}

fn generate_assignment_html(assignment: &Assignment) -> String {
    let mut html = String::new();

    html.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    html.push_str("    <meta charset=\"UTF-8\">\n");
    html.push_str(&format!(
        "    <title>{}</title>\n",
        html_escape(&assignment.name)
    ));
    html.push_str(r#"    <style>
        body { font-family: system-ui, -apple-system, "Segoe UI", Arial, sans-serif; font-size:16px; line-height:1.5; max-width:900px; margin:20px auto; padding:0 20px; }

        .assignment-header { background:#f9f9f9; border-left:4px solid #2196F3; padding:20px; margin-bottom:30px; }
        .assignment-title { font-size:1.5rem; font-weight:600; margin-bottom:12px; }
        .assignment-meta { color:#666; font-size:0.8rem; margin-bottom:2px; }
        .assignment-meta-label { font-weight:500; display:inline-block; min-width:8.5rem; }

        .assignment-description { font-size:0.95rem; margin-top:20px; padding-top:20px; border-top:1px solid #ddd; }
        .assignment-description p { margin:0; }
        .assignment-description p + p { margin-top:0.75em; }

        .submission-types { display:inline-flex; gap:8px; flex-wrap:wrap; }
        .submission-type { background:#e3f2fd; padding:0px 5px; border-radius:4px; font-size:0.8rem; }
    </style>
"#);
    html.push_str("</head>\n<body>\n");

    // Assignment header
    html.push_str("    <div class=\"assignment-header\">\n");
    html.push_str(&format!(
        "        <div class=\"assignment-title\">{}</div>\n",
        html_escape(&assignment.name)
    ));

    // Created date
    if let Some(ref created_at) = assignment.created_at {
        html.push_str("        <div class=\"assignment-meta\">\n");
        html.push_str("            <span class=\"assignment-meta-label\">Created:</span>\n");
        html.push_str(&format!("            {}\n", html_escape(created_at)));
        html.push_str("        </div>\n");
    }

    // Due date
    if let Some(ref due_at) = assignment.due_at {
        html.push_str("        <div class=\"assignment-meta\">\n");
        html.push_str("            <span class=\"assignment-meta-label\">Due:</span>\n");
        html.push_str(&format!("            {}\n", html_escape(due_at)));
        html.push_str("        </div>\n");
    }

    // Submission types
    if let Some(ref submission_types) = assignment.submission_types
        && !submission_types.is_empty()
    {
        html.push_str("        <div class=\"assignment-meta\">\n");
        html.push_str(
            "            <span class=\"assignment-meta-label\">Submission Types:</span>\n",
        );
        html.push_str("            <div class=\"submission-types\">\n");
        for submission_type in submission_types {
            html.push_str(&format!(
                "                <span class=\"submission-type\">{}</span>\n",
                html_escape(submission_type)
            ));
        }
        html.push_str("            </div>\n");
        html.push_str("        </div>\n");
    }

    // Description
    html.push_str("        <div class=\"assignment-description\">\n");
    // assignment.description is an Option<String>
    html.push_str(&format!(
        "            {}\n",
        assignment.description.as_deref().unwrap_or("")
    ));
    html.push_str("        </div>\n");
    html.push_str("    </div>\n");

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

async fn process_submissions(
    (url, path, assignment): (String, PathBuf, Assignment),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let submissions_url = format!("{}{}", url, options.user.id);

    let resp = get_canvas_api(submissions_url.clone(), &options).await?;
    let submissions_body = resp.text().await?;

    let assignment_name = sanitize_filename::sanitize(&assignment.name);
    let assignment_folder_path = path.join(assignment_name.clone());
    if let Some(submissions_json) = get_raw_json_path(
        &path,
        &format!("{assignment_name}.json"),
        &options.base_path,
        options.save_json,
    )? {
        let mut submissions_file = std::fs::File::create(submissions_json.clone())
            .with_context(|| format!("Unable to create file for {:?}", submissions_json))?;

        let pretty_json = prettify_json(&submissions_body).unwrap_or(submissions_body.clone());
        submissions_file
            .write_all(pretty_json.as_bytes())
            .with_context(|| format!("Unable to write to file for {:?}", submissions_json))?;
    }

    let submissions_result = serde_json::from_str::<Submission>(&submissions_body);
    match submissions_result {
        Result::Ok(submissions) => {
            let mut filtered_files =
                filter_files(&options, &assignment_folder_path, submissions.attachments);

            if !filtered_files.is_empty() {
                // create folder for assignment if there are files to download
                create_folder_if_not_exist_or_ignored(&assignment_folder_path, &options)?;

                let mut lock = options.files_to_download.lock().await;
                lock.append(&mut filtered_files);
            }
        }
        Result::Err(e) => {
            tracing::error!(
                "Error when getting submissions at link:{submissions_url}, path:{path:?}\n{e:?}",
            );
        }
    }

    // Generate HTML file for the assignment
    let html_content = generate_assignment_html(&assignment);
    let html_path = path.join(format!("{assignment_name}.html"));
    let mut html_file = std::fs::File::create(html_path.clone())
        .with_context(|| format!("Unable to create file for {:?}", html_path))?;
    html_file
        .write_all(html_content.as_bytes())
        .with_context(|| format!("Could not write to file {:?}", html_path))?;

    Ok(())
}
