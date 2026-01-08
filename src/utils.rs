use crate::canvas::Course;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

pub fn print_all_courses_by_term(courses: &[Course]) {
    let mut grouped_courses: HashMap<u32, Vec<&str>> = HashMap::new();

    for course in courses.iter() {
        let course_id: u32 = course.enrollment_term_id;
        grouped_courses
            .entry(course_id)
            .or_insert_with(Vec::new)
            .push(&course.course_code);
    }
    println!("{: <10}| {:?}", "Term IDs", "Courses");
    for (key, value) in &grouped_courses {
        println!("{: <10}| {:?}", key, value);
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
