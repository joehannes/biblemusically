const presets = [
  {
    id: "default",
    label: "Default (keep current settings)",
    defaultName: "New Project",
    defaultTopic: "",
    defaultSchedule: "",
    projectOverrides: {},
    resetDefaults: false,
    resetChannels: false,
  },
  {
    id: "reset-defaults",
    label: "Reset to app defaults (also clears channels)",
    defaultName: "Fresh Project",
    projectOverrides: {},
    resetDefaults: true,
    resetChannels: true,
    defaults: {
      // lightweight defaults snapshot — adjust as needed
      language: "en",
      uploads: { privacy: "private" },
    }
  },
  {
    id: "biblical-multilingual",
    label: "Biblical — Multilingual (styles preset)",
    defaultName: "Biblical Multilingual",
    projectOverrides: { multi_language: true, multi_style: true, styles: ["ethereal","radiant"] },
    resetDefaults: false,
    resetChannels: false,
  },
  {
    id: "youtube-short-campaign",
    label: "YouTube Shorts Campaign (shorts-first)",
    defaultName: "Shorts Campaign",
    projectOverrides: { schedule: "daily", formats: ["shorts_9x16_1080p"] },
    resetDefaults: false,
    resetChannels: false,
  }
];

export default presets;
