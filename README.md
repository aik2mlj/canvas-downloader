# Canvas Downloader

A command-line tool to download and organize all your Canvas course materials—files, syllabi, pages, modules, assignments, discussions, and announcements—into a clean local folder structure. Made in Rust⚡.

## Quick Start

### 1. Create Configuration File

Create a config file in TOML format with your Canvas URL and access token:

```toml
canvas_url = "https://canvas.stanford.edu"
canvas_token = "12345~jfkdlejoiferjiofu"
```

**How to get your token:**

- Log in to Canvas → Account → Settings → **New Access Token**

**Config file locations (searched in order):**

1. Custom path via `--config` option
1. `canvas-downloader.toml` in current directory
1. `config.toml` in platform-specific config directory:
   - Linux: `~/.config/canvas-downloader/config.toml`
   - macOS: `~/.config/canvas-downloader/config.toml` or `~/Library/Application Support/canvas-downloader/config.toml`
   - Windows: `%APPDATA%\canvas-downloader\config.toml`

### 2. Discover Your Courses

Run the tool to see which courses are available:

```shell
$ canvas-downloader
Please provide either Term ID(s) via -t or course name(s)/code(s) via -c
Term ID    | Course Code | Course Name
-----------------------------------------------------------
115        | CS1101S     | Programming Methodology
           | CS1231S     | Discrete Structures
-----------------------------------------------------------
120        | CS2040S     | Data Structures and Algorithms
           | CS2030      | Programming Methodology II
-----------------------------------------------------------
125        | CS3230      | Design and Analysis of Algorithms
```

### 3. Download Your Courses

You can download courses by term ID or by course name/code:

**Download by term (all courses in specific terms):**

```shell
$ canvas-downloader -t 115 120
```

**Download by course name or code (specific courses only):**

```shell
$ canvas-downloader -c CS1101S "Introduction to Data Structures"
```

**Combine both (courses matching both criteria):**

```shell
$ canvas-downloader -t 115 -c CS1101S
```

The tool will show you all files to be downloaded with their sizes, then ask for confirmation before proceeding. Downloads are organized by course, preserving Canvas's folder structure.

> **Note:** Course name matching is exact match - use the exact course code (e.g., "CS1101S") or the exact course name as shown in the discovery step.

## What Gets Downloaded

- [x] Files
- [x] Modules
- [x] Syllabi (in HTML and JSON)
- [x] Assignments (in HTML and JSON)
- [x] Discussions and announcements (in HTML and JSON)
- [x] Pages (in HTML and JSON)
- [x] User information (in JSON)
- [ ] Panopto lecture videos (seems still buggy)

## Common Workflows

### Filter What You Download

Create a `.canvasignore` file in your current directory to skip certain files using `.gitignore` syntax:

```shell
# Ignore all videos
*.mp4
*.mov

# Ignore specific courses
/CS1101S/

# Ignore lecture recordings folder
lecture-recordings/
```

The tool automatically loads `.canvasignore` from the current directory if it exists. You can also specify a custom ignore file with `-i`:

```shell
$ canvas-downloader -t 115 -i custom-ignore.txt
```

See `.canvasignore.example` for more patterns.

### Keep Your Files Updated

Use `-n` to overwrite local files with newer versions from Canvas:

```shell
$ canvas-downloader -t 115 -n
```

By default, existing local files won't be overwritten even if Canvas has newer versions.

### Choose Download Location

Specify a custom folder with `-d`:

```shell
$ canvas-downloader -t 115 -d ~/Canvas
```

### See More Details

Use `-v` for verbose output showing rate limiting, access warnings, and what's being skipped:

```shell
$ canvas-downloader -t 115 -v
```

## All Options

```
Usage: canvas-downloader [OPTIONS]

Options:
      --config <FILE>                Path to config file (TOML format)
  -d, --destination-folder <FOLDER>  Download location [default: .]
  -n, --download-newer               Overwrite local files with newer Canvas versions
  -t, --term-ids <ID>...             Term IDs to download
  -c, --course-names <NAME>...       Course names or codes to download (exact match)
  -i, --ignore-file <FILE>           Ignore patterns file
      --dry-run                      Preview downloads without executing
  -v, --verbose                      Show detailed progress
  -h, --help                         Print help
  -V, --version                      Print version
```

## MacOS Setup

If you download the executable from Releases, the following commands may be needed because the binary isn't signed with an Apple developer account:

```shell
# Remove quarantine attribute
$ xattr -d com.apple.quarantine canvas-downloader

# Make executable
$ chmod +x canvas-downloader
```

Also see [Apple's official doc](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac?utm_source=chatgpt.com) on this.
