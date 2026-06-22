#!/usr/bin/env node
/**
 * youtube-channel-switcher.js
 *
 * Opens youtube.com/channel_switcher using a persistent Playwright browser context
 * (reusing the existing logged-in session), scrapes the HTML for all channel handles
 * and channel IDs listed there, and prints them as JSON to stdout.
 *
 * Usage:
 *   node youtube-channel-switcher.js [profileDir] [timeoutSeconds]
 *
 * The profile directory should point to a Chrome/Chromium profile that is already
 * logged into YouTube. If not provided, a temporary profile is used.
 *
 * Output (stdout): { ok: true, channels: [ { handle, channel_id, title, avatar }, ... ] }
 * On error:         { ok: false, error: "message", detail: "..." }
 */

import { chromium } from "playwright";
import fs from "fs";
import path from "path";

const profileArg = process.argv[2];
const timeoutSeconds = Number(process.argv[3] || 120);
const timeoutMs = timeoutSeconds * 1000;

const profileDir = profileArg
  ? path.resolve(profileArg)
  : path.resolve(process.cwd(), "tmp", "youtube-channel-switcher-profile");

const TARGET_URL = "https://www.youtube.com/channel_switcher";

function dumpJson(value) {
  process.stdout.write(JSON.stringify(value));
}

/**
 * Extract channel entries from the ytInitialData JSON blob embedded in the page.
 */
function extractChannelsFromInitialData(body) {
  // Find ytInitialData = { ... };
  const markers = [
    'window["ytInitialData"]',
    "window['ytInitialData']",
    "ytInitialData",
  ];

  for (const marker of markers) {
    const idx = body.indexOf(marker);
    if (idx === -1) continue;
    const afterMarker = body.slice(idx + marker.length);
    const eqIdx = afterMarker.indexOf("=");
    if (eqIdx === -1) continue;
    let jsonStr = afterMarker.slice(eqIdx + 1);

    // Find the opening {
    const braceIdx = jsonStr.indexOf("{");
    if (braceIdx === -1) continue;
    jsonStr = jsonStr.slice(braceIdx);

    // Match braces to find the end of the JSON object
    let depth = 0;
    let inString = false;
    let escaped = false;
    let endPos = 0;
    for (let i = 0; i < jsonStr.length; i++) {
      const ch = jsonStr[i];
      if (escaped) {
        escaped = false;
        continue;
      }
      if (ch === "\\") {
        escaped = true;
        continue;
      }
      if (ch === '"') {
        inString = !inString;
        continue;
      }
      if (inString) continue;
      if (ch === "{") depth++;
      if (ch === "}") depth--;
      if (depth === 0) {
        endPos = i + 1;
        break;
      }
    }
    if (endPos === 0) continue;

    jsonStr = jsonStr.slice(0, endPos);
    if (jsonStr.length < 3) continue;

    try {
      return JSON.parse(jsonStr);
    } catch {
      continue;
    }
  }
  return null;
}

/**
 * Walk the ytInitialData structure to find channel entries.
 */
function walkChannels(data) {
  const channels = [];

  function walk(obj, pathStr) {
    if (!obj || typeof obj !== "object") return;

    // Look for accountChannelSwitcherRenderer or channelSwitcherRenderer
    const switcher =
      obj.accountChannelSwitcherRenderer || obj.channelSwitcherRenderer;
    if (switcher && switcher.contents) {
      const items = switcher.contents;
      if (Array.isArray(items)) {
        for (const item of items) {
          // Each item may be a channelRenderer
          const ch = item.channelRenderer;
          if (ch) {
            const channelId = ch.channelId || "";
            const title =
              ch.title?.simpleText || ch.title?.runs?.[0]?.text || "";
            let handle = "";
            // Handle often appears in subscriberCountText or navigationEndpoint
            if (
              ch.navigationEndpoint?.commandMetadata?.webCommandMetadata?.url
            ) {
              const url =
                ch.navigationEndpoint.commandMetadata.webCommandMetadata.url;
              const handleMatch = url.match(/^\/(@[\w.-]+)/);
              if (handleMatch) handle = handleMatch[1];
            }
            // Also try to get it from subscriberCountText or description
            if (!handle && ch.subscriberCountText?.simpleText) {
              const htMatch =
                ch.subscriberCountText.simpleText.match(/@[\w.-]+/);
              if (htMatch) handle = htMatch[0];
            }
            if (!handle && ch.description?.simpleText) {
              const htMatch = ch.description.simpleText.match(/@[\w.-]+/);
              if (htMatch) handle = htMatch[0];
            }

            const avatar =
              ch.thumbnail?.thumbnails?.[ch.thumbnail.thumbnails.length - 1]
                ?.url || "";

            if (channelId || title) {
              channels.push({
                channel_id: channelId,
                title: title,
                handle: handle,
                avatar: avatar,
              });
            }
          }
        }
      }
    }

    if (Array.isArray(obj)) {
      for (const item of obj) walk(item, pathStr);
    } else {
      for (const key of Object.keys(obj)) {
        if (typeof obj[key] === "object" && obj[key] !== null) {
          walk(obj[key], pathStr ? `${pathStr}.${key}` : key);
        }
      }
    }
  }

  walk(data, "");
  return channels;
}

