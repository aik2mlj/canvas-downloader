use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::api::get_canvas_api;
use crate::canvas::{ProcessOptions, Syllabus};
use crate::utils::{get_raw_json_path, prettify_json};

pub async fn process_syllabus(
    (course_id, path): (u32, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    // Get syllabus from Canvas API
    let syllabus_url = format!(
        "{}/api/v1/courses/{}?include[]=syllabus_body",
        options.canvas_url.trim_end_matches('/'),
        course_id
    );

    let syllabus_resp = get_canvas_api(syllabus_url, &options).await?;
    let syllabus_text = syllabus_resp.text().await?;

    // Try to parse the syllabus
    let syllabus_result = serde_json::from_str::<Syllabus>(&syllabus_text);

    match syllabus_result {
        Ok(syllabus) => {
            // Only create files if syllabus_body exists and is not empty
            if let Some(ref body) = syllabus.syllabus_body {
                if !body.trim().is_empty() {
                    // Save JSON file
                    if let Some(syllabus_json_path) = get_raw_json_path(
                        &path,
                        "syllabus.json",
                        &options.base_path,
                        options.save_json,
                    )? {
                        let mut json_file = std::fs::File::create(syllabus_json_path.clone())
                            .with_context(|| {
                                format!("Unable to create file for {:?}", syllabus_json_path)
                            })?;
                        let pretty_json =
                            prettify_json(&syllabus_text).unwrap_or(syllabus_text.clone());
                        json_file
                            .write_all(pretty_json.as_bytes())
                            .with_context(|| {
                                format!("Could not write to file {:?}", syllabus_json_path)
                            })?;
                    }

                    // Save HTML file
                    let syllabus_html = format!(
                        "<html><head><title>Syllabus - {}</title></head><body>{}</body></html>",
                        syllabus.name, body
                    );

                    let syllabus_html_path = path.join("syllabus.html");
                    let mut html_file = std::fs::File::create(syllabus_html_path.clone())
                        .with_context(|| {
                            format!("Unable to create file for {:?}", syllabus_html_path)
                        })?;

                    html_file
                        .write_all(syllabus_html.as_bytes())
                        .with_context(|| {
                            format!("Could not write to file {:?}", syllabus_html_path)
                        })?;

                    println!("📜 Syllabus synced");
                } else {
                    tracing::debug!(
                        "No syllabus content found for course {}",
                        syllabus.course_code
                    );
                }
            } else {
                tracing::debug!("No syllabus found for course {}", syllabus.course_code);
            }
        }
        Err(e) => {
            tracing::debug!("Error parsing syllabus: {}", e);
        }
    }

    Ok(())
}
