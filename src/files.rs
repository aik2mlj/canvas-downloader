use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Add;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Error, Result};
use chrono::{DateTime, Local};
use regex::Regex;
use reqwest::header;

use crate::api::get_canvas_api;
use crate::api::get_pages;
use crate::canvas::{File, FileResult, FolderResult, ProcessOptions};

pub async fn atomic_download_file(file: File, options: Arc<ProcessOptions>) -> Result<()> {
    // Create tmp file from hash
    let mut tmp_path = file.filepath.clone();
    tmp_path.pop();
    let mut h = DefaultHasher::new();
    file.display_name.hash(&mut h);
    tmp_path.push(&h.finish().to_string().add(".tmp"));

    // Aborted download?
    if let Err(e) = download_file((&tmp_path, &file), options.clone()).await {
        if let Err(e) = std::fs::remove_file(&tmp_path) {
            eprintln!(
                "Failed to remove temporary file {tmp_path:?} for {}, err={e:?}",
                file.display_name
            );
        }
        return Err(e);
    }

    // Update file time
    let updated_at = DateTime::parse_from_rfc3339(&file.updated_at)?;
    let updated_time = filetime::FileTime::from_unix_time(
        updated_at.timestamp(),
        updated_at.timestamp_subsec_nanos(),
    );
    if let Err(e) = filetime::set_file_mtime(&tmp_path, updated_time) {
        eprintln!(
            "Failed to set modified time of {} with updated_at of {}, err={e:?}",
            file.display_name, file.updated_at
        )
    }

    // Atomically rename file, doesn't change mtime
    std::fs::rename(&tmp_path, &file.filepath)?;
    Ok(())
}

async fn download_file(
    (tmp_path, canvas_file): (&PathBuf, &File),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    // Get file
    let mut resp = options
        .client
        .get(&canvas_file.url)
        .bearer_auth(&options.canvas_token)
        .send()
        .await
        .with_context(|| format!("Something went wrong when reaching {}", canvas_file.url))?;
    if !resp.status().is_success() {
        return Err(Error::msg(format!(
            "Failed to download {}, got {resp:?}",
            canvas_file.display_name
        )));
    }

    // Create + Open file
    let mut file = std::fs::File::create(tmp_path)
        .with_context(|| format!("Unable to create tmp file for {:?}", canvas_file.filepath))?;

    // Progress bar
    let download_size = resp
        .headers() // Gives us the HeaderMap
        .get(header::CONTENT_LENGTH) // Gives us an Option containing the HeaderValue
        .and_then(|ct_len| ct_len.to_str().ok()) // Unwraps the Option as &str
        .and_then(|ct_len| ct_len.parse().ok()) // Parses the Option as u64
        .unwrap_or(0); // Fallback to 0
    let progress_bar = options.progress_bars.add(indicatif::ProgressBar::new(download_size));
    progress_bar.set_message(canvas_file.display_name.to_string());
    progress_bar.set_style(options.progress_style.clone());

    // Download
    while let Some(chunk) = resp.chunk().await? {
        progress_bar.inc(chunk.len() as u64);
        let mut cursor = std::io::Cursor::new(chunk);
        std::io::copy(&mut cursor, &mut file)
            .with_context(|| format!("Could not write to file {:?}", canvas_file.filepath))?;
    }

    progress_bar.finish();
    Ok(())
}

