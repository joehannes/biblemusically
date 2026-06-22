#!/usr/bin/env node
// Playwright helper to open suno.com, wait for user login, and extract the studio-api_key cookie.
// Usage: node suno-session-capture.js --timeout 300

import { chromium } from "playwright";

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {};
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--timeout") {
      out.timeout = parseInt(args[++i], 10);
    }
  }
  return out;
}

(async () => {
  const { timeout = 300 } = parseArgs();
  const browser = await chromium.launch({ headless: false });
  const context = await browser.newContext();
  const page = await context.newPage();

  try {
    await page.goto("https://suno.com/", {
      waitUntil: "domcontentloaded",
      timeout: 60000,
    });
    console.log("Opened Suno.com. Waiting for login and cookie...");

    const start = Date.now();
    while ((Date.now() - start) / 1000 < timeout) {
      // Check cookies for domains that Suno may set (suno.com or studio-api.suno.com)
      const cookies = await context.cookies();
      const found = cookies.find(
        (c) => c.name === "studio-api_key" || c.name === "studio-api_key_local",
      );
      if (found && found.value) {
        // Print JSON to stdout
        console.log(
          JSON.stringify({ ok: true, cookie: `${found.name}=${found.value}` }),
        );
        await browser.close();
        process.exit(0);
      }
      // small delay
      await page.waitForTimeout(1500);
    }

    await browser.close();
    console.error(
      JSON.stringify({
        ok: false,
        detail: "timeout waiting for Suno login/cookie",
      }),
    );
    process.exit(2);
  } catch (e) {
    try {
      await browser.close();
    } catch (_) {}
    console.error(
      JSON.stringify({
        ok: false,
        detail: e && e.message ? e.message : String(e),
      }),
    );
    process.exit(1);
  }
})();
