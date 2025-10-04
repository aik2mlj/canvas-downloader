use std::collections::HashMap;
use std::path::PathBuf;
use anyhow::{Context, Result};
use serde_json::Value;
use crate::canvas::Course;

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
