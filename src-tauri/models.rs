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
    /// Which backend generates songs: "suno" (default) or "acestep".
    #[serde(default = "default_music_engine")]
    pub music_engine: String,
    /// Engine to retry with when `music_engine` fails — "none" (default), "suno", "acestep" or
    /// "heartmula". The music integrations ride on unofficial sessions and free notebook servers
    /// that go away without warning; a fallback turns a whole failed batch into one extra attempt.
    #[serde(default = "default_none")]
    pub music_engine_fallback: String,
    /// Base URL of a running ACE-Step REST server (e.g. a Kaggle/Colab `acestep-api --share`
    /// public URL, or http://localhost:8001). Used only when `music_engine == "acestep"`.
    #[serde(default)]
    pub acestep_api_url: String,
    /// Optional ACE-Step API key (sent as `Authorization: Bearer …`). Empty if the server has none.
    #[serde(default)]
    pub acestep_api_key: String,
    /// Target song length in seconds for ACE-Step / HeartMuLa generations (10–600). Defaults to 4 min.
    #[serde(default = "default_acestep_duration")]
    pub acestep_duration: f64,
    /// Base URL of a running HeartMuLa server (Kaggle/Colab share URL or http://localhost:8003).
    #[serde(default)]
    pub heartmula_api_url: String,
    #[serde(default)]
    pub heartmula_api_key: String,
    #[serde(default)]
    pub mj_profile_dir: String,
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
    /// Which LLM backend generates lyrics/assist text. Free: "openrouter" (default), "gemini".
    /// Paid: "anthropic" (Claude), "openai" (ChatGPT), or a billed Gemini key on "gemini".
    #[serde(default = "default_ai_provider")]
    pub ai_provider: String,
    /// Claude, paid per token (console.anthropic.com/settings/keys).
    #[serde(default)]
    pub anthropic_api_key: String,
    #[serde(default = "default_anthropic_model")]
    pub anthropic_model: String,
    /// ChatGPT via the OpenAI API — separate from a ChatGPT subscription (platform.openai.com/api-keys).
    #[serde(default)]
    pub openai_api_key: String,
    #[serde(default = "default_openai_model")]
    pub openai_model: String,
    /// Printify Personal Access Token (My Profile → Connections). Its API is real and documented,
    /// unlike every distributor and ebook store in this app.
    #[serde(default)]
    pub printify_api_token: String,
    #[serde(default)]
    pub printify_shop_id: String,
    /// Push new products to the connected storefront. Off by default: a listing is public and priced.
    #[serde(default)]
    pub printify_auto_publish: bool,
    /// Apply today's wording and art to the picked products on a schedule.
    #[serde(default)]
    pub printify_daily_run: bool,
    #[serde(default)]
    pub openrouter_api_key: String,
    #[serde(default)]
    pub openrouter_email: String,
    #[serde(default = "default_model")]
    pub openrouter_model: String,
    /// Google Gemini free-tier API key (aistudio.google.com/apikey). Used when ai_provider == "gemini".
    #[serde(default)]
    pub gemini_api_key: String,
    #[serde(default = "default_gemini_model")]
    pub gemini_model: String,
    #[serde(default)]
    pub mj_proxy_url: String,
    /// Which backend generates images: "midjourney" (default), "flux", or "comfyui".
    #[serde(default = "default_image_engine")]
    pub image_engine: String,
    /// Base URL of a running FLUX/SD image server (e.g. a Kaggle/Colab share URL or http://localhost:8002).
    #[serde(default)]
    pub flux_api_url: String,
    /// Optional API key for the FLUX image server (sent as `Authorization: Bearer …`).
    #[serde(default)]
    pub flux_api_key: String,
    /// Base URL of a running ComfyUI server (Kaggle/Colab share URL or http://localhost:8188).
    #[serde(default)]
    pub comfyui_api_url: String,
    #[serde(default)]
    pub comfyui_api_key: String,

    // ── The paid image APIs ─────────────────────────────────────────────────
    // Key, model and base URL for each of fal.ai, Leonardo, Ideogram and Recraft. The last two are
    // settings rather than constants because provider endpoints move — this app has already lived
    // through studio-api.suno.ai becoming studio-api.prod.suno.com — and an empty value means "use
    // the documented default" (see image_apis.rs).
    #[serde(default)]
    pub fal_api_key: String,
    #[serde(default)]
    pub fal_model: String,
    #[serde(default)]
    pub fal_base_url: String,
    #[serde(default)]
    pub leonardo_api_key: String,
    #[serde(default)]
    pub leonardo_model: String,
    #[serde(default)]
    pub leonardo_base_url: String,
    #[serde(default)]
    pub ideogram_api_key: String,
    #[serde(default)]
    pub ideogram_model: String,
    #[serde(default)]
    pub ideogram_base_url: String,
    #[serde(default)]
    pub ideogram_rendering_speed: String,
    #[serde(default)]
    pub recraft_api_key: String,
    #[serde(default)]
    pub recraft_model: String,
    #[serde(default)]
    pub recraft_base_url: String,
    #[serde(default)]
    pub recraft_style: String,
    /// The shape paid engines are asked for, e.g. "16:9". One vocabulary in, four dialects out.
    #[serde(default)]
    pub image_aspect: String,
    /// "Things to avoid", sent only to engines that actually honour one.
    #[serde(default)]
    pub image_negative_prompt: String,
    /// Suno and Midjourney reach an account the user holds, so they are hidden until asked for.
    #[serde(default)]
    pub show_risky_engines: bool,
    /// SDXL checkpoint filename as it appears in ComfyUI's models/checkpoints folder.
    #[serde(default = "default_comfy_ckpt")]
    pub comfyui_ckpt: String,
    /// Active style preset id (photoreal, comic, graphic_novel, …) — prepends style text + negative.
    #[serde(default = "default_comfy_style")]
    pub comfyui_style: String,
    #[serde(default = "default_comfy_steps")]
    pub comfyui_steps: i64,
    #[serde(default = "default_comfy_cfg")]
    pub comfyui_cfg: f64,
    #[serde(default = "default_comfy_size")]
    pub comfyui_width: i64,
    #[serde(default = "default_comfy_size")]
    pub comfyui_height: i64,
    /// Extra negative-prompt text appended to the style preset's negative.
    #[serde(default)]
    pub comfyui_negative: String,
    /// Style-nuance text prepended to every prompt (after the style preset prefix). Per-channel
    /// sticky styles typically override this to give each channel its own look.
    #[serde(default)]
    pub comfyui_prompt_prefix: String,
    /// IP-Adapter reference strength for character-consistent generation (0.0–1.0).
    #[serde(default = "default_comfy_ipweight")]
    pub comfyui_ip_weight: f64,
    /// "auto" (use bundled photoreal/character templates) or "custom" (use comfyui_custom_workflow).
    #[serde(default = "default_comfy_wf_mode")]
    pub comfyui_workflow_mode: String,
    /// A full ComfyUI API-format workflow JSON (with __PROMPT__/__NEGATIVE__/__SEED__/__WIDTH__/
    /// __HEIGHT__/__STEPS__/__CFG__/__CKPT__/__REF_IMAGE__ tokens). Export it from the ComfyUI web UI
    /// (Save API Format) — this is how animation (AnimateDiff) and other advanced graphs are driven.
    #[serde(default)]
    pub comfyui_custom_workflow: String,
    /// Caps how many jobs (Suno/Midjourney/FFmpeg/YouTube upload) run concurrently. Read once at
    /// startup into `AppState::job_semaphore` — changing it takes effect on the next app launch.
    #[serde(default = "default_max_concurrent_jobs")]
    pub max_concurrent_jobs: i32,
    // ── Video composition ──────────────────────────────────────────
    #[serde(default = "default_video_width")]
    pub video_width: i64,
    #[serde(default = "default_video_height")]
    pub video_height: i64,
    /// Optional looping animation overlay (URL or local path) composited over the whole video —
    /// e.g. light leaks, particles, film grain. Empty = no overlay.
    #[serde(default)]
    pub video_overlay_url: String,
    /// When no global overlay URL is set, automatically composite the song's own companion
    /// overlay (generated in Overlay Studio from the song's audio) over its video.
    #[serde(default = "yes_bool")]
    pub overlay_auto_use: bool,
    /// ffmpeg visualizer style for generated companion overlays:
    /// cqt | spectrum | waves | vectorscope | freqs
    #[serde(default = "default_overlay_style")]
    pub overlay_style: String,
    /// "ffmpeg" (CPU-cheap audio-reactive filters, default) or "projectm" (opt-in: OpenGL
    /// Milkdrop presets rendered by an external projectM command — see projectm_path).
    #[serde(default = "default_overlay_engine")]
    pub overlay_engine: String,
    /// Command that renders a projectM visualization offline. May contain {audio} and {out}
    /// placeholders; otherwise the audio path and output path are appended as arguments.
    /// Empty = projectM tier disabled (ffmpeg styles are used).
    #[serde(default)]
    pub projectm_path: String,
    /// How the overlay blends: "screen" (additive glow, great for light leaks) or "overlay" (alpha).
    #[serde(default = "default_overlay_mode")]
    pub video_overlay_mode: String,
    /// Overlay strength 0.0–1.0.
    #[serde(default = "default_overlay_opacity")]
    pub video_overlay_opacity: f64,
    /// Crossfade between scenes in the composed video.
    #[serde(default)]
    pub video_transitions_enabled: bool,
    /// Default xfade transition when a section doesn't specify its own.
    #[serde(default = "default_transition")]
    pub video_transition: String,
    /// Crossfade length in seconds.
    #[serde(default = "default_transition_dur")]
    pub video_transition_dur: f64,
}

