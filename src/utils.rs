use crate::canvas::{Course, ProcessOptions};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn print_all_courses_by_term(courses: &[Course]) {
    let mut grouped_courses: HashMap<u32, Vec<(&str, &str)>> = HashMap::new();

    for course in courses.iter() {
        let course_id: u32 = course.enrollment_term_id;
        grouped_courses
            .entry(course_id)
            .or_insert_with(Vec::new)
            .push((&course.course_code, &course.name));
    }

    // Calculate column widths
    let max_code_width = courses
        .iter()
        .map(|c| c.course_code.len())
        .max()
        .unwrap_or(12)
        .max(12); // At least 12 for "Course Code" header

    // Print header
    println!(
        "{:<10} | {:<width$} | {}",
        "Term ID",
        "Course Code",
        "Course Name",
        width = max_code_width
    );
    println!("{}", "-".repeat(10 + 3 + max_code_width + 3 + 40));

    // Sort by term ID for consistent output
    let mut term_ids: Vec<_> = grouped_courses.keys().collect();
    term_ids.sort();

    for (term_idx, term_id) in term_ids.iter().enumerate() {
        let courses_in_term = &grouped_courses[term_id];
        for (i, (code, name)) in courses_in_term.iter().enumerate() {
            if i == 0 {
                println!(
                    "{:<10} | {:<width$} | {}",
                    term_id,
                    code,
                    name,
                    width = max_code_width
                );
            } else {
                println!(
                    "{:<10} | {:<width$} | {}",
                    "",
                    code,
                    name,
                    width = max_code_width
                );
            }
        }

        // Add separator line between terms (but not after the last one)
        if term_idx < term_ids.len() - 1 {
            println!("{}", "-".repeat(10 + 3 + max_code_width + 3 + 40));
        }
    }
}

pub fn ignored(
    filepath: &Path,
    is_dir: bool,
    base_path: &Path,
    ignore_matcher: Option<&ignore::gitignore::Gitignore>,
) -> bool {
    let matcher = match ignore_matcher {
        Some(m) => m,
        None => return false,
    };

    let relative_path = filepath.strip_prefix(base_path).unwrap_or(filepath);
    let ignored = matcher
        .matched_path_or_any_parents(relative_path, is_dir)
        .is_ignore();
    if ignored {
        tracing::debug!("Ignoring path: {}", filepath.display());
    }
    ignored
}

fn create_folder_if_not_exist(folder_path: &Path) -> Result<bool> {
    if !folder_path.exists() {
        std::fs::create_dir_all(folder_path).with_context(|| {
            format!(
                "Failed to create directory: {}",
                folder_path.to_string_lossy()
            )
        })?;
    }
    Ok(true)
}

// return Ok(true) if folder created or already exists, Ok(false) if ignored
pub fn create_folder_if_not_exist_or_ignored(
    folder_path: &Path,
    options: Arc<ProcessOptions>,
) -> Result<bool> {
    if ignored(
        folder_path,
        true,
        &options.base_path,
        options.ignore_matcher.as_deref(),
    ) {
        return Ok(false);
    }

    create_folder_if_not_exist(folder_path)
}

pub fn prettify_json(json_str: &str) -> Result<String> {
    let value: Value = serde_json::from_str(json_str)?;
    Ok(serde_json::to_string_pretty(&value)?)
}

/// Get the path for a raw JSON file in a parallel "raw" folder structure
/// Returns None if save_json is false
///
/// Example: if current_path is "/downloads/course1/assignments/Assignment 1"
/// and base_download_path is "/downloads", the raw path will be
/// "/downloads/raw/course1/assignments/Assignment 1/{filename}"
pub fn get_raw_json_path(
    current_path: &Path,
    filename: &str,
    base_path: &Path,
    save_json: bool,
) -> Result<Option<PathBuf>> {
    if !save_json {
        return Ok(None);
    }

    // Calculate relative path from base to current location
    let relative_path = current_path.strip_prefix(base_path).unwrap_or(current_path);

    // Create the mirrored structure in parallel "raw" folder
    let raw_path = base_path.join("raw").join(relative_path);

    create_folder_if_not_exist(&raw_path)?;
    Ok(Some(raw_path.join(filename)))
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];

    if bytes == 0 {
        return "0 B".to_string();
    }

    let bytes_f64 = bytes as f64;
    // For 1024-based: exponent = floor(log2(bytes) / 10) = floor(log2(bytes) / log2(1024))
    let exponent = (bytes_f64.log2() / 10.0).floor() as usize;
    let exponent = exponent.min(UNITS.len() - 1);

    let size = bytes_f64 / 1024_f64.powi(exponent as i32);
    let unit = UNITS[exponent];

    if size >= 100.0 {
        format!("{:.0} {}", size, unit)
    } else if size >= 10.0 {
        format!("{:.1} {}", size, unit)
    } else {
        format!("{:.2} {}", size, unit)
    }
}
