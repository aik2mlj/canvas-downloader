# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.2] - 2026-08-20

### Added

- Canvas Quiz module items are saved as `.url` shortcuts, alongside external URL module items.

### Changed

- Module items under a `SubHeader` are now placed inside that subheader's folder instead of being flattened into the module folder. Files, pages, and `.url` shortcuts following a `SubHeader` are routed into a sibling section folder until the next `SubHeader`; ignored subheader folders also skip their contents.

### Fixed

- Course directory names now use the same Windows-safe sanitization on Linux, macOS, and Windows. Invalid characters, reserved names, and trailing dots or spaces are replaced with `_`.
- Canvas and term IDs are handled as 64-bit values, supporting institutions whose IDs exceed the 32-bit range.
- Canvas API requests now retry transient failures, including timeouts, rate limits, server errors, and slow responses; the request timeout is increased to 60 seconds.
- Pagination errors now return useful context instead of panicking, and pagination follows Canvas's `next` link directly.
- Empty Panopto playlists are reported and skipped instead of aborting the download.

## [0.4.1] - 2026-04-19

### Added

- `--no-submissions` option to skip downloading assignment submission files.
- Per-course sync messages in verbose mode.
- Completion messages summarizing what was synced for each content type.

### Changed

- Download confirmation output is cleaner: redundant file-count info removed before the confirmation prompt, and trailing sync messages are consolidated into a single summary line.

### Fixed

- Files whose Canvas `display_name` is returned in different Unicode normalization forms (e.g. NFC vs NFD for `ú`) no longer get re-downloaded every run. `filter_files` now NFC-normalizes the sanitized filename and probes the target directory for canonically equivalent entries so existing on-disk files are reused instead of producing visual duplicates.

## [0.4.0] - 2026-01-17

### Added

- Multi-target release workflow: pre-built binaries for `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-musl`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`, and `aarch64-pc-windows-msvc`.

### Changed

- Module file processing batches filter-and-queue into a single critical section, reducing lock contention during the query phase.
- `regex` replaced with `lazy-regex` to eliminate `unwrap()` on regex compilation.

### Fixed

- `Assignment.description` now handles `null` from the Canvas API without failing deserialization.
- `File.folder_id` now handles `null` from the Canvas API.
- Submission files are downloaded into a per-assignment subfolder instead of being flattened into the top-level assignments folder.
- The `raw/` folder is no longer created up front — it's only created once matching courses are confirmed.
- HTTPS support restored after an earlier attempt to trim `reqwest` features had inadvertently removed it.

## [0.3.5] - 2026-01-14

### Added

- Pre-commit config running `cargo fmt` on commit.

### Changed

- Bumped Rust edition to 2024.
- Release binary size reduced via `opt-level = "z"`, `lto = "thin"`, `codegen-units = 1`, `panic = "abort"`, and `strip`.
- Upgraded to `reqwest` 0.13.
- Raw JSON responses now mirror the download tree under a parallel `raw/` folder instead of being interleaved with content files.
- Content files (assignments, discussions, announcements, pages, etc.) are stored directly in their category folder rather than nested in per-item subfolders.
- Top-level JSON summary files (e.g. `assignments.json`, `announcements.json`) now live directly under `raw/<course_code>/` rather than in category subfolders.

### Fixed

- Announcements JSON summary is now written as `announcements.json` instead of being misnamed `discussions.json`.

## [0.3.4] - 2026-01-12

### Added

- Shell completion generation via a `completions` subcommand (bash / zsh / fish / powershell / elvish).
- Descriptive help messages for every CLI option.

### Changed

- Assignment and discussion HTML pages got a modern CSS refresh.
- Discussion comments show author display name (mapped from participants) and better-formatted timestamps.

## [0.3.3] - 2026-01-11

### Added

- Folder-level filtering in `.canvasignore`: ignore patterns are now consulted before creating a directory, not only before downloading a file.

### Changed

- Page folders and output filenames use the page's Canvas title rather than its URL slug.
- Logging migrated to the `tracing` ecosystem.

## [0.3.2] - 2026-01-11

### Added

- `-c` / `--course-names` option to filter courses by exact name or code (may be combined with `-t`).
- `.canvasignore` in the current working directory is auto-loaded if present.

### Changed

- **Breaking:** configuration format switched from JSON to TOML, with auto-discovery in platform-specific config directories (`~/.config/canvas-downloader/config.toml` on Linux/macOS, `%APPDATA%\canvas-downloader\config.toml` on Windows) in addition to `--config` and `./canvas-downloader.toml`.
- Course discovery output switched to a three-column table grouped by term ID.
- Course discovery lists all available courses across all terms, not only the current term.
- Redundant "downloading newer" info removed when `-n` is passed.

## [0.3.1] - 2026-01-09

### Added

- Syllabus download (HTML + JSON).
- HTML rendering for assignments.
- HTML rendering for discussions and announcements.

### Changed

- HTTP `User-Agent` header now derived from `CARGO_PKG_*` rather than a hardcoded string.

## [0.3.0] - 2026-01-07

### Added

- **New content types:** modules, pages, discussions, announcements, assignments (with submissions), user lists, and Panopto lecture videos (including subfolders).
- **Ignore file support** via `.canvasignore` with full `.gitignore` pattern semantics.
- **Dry-run mode** (`--dry-run`) to preview downloads without executing them.
- **Confirmation prompt** before starting downloads, with total file count and size.
- **Verbose mode** (`-v`) for informational messages; non-verbose output is quiet by default.
- File sizes displayed using 1024-based binary units (KiB, MiB, GiB).
- HTML link parsing: files linked from page/assignment/discussion HTML are discovered and queued.
- Retry with exponential backoff for Canvas API rate limiting (3 attempts on 403).
- `User-Agent` header on all HTTP clients.
- JSON responses pretty-printed when saved to disk.

### Changed

- `main.rs` refactored into a modular structure (`api`, `files`, `modules`, `pages`, `assignments`, `discussions`, `syllabus`, `users`, `videos`, `html`, `utils`).
- Course discovery fetches all available courses, not just courses in the current term.
- Folder creation is conditional — empty category folders are no longer created.
- Ignore-file informational output moved behind `-v`.

### Fixed

- Module files now go through the same filename sanitization as files-tree files.
- Files with special characters are no longer re-downloaded every run (path-handling fix distinct from the Unicode-normalization fix landing in Unreleased).
- JSON decode error when processing file links from pages.
- `.canvasignore` parent-directory pattern matching uses the correct base path.
- Canvas API parsing errors for inconsistent response shapes.
- Video download bug from an earlier dependency upgrade.

[Unreleased]: https://github.com/aik2mlj/canvas-downloader/compare/v0.4.2...HEAD
[0.4.2]: https://github.com/aik2mlj/canvas-downloader/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/aik2mlj/canvas-downloader/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/aik2mlj/canvas-downloader/compare/v0.3.5...v0.4.0
[0.3.5]: https://github.com/aik2mlj/canvas-downloader/compare/v0.3.4...v0.3.5
[0.3.4]: https://github.com/aik2mlj/canvas-downloader/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/aik2mlj/canvas-downloader/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/aik2mlj/canvas-downloader/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/aik2mlj/canvas-downloader/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/aik2mlj/canvas-downloader/releases/tag/v0.3.0
