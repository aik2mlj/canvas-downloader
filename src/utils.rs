use crate::canvas::Course;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

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

pub fn create_folder_if_not_exist(folder_path: &PathBuf) -> Result<()> {
    if !folder_path.exists() {
        std::fs::create_dir(&folder_path).with_context(|| {
            format!(
                "Failed to create directory: {}",
                folder_path.to_string_lossy()
            )
        })?;
    }
    Ok(())
}

pub fn prettify_json(json_str: &str) -> Result<String> {
    let value: Value = serde_json::from_str(json_str)?;
    Ok(serde_json::to_string_pretty(&value)?)
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
