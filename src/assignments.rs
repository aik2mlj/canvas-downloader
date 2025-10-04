use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::api::{get_canvas_api, get_pages};
use crate::canvas::{AssignmentResult, ProcessOptions, Submission};
use crate::files::filter_files;
use crate::html::process_html_links;
use crate::utils::{create_folder_if_not_exist, prettify_json};

pub async fn process_assignments(
    (url, path): (String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let assignments_url = format!("{}assignments?include[]=submission&include[]=assignment_visibility&include[]=all_dates&include[]=overrides&include[]=observed_users&include[]=can_edit&include[]=score_statistics", url);
    let pages = get_pages(assignments_url, &options).await?;

    let assignments_json = path.join("assignments.json");
    let mut assignments_file = std::fs::File::create(assignments_json.clone())
        .with_context(|| format!("Unable to create file for {:?}", assignments_json))?;

    for pg in pages {
        let uri = pg.url().to_string();
        let page_body = pg.text().await?;

        let pretty_json = prettify_json(&page_body).unwrap_or(page_body.clone());
        assignments_file
            .write_all(pretty_json.as_bytes())
            .with_context(|| format!("Unable to write to file for {:?}", assignments_json))?;

        let assignment_result = serde_json::from_str::<AssignmentResult>(&page_body);

        match assignment_result {
            Ok(AssignmentResult::Ok(assignments)) | Ok(AssignmentResult::Direct(assignments)) => {
                for assignment in assignments {
                    let assignment_path = path.join(sanitize_filename::sanitize(assignment.name));
                    create_folder_if_not_exist(&assignment_path)?;
                    let submissions_url = format!("{}assignments/{}/submissions/", url, assignment.id);
                    fork!(
                        process_submissions,
                        (submissions_url, assignment_path.clone()),
                        (String, PathBuf),
                        options.clone()
                    );
                    fork!(
                        process_html_links,
                        (assignment.description, assignment_path),
                        (String, PathBuf),
                        options.clone()
                    );
                }
            }
            Ok(AssignmentResult::Err { status }) => {
                eprintln!(
                    "Failed to access assignments at link:{uri}, path:{path:?}, status:{status}",
                );
            }
            Ok(AssignmentResult::Empty(_)) => {
                eprintln!("No assignments found for url {} (empty response)", uri);
            }
            Err(e) => {
                eprintln!("Error when getting assignments at link:{uri}, path:{path:?}\n{e:?}",);
            }
        }
    }
    Ok(())
}

async fn process_submissions(
    (url, path): (String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let submissions_url = format!("{}{}", url, options.user.id);

    let resp = get_canvas_api(submissions_url, &options).await?;
    let submissions_body = resp.text().await?;
    let submissions_json = path.join("submission.json");
    let mut submissions_file = std::fs::File::create(submissions_json.clone())
        .with_context(|| format!("Unable to create file for {:?}", submissions_json))?;

    let pretty_json = prettify_json(&submissions_body).unwrap_or(submissions_body.clone());
    submissions_file
        .write_all(pretty_json.as_bytes())
        .with_context(|| format!("Unable to write to file for {:?}", submissions_json))?;

    let submissions_result = serde_json::from_str::<Submission>(&submissions_body);
    match submissions_result {
        Result::Ok(submissions) => {
            let mut filtered_files = filter_files(&options, &path, submissions.attachments);
            let mut lock = options.files_to_download.lock().await;
            lock.append(&mut filtered_files);
        }
        Result::Err(e) => {
            eprintln!("Error when getting submissions at link:{url}, path:{path:?}\n{e:?}",);
        }
    }
    Ok(())
}
