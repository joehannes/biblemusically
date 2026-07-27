#!/usr/bin/env node
// Seed a Playwright persistent Chromium profile with cookies captured from the app's
// embedded webview, so the existing remote-chrome automation scripts
// (midjourney-generator.js, the proxy autostart) reuse the in-app login session
// without a second visible login.
// Usage: node inject-cookies.js <profileDir> <cookiesJsonPath>
//   cookiesJsonPath: JSON array in Playwright addCookies() format.
// Prints JSON {ok, profile_dir, cookie_count} on stdout.

import { chromium } from "playwright";
import fs from "fs";
import path from "path";

const profileArg = process.argv[2];
const cookiesArg = process.argv[3];

if (!profileArg || !cookiesArg) {
  process.stderr.write(
    JSON.stringify({ ok: false, detail: "usage: inject-cookies.js <profileDir> <cookies.json>" }),
  );
  process.exit(1);
}

const profileDir = path.resolve(profileArg);
const cookiesPath = path.resolve(cookiesArg);

(async () => {
  let context = null;
  try {
    const cookies = JSON.parse(await fs.promises.readFile(cookiesPath, "utf8"));
    if (!Array.isArray(cookies) || cookies.length === 0) {
      throw new Error("cookies file holds no cookies");
    }
    await fs.promises.mkdir(profileDir, { recursive: true });
    context = await chromium.launchPersistentContext(profileDir, { headless: true });
    await context.addCookies(cookies);
    await context.close();
    context = null;
    process.stdout.write(
      JSON.stringify({ ok: true, profile_dir: profileDir, cookie_count: cookies.length }),
    );
    process.exit(0);
  } catch (err) {
    if (context) await context.close().catch(() => {});
    process.stderr.write(
      JSON.stringify({ ok: false, detail: err?.message || String(err) }),
    );
    process.exit(1);
  }
})();
