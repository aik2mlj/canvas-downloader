use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use futures::future::join_all;
use regex::Regex;
use reqwest::Url;
use select::document::Document;
use select::predicate::Name;

use crate::canvas::{File, ProcessOptions};
use crate::files::{filter_files, prepare_link_for_download, process_file_id};

pub async fn process_html_links(
    (html, path): (String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {

    // If file link is part of course files
    let re = Regex::new(r"/courses/[0-9]+/files/([0-9]+)").unwrap();
    let file_links = Document::from(html.as_str())
        .find(Name("a"))
        .filter_map(|n| n.attr("href"))
        .filter(|x| x.starts_with(&options.canvas_url))
        .map(|x| Url::parse(x))
        .filter(|x| x.is_ok())
        .map(|x| x.unwrap())
        .filter(|x| re.is_match(x.path()))
        .filter_map(|x| {
            // Extract file ID and use the correct Canvas API endpoint
            re.captures(x.path())
                .and_then(|cap| cap.get(1))
                .map(|file_id| format!("{}/api/v1/files/{}", options.canvas_url, file_id.as_str()))
        })
        .collect::<Vec<String>>();

    let mut link_files = join_all(file_links.into_iter()
        .map(|x| process_file_id((x, path.clone()), options.clone())))
        .await
        .into_iter()
        .filter_map(|x| x.ok())
        .collect::<Vec<File>>();

    // If image is from canvas it is likely the file url gives permission denied, so download from the CDN
    let image_links = Document::from(html.as_str())
        .find(Name("img"))
        .filter_map(|n| n.attr("src"))
        .filter(|x| x.starts_with(&options.canvas_url))
        .filter(|x| !x.contains("equation_images"))
        .map(|x| x.to_string())
        .collect::<Vec<String>>();

    link_files.append(join_all(image_links.into_iter()
        .map(|x| prepare_link_for_download((x, path.clone()), options.clone())))
        .await
        .into_iter()
        .filter_map(|x| x.ok())
        .collect::<Vec<File>>().as_mut());

    let mut filtered_files = filter_files(&options, &path, link_files);
    let mut lock = options.files_to_download.lock().await;
    lock.append(&mut filtered_files);

    Ok(())
}
