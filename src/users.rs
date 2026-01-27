use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use anyhow::{Context, Result};

use crate::api::get_pages;
use crate::canvas::ProcessOptions;
use crate::utils::{get_raw_json_path, prettify_json};

pub async fn process_users(
    (url, parent_path): (String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    if !options.save_json {
        return Ok(());
    }

    let users_url = format!(
        "{}users?include_inactive=true&include[]=avatar_url&include[]=enrollments&include[]=email&include[]=observed_users&include[]=can_be_removed&include[]=custom_links",
        url
    );
    let pages = get_pages(users_url, &options).await?;

    if let Some(users_path) = get_raw_json_path(
        &parent_path,
        "users.json",
        &options.base_path,
        options.save_json,
    )? {
        let users_path_str = users_path.to_string_lossy();
        let mut users_file = std::fs::File::create(users_path.clone())
            .with_context(|| format!("Unable to create file for {:?}", users_path_str))?;

        for pg in pages {
            let page_body = pg.text().await?;

            let pretty_json = prettify_json(&page_body).unwrap_or(page_body.clone());
            users_file
                .write_all(pretty_json.as_bytes())
                .with_context(|| format!("Unable to write to file for {:?}", users_path_str))?;
        }

        options.n_users.fetch_add(1, Ordering::Relaxed);
    }

    Ok(())
}
