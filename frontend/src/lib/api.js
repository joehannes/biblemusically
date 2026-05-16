import axios from "axios";
const BASE = process.env.REACT_APP_BACKEND_URL;
export const API = `${BASE}/api`;

const http = axios.create({ baseURL: API });

export const api = {
  // settings
  getSettings: () => http.get("/settings").then(r => r.data),
  saveSettings: (s) => http.put("/settings", s).then(r => r.data),
  testSuno: () => http.post("/settings/test-suno").then(r => r.data),
  testMj: () => http.post("/settings/test-mj").then(r => r.data),
  testFfmpeg: () => http.post("/settings/test-ffmpeg").then(r => r.data),
  // projects
  listProjects: () => http.get("/projects").then(r => r.data),
  createProject: (b) => http.post("/projects", b).then(r => r.data),
  getProject: (id) => http.get(`/projects/${id}`).then(r => r.data),
  updateProject: (id, b) => http.put(`/projects/${id}`, b).then(r => r.data),
  deleteProject: (id) => http.delete(`/projects/${id}`).then(r => r.data),
  importLyrics: (id, items) => http.post(`/projects/${id}/lyrics-import`, { items }).then(r => r.data),
  // songs
  listSongs: (id) => http.get(`/projects/${id}/songs`).then(r => r.data),
  getSong: (sid) => http.get(`/songs/${sid}`).then(r => r.data),
  deleteSong: (sid) => http.delete(`/songs/${sid}`).then(r => r.data),
  genMusic: (sid) => http.post(`/songs/${sid}/generate-music`).then(r => r.data),
  analyze: (sid) => http.post(`/songs/${sid}/analyze`).then(r => r.data),
  // sections
  listSections: (sid) => http.get(`/songs/${sid}/sections`).then(r => r.data),
  updateSection: (id, b) => http.put(`/sections/${id}`, b).then(r => r.data),
  genImage: (id) => http.post(`/sections/${id}/generate-image`).then(r => r.data),
  batchImages: (sid) => http.post(`/songs/${sid}/batch-generate-images`).then(r => r.data),
  // video / effects
  effectsPresets: () => http.get("/effects-presets").then(r => r.data),
  compose: (sid) => http.post(`/songs/${sid}/compose-video`).then(r => r.data),
  // channels
  listChannels: () => http.get("/channels").then(r => r.data),
  createChannel: (b) => http.post("/channels", b).then(r => r.data),
  deleteChannel: (id) => http.delete(`/channels/${id}`).then(r => r.data),
  oauthStart: (id, oauth_client_id) => http.post(`/channels/${id}/oauth-start`, oauth_client_id ? { oauth_client_id } : {}).then(r => r.data),
  oauthComplete: (id, b) => http.post(`/channels/${id}/oauth-complete`, b).then(r => r.data),
  channelPickedClient: (id) => http.get(`/channels/${id}/oauth-client`).then(r => r.data),
  // oauth clients
  listOauthClients: () => http.get("/oauth-clients").then(r => r.data),
  createOauthClient: (b) => http.post("/oauth-clients", b).then(r => r.data),
  updateOauthClient: (id, b) => http.put(`/oauth-clients/${id}`, b).then(r => r.data),
  deleteOauthClient: (id) => http.delete(`/oauth-clients/${id}`).then(r => r.data),
  // bible
  bibleTranslations: () => http.get("/bible/translations").then(r => r.data),
  bibleBooks: () => http.get("/bible/books").then(r => r.data),
  bibleChapter: (translation, book, chapter) => http.get("/bible/chapter", { params: { translation, book, chapter } }).then(r => r.data),
  listPasted: () => http.get("/bible/pasted").then(r => r.data),
  savePasted: (b) => http.post("/bible/pasted", b).then(r => r.data),
  deletePasted: (id) => http.delete(`/bible/pasted/${id}`).then(r => r.data),
  // drafts (auto-save)
  getDraft: (key) => http.get(`/drafts/${key}`).then(r => r.data),
  putDraft: (key, body) => http.put(`/drafts/${key}`, body).then(r => r.data),
  delDraft: (key) => http.delete(`/drafts/${key}`).then(r => r.data),
  // ai composer
  getComposeConfig: () => http.get("/ai/compose-config").then(r => r.data),
  saveComposeConfig: (b) => http.put("/ai/compose-config", b).then(r => r.data),
  composeLyrics: (b) => http.post("/ai/compose-lyrics", b).then(r => r.data),
  // uploads
  listUploads: () => http.get("/uploads").then(r => r.data),
  createUpload: (b) => http.post("/uploads", b).then(r => r.data),
  publish: (id) => http.post(`/uploads/${id}/publish`).then(r => r.data),
  publishAll: () => http.post("/uploads/publish-all").then(r => r.data),
  bulkFromVideos: (b) => http.post("/uploads/bulk-from-videos", b).then(r => r.data),
  uploadsPreflight: () => http.get("/uploads/preflight").then(r => r.data),
  aiEnrich: (b) => http.post("/uploads/ai-enrich", b).then(r => r.data),
  connectAllUrls: () => http.post("/channels/connect-all-urls").then(r => r.data),
  // jobs
  listJobs: () => http.get("/jobs").then(r => r.data),
  retryJob: (id) => http.post(`/jobs/${id}/retry`).then(r => r.data),
  cancelJob: (id) => http.delete(`/jobs/${id}`).then(r => r.data),
};
