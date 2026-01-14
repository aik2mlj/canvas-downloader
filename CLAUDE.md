# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Canvas Downloader is an async Rust CLI tool that downloads and organizes Canvas LMS course materials. It uses concurrent task processing with a custom fork-based architecture and implements bounded concurrency with semaphores.

## Build & Development Commands

```bash
# Build the project
cargo build

# Build release binary (optimized for size)
cargo build --release

# Run the tool
cargo run -- [OPTIONS]

# Format code (required before commits)
cargo fmt --all

# Run clippy (currently not enforced in pre-commit)
cargo clippy --all-targets --all-features -- -D warnings

# Check compilation without building
cargo check
```

The project uses Rust edition 2024 and has aggressive release optimizations (`opt-level = "z"`, `lto = "thin"`, strip enabled) to minimize binary size.

## Architecture

### Concurrency Model

The codebase uses a custom async concurrency pattern built around the `fork!` macro (src/macros.rs):

- **fork! macro**: Spawns async tasks with automatic reference counting and barrier synchronization
  - Increments `n_active_requests` before spawning
  - Decrements after completion
  - When counter reaches 0, notifies main thread via `notify_main`
  - Enforces bounded concurrency via `sem_requests` semaphore (max 8 concurrent requests)

**Key invariants** (see main.rs:394-402):

1. `n_active_requests == 0` only after all tasks complete (barrier semantics)
2. No starvation: forks are acyclic, all tasks +1/-1 exactly once
3. Bounded concurrency: tasks acquire semaphore before HTTP requests
4. No busy wait: last task notifies main thread

### Module Structure

- **main.rs**: Entry point, orchestrates the download workflow, handles CLI arguments, and manages the two-phase process (query phase → download phase)
- **canvas.rs**: Core data structures (`Course`, `File`, `User`, etc.) and `ProcessOptions` shared across all async tasks
- **api.rs**: Canvas API client with pagination support (`get_pages`) and retry logic with exponential backoff
- **macros.rs**: The `fork!` macro that enables the recursive async task pattern
- **files.rs**: File/folder traversal and download logic
- **modules.rs, pages.rs, assignments.rs, discussions.rs, syllabus.rs, users.rs, videos.rs**: Content-specific processors that fork subtasks
- **html.rs**: HTML generation utilities for downloaded content
- **utils.rs**: Helper functions for path handling, ignore patterns, and formatting

### Two-Phase Process

1. **Query Phase** (main.rs:287-405):
   - Recursively fork tasks to query Canvas API for all content
   - Each task discovers files and adds them to `files_to_download` vector
   - Waits on barrier (`notify_main.notified().await`) until all queries complete

2. **Download Phase** (main.rs:466-546):
   - Shows user what will be downloaded and asks for confirmation
   - Forks download tasks for each file
   - Waits on barrier again until all downloads complete

### Content Organization

Downloaded files are organized as:

```
<destination>/
├── <course_code>/
│   ├── files/                 # Canvas files (preserves folder structure)
│   ├── assignments/           # Assignment HTML pages (*.html)
│   ├── discussions/           # Discussion thread HTML pages
│   ├── announcements/         # Announcement HTML pages
│   ├── pages/                 # Course page HTML files
│   ├── modules/               # Module overview HTML
│   ├── syllabus.html          # Course syllabus HTML
│   └── ...
└── raw/
    └── <course_code>/
        ├── assignments.json       # Summary JSON for all assignments
        ├── announcements.json     # Summary JSON for all announcements
        ├── discussions.json       # Summary JSON for all discussions
        ├── syllabus.json          # Syllabus JSON
        ├── users.json             # User info JSON
        ├── assignments/           # Individual assignment JSON files (*.json)
        ├── announcements/         # Individual announcement JSON files
        ├── discussions/           # Individual discussion JSON files
        └── ...
```

As of v0.3.5, JSON summary files (e.g., `assignments.json`) are stored directly under `raw/<course_code>/`, not in subfolders.

## Important Patterns

### Error Handling

- Code enforces `#![deny(clippy::unwrap_used)]` - never use `.unwrap()`, use `.unwrap_or_else()` with panic messages or proper error handling
- Use `anyhow::Context` to add context to errors: `.with_context(|| "description")?`
- Fork macro catches errors and logs them via `tracing::error!`

### Canvas API Requests

- Always use `api::get_canvas_api()` or `api::get_pages()` for Canvas API calls
- `get_pages()` handles pagination automatically via LINK headers
- Exponential backoff retry logic is built into `get_canvas_api()` (3 retries for 403s)
- API responses use `#[serde(untagged)]` enums to handle Canvas's inconsistent response formats

### File Ignore Patterns

- Uses `.canvasignore` file with gitignore syntax (via the `ignore` crate)
- Check if paths are ignored with `utils::ignored()` before creating folders or downloading files
- Ignore matcher is Arc-wrapped and shared across all tasks

### Path Handling

- Use `sanitize-filename` crate to sanitize file/folder names from Canvas
- Course codes with slashes are replaced with underscores for filesystem compatibility
- Always check and create folders with `utils::create_folder_if_not_exist_or_ignored()`

## Configuration

Canvas credentials are loaded from TOML files in this search order:

1. Path specified via `--config` option
2. `canvas-downloader.toml` in current directory
3. Platform-specific config directory: `~/.config/canvas-downloader/config.toml` (Linux/macOS) or `%APPDATA%\canvas-downloader\config.toml` (Windows)

Required config format:

```toml
canvas_url = "https://canvas.example.edu"
canvas_token = "your_access_token_here"
```

## Git Workflow

**IMPORTANT: Do not automatically commit changes.** Only create commits when explicitly asked by the user.

When creating commit messages:

- **Do not include Claude co-authorship** (no "Co-Authored-By: Claude" lines)
- **Base commit messages on actual file changes** from `git diff` and `git status`, not on Claude's memory of what was changed
- The user may have made additional modifications after Claude's changes, so always check the actual diff before writing the commit message
- The commit message should be concise and precise.

## Known Issues

- Panopto video downloads are buggy (see videos.rs)
- The project has no test suite
- Type inference errors with reqwest 0.13 update (see diagnostic warnings for main.rs and videos.rs)
