use serde::Serialize;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileStatus {
    Ready,
    Inspecting,
    Queued,
    Processing,
    Verifying,
    Processed,
    Warning,
    Failed,
    Unsupported,
    Cancelled,
}

#[derive(Serialize, Clone, Debug)]
pub struct PublicSelectedFile {
    pub id: String,
    pub display_name: String,
    pub extension: Option<String>,
    pub relative_path: Option<String>,
    pub size: u64,
    pub status: FileStatus,
}
