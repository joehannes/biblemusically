import { chromium } from "playwright";
import fs from "fs";
import path from "path";

const profileDir = process.argv[2]
  ? path.resolve(process.argv[2])
  : path.resolve(process.cwd(), "tmp", "midjourney-playwright-profile");
const timeoutSeconds = Number(process.argv[3] || 300);
const timeoutMs = timeoutSeconds * 1000;
const targetUrl = "https://www.midjourney.com/app/";

async function dumpJson(value) {
  process.stdout.write(JSON.stringify(value, null, 0));
}

async function main() {
  await fs.promises.mkdir(profileDir, { recursive: true });

  const context = await chromium.launchPersistentContext(profileDir, {
    headless: false,
    viewport: null,
    args: ["--start-maximized"],
  });

  const pages = context.pages();
  const page = pages.length ? pages[0] : await context.newPage();

  await page.goto(targetUrl, { waitUntil: "domcontentloaded", timeout: 60000 });

  const apiCalls = [];
  page.on("response", async (response) => {
    try {
      const url = response.url();
      if (url.includes("/api/app/") || url.includes("/api/")) {
        apiCalls.push({ url, status: response.status() });
      }
    } catch {
      // ignore
    }
  });

  const start = Date.now();
  let cookies = [];
  let authCookie = null;
  while (Date.now() - start < timeoutMs) {
    cookies = await context.cookies();
    authCookie = cookies.find((c) => /next-auth.session-token|__Secure-next-auth.session-token|session-token|midjourney|mj_session/i.test(c.name));
    if (authCookie) break;
    await page.waitForTimeout(1000);
  }

  if (!authCookie) {
    const error = {
      ok: false,
      error: "timeout",
      detail: "No Midjourney authentication cookie was detected within the timeout. Please complete the login manually in the opened browser.",
      url: page.url(),
    };
    await context.close();
    process.stderr.write(JSON.stringify(error));
    process.exit(1);
  }

  await page.waitForTimeout(1500);

  const localStorageValues = await page.evaluate(() => {
    const data = {};
    for (let i = 0; i < localStorage.length; i += 1) {
      const key = localStorage.key(i);
      if (key) {
        data[key] = localStorage.getItem(key);
      }
    }
    return data;
  });

  const nextData = await page.evaluate(() => {
    const script = document.querySelector("#__NEXT_DATA__");
    if (!script) return null;
    try {
      return JSON.parse(script.textContent || "{}");
    } catch {
      return null;
    }
  });

  const cookieHeader = cookies.map((c) => `${c.name}=${c.value}`).join("; ");
  const result = {
    ok: true,
    cookie: cookieHeader,
    cookie_names: cookies.map((c) => c.name),
    auth_cookie_name: authCookie.name,
    auth_cookie_value: authCookie.value,
    api_calls: apiCalls.slice(0, 25),
    local_storage: localStorageValues,
    next_data: nextData,
    url: page.url(),
    title: await page.title(),
    profile_dir: profileDir,
  };

  await context.close();
  await dumpJson(result);
}

main().catch(async (err) => {
  const error = {
    ok: false,
    error: "unexpected_error",
    detail: err.message || String(err),
  };
  try {
    process.stderr.write(JSON.stringify(error));
  } catch {
    console.error(error);
  }
  process.exit(1);
});