async function main() {
  await fs.promises.mkdir(profileDir, { recursive: true });

  const browserContext = await chromium.launchPersistentContext(profileDir, {
    headless: false,
    viewport: null,
    args: ["--start-maximized"],
  });

  const pages = browserContext.pages();
  const page = pages.length ? pages[0] : await browserContext.newPage();

  try {
    // Navigate to channel_switcher
    await page.goto(TARGET_URL, {
      waitUntil: "domcontentloaded",
      timeout: 60000,
    });

    // Wait a moment for the page to render fully
    await page.waitForTimeout(2000);

    // Check if we were redirected to a sign-in page
    const currentUrl = page.url();
    const pageContent = await page.content();
    const isSignInPage =
      currentUrl.includes("accounts.google.com") ||
      currentUrl.includes("signin") ||
      pageContent.includes('id="identifierId"') ||
      pageContent.includes("data-signin-recaptcha-option");

    if (isSignInPage) {
      // User needs to sign in — wait for redirect back to YouTube after authentication
      // This gives the user time to enter their credentials without the browser closing
      try {
        await page.waitForURL(
          (url) => {
            const hostname = url.hostname();
            return (
              hostname.includes("youtube.com") &&
              !url.pathname().includes("signin")
            );
          },
          { timeout: 110000 }, // Leave ~10s buffer within the 120s overall timeout
        );
        // Wait for the page to fully load after redirect
        await page.waitForTimeout(3000);
      } catch (e) {
        throw new Error(
          "Sign-in timeout: Please complete the Google sign-in within the time limit, or use a Chrome profile that is already logged into YouTube.",
        );
      }
    }

    // Try method 1: Extract from ytInitialData in page source
    const html = await page.content();

    const parsedData = extractChannelsFromInitialData(html);

    let channels = [];
    if (parsedData) {
      channels = walkChannels(parsedData);
    }

    // Method 2 (fallback): Extract from DOM if ytInitialData didn't yield results
    if (!channels.length) {
      const domChannels = await page.evaluate(() => {
        const results = [];
        const seen = new Set();

        // Look for channel links in the switcher page
        const channelElements = document.querySelectorAll(
          'a[href*="/channel/"], a[href*="/@"], yt-channel-switcher-renderer a, ' +
            "#channel-switcher-container a, [channel-id], yt-simple-endpoint",
        );

        for (const el of channelElements) {
          const href = el.getAttribute("href") || "";
          const channelIdMatch = href.match(/\/channel\/(UC[\w-]+)/);
          const handleMatch = href.match(/\/(@[\w.-]+)/);
          const title =
            el
              .querySelector("#channel-title, #text, .channel-title")
              ?.textContent?.trim() ||
            el.textContent?.trim() ||
            "";
          const avatarEl = el.querySelector("img");
          const avatar = avatarEl?.src || "";

          const key = channelIdMatch?.[1] || handleMatch?.[0] || href;
          if (seen.has(key)) continue;
          seen.add(key);

          if (channelIdMatch || handleMatch || title) {
            results.push({
              channel_id: channelIdMatch?.[1] || "",
              title: title,
              handle: handleMatch?.[0] || "",
              avatar: avatar,
            });
          }
        }

        return results;
      });

      channels = domChannels;
    }

    // Deduplicate by channel_id
    const seenIds = new Set();
    const uniqueChannels = channels.filter((ch) => {
      const key = ch.channel_id || ch.handle;
      if (!key || seenIds.has(key)) return false;
      seenIds.add(key);
      return true;
    });

    await browserContext.close();

    dumpJson({
      ok: true,
      channels: uniqueChannels,
      count: uniqueChannels.length,
    });
  } catch (err) {
    await browserContext.close().catch(() => {});
    const error = {
      ok: false,
      error: "scrape_failed",
      detail: err?.message || String(err),
    };
    dumpJson(error);
    process.exit(1);
  }
}

main().catch(async (err) => {
  const error = {
    ok: false,
    error: "unexpected_error",
    detail: err?.message || String(err),
  };
  try {
    process.stdout.write(JSON.stringify(error));
  } catch {
    console.error(error);
  }
  process.exit(1);
});
