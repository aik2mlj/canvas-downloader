#![deny(clippy::unwrap_used)]

#[macro_use]
mod macros;

mod api;
mod assignments;
mod canvas;
mod discussions;
mod files;
mod html;
mod modules;
mod pages;
mod users;
mod utils;
mod videos;

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use std::{path::PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use futures::future::ready;
use futures::{stream, StreamExt, TryStreamExt};
use indicatif::{ProgressStyle};

use canvas::ProcessOptions;
use api::get_pages;
use files::{atomic_download_file, process_folders};
use assignments::process_assignments;
use discussions::process_discussions;
use modules::process_modules;
use pages::process_pages;
use users::process_users;
use videos::process_videos;
use utils::{create_folder_if_not_exist, print_all_courses_by_term};

#[derive(Parser)]
#[command(name = "Canvas Downloader")]
#[command(version)]
struct CommandLineOptions {
    #[arg(short = 'c', long, value_name = "FILE")]
    credential_file: PathBuf,
    #[arg(short = 'd', long, value_name = "FOLDER", default_value = ".")]
    destination_folder: PathBuf,
    #[arg(short = 'n', long)]
    download_newer: bool,
    #[arg(short = 't', long, value_name = "ID", num_args(1..))]
    term_ids: Option<Vec<u32>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = CommandLineOptions::parse();

    // Load credentials
    let file = std::fs::File::open(&args.credential_file)
        .with_context(|| "Could not open credential file")?;
    let cred: canvas::Credentials =
        serde_json::from_reader(file).with_context(|| "Credential file is not valid json")?;

    // Create sub-folder if not exists
    if !args.destination_folder.exists() {
        std::fs::create_dir(&args.destination_folder)
            .unwrap_or_else(|e| panic!("Failed to create destination directory, err={e}"));
    }

    // Prepare GET request options
    let client = reqwest::ClientBuilder::new()
        .tcp_keepalive(Some(Duration::from_secs(10)))
        .http2_keep_alive_interval(Some(Duration::from_secs(2)))
        .build()
        .with_context(|| "Failed to create HTTP client")?;
    let user_link = format!("{}/api/v1/users/self", cred.canvas_url);
    let user = client
        .get(&user_link)
        .bearer_auth(&cred.canvas_token)
        .send()
        .await?
        .json::<canvas::User>()
        .await
        .with_context(|| "Failed to get user info")?;
    let courses_link = format!("{}/api/v1/users/self/favorites/courses", cred.canvas_url);
    let options = Arc::new(ProcessOptions {
        canvas_token: cred.canvas_token.clone(),
        canvas_url: cred.canvas_url.clone(),
        client: client.clone(),
        user: user.clone(),
        // Process
        files_to_download: tokio::sync::Mutex::new(Vec::new()),
        download_newer: args.download_newer,
        // Download
        progress_bars: indicatif::MultiProgress::new(),
        progress_style: {
            let style_template = if termsize::get().map_or(false, |size| size.cols < 100) {
                "[{wide_bar:.cyan/blue}] {total_bytes} - {msg}"
            } else {
                "[{bar:20.cyan/blue}] {bytes}/{total_bytes} - {bytes_per_sec} - {msg}"
            };
            ProgressStyle::default_bar()
                .template(style_template)
                .unwrap_or_else(|e| panic!("Please report this issue on GitHub: error with progress bar style={style_template}, err={e}"))
                .progress_chars("=>-")
        },
        // Synchronization
        n_active_requests: AtomicUsize::new(0),
        sem_requests: tokio::sync::Semaphore::new(8), // WARN magic constant.
        notify_main: tokio::sync::Notify::new(),
        // TODO handle canvas rate limiting errors, maybe scale up if possible
    });

    // Get courses
    let courses: Vec<canvas::Course> = get_pages(courses_link.clone(), &options)
        .await?
        .into_iter()
        .map(|resp| resp.json::<Vec<serde_json::Value>>()) // resp --> Result<Vec<json>>
        .collect::<stream::FuturesUnordered<_>>() // (in any order)
        .flat_map_unordered(None, |json_res| {
            let jsons = json_res.unwrap_or_else(|e| panic!("Failed to parse courses, err={e}")); // Result<Vec<json>> --> Vec<json>
            stream::iter(jsons.into_iter()) // Vec<json> --> json
        })
        .filter(|json| ready(json.get("enrollments").is_some())) // (enrolled?)
        .map(serde_json::from_value) // json --> Result<course>
        .try_collect()
        .await
        .with_context(|| "Error when getting course json")?; // Result<course> --> course

    // Filter courses by term IDs
    let Some(term_ids) = args.term_ids else {
        println!("Please provide the Term ID(s) to download via -t");
        print_all_courses_by_term(&courses);
        return Ok(());
    };
    let courses_matching_term_ids: Vec<&canvas::Course> = courses
        .iter()
        .filter(|course_json| term_ids.contains(&course_json.enrollment_term_id))
        .collect();
    if courses_matching_term_ids.is_empty() {
        println!("Could not find any course matching Term ID(s) {term_ids:?}");
        println!("Please try the following ID(s) instead");
        print_all_courses_by_term(&courses);
        return Ok(());
    }

    println!("Courses found:");
    for course in courses_matching_term_ids {
        println!("  * {} - {}", course.course_code, course.name);

        // Prep path and mkdir -p
        let course_folder_path = args
            .destination_folder
            .join(course.course_code.replace('/', "_"));
        create_folder_if_not_exist(&course_folder_path)?;
        // Prep URL for course's root folder
        let course_folders_link = format!(
            "{}/api/v1/courses/{}/folders/by_path/",
            cred.canvas_url, course.id
        );

        let folder_path = course_folder_path.join("files");
        create_folder_if_not_exist(&folder_path)?;
        fork!(
            process_folders,
            (course_folders_link, folder_path),
            (String, PathBuf),
            options.clone()
        );

        let course_api_link = format!(
            "{}/api/v1/courses/{}/",
            cred.canvas_url, course.id
        );
        fork!(
            process_data,
            (course_api_link, course_folder_path.clone()),
            (String, PathBuf),
            options.clone()
        );

        fork!(
            process_videos,
            (cred.canvas_url.clone(), course.id, course_folder_path.clone()),
            (String, u32, PathBuf),
            options.clone()
        );
    }

    // Invariants
    // 1. Barrier semantics:
    //    1. Initial: n_active_requests > 0 by +1 synchronously in fork!()
    //    2. Recursion: fork()'s func +1 for subtasks before -1 own task
    //    3. --> n_active_requests == 0 only after all tasks done
    //    4. --> main() progresses only after all files have been queried
    // 2. No starvation: forks are done acyclically, all tasks +1 and -1 exactly once
    // 3. Bounded concurrency: acquire or block on semaphore before request
    // 4. No busy wait: Last task will see that there are 0 active requests and notify main
    options.notify_main.notified().await;
    assert_eq!(options.n_active_requests.load(Ordering::Acquire), 0);
    println!();

    let files_to_download = options.files_to_download.lock().await;
    println!(
        "Downloading {} file{}",
        files_to_download.len(),
        if files_to_download.len() == 1 {
            ""
        } else {
            "s"
        }
    );

    // Download files
    options.n_active_requests.fetch_add(1, Ordering::AcqRel); // prevent notifying until all spawned
    for canvas_file in files_to_download.iter() {
        fork!(
            atomic_download_file,
            canvas_file.clone(),
            canvas::File,
            options.clone()
        );
    }

    // Wait for downloads
    let new_val = options.n_active_requests.fetch_sub(1, Ordering::AcqRel) - 1;
    if new_val == 0 {
        // notify if all finished immediately
        options.notify_main.notify_one();
    }
    options.notify_main.notified().await;
    // Sanity check: running tasks trying to acquire sem will panic
    options.sem_requests.close();
    assert_eq!(options.n_active_requests.load(Ordering::Acquire), 0);

    for canvas_file in files_to_download.iter() {
        println!(
            "Downloaded {} to {}",
            canvas_file.display_name,
            canvas_file.filepath.to_string_lossy()
        );
    }

    Ok(())
}

async fn process_data(
    (url, path): (String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let assignments_path = path.join("assignments");
    create_folder_if_not_exist(&assignments_path)?;
    fork!(
        process_assignments,
        (url.clone(), assignments_path),
        (String, PathBuf),
        options.clone()
    );
    let users_path = path.join("users.json");
    fork!(
        process_users,
        (url.clone(), users_path),
        (String, PathBuf),
        options.clone()
    );
    fork!(
        process_discussions,
        (url.clone(), false, path.clone()),
        (String, bool, PathBuf),
        options.clone()
    );
    fork!(
        process_discussions,
        (url.clone(), true, path.clone()),
        (String, bool, PathBuf),
        options.clone()
    );
    fork!(
        process_pages,
        (url.clone(), path.clone()),
        (String, PathBuf),
        options.clone()
    );
    fork!(
        process_modules,
        (url.clone(), path.clone()),
        (String, PathBuf),
        options.clone()
    );
    Ok(())
}
