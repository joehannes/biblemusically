import { invoke } from "@tauri-apps/api/core";

const localComposeConfigKey = "studio:composer-config";
const hasLocalStorage = typeof window !== "undefined" && typeof window.localStorage !== "undefined";
const isTauri = typeof window !== "undefined" && (typeof window.__TAURI__ !== "undefined" || typeof window.__TAURI_IPC__ !== "undefined");

const loadComposeConfigFromStorage = () => {
  if (!hasLocalStorage) return {};
  try {
    const raw = window.localStorage.getItem(localComposeConfigKey);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
};

const saveComposeConfigToStorage = (cfg) => {
  if (!hasLocalStorage) return cfg;
  try {
    window.localStorage.setItem(localComposeConfigKey, JSON.stringify(cfg));
  } catch {
    // ignore
  }
  return cfg;
};

// Generic Tauri invoke wrapper with error handling
const invokeCommand = async (command, payload = null) => {
  if (!isTauri) {
    throw new Error("Tauri is not available in this environment");
  }

  try {
    if (payload) {
      return await invoke(command, payload);
    }
    return await invoke(command);
  } catch (error) {
    console.error(`[Tauri API Error] ${command}:`, error);
    throw error;
  }
};

export const api = {
  // ============ Settings ============
  getSettings: () => invokeCommand("get_settings"),
  saveSettings: (s) => invokeCommand("update_settings", { payload: s }),
  testSuno: () => invokeCommand("test_suno"),
  openSunoLogin: () => invokeCommand("open_suno_login"),
  captureSunoSession: () => invokeCommand("capture_suno_session"),
  testMj: () => invokeCommand("test_mj"),
  openMjLogin: () => invokeCommand("open_midjourney_login"),
  captureMjSession: () => invokeCommand("capture_midjourney_session"),
  generateMjNow: (prompt) => invokeCommand("generate_mj_now", { prompt }),
  ensureMjAutostart: () => invokeCommand("ensure_mj_autostart"),
  mjAutoLogin: (account, password, twofa) => invokeCommand("mj_auto_login", { login_account: account, login_password: password, login_2fa: twofa }),
  testFfmpeg: () => invokeCommand("test_ffmpeg"),
  getNodePath: () => invokeCommand("probe_node"),
  putDraft: async (key, payload) => {
    if (!hasLocalStorage) return null;
    try {
      window.localStorage.setItem(`draft:${key}`, JSON.stringify(payload));
      return payload;
    } catch {
      return null;
    }
  },
  getDraft: async (key) => {
    if (!hasLocalStorage) return null;
    try {
      const raw = window.localStorage.getItem(`draft:${key}`);
      return raw ? JSON.parse(raw) : null;
    } catch {
      return null;
    }
  },

  // ============ Projects ============
  listProjects: () => invokeCommand("list_projects"),
  createProject: (b) => invokeCommand("create_project", { body: b }),
  getProject: (id) => invokeCommand("get_project", { pid: id }),
  updateProject: (id, b) => invokeCommand("update_project", { pid: id, body: b }),
  deleteProject: (id) => invokeCommand("delete_project", { pid: id }),
  exportProject: (id) => invokeCommand("export_project", { pid: id }),
  importProject: (body, sourceDir) => invokeCommand("import_project", { body, source_dir: sourceDir }),
  importLyrics: (id, items) => invokeCommand("import_lyrics", { pid: id, body: { items } }),

  // ============ Songs ============
  listSongs: (id) => invokeCommand("list_songs", { pid: id }),
  getSong: (sid) => invokeCommand("get_song", { sid }),
  deleteSong: (sid) => invokeCommand("delete_song", { sid }),
  genMusic: (sid) => invokeCommand("generate_music", { sid }),
  analyze: (sid) => invokeCommand("analyze_song", { sid }),
  downloadAudio: (audioUrl, format, filename) => invokeCommand("download_and_convert_audio", { audioUrl, format, filename }),
  downloadAllAudio: (songs) => invokeCommand("download_all_songs", { songs }),
  selectSongVariant: (sid, variant) => invokeCommand("select_song_variant", { sid, variant }),

  // ============ Sections ============
  listSections: (sid) => invokeCommand("list_sections", { sid }),
  updateSection: (id, b) => invokeCommand("update_section", { secid: id, body: b }),
  genImage: (id) => invokeCommand("generate_section_image", { secid: id }),
  batchImages: (sid) => invokeCommand("batch_generate_images", { sid }),
  bulkGenerateAll: () => invokeCommand("bulk_generate_all_songs"),

  // ============ Video / Effects ============
  effectsPresets: () => invokeCommand("get_effects_presets"),
  compose: (sid) => invokeCommand("compose_video", { sid }),

  // ============ Channels ============
  listChannels: () => invokeCommand("list_channels"),
  createChannel: (b) => invokeCommand("create_channel", { body: b }),
  deleteChannel: (id) => invokeCommand("delete_channel", { cid: id }),
  discoverFromChannelSwitcher: (profileDir, timeoutSec) =>
    invokeCommand("discover_from_channel_switcher", { profile_dir: profileDir, timeout_sec: timeoutSec }),
  oauthStart: (id, oauth_client_id) =>
    invokeCommand("oauth_start", { cid: id, ...(oauth_client_id ? { body: { oauth_client_id } } : {}) }),
  oauthStartForChannel: (id, oauth_client_id) =>
    invokeCommand("oauth_start_for_channel", { cid: id, ...(oauth_client_id ? { body: { oauth_client_id } } : {}) }),
  oauthComplete: (id, b) => invokeCommand("oauth_complete_channel", { cid: id, body: b }),
  channelPickedClient: (id) => invokeCommand("channel_picked_client", { cid: id }),
  discoverYoutubeChannels: (users, timeoutSec) => invokeCommand("discover_youtube_channels", { users, timeout_sec: timeoutSec }),
  importDiscoveredChannels: (channels) => invokeCommand("import_discovered_channels", { channels }),
  connectAllChannelsOneShot: (oauthClientId) => invokeCommand("connect_all_channels_one_shot", { oauth_client_id: oauthClientId }),
  refreshAllChannelMetadata: () => invokeCommand("refresh_all_channel_metadata"),
  importFromGoogleAccount: (oauthClientId) => invokeCommand("import_from_google_account", { oauthClientId: oauthClientId }),

  // ============ OAuth Clients ============
  listOauthClients: () => invokeCommand("list_oauth_clients"),
  createOauthClient: (b) => invokeCommand("create_oauth_client", { body: b }),
  updateOauthClient: (id, b) => invokeCommand("update_oauth_client", { oid: id, body: b }),
  deleteOauthClient: (id) => invokeCommand("delete_oauth_client", { oid: id }),
  validateOauthClient: (id) => invokeCommand("validate_oauth_client", { oid: id }),

  // ============ Bible ============
  bibleTranslations: () => invokeCommand("list_translations"),
  bibleBooks: () => invokeCommand("list_bible_books"),
  bibleChapter: (translation, book, chapter) =>
    invokeCommand("fetch_chapter", { translation, book, chapter }),
  listPasted: () => invokeCommand("list_pasted_chapters"),
  savePasted: (b) => invokeCommand("save_pasted_chapter", { body: b }),
  deletePasted: (id) => invokeCommand("delete_pasted_chapter", { pid: id }),

  // ============ Jobs ============
  listJobs: () => invokeCommand("list_jobs"),
  retryJob: (id) => invokeCommand("retry_job", { jid: id }),
  cancelJob: (id) => invokeCommand("cancel_job", { jid: id }),

  // ============ Uploads ============
  listUploads: () => invokeCommand("list_uploads"),
  createUpload: (b) => invokeCommand("create_upload", { body: b }),
  publish: (id) => invokeCommand("publish_upload", { uid: id }),
  publishAll: () => invokeCommand("publish_all_uploads"),
  bulkFromVideos: (b) => invokeCommand("bulk_uploads_from_videos", { body: b }),
  uploadsPreflight: () => invokeCommand("uploads_preflight"),
  aiEnrich: (b) => invokeCommand("ai_enrich_uploads", { body: b }),
  connectAllUrls: () => invokeCommand("channels_connect_all_urls"),

  // ============ AI Composer ============
  getComposeConfig: async () => {
    if (!isTauri) return loadComposeConfigFromStorage();
    try {
      return await invokeCommand("get_compose_config");
    } catch {
      return loadComposeConfigFromStorage();
    }
  },
  saveComposeConfig: async (cfg) => {
    saveComposeConfigToStorage(cfg);
    if (!isTauri) return cfg;
    try {
      return await invokeCommand("save_compose_config", { payload: cfg });
    } catch {
      return cfg;
    }
  },
  composeLyrics: async (payload) => {
    if (!isTauri) {
      return {
        error: "Tauri invoke is unavailable. Run this app through Tauri or configure a backend that exposes compose_lyrics.",
      };
    }
    try {
      // Ensure payload is passed under the `payload` key expected by the Tauri command
      return await invokeCommand("compose_lyrics", { payload });
    } catch (error) {
      console.warn("[Tauri API] compose_lyrics unavailable or failed:", error);
      return { error: "AI Composer is not available on this backend." };
    }
  },

};
