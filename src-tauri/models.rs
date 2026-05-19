use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn default_uuid() -> String {
    Uuid::new_v4().to_string()
}

// ────────────────────────────────────────────────────────────────
// Settings
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub suno_cookie: String,
    #[serde(default)]
    pub mj_cookie: String,
    #[serde(default)]
    pub mj_discord_token: String,
    #[serde(default)]
    pub google_client_id: String,
    #[serde(default)]
    pub google_client_secret: String,
    #[serde(default)]
    pub google_redirect_uri: String,
    #[serde(default = "default_ffmpeg")]
    pub ffmpeg_path: String,
    #[serde(default = "default_ffprobe")]
    pub ffprobe_path: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub qwen_endpoint: String,
    #[serde(default)]
    pub openrouter_api_key: String,
    #[serde(default = "default_model")]
    pub openrouter_model: String,
    #[serde(default)]
    pub mj_proxy_url: String,
}

fn default_ffmpeg() -> String  { "ffmpeg".into() }
fn default_ffprobe() -> String { "ffprobe".into() }
fn default_theme() -> String   { "obsidian".into() }
fn default_model() -> String   { "qwen/qwen-2.5-72b-instruct:free".into() }

// ────────────────────────────────────────────────────────────────
// Project
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    #[serde(default = "default_uuid")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default = "yes")]
    pub multi_language: bool,
    #[serde(default = "yes")]
    pub multi_style: bool,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default = "now_iso")]
    pub created_at: String,
}

fn yes() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCreate {
    pub name: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub schedule: Option<String>,
}

// ────────────────────────────────────────────────────────────────
// Song
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Song {
    #[serde(default = "default_uuid")]
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub language: String,
    pub styles: String,
    pub lyrics: String,
    #[serde(default)]
    pub annotations: String,
    #[serde(default)]
    pub image_styles: String,
    #[serde(default)]
    pub audio_url: Option<String>,
    #[serde(default)]
    pub video_url: Option<String>,
    #[serde(default)]
    pub duration: f64,
    /// draft | music_ready | analyzed | images_ready | video_ready | uploaded
    #[serde(default = "draft")]
    pub status: String,
    #[serde(default = "now_iso")]
    pub created_at: String,
}

fn draft() -> String { "draft".into() }

// ────────────────────────────────────────────────────────────────
// Section
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    #[serde(default = "default_uuid")]
    pub id: String,
    pub song_id: String,
    pub index: i32,
    pub start: f64,
    pub end: f64,
    pub line: String,
    pub image_prompt: String,
    pub mood: String,
    #[serde(default)]
    pub mood_prev: String,
    #[serde(default)]
    pub mood_next: String,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub image_variants: Vec<String>,
    #[serde(default)]
    pub is_video: bool,
    #[serde(default)]
    pub effects: Vec<String>,
}

// ────────────────────────────────────────────────────────────────
// Channel
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    #[serde(default = "default_uuid")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub youtube_channel_id: String,
    #[serde(default = "english")]
    pub language: String,
    #[serde(default)]
    pub styles: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub subscriber_count: i64,
    #[serde(default)]
    pub oauth_client_id: Option<String>,
}

fn english() -> String { "English".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelCreate {
    pub name: String,
    #[serde(default)]
    pub youtube_channel_id: String,
    #[serde(default = "english")]
    pub language: String,
    #[serde(default)]
    pub styles: String,
    #[serde(default)]
    pub region: String,
}

// ────────────────────────────────────────────────────────────────
// OAuthClient
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthClient {
    #[serde(default = "default_uuid")]
    pub id: String,
    pub label: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default = "now_iso")]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthClientCreate {
    pub label: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

// ────────────────────────────────────────────────────────────────
// Job
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    #[serde(default = "default_uuid")]
    pub id: String,
    /// music | analysis | image | video | upload
    pub kind: String,
    pub target_id: String,
    /// queued | running | done | failed
    #[serde(default = "queued")]
    pub status: String,
    #[serde(default)]
    pub progress: i32,
    #[serde(default)]
    pub logs: Vec<String>,
    #[serde(default)]
    pub attempts: i32,
    #[serde(default = "now_iso")]
    pub created_at: String,
    #[serde(default = "now_iso")]
    pub updated_at: String,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub result: serde_json::Value,
}

fn queued() -> String { "queued".into() }

// ────────────────────────────────────────────────────────────────
// Upload
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upload {
    #[serde(default = "default_uuid")]
    pub id: String,
    pub song_id: String,
    pub channel_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "music_cat")]
    pub category: String,
    #[serde(default = "private_str")]
    pub privacy: String,
    #[serde(default = "youtube_str")]
    pub format: String,
    #[serde(default = "pending_str")]
    pub status: String,
    #[serde(default)]
    pub youtube_video_id: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default = "now_iso")]
    pub created_at: String,
}

fn music_cat()    -> String { "Music".into() }
fn private_str()  -> String { "private".into() }
fn youtube_str()  -> String { "youtube".into() }
fn pending_str()  -> String { "pending".into() }

// ────────────────────────────────────────────────────────────────
// AI Compose request
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeRequest {
    pub chapter_text: String,
    #[serde(default)]
    pub sections: Vec<serde_json::Value>,
    #[serde(default)]
    pub targets: Vec<serde_json::Value>,
    #[serde(default)]
    pub themes: serde_json::Value,
    #[serde(default = "default_mj_params")]
    pub mj_params: String,
    #[serde(default)]
    pub style_keywords: Vec<String>,
    #[serde(default)]
    pub generate: serde_json::Value,
    #[serde(default = "default_title_pattern")]
    pub title_pattern: String,
    #[serde(default = "default_artist")]
    pub artist: String,
}

fn default_mj_params()     -> String { "--ar 16:9 --v 8.1".into() }
fn default_title_pattern() -> String { "{artist} - {book} {chapter} ({styles})".into() }
fn default_artist()        -> String { "Joehannes Lightkid".into() }
