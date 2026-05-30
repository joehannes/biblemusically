/**
 * Tauri-based API wrapper for the AI Music Video Studio
 * Maps frontend API calls to Tauri commands instead of HTTP endpoints
 */

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
  testMj: () => invokeCommand("test_mj"),
  ensureMjAutostart: () => invokeCommand("ensure_mj_autostart"),
  mjAutoLogin: (account, password, twofa) => invokeCommand("mj_auto_login", { login_account: account, login_password: password, login_2fa: twofa }),
  testFfmpeg: () => invokeCommand("test_ffmpeg"),

  // ============ Projects ============
  listProjects: () => invokeCommand("list_projects"),
  createProject: (b) => invokeCommand("create_project", { body: b }),
  getProject: (id) => invokeCommand("get_project", { pid: id }),
  updateProject: (id, b) => invokeCommand("update_project", { pid: id, body: b }),
  deleteProject: (id) => invokeCommand("delete_project", { pid: id }),
  importLyrics: (id, items) => invokeCommand("import_lyrics", { pid: id, body: { items } }),

  // ============ Songs ============
  listSongs: (id) => invokeCommand("list_songs", { pid: id }),
  getSong: (sid) => invokeCommand("get_song", { sid }),
  deleteSong: (sid) => invokeCommand("delete_song", { sid }),
  genMusic: (sid) => invokeCommand("generate_music", { sid }),
  analyze: (sid) => invokeCommand("analyze_song", { sid }),

  // ============ Sections ============
  listSections: (sid) => invokeCommand("list_sections", { sid }),
  updateSection: (id, b) => invokeCommand("update_section", { id, body: b }),
  genImage: (id) => invokeCommand("generate_section_image", { id }),
  batchImages: (sid) => invokeCommand("batch_generate_images", { sid }),

  // ============ Video / Effects ============
  effectsPresets: () => invokeCommand("get_effects_presets"),
  compose: (sid) => invokeCommand("compose_video", { sid }),

  // ============ Channels ============
  listChannels: () => invokeCommand("list_channels"),
  createChannel: (b) => invokeCommand("create_channel", { body: b }),
  deleteChannel: (id) => invokeCommand("delete_channel", { id }),
  oauthStart: (id, oauth_client_id) =>
    invokeCommand("oauth_start", { id, ...(oauth_client_id ? { oauth_client_id } : {}) }),
  oauthComplete: (id, b) => invokeCommand("oauth_complete_channel", { id, body: b }),
  channelPickedClient: (id) => invokeCommand("channel_picked_client", { id }),

  // ============ OAuth Clients ============
  listOauthClients: () => invokeCommand("list_oauth_clients"),
  createOauthClient: (b) => invokeCommand("create_oauth_client", { body: b }),
  updateOauthClient: (id, b) => invokeCommand("update_oauth_client", { id, body: b }),
  deleteOauthClient: (id) => invokeCommand("delete_oauth_client", { id }),

  // ============ Bible ============
  bibleTranslations: () => invokeCommand("list_translations"),
  bibleBooks: () => invokeCommand("list_bible_books"),
  bibleChapter: (translation, book, chapter) =>
    invokeCommand("fetch_chapter", { translation, book, chapter }),
  listPasted: () => invokeCommand("list_pasted_chapters"),
  savePasted: (b) => invokeCommand("save_pasted_chapter", { body: b }),
  deletePasted: (id) => invokeCommand("delete_pasted_chapter", { id }),

  // ============ Jobs ============
  listJobs: () => invokeCommand("list_jobs"),
  retryJob: (id) => invokeCommand("retry_job", { jid: id }),
  cancelJob: (id) => invokeCommand("cancel_job", { jid: id }),

  // ============ Uploads ============
  listUploads: () => invokeCommand("list_uploads"),
  createUpload: (b) => invokeCommand("create_upload", { body: b }),
  publish: (id) => invokeCommand("publish_upload", { id }),
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
      return await invokeCommand("compose_lyrics", payload);
    } catch (error) {
      console.warn("[Tauri API] compose_lyrics unavailable or failed:", error);
      return { error: "AI Composer is not available on this backend." };
    }
  },
};
