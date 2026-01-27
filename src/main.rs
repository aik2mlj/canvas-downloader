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
mod syllabus;
mod users;
mod utils;
mod videos;

use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use futures::future::ready;
use futures::{StreamExt, TryStreamExt, stream};
use ignore::gitignore::GitignoreBuilder;
use indicatif::ProgressStyle;

use api::get_pages;
use assignments::process_assignments;
use canvas::ProcessOptions;
use discussions::process_discussions;
use files::{atomic_download_file, process_folders};
use modules::process_modules;
use pages::process_pages;
use syllabus::process_syllabus;
use users::process_users;
use utils::{
    create_folder_if_not_exist_or_ignored, format_bytes, ignored, print_all_courses_by_term,
};
use videos::process_videos;

#[derive(Subcommand)]
enum Commands {
    /// Generate shell completion scripts
    Completions {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Parser)]
#[command(name = "Canvas Downloader")]
#[command(version)]
#[command(about = "Download and organize all your Canvas course materials 📚", long_about = None)]
struct CommandLineOptions {
    #[command(subcommand)]
    command: Option<Commands>,
    #[arg(
        long,
        value_name = "FILE",
        help = "Path to config file (default: platform-specific config locations)"
    )]
    config: Option<PathBuf>,

    #[arg(
        short = 'd',
        long,
        value_name = "FOLDER",
        default_value = ".",
        help = "Download location"
    )]
    destination_folder: PathBuf,

    #[arg(
        short = 'n',
        long,
        help = "Overwrite local files with newer Canvas versions"
    )]
    download_newer: bool,

    #[arg(
        short = 't',
        long,
        value_name = "ID",
        num_args(1..),
        help = "Term IDs to download"
    )]
    term_ids: Option<Vec<u32>>,

    #[arg(
        short = 'c',
        long,
        value_name = "NAME",
        num_args(1..),
        help = "Course names or codes to download - exact match"
    )]
    course_names: Option<Vec<String>>,

    #[arg(
        short = 'i',
        long,
        value_name = "FILE",
        default_value = ".canvasignore",
        help = "Path to ignore patterns file"
    )]
    ignore_file: PathBuf,

    #[arg(long, help = "Preview downloads without executing")]
    dry_run: bool,

    #[arg(long, default_value = "false", help = "Do not save raw JSON responses")]
    no_raw: bool,

    #[arg(short = 'v', long, help = "Enable debug logging")]
    verbose: bool,
}

fn load_ignore_file(
    ignore_file_path: &Path,
    base_path: &Path,
) -> Result<ignore::gitignore::Gitignore> {
    let mut builder = GitignoreBuilder::new(base_path);
    builder.add(ignore_file_path);
    builder
        .build()
        .with_context(|| format!("Failed to parse ignore file: {:?}", ignore_file_path))
}

