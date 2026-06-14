import fs from "fs";
import path from "path";
import { chromium } from "playwright";

const argv = process.argv.slice(2);
const args = {};
for (let i = 0; i < argv.length; i += 1) {
  const arg = argv[i];
  if (arg === "--timeout") {
    args.timeout = argv[++i];
  } else if (arg === "--users") {
    args.users = argv[++i];
  } else if (arg === "--profile") {
    args.profile = argv[++i];
  }
}

function dumpJson(value) {
  process.stdout.write(JSON.stringify(value));
}

function parseUsers(raw) {
  try {
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) return parsed.map((u) => `${u}`.trim()).filter(Boolean);
  } catch {
    // fallback: raw newline-delimited list
  }
  return `${raw}`.split(/\r?\n/).map((u) => u.trim()).filter(Boolean);
}

function buildCandidateUrls(user) {
  const normalized = user.trim();
  const candidates = new Set();
  if (!normalized) return [];
  if (normalized.startsWith("http://") || normalized.startsWith("https://")) {
    candidates.add(normalized);
  } else if (normalized.startsWith("@")) {
    candidates.add(`https://www.youtube.com/${normalized}`);
    candidates.add(`https://www.youtube.com/${normalized}/channels`);
  } else if (normalized.startsWith("UC")) {
    candidates.add(`https://www.youtube.com/channel/${normalized}`);
    candidates.add(`https://www.youtube.com/channel/${normalized}/channels`);
  } else {
    candidates.add(`https://www.youtube.com/@${normalized}`);
    candidates.add(`https://www.youtube.com/@${normalized}/channels`);
    candidates.add(`https://www.youtube.com/c/${normalized}`);
    candidates.add(`https://www.youtube.com/c/${normalized}/channels`);
    candidates.add(`https://www.youtube.com/user/${normalized}`);
    candidates.add(`https://www.youtube.com/user/${normalized}/channels`);
  }
  return Array.from(candidates);
}

function parseChannelIdFromUrl(url) {
  const match = url.match(/youtu(?:\.be\/|be\.com\/(?:channel\/|user\/|c\/|@))([A-Za-z0-9_-]+)/);
  return match ? match[1] : null;
}

async function extractChannels(page) {
  await page.evaluate(() => {
    window.scrollTo({ top: document.body.scrollHeight, behavior: "smooth" });
  });
  await page.waitForTimeout(1000);

  const raw = await page.evaluate(() => {
    const channels = [];
    const nodes = Array.from(document.querySelectorAll("ytd-channel-renderer, ytd-grid-channel-renderer, ytd-rich-item-renderer"));
    const seen = new Set();
    for (const node of nodes) {
      const link = node.querySelector("a#main-link, a.yt-simple-endpoint, a#video-title, a[href*='/channel/'], a[href*='/@']");
      if (!link?.href) continue;
      const url = link.href.split("?")[0].split("#")[0];
      if (seen.has(url)) continue;
      seen.add(url);
      const title = node.querySelector("#channel-title, #text, yt-formatted-string, span#text, a#video-title")?.textContent?.trim() || link.textContent?.trim() || "";
      const subscriberText = node.querySelector("#subscriber-count, span#subscriber-count, yt-formatted-string#subscriber-count, span#text")?.textContent?.trim() || "";
      channels.push({ channel_url: url, title: title || "", subscriber_count: subscriberText || "" });
    }
    return channels;
  });

  const cleaned = raw.map((item) => ({
    channel_url: item.channel_url,
    channel_id: parseChannelIdFromUrl(item.channel_url) || "",
    title: item.title,
    subscriber_count: item.subscriber_count,
  })).filter((item) => item.channel_id || item.title);

  if (cleaned.length) return cleaned;

  const pageInfo = await page.evaluate(() => {
    const canonical = document.querySelector("link[rel='canonical']")?.href || window.location.href;
    const title = document.querySelector("meta[name='title']")?.content || document.title || "";
    const subscriber = document.querySelector("span#subscriber-count, span#owner-sub-count, yt-formatted-string#subscriber-count, yt-formatted-string#owner-sub-count")?.textContent?.trim() || "";
    return { canonical, title, subscriber };
  });
  const pageUrl = page.url();
  const channelId = parseChannelIdFromUrl(pageInfo.canonical || pageUrl) || parseChannelIdFromUrl(pageUrl);
  if (channelId) {
    return [{
      channel_url: pageInfo.canonical || pageUrl,
      channel_id: channelId,
      title: pageInfo.title || "",
      subscriber_count: pageInfo.subscriber || "",
    }];
  }
  return [];
}

async function main() {
  const timeoutSeconds = Number(args.timeout || 180);
  const timeoutMs = Math.max(30, timeoutSeconds) * 1000;
  const users = parseUsers(args.users || "");
  if (!users.length) {
    dumpJson({ ok: false, error: "no_users", detail: "No YouTube users or URLs were provided." });
    process.exit(1);
  }

  const profileDir = args.profile ? path.resolve(args.profile) : path.resolve(process.cwd(), "tmp", "youtube-playwright-profile");
  await fs.promises.mkdir(profileDir, { recursive: true });

  const browserContext = await chromium.launchPersistentContext(profileDir, {
    headless: false,
    viewport: null,
    args: ["--start-maximized"],
  });
  const pages = browserContext.pages();
  const page = pages.length ? pages[0] : await browserContext.newPage();

  const discovered = [];
  const errors = [];
  const start = Date.now();

  for (const user of users) {
    const targets = buildCandidateUrls(user);
    let matched = false;
    let lastError = null;

    for (const target of targets) {
      if (Date.now() - start > timeoutMs) {
        errors.push({ query: user, error: "timeout", detail: "Discovery timed out before all targets were scanned." });
        break;
      }
      try {
        await page.goto(target, { waitUntil: "domcontentloaded", timeout: 60000 });
      } catch (err) {
        lastError = err;
        continue;
      }
      await page.waitForTimeout(1500);

      const channels = await extractChannels(page);
      if (channels.length) {
        discovered.push({ query: user, source: target, final_url: page.url(), channels });
        matched = true;
        break;
      }
      lastError = { message: `No channels found on ${target}` };
    }

    if (!matched) {
      errors.push({ query: user, error: "not_found", detail: lastError?.message || "Could not discover channels for this entry." });
    }
  }

  await browserContext.close();
  dumpJson({ ok: true, requested_users: users, discovered, errors });
}

main().catch((err) => {
  const error = {
    ok: false,
    error: "unexpected_error",
    detail: err?.message || String(err),
  };
  try {
    process.stderr.write(JSON.stringify(error));
  } catch {
    console.error(error);
  }
  process.exit(1);
});
