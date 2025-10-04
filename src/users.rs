use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::api::get_pages;
use crate::canvas::ProcessOptions;
use crate::utils::prettify_json;

pub async fn process_users (
    (url, path): (String, PathBuf),
    options: Arc<ProcessOptions>,
) -> Result<()> {
    let users_url = format!("{}users?include_inactive=true&include[]=avatar_url&include[]=enrollments&include[]=email&include[]=observed_users&include[]=can_be_removed&include[]=custom_links", url);
    let pages = get_pages(users_url, &options).await?;

    let users_path = path.to_string_lossy();
    let mut users_file = std::fs::File::create(path.clone())
        .with_context(|| format!("Unable to create file for {:?}", users_path))?;

    for pg in pages {
        let page_body = pg.text().await?;

        let pretty_json = prettify_json(&page_body).unwrap_or(page_body.clone());
        users_file
            .write_all(pretty_json.as_bytes())
            .with_context(|| format!("Unable to write to file for {:?}", users_path))?;
    }

    Ok(())
}
