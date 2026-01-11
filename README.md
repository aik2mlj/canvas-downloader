# Canvas Downloader

A command-line tool to download and organize all your Canvas course materials—files, syllabi, pages, modules, assignments, discussions, and announcements—into a clean local folder structure. Made in Rust⚡.

## Quick Start

### 1. Get Your Canvas Access Token

Create a credential file (e.g., `cred.json`):

```json
{
  "canvasUrl": "https://canvas.stanford.edu",
  "canvasToken": "12345~jfkdlejoiferjiofu"
}
```

**How to get your token:**

- Log in to Canvas → Account → Settings → **New Access Token**

### 2. Discover Your Courses

Run the tool to see which courses are available:

```shell
$ canvas-downloader --credential-file cred.json
Please provide the Term ID(s) to download via -t
Term IDs  | Courses
115       | ["CS1101S", "CS1231S"]
120       | ["CS2040S", "CS2030"]
125       | ["CS3230"]
```

### 3. Download Your Courses

You can download courses by term ID or by course name:

**Download by term (all courses in specific terms):**
```shell
$ canvas-downloader --credential-file cred.json -t 115 120
```

**Download by course name or code (specific courses only):**
```shell
$ canvas-downloader --credential-file cred.json -C CS1101S "Introduction to Data Structures"
```

**Combine both (courses matching both criteria):**
```shell
$ canvas-downloader --credential-file cred.json -t 115 -C CS1101S
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

Create an ignore file (`.canvasignore`) to skip certain files using `.gitignore` syntax:

```shell
# Ignore all videos
*.mp4
*.mov

# Ignore specific courses
/CS1101S/

# Ignore lecture recordings folder
lecture-recordings/
```

Then use it with `-i`:

```shell
$ canvas-downloader -c cred.json -t 115 -i .canvasignore
```

See `.canvasignore.example` for more patterns.

### Download Specific Courses

Instead of downloading entire terms, you can download specific courses by exact name or code:

```shell
# Download a specific course by code
$ canvas-downloader -c cred.json -C CS2040S

# Download multiple specific courses
$ canvas-downloader -c cred.json -C CS2040S "Introduction to Algorithms"

# Download specific courses from a specific term
$ canvas-downloader -c cred.json -t 115 -C CS1101S
```

Course matching requires exact match with the course code or full course name as displayed in step 2.

### Keep Your Files Updated

Use `-n` to overwrite local files with newer versions from Canvas:

```shell
$ canvas-downloader -c cred.json -t 115 -n
```

By default, existing local files won't be overwritten even if Canvas has newer versions.

### Choose Download Location

Specify a custom folder with `-d`:

```shell
$ canvas-downloader -c cred.json -t 115 -d ~/Canvas
```

### See More Details

Use `-v` for verbose output showing rate limiting, access warnings, and what's being skipped:

```shell
$ canvas-downloader -c cred.json -t 115 -v
```

## All Options

```
Usage: canvas-downloader [OPTIONS] --credential-file <FILE>

Options:
  -c, --credential-file <FILE>       Path to credentials JSON file
  -d, --destination-folder <FOLDER>  Download location [default: .]
  -n, --download-newer               Overwrite local files with newer Canvas versions
  -t, --term-ids <ID>...             Term IDs to download
  -C, --course-names <NAME>...       Course names or codes to download (exact match)
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