fn find_config_file(config_path: Option<PathBuf>) -> Result<PathBuf> {
    // If config path is explicitly provided, use it
    if let Some(path) = config_path {
        if path.exists() {
            return Ok(path);
        } else {
            anyhow::bail!("Config file not found: {}", path.display());
        }
    }

    // Try <package-name>.toml in current directory
    let cwd_config = PathBuf::from(format!("{}.toml", env!("CARGO_PKG_NAME")));
    if cwd_config.exists() {
        return Ok(cwd_config);
    }

    // Try config.toml in platform-specific config directory
    if let Some(proj_dirs) = directories::ProjectDirs::from("", "", env!("CARGO_PKG_NAME")) {
        let config_dir_path = proj_dirs.config_dir().join("config.toml");
        if config_dir_path.exists() {
            return Ok(config_dir_path);
        }
    }

    anyhow::bail!(
        "Config file not found. Please create {}.toml in the current directory, or config.toml in your config directory, or use --config to specify a path.",
        env!("CARGO_PKG_NAME")
    )
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = CommandLineOptions::parse();

    // Handle subcommands
    if let Some(command) = args.command {
        match command {
            Commands::Completions { shell } => {
                let mut cmd = CommandLineOptions::command();
                generate(shell, &mut cmd, "canvas-downloader", &mut std::io::stdout());
                return Ok(());
            }
        }
    }

    // Initialize tracing
    let filter = if args.verbose {
        "canvas_downloader=debug"
    } else {
        "canvas_downloader=info"
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    // Load credentials
    let config_path = find_config_file(args.config)?;
    let config_content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Could not read config file: {}", config_path.display()))?;
    let cred: canvas::Credentials =
        toml::from_str(&config_content).with_context(|| "Config file is not valid TOML")?;

    // Create sub-folder if not exists
    if !args.destination_folder.exists() {
        std::fs::create_dir(&args.destination_folder)
            .unwrap_or_else(|e| panic!("Failed to create destination directory, err={e}"));
    }

    // Prepare GET request options
    let user_agent = format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    let client = reqwest::ClientBuilder::new()
        .user_agent(user_agent)
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
    let courses_link = format!("{}/api/v1/users/self/courses", cred.canvas_url);

    // Load ignore file if it exists
    let ignore_matcher = if args.ignore_file.exists() {
        Some(Arc::new(load_ignore_file(
            &args.ignore_file,
            &args.destination_folder,
        )?))
    } else {
        None
    };

    // if ignored by ignore file, disable saving raw json
    let raw_folder_path = args.destination_folder.join("raw");
    if ignored(
        &raw_folder_path,
        true,
        &args.destination_folder,
        ignore_matcher.as_deref(),
    ) {
        args.no_raw = true;
    }

    let options = Arc::new(ProcessOptions {
        canvas_token: cred.canvas_token.clone(),
        canvas_url: cred.canvas_url.clone(),
        client: client.clone(),
        user: user.clone(),
        // Process
        files_to_download: tokio::sync::Mutex::new(Vec::new()),
        download_newer: args.download_newer,
        ignore_matcher,
        base_path: args.destination_folder.clone(),
        // dry_run: args.dry_run,
        save_json: !args.no_raw,
        // Download
        progress_bars: indicatif::MultiProgress::new(),
        progress_style: {
            let style_template = if termsize::get().is_some_and(|size| size.cols < 100) {
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
        // Progress counters
        n_syllabi: AtomicUsize::new(0),
        n_users: AtomicUsize::new(0),
        n_assignments: AtomicUsize::new(0),
        n_pages: AtomicUsize::new(0),
        n_discussions: AtomicUsize::new(0),
        n_announcements: AtomicUsize::new(0),
        n_modules: AtomicUsize::new(0),
        n_videos: AtomicUsize::new(0),
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

    // Filter courses by term IDs and/or course names
    if args.term_ids.is_none() && args.course_names.is_none() {
        println!("Please provide either Term ID(s) via -t or course name(s)/code(s) via -c");
        print_all_courses_by_term(&courses);
        return Ok(());
    }

    let courses_to_download: Vec<&canvas::Course> = courses
        .iter()
        .filter(|course| {
            // Filter by term IDs if provided
            let matches_term = args
                .term_ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&course.enrollment_term_id));

            // Filter by course names if provided (exact match)
            let matches_name = args.course_names.as_ref().is_none_or(|names| {
                names
                    .iter()
                    .any(|name| &course.name == name || &course.course_code == name)
            });

            matches_term && matches_name
        })
        .collect();

    if courses_to_download.is_empty() {
        if let Some(ref term_ids) = args.term_ids {
            if let Some(ref course_names) = args.course_names {
                tracing::warn!(
                    "Could not find any course matching Term ID(s) {term_ids:?} AND course name(s) {course_names:?}"
                );
            } else {
                tracing::warn!("Could not find any course matching Term ID(s) {term_ids:?}");
            }
        } else if let Some(ref course_names) = args.course_names {
            tracing::warn!("Could not find any course matching course name(s) {course_names:?}");
        }
        println!("Please try the following instead:");
        print_all_courses_by_term(&courses);
        return Ok(());
    }

    println!("Courses found:");

    // create raw folder if needed
    if !args.no_raw && !raw_folder_path.exists() {
        std::fs::create_dir(&raw_folder_path).with_context(|| {
            format!(
                "Failed to create raw JSON directory: {}",
                raw_folder_path.to_string_lossy()
            )
        })?;
    }

    for course in courses_to_download {
        println!("  * {} - {}", course.course_code, course.name);

        // Prep path and mkdir -p
        let course_folder_path = args
            .destination_folder
            .join(course.course_code.replace('/', "_"));
        if !create_folder_if_not_exist_or_ignored(&course_folder_path, &options)? {
            continue;
        }
        // Prep URL for course's root folder
        let course_folders_link = format!(
            "{}/api/v1/courses/{}/folders/by_path/",
            cred.canvas_url, course.id
        );

        let folder_path = course_folder_path.join("files"); // TODO: if no files, skip creating folder
        if create_folder_if_not_exist_or_ignored(&folder_path, &options)? {
            fork!(
                process_folders,
                (course_folders_link, folder_path),
                (String, PathBuf),
                options.clone()
            );
        }

        let course_api_link = format!("{}/api/v1/courses/{}/", cred.canvas_url, course.id);
        fork!(
            process_data,
            (course_api_link, course.id, course_folder_path.clone()),
            (String, u32, PathBuf),
            options.clone()
        );

        fork!(
            process_videos,
            (
                cred.canvas_url.clone(),
                course.id,
                course_folder_path.clone()
            ),
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

    // Print sync summary
    let mut synced = Vec::new();
    if options.n_syllabi.load(Ordering::Relaxed) > 0 {
        synced.push("📜 Syllabi");
    }
    if options.n_users.load(Ordering::Relaxed) > 0 {
        synced.push("👥 Users");
    }
    if options.n_assignments.load(Ordering::Relaxed) > 0 {
        synced.push("📝 Assignments");
    }
    if options.n_pages.load(Ordering::Relaxed) > 0 {
        synced.push("📄 Pages");
    }
    if options.n_discussions.load(Ordering::Relaxed) > 0 {
        synced.push("💬 Discussions");
    }
    if options.n_announcements.load(Ordering::Relaxed) > 0 {
        synced.push("📢 Announcements");
    }
    if options.n_modules.load(Ordering::Relaxed) > 0 {
        synced.push("📦 Modules");
    }
    if options.n_videos.load(Ordering::Relaxed) > 0 {
        synced.push("🎬 Videos");
    }
    if !synced.is_empty() {
        println!("{} synced", synced.join(", "));
    }
    println!();

    let files_to_download = options.files_to_download.lock().await;

    if args.dry_run {
        // Dry run mode: just display what would be downloaded
        if files_to_download.is_empty() {
            println!("[DRY RUN] No files to download.");
            return Ok(());
        }

        println!("[DRY RUN] Active filters:");
        if args.ignore_file.exists() {
            println!("  - Ignore file: {}", args.ignore_file.display());
        } else {
            println!("  - Ignore file: none");
        }
        println!(
            "  - Download newer files: {}",
            if args.download_newer {
                "enabled"
            } else {
                "disabled"
            }
        );
        println!();

        // Calculate total size
        let total_size: u64 = files_to_download.iter().map(|f| f.size).sum();

        println!(
            "[DRY RUN] Would download {} file{} ({}):",
            files_to_download.len(),
            if files_to_download.len() == 1 {
                ""
            } else {
                "s"
            },
            format_bytes(total_size)
        );
        println!();
        for canvas_file in files_to_download.iter() {
            println!(
                "  {} -> {} ({})",
                canvas_file.url,
                canvas_file.filepath.to_string_lossy(),
                format_bytes(canvas_file.size)
            );
        }
        println!();
        println!(
            "[DRY RUN] Total: {} file{} ({})",
            files_to_download.len(),
            if files_to_download.len() == 1 {
                ""
            } else {
                "s"
            },
            format_bytes(total_size)
        );
    } else {
        // Normal mode: actually download files
        // Calculate total size
        let total_size: u64 = files_to_download.iter().map(|f| f.size).sum();

        // Check if there are no files to download
        if files_to_download.is_empty() {
            println!("No files to download.");
            return Ok(());
        }

        // Display files to be downloaded
        println!(
            "Will download {} file{} ({}):",
            files_to_download.len(),
            if files_to_download.len() == 1 {
                ""
            } else {
                "s"
            },
            format_bytes(total_size)
        );
        println!();
        for canvas_file in files_to_download.iter() {
            println!(
                "  {} ({})",
                canvas_file.filepath.to_string_lossy(),
                format_bytes(canvas_file.size)
            );
        }
        println!();
        println!(
            "Total: {} file{} ({})",
            files_to_download.len(),
            if files_to_download.len() == 1 {
                ""
            } else {
                "s"
            },
            format_bytes(total_size)
        );

        // Ask for confirmation
        print!("Proceed with download? [y]/n: ");
        std::io::Write::flush(&mut std::io::stdout()).expect("Failed to flush stdout");

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .expect("Failed to read user input");

        let input = input.trim().to_lowercase();
        if !input.is_empty() && input != "y" && input != "yes" {
            println!("Download cancelled.");
            return Ok(());
        }

        println!();
        println!("Starting download...");

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

        println!("📁 Files downloaded");
    }

    Ok(())
}

async fn process_data(
    (url, course_id, path): (String, u32, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    fork!(
        process_assignments,
        (url.clone(), path.clone()),
        (String, PathBuf),
        options.clone()
    );
    fork!(
        process_users,
        (url.clone(), path.clone()),
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
    fork!(
        process_syllabus,
        (course_id, path.clone()),
        (u32, PathBuf),
        options.clone()
    );
    Ok(())
}
