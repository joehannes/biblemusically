#!/usr/bin/env node
// Playwright helper to submit a prompt to midjourney.com and download resulting images.
// Usage: node midjourney-generator.js --prompt "..." --cookie "k=v;..." --outdir /tmp/dir

const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {};
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--prompt') { out.prompt = args[++i]; }
    else if (args[i] === '--cookie') { out.cookie = args[++i]; }
    else if (args[i] === '--profile') { out.profile = args[++i]; }
    else if (args[i] === '--outdir') { out.outdir = args[++i]; }
  }
  return out;
}

(async () => {
  const { prompt, cookie, outdir } = parseArgs();
  if (!prompt || !outdir) {
    console.error('Missing --prompt or --outdir');
    process.exit(2);
  }

  if (!fs.existsSync(outdir)) fs.mkdirSync(outdir, { recursive: true });

  let browser;
  let context;
  let page;

  if (profile) {
    // Reuse an existing Playwright profile so the logged-in Midjourney session is available
    context = await chromium.launchPersistentContext(profile, { headless: false, viewport: null, args: ['--start-maximized'] });
    const pages = context.pages();
    page = pages.length ? pages[0] : await context.newPage();
  } else {
    browser = await chromium.launch({ headless: false });
    context = await browser.newContext();
    page = await context.newPage();
  }

  try {
    // Navigate to the app
    await page.goto('https://www.midjourney.com/app/', { waitUntil: 'domcontentloaded', timeout: 60000 });

    // If cookie is provided and no profile is used, inject it and reload
    if (cookie && !profile) {
      try {
        await page.evaluate((c) => { document.cookie = c; }, cookie);
        await page.reload({ waitUntil: 'networkidle', timeout: 60000 });
      } catch (e) {
        // non-fatal
      }
    }

    // Wait for the app to be interactive
    await page.waitForTimeout(2000);

    // Attempt to find a message input (contenteditable or textarea)
    let inputHandle = null;
    const selectors = [
      'textarea',
      'input[type="text"]',
      'div[contenteditable="true"]',
      'form textarea',
    ];
    for (const s of selectors) {
      try {
        const h = await page.$(s);
        if (h) { inputHandle = h; break; }
      } catch (e) {}
    }

    if (inputHandle) {
      try {
        await inputHandle.fill('');
        await inputHandle.type(prompt, { delay: 20 });
        await inputHandle.press('Enter');
      } catch (e) {
        // ignore
      }
    } else {
      console.log('No obvious input element found. Please paste the prompt into the Midjourney UI and send it manually. Waiting for images...');
    }

    // Monitor network responses and DOM for image URLs
    const imageUrls = new Set();
    page.on('response', async (res) => {
      try {
        const ct = res.headers()['content-type'] || '';
        const url = res.url();
        if (ct.startsWith('image/') || url.endsWith('.png') || url.endsWith('.jpg') || url.includes('/_next/image')) {
          imageUrls.add(url);
        }
      } catch (e) {}
    });

    // Also poll DOM for <img> tags inside the app
    for (let i = 0; i < 60; i++) {
      const imgs = await page.$$eval('img', imgs => imgs.map(i => i.src).filter(Boolean));
      imgs.forEach(u => { if (u) imageUrls.add(u); });
      if (imageUrls.size >= 1) break;
      await page.waitForTimeout(3000);
    }

    if (imageUrls.size === 0) {
      console.error('No images detected within timeout.');
      await browser.close();
      process.exit(3);
    }

    // Download up to 4 images
    const urls = Array.from(imageUrls).slice(0, 4);
    const saved = [];
    for (let i = 0; i < urls.length; i++) {
      const u = urls[i];
      try {
        const resp = await page.request.get(u, { timeout: 30000 });
        if (!resp.ok()) continue;
        const buf = await resp.body();
        const ext = (u.split('.').pop().split('?')[0] || 'jpg').slice(0, 5);
        const file = path.join(outdir, `mj_${i}.${ext}`);
        fs.writeFileSync(file, buf);
        saved.push(file);
      } catch (e) {
        // continue
      }
    }

    try { if (browser) await browser.close(); } catch (_) {}
    try { if (context && context.close) await context.close(); } catch (_) {}

    if (saved.length === 0) {
      console.error('Failed to download images.');
      process.exit(4);
    }

    // Print JSON array of saved paths to stdout
    console.log(JSON.stringify(saved));
    process.exit(0);
  } catch (e) {
    try { await browser.close(); } catch (_) {}
    console.error('Generator error:', e && e.message ? e.message : e);
    process.exit(1);
  }
})();
