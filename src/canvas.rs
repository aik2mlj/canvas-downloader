use std::sync::atomic::AtomicUsize;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Clone, Deserialize, Serialize)]
pub struct Credentials {
    pub canvas_url: String,
    pub canvas_token: String,
}

#[derive(Deserialize)]
pub struct Course {
    pub id: u32,
    pub name: String,
    pub course_code: String,
    pub enrollment_term_id: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct User {
    pub id: u32,
    pub name: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum FolderResult {
    Err { status: String },
    Ok(Vec<Folder>),
}

#[derive(Deserialize)]
pub struct Folder {
    pub id: u32,
    pub name: String,
    pub folders_url: String,
    pub files_url: String,
    pub for_submissions: bool,
    pub can_upload: bool,
    pub parent_folder_id: Option<u32>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum FileResult {
    Err { status: String },
    Ok(Vec<File>),
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum PageResult {
    Err { status: String },
    Ok(Vec<Page>),
    // Handle direct array response without wrapper
    Direct(Vec<Page>),
    // Handle empty response or null
    Empty(Option<serde_json::Value>),
}

#[derive(Clone, Debug, Deserialize)]
pub struct Page {
    pub page_id: u32,
    pub url: String,
    pub title: String,
    pub updated_at: String,
    pub locked_for_user: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PageBody {
    pub page_id: u32,
    pub url: String,
    pub title: String,
    pub body: String,
    pub updated_at: String,
    pub locked_for_user: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum AssignmentResult {
    Err { status: String },
    Ok(Vec<Assignment>),
    // Handle direct array response without wrapper
    Direct(Vec<Assignment>),
    // Handle empty response or any other format
    Empty(Option<serde_json::Value>),
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Assignment {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub created_at: Option<String>,
    pub due_at: Option<String>,
    pub submission_types: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Submission {
    pub id: Option<u32>,
    pub body: Option<String>,
    #[serde(default)]
    pub attachments: Vec<File>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum DiscussionResult {
    Err { status: String },
    Ok(Vec<Discussion>),
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Discussion {
    pub id: u32,
    pub title: String,
    pub message: String,
    pub posted_at: Option<String>,
    pub author: Option<DiscussionAuthor>,
    pub attachments: Vec<File>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscussionAuthor {
    pub id: Option<u32>,
    pub display_name: Option<String>,
    pub avatar_image_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DiscussionView {
    pub unread_entries: Vec<u32>,
    pub view: Vec<Comments>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Comments {
    pub id: u32,
    pub user_id: Option<u32>,
    pub user_name: Option<String>,
    pub message: Option<String>,
    pub created_at: Option<String>,
    pub attachment: Option<File>,
    pub attachments: Option<Vec<File>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct File {
    pub id: u32,
    pub folder_id: u32,
    pub display_name: String,
    pub size: u64,
    pub url: String,
    pub updated_at: String,
    pub locked_for_user: bool,
    #[serde(skip)]
    pub filepath: std::path::PathBuf,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Session {
    pub session_url: String,
    pub requires_terms_acceptance: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct PanoptoSessionInfo {
    pub TotalNumber: u32,
    pub Results: Vec<PanoptoResult>,
    pub Subfolders: Vec<PanoptoSubfolder>,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct PanoptoResult {
    pub DeliveryID: String,
    pub FolderID: String,
    pub SessionID: String,
    pub SessionName: String,
    pub StartTime: String,
    pub IosVideoUrl: String,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct PanoptoSubfolder {
    pub ID: String,
    pub Name: String,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct PanoptoDeliveryInfo {
    pub SessionId: String,
    pub ViewerFileId: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum ModuleResult {
    Err { status: String },
    Ok(Vec<Module>),
    // Handle direct array response without wrapper
    Direct(Vec<Module>),
    // Handle empty response or null
    Empty(Option<serde_json::Value>),
}

#[derive(Clone, Debug, Deserialize)]
pub struct Module {
    pub id: u32,
    pub name: String,
    pub position: u32,
    pub unlock_at: Option<String>,
    pub require_sequential_progress: Option<bool>,
    pub publish_final_grade: Option<bool>,
    pub prerequisite_module_ids: Vec<u32>,
    pub state: Option<String>,
    pub completed_at: Option<String>,
    pub items_count: u32,
    pub items_url: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum ModuleItemResult {
    Err { status: String },
    Ok(Vec<ModuleItem>),
    // Handle direct array response without wrapper
    Direct(Vec<ModuleItem>),
    // Handle empty response or null
    Empty(Option<serde_json::Value>),
}

#[derive(Clone, Debug, Deserialize)]
pub struct ModuleItem {
    pub id: u32,
    pub title: String,
    #[serde(rename = "type")]
    pub item_type: String, // "File", "Page", "Discussion", "Assignment", "Quiz", "SubHeader", "ExternalUrl", "ExternalTool"
    pub content_id: Option<u32>,
    pub html_url: Option<String>,
    pub url: Option<String>,
    pub page_url: Option<String>,
    pub external_url: Option<String>,
    pub position: u32,
    pub indent: u32,
    pub completion_requirement: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Syllabus {
    pub id: u32,
    pub name: String,
    pub course_code: String,
    pub syllabus_body: Option<String>,
}

pub struct ProcessOptions {
    pub canvas_token: String,
    pub canvas_url: String,
    pub client: reqwest::Client,
    pub user: User,
    // Process
    pub download_newer: bool,
    pub files_to_download: Mutex<Vec<File>>,
    pub ignore_matcher: Option<std::sync::Arc<ignore::gitignore::Gitignore>>,
    pub ignore_base_path: std::path::PathBuf,
    pub dry_run: bool,
    pub verbose: bool,
    // Download
    pub progress_bars: indicatif::MultiProgress,
    pub progress_style: indicatif::ProgressStyle,
    // Synchronization
    pub n_active_requests: AtomicUsize, // main() waits for this to be 0
    pub sem_requests: tokio::sync::Semaphore, // Limit #active requests
    pub notify_main: tokio::sync::Notify,
}