// The default is an engine with nothing to breach and no account to lose. Suno and Midjourney are
// hidden behind `show_risky_engines` (see lib/engineCapabilities.js) precisely because reaching them
// means driving the user's own logged-in session — so neither may be what a fresh install picks.
fn default_music_engine() -> String { "heartmula".into() }
fn default_none() -> String { "none".into() }
fn default_acestep_duration() -> f64 { 240.0 }
fn default_ffmpeg() -> String  { "ffmpeg".into() }
fn default_ffprobe() -> String { "ffprobe".into() }
fn default_theme() -> String   { "obsidian".into() }
fn default_model() -> String   { "qwen/qwen3-next-80b-a3b-instruct:free".into() }
fn default_ai_provider() -> String { "openrouter".into() }
// Paid-provider defaults. Both pickers list what the key can actually reach (`list_ai_models`), so
// these are only the starting point for a key that has just been pasted in.
fn default_anthropic_model() -> String { "claude-opus-5".into() }
fn default_openai_model() -> String { "gpt-5.1".into() }
fn default_gemini_model() -> String { "gemini-3.5-flash".into() }
fn default_image_engine() -> String { "flux".into() }
fn default_comfy_ckpt() -> String { "sd_xl_base_1.0.safetensors".into() }
fn default_comfy_style() -> String { "photoreal".into() }
fn default_comfy_steps() -> i64 { 30 }
fn default_comfy_cfg() -> f64 { 6.5 }
fn default_comfy_size() -> i64 { 1024 }
fn default_comfy_ipweight() -> f64 { 0.7 }
fn default_comfy_wf_mode() -> String { "auto".into() }
fn default_max_concurrent_jobs() -> i32 { 2 }
fn default_video_width() -> i64 { 1280 }
fn default_video_height() -> i64 { 720 }
fn default_overlay_mode() -> String { "screen".into() }
fn default_overlay_opacity() -> f64 { 0.35 }
fn yes_bool() -> bool { true }
fn default_overlay_style() -> String { "cqt".into() }
fn default_overlay_engine() -> String { "ffmpeg".into() }
fn default_transition() -> String { "fade".into() }
fn default_transition_dur() -> f64 { 0.7 }

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
    /// How far a scheduled run may carry on by itself: "lyrics" (default — generate and queue music,
    /// stop), "video" (through to a finished video, nothing public), "publish" (upload too).
    ///
    /// Absent means "lyrics". Escalation is never implicit: a schedule that started publishing because
    /// a default changed would be the worst kind of surprise.
    #[serde(default)]
    pub schedule_autonomy: Option<String>,
    #[serde(default = "yes")]
    pub multi_language: bool,
    #[serde(default = "yes")]
    pub multi_style: bool,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default)]
    pub local_path: Option<String>,
    /// Created once, at project creation, under ~/Documents — where JSON exports and other
    /// project detail files land automatically (see commands::projects::save_project_file).
    /// Distinct from `local_path`, which only exists once/if the user runs "Save to Local
    /// Disk" and is specifically a git repository checkout, not a plain folder.
    #[serde(default)]
    pub project_folder: Option<String>,
    #[serde(default)]
    pub current_branch: Option<String>,
    #[serde(default)]
    pub git_status: Option<String>,
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
    /// The destination channel for this song's eventual video — set (auto-suggested by
    /// language match, overridable) when the AI generation pipeline starts, and reused at
    /// Upload time so the two stages agree on where a song's assets/video are headed.
    #[serde(default)]
    pub channel_id: Option<String>,
    /// Absolute path to the mp3 the generation pipeline downloaded to disk (under the
    /// project's `auto/<channel-slug>/` folder — see commands::songs::save_generated_asset),
    /// distinct from `audio_url` which is the remote CDN url used for immediate preview.
    #[serde(default)]
    pub local_audio_path: Option<String>,
    /// Same as `local_audio_path`, for Suno's second clip variant (see `audio_url_alt`).
    #[serde(default)]
    pub local_audio_path_alt: Option<String>,
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
    /// queued | running | done | failed | cancelled
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
    /// Coarse pipeline stage for the UI's staged progress view. For music jobs:
    /// preparing | submitting | queued | generating | fetching | done | failed.
    /// Written by `set_stage` so the frontend never has to regex-scrape `logs`.
    #[serde(default)]
    pub stage: Option<String>,
    /// Human-readable one-liner describing what `stage` is currently doing.
    #[serde(default)]
    pub stage_message: Option<String>,
    /// Unix seconds when the current stage started — lets the UI show a live elapsed timer
    /// and an ETA without guessing from `updated_at`.
    #[serde(default)]
    pub stage_started_at: Option<i64>,
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

// ────────────────────────────────────────────────────────────────
// Character
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    #[serde(default = "default_uuid")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub song_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub image_prompt: String,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub image_variants: Vec<String>,
    #[serde(default)]
    pub selected_variant: i32,
    /// Short, stable visual descriptors (e.g. "silver beard", "blue traveling cloak", "golden
    /// halo") that get prepended to every Midjourney prompt for this character — initial
    /// generation and every "Vary" — so regenerated portraits stay visually consistent instead
    /// of drifting each time the underlying `image_prompt`/`description` free text is re-sent.
    #[serde(default)]
    pub appearance_tags: Vec<String>,
    #[serde(default = "now_iso")]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCreate {
    pub name: String,
    #[serde(default)]
    pub song_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub image_prompt: Option<String>,
    #[serde(default)]
    pub appearance_tags: Option<Vec<String>>,
}

fn default_mj_params()     -> String { "--ar 16:9 --v 8.1".into() }
fn default_title_pattern() -> String { "{artist} - {book} {chapter} ({styles})".into() }
fn default_artist()        -> String { "Joehannes Lightkid".into() }