// async recursion needs boxing
pub async fn process_folders(
    (url, path): (String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let pages = get_pages(url, &options).await?;

    // For each page
    for pg in pages {
        let uri = pg.url().to_string();
        let folders_result = pg.json::<FolderResult>().await;

        match folders_result {
            // Got folders
            Ok(FolderResult::Ok(folders)) => {
                for folder in folders {
                    // println!("  * {} - {}", folder.id, folder.name);
                    let sanitized_folder_name = sanitize_filename::sanitize(folder.name);
                    // if the folder has no parent, it is the root folder of a course
                    // so we avoid the extra directory nesting by not appending the root folder name
                    let folder_path = if folder.parent_folder_id.is_some() {
                        path.join(sanitized_folder_name)
                    } else {
                        path.clone()
                    };
                    if !folder_path.exists() {
                        if let Err(e) = std::fs::create_dir(&folder_path) {
                            eprintln!(
                                "Failed to create directory: {}, err={e}",
                                folder_path.to_string_lossy()
                            );
                            continue;
                        };
                    }

                    fork!(
                        process_files,
                        (folder.files_url, folder_path.clone()),
                        (String, PathBuf),
                        options.clone()
                    );
                    fork!(
                        process_folders,
                        (folder.folders_url, folder_path),
                        (String, PathBuf),
                        options.clone()
                    );
                }
            }

            // Got status code
            Ok(FolderResult::Err { status }) => {
                let course_has_no_folders = status == "unauthorized";
                if !course_has_no_folders {
                    eprintln!(
                        "Failed to access folders at link:{uri}, path:{path:?}, status:{status}",
                    );
                }
            }

            // Parse error
            Err(e) => {
                eprintln!("Error when getting folders at link:{uri}, path:{path:?}\n{e:?}",);
            }
        }
    }

    Ok(())
}

pub async fn process_files((url, path): (String, PathBuf), options: Arc<ProcessOptions>) -> Result<()> {
    let pages = get_pages(url, &options).await?;

    // For each page
    for pg in pages {
        let uri = pg.url().to_string();
        let files_result = pg.json::<FileResult>().await;

        match files_result {
            // Got files
            Ok(FileResult::Ok(files)) => {
                let mut filtered_files = filter_files(&options, &path, files);
                let mut lock = options.files_to_download.lock().await;
                lock.append(&mut filtered_files);
            }

            // Got status code
            Ok(FileResult::Err { status }) => {
                let course_has_no_files = status == "unauthorized";
                if !course_has_no_files {
                    eprintln!(
                        "Failed to access files at link:{uri}, path:{path:?}, status:{status}",
                    );
                }
            }

            // Parse error
            Err(e) => {
                eprintln!("Error when getting files at link:{uri}, path:{path:?}\n{e:?}",);
            }
        };
    }

    Ok(())
}

fn updated(filepath: &PathBuf, new_modified: &str) -> bool {
    (|| -> Result<bool> {
        let old_modified = std::fs::metadata(filepath)?.modified()?;
        let new_modified =
            std::time::SystemTime::from(DateTime::parse_from_rfc3339(new_modified)?);
        let updated = old_modified < new_modified;
        if updated {
            println!("Found update for {filepath:?}. Use -n to download updated files.");
        }
        Ok(updated)
    })()
    .unwrap_or(false)
}

pub fn filter_files(options: &ProcessOptions, path: &Path, files: Vec<File>) -> Vec<File> {

    // only download files that do not exist or are updated
    files
        .into_iter()
        .map(|mut f| {
            let sanitized_filename = sanitize_filename::sanitize(&f.display_name);
            f.filepath = path.join(sanitized_filename);
            f
        })
        .filter(|f| !f.locked_for_user)
        .filter(|f| {
            if DateTime::parse_from_rfc3339(&f.updated_at).is_ok() {
                return true;
            }
            eprintln!(
                "Failed to parse updated_at time for {}, {}",
                f.display_name, f.updated_at
            );
            false
        })
        .filter(|f| {
            !f.filepath.exists() || (updated(&f.filepath, &f.updated_at) && options.download_newer)
        })
        .collect()
}

pub async fn process_file_id(
    (url, path): (String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<File> {
    let file_resp = get_canvas_api(url.clone(), &options).await?;
    let file_result = file_resp.json::<File>().await;
    match file_result {
        Result::Ok(mut file) => {
            let sanitized_filename = sanitize_filename::sanitize(&file.display_name);
            let file_path = path.join(sanitized_filename);
            file.filepath = file_path;
            return Ok(file);
        }
        Err(e) => {
            eprintln!("Error when getting file info at link:{url}, path:{path:?}\n{e:?}",);
            return Err(Into::into(e));
        }
    }
}
pub async fn prepare_link_for_download(
    (link, path): (String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<File> {

    let resp = options
        .client
        .head(&link)
        .bearer_auth(&options.canvas_token)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    let headers = resp.headers();
    // get filename out of Content-Disposition header
    let filename = headers
        .get(header::CONTENT_DISPOSITION)
        .and_then(|x| x.to_str().ok())
        .and_then(|x| {
            let re = Regex::new(r#"filename="(.*)""#).unwrap();
            re.captures(x)
        })
        .and_then(|x| x.get(1))
        .map(|x| x.as_str())
        .unwrap_or_else(|| {
            let re = Regex::new(r"/([^/]+)$").unwrap();
            re.captures(&link)
                .and_then(|x| x.get(1))
                .map(|x| x.as_str())
                .unwrap_or("unknown")
        });
    // last-modified header to TZ string
    let updated_at = headers
        .get(header::LAST_MODIFIED)
        .and_then(|x| x.to_str().ok())
        .and_then(|x| {
            let dt = DateTime::parse_from_rfc2822(x).ok()?;
            Some(dt.with_timezone(&Local).to_rfc3339())
        })
        .unwrap_or_else(|| Local::now().to_rfc3339());

    let sanitized_filename = sanitize_filename::sanitize(filename);
    let file = File {
        id: 0,
        folder_id: 0,
        display_name: filename.to_string(),
        size: 0,
        url: link.clone(),
        updated_at: updated_at,
        locked_for_user: false,
        filepath: path.join(sanitized_filename),
    };
    Ok(file)
}
