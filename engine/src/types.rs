use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

pub fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn identifier() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Root {
    pub id: String,
    pub path: String,
    pub name: String,
    pub policy: String,
    pub enabled: bool,
    pub exclusions: Vec<String>,
    pub created_at: String,
    pub last_scan: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub goal: String,
    pub approach: String,
    pub decisions: Vec<Decision>,
    pub summary_evidence: Vec<Evidence>,
    pub updated_at: String,
    #[serde(default)]
    pub summary_stale: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Decision {
    pub date: String,
    pub text: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Document {
    pub id: String,
    pub path: String,
    pub title: String,
    pub root_id: String,
    pub project_id: String,
    pub extension: String,
    pub version: String,
    pub size: u64,
    pub modified_ms: u64,
    pub updated_at: String,
    pub status: String,
    pub error: Option<String>,
    pub chunks: usize,
    #[serde(default)]
    pub ocr: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct Location {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_start: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_end: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cells: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SourceBlock {
    pub text: String,
    pub location: Location,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ParsedFile {
    pub blocks: Vec<SourceBlock>,
    #[serde(default)]
    pub ocr: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Chunk {
    pub id: String,
    pub document_id: String,
    pub ordinal: usize,
    pub text: String,
    pub location: Location,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Evidence {
    pub document_id: String,
    pub version: String,
    pub title: String,
    pub chunk_id: Option<String>,
    pub location: Option<Location>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Memory {
    pub id: String,
    pub content: String,
    pub category: String,
    pub status: String,
    pub project_id: Option<String>,
    pub evidence: Vec<Evidence>,
    pub origin: String,
    pub created_at: String,
    pub updated_at: String,
    pub history: Vec<Value>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    pub project_ids: Vec<String>,
    pub include_profile: bool,
    pub can_propose: bool,
    pub created_at: String,
    pub last_used: Option<String>,
    pub config_path: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Provider {
    pub base_url: String,
    pub model: String,
    pub protocol: String,
    pub has_key: bool,
    pub credential_id: Option<String>,
    #[serde(default)]
    pub endpoint: String,
}

impl Default for Provider {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            model: String::new(),
            protocol: "openai".into(),
            has_key: false,
            credential_id: None,
            endpoint: String::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub organization: String,
    pub chat: Provider,
    pub embedding: Provider,
    pub embedding_mode: String,
    pub rerank: Provider,
    pub rerank_enabled: bool,
    pub auto_summarize: bool,
    pub ocr_enabled: bool,
    pub onboarding_complete: bool,
    pub paused: bool,
    pub max_file_mb: u64,
    pub reconcile_seconds: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            organization: String::new(),
            chat: Provider::default(),
            embedding: Provider::default(),
            embedding_mode: "auto".into(),
            rerank: Provider::default(),
            rerank_enabled: false,
            auto_summarize: false,
            ocr_enabled: true,
            onboarding_complete: false,
            paused: false,
            max_file_mb: 100,
            reconcile_seconds: 300,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct RuntimeConfig {
    pub parser_runtime: PathBuf,
    pub parser_script: PathBuf,
    pub resources: PathBuf,
    pub web_root: PathBuf,
    pub mcp_executable: PathBuf,
    pub electron_runtime: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ServiceInfo {
    pub port: u16,
    pub pid: u32,
    pub token: String,
    pub version: String,
}

#[derive(Clone, Debug)]
pub enum Scope {
    Owner,
    Agent(Connection),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SearchHit {
    pub chunk_id: String,
    pub document_id: String,
    pub title: String,
    pub project_id: String,
    pub project_name: String,
    pub path: String,
    pub version: String,
    pub updated_at: String,
    pub text: String,
    pub location: Location,
    pub score: f64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Event {
    pub id: i64,
    pub created_at: String,
    pub kind: String,
    pub message: String,
    pub details: Value,
}
