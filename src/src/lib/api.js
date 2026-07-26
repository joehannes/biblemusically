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

// Rects go to the backend as CSS pixels straight from getBoundingClientRect(). On Linux the
// backend places the webviews through GTK, whose units are logical pixels — the same size as a
// CSS pixel, since WebKitGTK derives devicePixelRatio from the very GDK scale factor GTK
// positions in. So no conversion belongs here.
const asRect = (rect) => ({
  x: rect.x,
  y: rect.y,
  width: Math.max(1, rect.width),
  height: Math.max(1, rect.height),
});

export const api = {
  // ============ Embedded browser webview (multi-page) ============
  // pageId identifies a browser "tab" (its own native child webview). Omitting it targets
  // the active page (open falls back to a "default" page).
  webviewOpen: (url, rect, pageId) => invokeCommand("webview_open", { url, pageId, rect: asRect(rect) }),
  // `url` lets the backend create the page if it isn't open yet (cold start / restored tab)
  // instead of throwing.
  webviewShowPage: (pageId, rect, url) => invokeCommand("webview_show_page", { pageId, rect: asRect(rect), url: url ?? null }),
  webviewClosePage: (pageId) => invokeCommand("webview_close_page", { pageId }),
  webviewListPages: () => invokeCommand("webview_list_pages"),
  webviewSetRect: (rect) => invokeCommand("webview_set_rect", { rect: asRect(rect) }),
  webviewHide: () => invokeCommand("webview_hide"),
  webviewNavigate: (url, pageId) => invokeCommand("webview_navigate", { url, pageId }),
  webviewEval: (js, pageId) => invokeCommand("webview_eval", { js, pageId }),
  webviewCurrentUrl: (pageId) => invokeCommand("webview_current_url", { pageId }),
  webviewTakeScrape: (key) => invokeCommand("webview_take_scrape", { key }),
  // Redirects the next native browser download into ~/Documents/BibleMusically/MacroDownloads/
  // <folderName>/<filename or the site's own suggested name> — see the "download" macro step.
  webviewArmDownload: (folderName, filename, resultKey) =>
    invokeCommand("webview_arm_download", { folderName, filename: filename || null, resultKey }),

  // ============ AI macro recorder (embedded browser) ============
  macroStart: () => invokeCommand("macro_start"),
  macroStop: () => invokeCommand("macro_stop"),
  macroStatus: () => invokeCommand("macro_status"),

  // ============ Webview session capture (Suno / Midjourney) ============
  // Lift session cookies out of the embedded webview after an in-app login; the MJ
  // variant also seeds the Playwright profile the remote-chrome scripts drive.
  webviewCaptureSunoSession: () => invokeCommand("webview_capture_suno_session"),
  webviewCaptureMjSession: (probeOnly = false) => invokeCommand("webview_capture_mj_session", { probeOnly }),

  // ============ Settings ============
  getSettings: (projectId) => invokeCommand("get_settings", { projectId }),
  saveSettings: (s, projectId) => invokeCommand("update_settings", { payload: s, projectId }),
  testSuno: () => invokeCommand("test_suno"),
  testAcestep: () => invokeCommand("test_acestep"),
  testHeartmula: () => invokeCommand("test_heartmula"),
  openSunoLogin: () => invokeCommand("open_suno_login"),
  captureSunoSession: () => invokeCommand("capture_suno_session"),
  testMj: () => invokeCommand("test_mj"),
  testFlux: () => invokeCommand("test_flux"),
  testComfy: () => invokeCommand("test_comfy"),
  openKaggleNotebook: (engine) => invokeCommand("open_kaggle_notebook", { engine }),
  openKaggleTokenPage: () => invokeCommand("open_kaggle_token_page"),
  kaggleNotebookUrl: (engine) => invokeCommand("kaggle_notebook_url", { engine }),
  openKaggleLogin: () => invokeCommand("open_kaggle_login"),
  saveKaggleToken: (tokenJson) => invokeCommand("save_kaggle_token", { tokenJson }),
  listKaggleAccounts: () => invokeCommand("list_kaggle_accounts"),
  activateKaggleAccount: (username) => invokeCommand("activate_kaggle_account", { username }),
  removeKaggleAccount: (username) => invokeCommand("remove_kaggle_account", { username }),
  rotateKaggleAccount: () => invokeCommand("rotate_kaggle_account"),
  pickDirectory: (title) => invokeCommand("pick_directory", { title }),
  fetchKaggleUrl: (engine) => invokeCommand("fetch_kaggle_url", { engine }),
  startKaggleServer: (engine) => invokeCommand("start_kaggle_server", { engine }),
  supersedeKaggleSession: (engine) => invokeCommand("supersede_kaggle_session", { engine }),
  stopKaggleServer: (engine) => invokeCommand("stop_kaggle_server", { engine }),
  kaggleStartMonitor: (engine, fresh = false) => invokeCommand("kaggle_start_monitor", { engine, fresh }),
  kaggleProgress: (engine) => invokeCommand("kaggle_progress", { engine }),
  kaggleStopMonitor: (engine) => invokeCommand("kaggle_stop_monitor", { engine }),
  listStylePresets: () => invokeCommand("list_style_presets"),
  saveStylePreset: (payload) => invokeCommand("save_style_preset", { payload }),
  deleteStylePreset: (id) => invokeCommand("delete_style_preset", { id }),
  getChannelStyle: (channelId) => invokeCommand("get_channel_style", { channelId }),
  setChannelStyle: (channelId, payload) => invokeCommand("set_channel_style", { channelId, payload }),
  mixGenres: (payload) => invokeCommand("mix_genres", { payload }),
  researchProjectChannels: (projectId) => invokeCommand("research_project_channels", { projectId }),
  listStyleSamples: () => invokeCommand("list_style_samples"),
  generateStyleSample: (kind, key, label, spec) => invokeCommand("generate_style_sample", { kind, key, label, spec }),
  deleteStyleSample: (kind, key) => invokeCommand("delete_style_sample", { kind, key }),
  getUserLearnings: () => invokeCommand("get_user_learnings"),
  updateUserLearnings: (patch) => invokeCommand("update_user_learnings", { patch }),
  getProjectLearnings: (projectId) => invokeCommand("get_project_learnings", { projectId }),
  updateProjectLearnings: (projectId, patch) => invokeCommand("update_project_learnings", { projectId, patch }),
  recordLearningSignal: (scope, projectId, kind, key, detail) => invokeCommand("record_learning_signal", { scope, projectId, kind, key, detail }),
  learningsLocations: (projectId) => invokeCommand("learnings_locations", { projectId }),
  openYoutubeCreateChannel: () => invokeCommand("open_youtube_create_channel"),
  importChannelByHandle: (handle) => invokeCommand("import_channel_by_handle", { handle }),
  aiTranslateUi: (strings, language) => invokeCommand("ai_translate_ui", { payload: { strings, language } }),
  // Drains one-shot notices about automatic provider fallbacks (each is delivered once).
  takeAiNotices: () => invokeCommand("take_ai_notices"),
  // AI providers (free + paid) and the live model list a given key can actually reach.
  listAiProviders: () => invokeCommand("list_ai_providers"),
  listAiModels: (provider) => invokeCommand("list_ai_models", { provider }),
  // Guided-flow proposals: per-step picks personalised from brief + learnings + engine caps.
  guideProposal: (payload) => invokeCommand("guide_proposal", { payload }),
  // Live-aggregated starter templates for a view, validated against its control catalogue.
  guideTemplates: (payload) => invokeCommand("guide_templates", { payload }),
  getWorkflowState: (projectId, view) => invokeCommand("get_workflow_state", { projectId, view }),
  saveWorkflowState: (projectId, view, patch) => invokeCommand("save_workflow_state", { projectId, view, patch }),
  resetWorkflowState: (projectId, view) => invokeCommand("reset_workflow_state", { projectId, view }),
  // ============ Assistant voice ============
  listAssistantVoices: () => invokeCommand("list_assistant_voices"),
  ttsSpeak: (payload) => invokeCommand("tts_speak", { payload }),
  sttTranscribe: (payload) => invokeCommand("stt_transcribe", { payload }),
  guideInterpret: (payload) => invokeCommand("guide_interpret", { payload }),
  // What a stated goal actually requires: which setup steps, and what can be preset outright.
  setupRecommendation: (payload) => invokeCommand("setup_recommendation", { payload }),
  // ============ Publicity ============
  authorPublicityPiece: (payload) => invokeCommand("author_publicity_piece", { payload }),
  authorPublicitySet: (songId, platforms) => invokeCommand("author_publicity_set", { songId, platforms: platforms || null }),
  generatePublicityCovers: (id) => invokeCommand("generate_publicity_covers", { id }),
  listPublicityPieces: (songId, projectId) => invokeCommand("list_publicity_pieces", { songId: songId || null, projectId: projectId || null }),
  updatePublicityPiece: (id, patch) => invokeCommand("update_publicity_piece", { id, patch }),
  deletePublicityPiece: (id) => invokeCommand("delete_publicity_piece", { id }),
  // ============ Compilations + music-only distribution ============
  // A whole book of the Bible as one long video, with a timestamped tracklist.
  compilationPlan: (payload) => invokeCommand("compilation_plan", { payload }),
  buildCompilation: (payload) => invokeCommand("build_compilation", { payload }),
  listCompilations: (projectId) => invokeCommand("list_compilations", { projectId }),
  deleteCompilation: (id) => invokeCommand("delete_compilation", { id }),
  // Distribution to Spotify and the rest. No distributor has a public upload API (checked July
  // 2026), so the app builds the release package and a macro or a person does the upload.
  listDistributors: () => invokeCommand("list_distributors"),
  saveArtistProfile: (payload) => invokeCommand("save_artist_profile", { payload }),
  listArtistProfiles: () => invokeCommand("list_artist_profiles"),
  planReleases: (projectId) => invokeCommand("plan_releases", { projectId }),
  saveRelease: (payload) => invokeCommand("save_release", { payload }),
  listReleases: (projectId) => invokeCommand("list_releases", { projectId }),
  deleteRelease: (id) => invokeCommand("delete_release", { id }),
  exportReleasePackage: (id) => invokeCommand("export_release_package", { id }),
  generateReleaseCover: (payload) => invokeCommand("generate_release_cover", { payload }),
  // ============ Poetic graphic novels → ebooks ============
  // EPUB 3 is the target because it is the one ebook format that carries the song itself.
  listNovelStyles: () => invokeCommand("list_novel_styles"),
  authorEdition: (payload) => invokeCommand("author_edition", { payload }),
  listEditions: (projectId, songId) => invokeCommand("list_editions", { projectId: projectId || null, songId: songId || null }),
  updateEdition: (id, patch) => invokeCommand("update_edition", { id, patch }),
  deleteEdition: (id) => invokeCommand("delete_edition", { id }),
  generateEditionArt: (id) => invokeCommand("generate_edition_art", { id }),
  collectEditionArt: (id) => invokeCommand("collect_edition_art", { id }),
  buildEpub: (payload) => invokeCommand("build_epub", { payload }),
  listEbookStores: () => invokeCommand("list_ebook_stores"),
  ebookPricing: (payload) => invokeCommand("ebook_pricing", { payload }),
  // ============ Print on demand (Printify) ============
  // The one integration here with a real API — shapes taken from its OpenAPI spec.
  printifyStorefronts: () => invokeCommand("printify_storefronts"),
  printifyConnect: () => invokeCommand("printify_connect"),
  printifyCatalog: (payload) => invokeCommand("printify_catalog", { payload }),
  printifyBlueprintDetail: (blueprintId) => invokeCommand("printify_blueprint_detail", { blueprintId }),
  printifyPickProducts: (payload) => invokeCommand("printify_pick_products", { payload }),
  printifySelection: () => invokeCommand("printify_selection"),
  printifyMakeProducts: (payload) => invokeCommand("printify_make_products", { payload }),
  listPrintifyProducts: (projectId) => invokeCommand("list_printify_products", { projectId: projectId || null }),
  // ============ Autosave: staged on blur, committed on view change ============
  stageField: (payload) => invokeCommand("stage_field", { payload }),
  autosaveCommit: (payload) => invokeCommand("autosave_commit", { payload }),
  autosaveStatus: (projectId) => invokeCommand("autosave_status", { projectId }),
  saveAndPush: (projectId, view) => invokeCommand("save_and_push", { projectId, view: view || null }),
  // ============ Workflow runs (backend-sequenced, survive a project switch) ============
  startWorkflowRun: (payload) => invokeCommand("start_workflow_run", { payload }),
  workflowRunStatus: (projectId) => invokeCommand("workflow_run_status", { projectId: projectId || null }),
  setWorkflowRunStatus: (projectId, status) => invokeCommand("set_workflow_run_status", { projectId, status }),
  // ============ Posting macros: record once, replay per song ============
  listPostRecipes: () => invokeCommand("list_post_recipes"),
  preparePostMacro: (payload) => invokeCommand("prepare_post_macro", { payload }),
  linkPostMacro: (payload) => invokeCommand("link_post_macro", { payload }),
  unlinkPostMacro: (platform) => invokeCommand("unlink_post_macro", { platform }),
  // ============ Users, shared projects, merging, rewinding ============
  signIn: (payload) => invokeCommand("sign_in", { payload }),
  currentUser: () => invokeCommand("current_user"),
  signOut: () => invokeCommand("sign_out"),
  addProjectMember: (payload) => invokeCommand("add_project_member", { payload }),
  listProjectMembers: (projectId) => invokeCommand("list_project_members", { projectId }),
  removeProjectMember: (projectId, who) => invokeCommand("remove_project_member", { projectId, who }),
  myProjectRole: (projectId) => invokeCommand("my_project_role", { projectId }),
  shareProject: (projectId, role) => invokeCommand("share_project", { projectId, role }),
  acceptInvite: (payload) => invokeCommand("accept_invite", { payload }),
  teamPull: (projectId) => invokeCommand("team_pull", { projectId }),
  resolveConflict: (payload) => invokeCommand("resolve_conflict", { payload }),
  finishMerge: (projectId) => invokeCommand("finish_merge", { projectId }),
  projectHistory: (projectId, limit) => invokeCommand("project_history", { projectId, limit: limit || null }),
  rewindProject: (projectId, commit) => invokeCommand("rewind_project", { projectId, commit }),
  // ============ Signing in from a phone ============
  platformCapabilities: () => invokeCommand("platform_capabilities"),
  exportChannelTokens: (passphrase) => invokeCommand("export_channel_tokens", { passphrase }),
  importChannelTokens: (payload) => invokeCommand("import_channel_tokens", { payload }),
  deeplinkSetupSteps: () => invokeCommand("deeplink_setup_steps"),
  // ============ Suno without a browser ============
  sunoGenerate: (payload) => invokeCommand("suno_generate", { payload }),
  sunoPoll: (ids) => invokeCommand("suno_poll", { ids }),
  saveSunoCookie: (payload) => invokeCommand("save_suno_cookie", { payload }),
  sunoStatus: () => invokeCommand("suno_status"),
  // ============ Third-party access guides ============
  listAccessGuides: () => invokeCommand("list_access_guides"),
  accessGuide: (id) => invokeCommand("access_guide", { id }),
  setAccessStep: (payload) => invokeCommand("set_access_step", { payload }),
  saveAccessCapture: (payload) => invokeCommand("save_access_capture", { payload }),
  // ============ Channel publication times ============
  suggestPublishTime: (payload) => invokeCommand("suggest_publish_time", { payload }),
  setPublishTime: (payload) => invokeCommand("set_publish_time", { payload }),
  listPublishTimes: () => invokeCommand("list_publish_times"),
  channelsMissingPublishTime: () => invokeCommand("channels_missing_publish_time"),
  // ============ Clipboard: system mirror, append-only vault, paste queue ============
  clipboardSync: () => invokeCommand("clipboard_sync"),
  clipboardHistory: (kind, limit) => invokeCommand("clipboard_history", { kind: kind || null, limit: limit || null }),
  clipboardUse: (id) => invokeCommand("clipboard_use", { id }),
  clipboardAdd: (payload) => invokeCommand("clipboard_add", { payload }),
  clipboardPop: (kind) => invokeCommand("clipboard_pop", { kind: kind || null }),
  clipboardPin: (id, pinned) => invokeCommand("clipboard_pin", { id, pinned }),
  clipboardClear: (kind) => invokeCommand("clipboard_clear", { kind: kind || null }),
  clipboardQueueSet: (payload) => invokeCommand("clipboard_queue_set", { payload }),
  clipboardQueueNext: () => invokeCommand("clipboard_queue_next"),
  // Used by the macro player: returns the value to paste now and advances in one call.
  clipboardQueueTake: () => invokeCommand("clipboard_queue_take"),
  clipboardQueueStatus: () => invokeCommand("clipboard_queue_status"),
  // ============ AI-authored macros ============
  // Command-line steps (ffmpeg, yt-dlp, imagemagick) on this device or on a remote host.
  listCliRunners: () => invokeCommand("list_cli_runners"),
  remoteExec: (job) => invokeCommand("remote_exec", { job }),
  // Vertical cut from the hook, with a link card back to the full video.
  buildShort: (payload) => invokeCommand("build_short", { payload }),
  authorMacro: (payload) => invokeCommand("author_macro", { payload }),
  listAuthoredMacros: () => invokeCommand("list_authored_macros"),
  deleteAuthoredMacro: (id) => invokeCommand("delete_authored_macro", { id }),
  // ============ Remote rendering ============
  listRenderProviders: () => invokeCommand("list_render_providers"),
  buildRenderSpec: (songId, options) => invokeCommand("build_render_spec", { songId, options: options || null }),
  submitRemoteRender: (songId, options) => invokeCommand("submit_remote_render", { songId, options: options || null }),
  listRenderJobs: (limit) => invokeCommand("list_render_jobs", { limit: limit ?? null }),
  recordRenderResult: (result) => invokeCommand("record_render_result", { result }),
  writeRenderWorkflow: (projectId) => invokeCommand("write_render_workflow", { projectId }),
  listGenrePresets: () => invokeCommand("list_genre_presets"),
  saveGenrePreset: (payload) => invokeCommand("save_genre_preset", { payload }),
  deleteGenrePreset: (id) => invokeCommand("delete_genre_preset", { id }),
  updateSongStyles: (songIds, styles) => invokeCommand("update_song_styles", { songIds, styles }),
  listTransitionPresets: () => invokeCommand("list_transition_presets"),
  saveTransitionPreset: (payload) => invokeCommand("save_transition_preset", { payload }),
  deleteTransitionPreset: (id) => invokeCommand("delete_transition_preset", { id }),
  suggestTransitions: (payload) => invokeCommand("suggest_transitions", { payload }),
  setSectionTransitions: (updates) => invokeCommand("set_section_transitions", { updates }),
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
  // Writes straight into the project's own ~/Documents/<project> folder — no save dialog, same
  // "just saves it" feel as a browser download, but landing next to the rest of the project's
  // files under a name you actually chose instead of the OS Downloads folder.
  saveProjectFile: (pid, filename, content) => invokeCommand("save_project_file", { pid, filename, content }),
  deleteProject: (id) => invokeCommand("delete_project", { pid: id }),
  exportProject: (id) => invokeCommand("export_project", { pid: id }),
  importProject: (body, sourceDir) => invokeCommand("import_project", { body, source_dir: sourceDir }),
  importLyrics: (id, items) => invokeCommand("import_lyrics", { pid: id, body: { items } }),
  getProjectGitInfo: (projectId) => invokeCommand("get_project_git_info", { projectId }),
  saveProjectVersion: (projectId, saveType, branchToCreate) => invokeCommand("save_project_version", { projectId, saveType, branchToCreate }),
  checkoutProjectGitTag: (projectId, tag) => invokeCommand("checkout_project_git_tag", { projectId, tag }),
  checkoutProjectGitBranch: (projectId, branch) => invokeCommand("checkout_project_git_branch", { projectId, branch }),
  createProjectGitBranch: (projectId, branchName) => invokeCommand("create_project_git_branch", { projectId, branchName }),
  authorizeProjectGDrive: (projectId) => invokeCommand("authorize_project_gdrive", { projectId }),

  // ============ Scheduler ============
  generateNextChapterNow: (projectId) => invokeCommand("generate_next_chapter_now", { projectId }),

  // ============ Songs ============
  listSongs: (id) => invokeCommand("list_songs", { pid: id }),
  generateOverlay: (sid) => invokeCommand("generate_overlay", { sid }),
  generateOverlaysBulk: (pid, force) => invokeCommand("generate_overlays_bulk", { pid, force }),
  getSong: (sid) => invokeCommand("get_song", { sid }),
  updateSong: (sid, patch) => invokeCommand("update_song", { sid, patch }),
  deleteSong: (sid) => invokeCommand("delete_song", { sid }),
  genMusic: (sid) => invokeCommand("generate_music", { sid }),
  analyze: (sid) => invokeCommand("analyze_song", { sid }),
  downloadAudio: (audioUrl, format, filename) => invokeCommand("download_and_convert_audio", { audioUrl, format, filename }),
  downloadAllAudio: (songs) => invokeCommand("download_all_songs", { songs }),
  // Dialog-free save for the AI generation pipeline: writes straight to
  // <project_folder>/auto/<channel-slug>/<filename>, no native save dialog.
  saveGeneratedAsset: (audioUrl, projectId, channelId, filename, format) =>
    invokeCommand("save_generated_asset", { audioUrl, projectId, channelId: channelId || null, filename, format }),
  selectSongVariant: (sid, variant) => invokeCommand("select_song_variant", { sid, variant }),

  // ============ Characters ============
  listCharacters: (songId, projectId) => invokeCommand("list_characters", { songId: songId || null, projectId: projectId || null }),
  createCharacter: (b) => invokeCommand("create_character", { body: b }),
  updateCharacter: (id, b) => invokeCommand("update_character", { charId: id, body: b }),
  deleteCharacter: (id) => invokeCommand("delete_character", { charId: id }),
  generateCharacterImage: (id) => invokeCommand("generate_character_image", { charId: id }),
  varyCharacterImage: (id) => invokeCommand("vary_character_image", { charId: id }),
  generateCharacterChannelVariant: (id, channelId) => invokeCommand("generate_character_channel_variant", { charId: id, channelId }),
  selectCharacterVariant: (id, idx) => invokeCommand("select_character_variant", { charId: id, variantIndex: idx }),
  discardCharacterVariant: (id, idx) => invokeCommand("discard_character_variant", { charId: id, variantIndex: idx }),
  discardAllCharacterVariants: (id) => invokeCommand("discard_all_character_variants", { charId: id }),
  proposeCharacters: (songId) => invokeCommand("propose_characters", { songId }),

  // ============ Sections ============
  listSections: (sid) => invokeCommand("list_sections", { sid }),
  updateSection: (id, b) => invokeCommand("update_section", { secid: id, body: b }),
  genImage: (id) => invokeCommand("generate_section_image", { secid: id }),
  batchImages: (sid) => invokeCommand("batch_generate_images", { sid }),
  bulkGenerateAll: (projectId) => invokeCommand("bulk_generate_all_images", { projectId }),

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
  connectAllChannelsOneShot: (oauthClientId) => invokeCommand("connect_all_channels_one_shot", { oauthClientId }),
  refreshAllChannelMetadata: () => invokeCommand("refresh_all_channel_metadata"),
  importFromGoogleAccount: (oauthClientId) => invokeCommand("import_from_google_account", { oauthClientId: oauthClientId }),

  // ============ Channel Creation Watcher ============
  startChannelCreationWatcher: (port) => invokeCommand("start_channel_creation_watcher", { port }),
  injectChannelHandle: (handle) => invokeCommand("inject_channel_handle", { handle }),

  // ============ Channel Settings & AI Translation ============
  getGlobalChannelSettings: (projectId) => invokeCommand("get_global_channel_settings", { projectId }),
  saveGlobalChannelSettings: (projectId, settings) => invokeCommand("save_global_channel_settings", { projectId, settings }),
  translateAndApplySettings: (projectId, channelIds) => invokeCommand("translate_and_apply_settings", { projectId, channelIds }),
  getChannelSettings: (channelId) => invokeCommand("get_channel_settings", { channelId }),
  updateChannelOverrides: (channelId, overrides) => invokeCommand("update_channel_overrides", { channelId, overrides }),
  syncChannelToYouTube: (channelId) => invokeCommand("sync_channel_to_youtube", { channelId }),
  // kind: "musical" | "visual" — culturally adapts a style descriptor for one channel's
  // language/region via the free AI. Preview-only: never writes anything itself.
  aiFlavorStyle: (channelId, kind, baseText) =>
    invokeCommand("ai_flavor_style", { request: { channel_id: channelId, kind, base_text: baseText } }),

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
  composeFreeform: async (payload) => {
    if (!isTauri) {
      return { error: "Tauri invoke is unavailable. Run this app through Tauri to use the freeform composer.", items: [] };
    }
    try {
      return await invokeCommand("compose_freeform", { payload });
    } catch (error) {
      console.warn("[Tauri API] compose_freeform unavailable or failed:", error);
      return { error: "Freeform composer is not available on this backend.", items: [] };
    }
  },
  composeAssist: async (payload) => {
    if (!isTauri) {
      return {
        error: "Tauri invoke is unavailable. Run this app through Tauri to use Qwen assist.",
        text: "",
        json: null,
      };
    }
    try {
      return await invokeCommand("compose_assist", { payload });
    } catch (error) {
      console.warn("[Tauri API] compose_assist unavailable or failed:", error);
      return { error: "Qwen assist is not available on this backend.", text: "", json: null };
    }
  },

  // ============ Insights (pipeline, performance, quotas) ============
  pipelineOverview: (projectId) => invokeCommand("pipeline_overview", { projectId: projectId || null }),
  refreshUploadAnalytics: (channelId) => invokeCommand("refresh_upload_analytics", { channelId: channelId || null }),
  performanceReport: (projectId) => invokeCommand("performance_report", { projectId: projectId || null }),
  quotaReport: () => invokeCommand("quota_report"),

  // ============ Social presence (accounts, profile, derivatives, publishing) ============
  listSocialPlatforms: () => invokeCommand("list_social_platforms"),
  connectSocialAccount: (platform, fields) => invokeCommand("connect_social_account", { platform, fields }),
  listSocialAccounts: () => invokeCommand("list_social_accounts"),
  disconnectSocialAccount: (platform) => invokeCommand("disconnect_social_account", { platform }),
  verifySocialAccount: (platform) => invokeCommand("verify_social_account", { platform }),
  ingestSocialActivity: (limit) => invokeCommand("ingest_social_activity", { limit: limit || 50 }),
  refreshTasteProfile: () => invokeCommand("refresh_taste_profile"),
  ideateNext: (projectId, question) => invokeCommand("ideate_next", { projectId: projectId || null, question: question || null }),
  deriveSongVersions: (songId, platforms, kinds) => invokeCommand("derive_song_versions", { songId, platforms, kinds: kinds || null }),
  listDerivatives: (songId) => invokeCommand("list_derivatives", { songId: songId || null }),
  updateDerivative: (id, patch) => invokeCommand("update_derivative", { id, patch }),
  deleteDerivative: (id) => invokeCommand("delete_derivative", { id }),
  publishDerivative: (id) => invokeCommand("publish_derivative", { id }),
  publishAllDerivatives: (songId) => invokeCommand("publish_all_derivatives", { songId }),

  // ============ Character → section linking ============
  applyCharacterToSections: (charId, sectionIds, mode) => invokeCommand("apply_character_to_sections", { charId, sectionIds, mode: mode || "prepend" }),
  detachCharacterFromSections: (charId, sectionIds) => invokeCommand("detach_character_from_sections", { charId, sectionIds }),
  characterSectionLinks: (songId) => invokeCommand("character_section_links", { songId }),

  // ============ Service health ============
  serviceHealth: () => invokeCommand("service_health"),

  // ============ Data & storage (JSON store, legacy import, per-project git) ============
  storeInfo: () => invokeCommand("store_info"),
  runLegacyMigration: (force = false) => invokeCommand("run_legacy_migration", { force }),
  purgeLegacyData: () => invokeCommand("purge_legacy_data"),
  commitProjectData: (projectId, message) => invokeCommand("commit_project_data", { projectId, message: message || null }),
  commitAllProjectData: () => invokeCommand("commit_all_project_data"),
  revealInFileManager: (path) => invokeCommand("reveal_in_file_manager", { path }),

  // ============ Encrypted credential vault ============
  vaultStatus: () => invokeCommand("vault_status"),
  vaultList: () => invokeCommand("vault_list"),
  vaultPut: (key, secret) => invokeCommand("vault_put", { key, secret }),
  vaultDelete: (key) => invokeCommand("vault_delete", { key }),
  vaultUnlock: (passphrase) => invokeCommand("vault_unlock", { passphrase }),
  vaultLock: () => invokeCommand("vault_lock"),
  vaultSetPassphrase: (passphrase, current) => invokeCommand("vault_set_passphrase", { passphrase, current: current || null }),

  // ============ Remote sync (free git hosts + asset platforms) ============
  listSyncProviders: () => invokeCommand("list_sync_providers"),
  getSyncConfig: (projectId) => invokeCommand("get_sync_config", { projectId }),
  saveSyncConfig: (projectId, config) => invokeCommand("save_sync_config", { projectId, config }),
  saveAssetCredentials: (provider, accessKey, secretKey) => invokeCommand("save_asset_credentials", { provider, accessKey, secretKey }),
  testSyncConnection: (provider, token) => invokeCommand("test_sync_connection", { provider, token: token || null }),
  syncProjectNow: (projectId) => invokeCommand("sync_project_now", { projectId }),
  pullProjectNow: (projectId) => invokeCommand("pull_project_now", { projectId }),
  listProjectAssets: (projectId) => invokeCommand("list_project_assets", { projectId }),
  restoreProjectAssets: (projectId) => invokeCommand("restore_project_assets", { projectId }),

};
