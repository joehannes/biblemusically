// Preset sites for the embedded Browser tab, plus first-draft injected-JS automation.
//
// URLs that depend on a running Kaggle tunnel (ACE-Step / HeartMuLa / ComfyUI) resolve from
// Settings; when no live URL is saved we fall back to the engine's Kaggle notebook page so the
// tab is still useful. Automation scripts are DOM-scripting drafts — these SPAs change their
// markup often, so selectors are defensive and may need tuning against the live sites.

// Google sign-in entry point. Signed-in visitors get redirected to myaccount.google.com,
// which is how the Browser's login flow detects an active Google session it can reuse for
// "Continue with Google" social login on the engines (all pages share one cookie store).
export const GOOGLE_LOGIN_URL = "https://accounts.google.com/";

export const KAGGLE_NOTEBOOK = {
  acestep: "https://www.kaggle.com/code/joehannes/biblemusically-acestep-server",
  heartmula: "https://www.kaggle.com/code/joehannes/biblemusically-heartmula-server",
  comfyui: "https://www.kaggle.com/code/joehannes/biblemusically-comfyui-server",
  flux: "https://www.kaggle.com/code/joehannes/biblemusically-flux-server",
};

// Build the site list, resolving dynamic engine URLs from the settings object.
export function buildSites(settings = {}) {
  const s = settings || {};
  return [
    {
      key: "youtube",
      label: "YouTube Studio",
      group: "Publishing",
      url: "https://studio.youtube.com/",
      // The channel manager / switcher, per request.
      altUrls: [
        { label: "Channel switcher", url: "https://www.youtube.com/channel_switcher" },
        { label: "Channel settings", url: "https://studio.youtube.com/channel/settings" },
      ],
      needsGoogleLogin: true,
      loginUrl: "https://accounts.google.com/ServiceLogin?continue=https://studio.youtube.com/",
    },
    {
      key: "suno",
      label: "Suno",
      group: "Audio engines",
      url: "https://suno.com/create",
      // Clerk-hosted sign-in page; offers "Continue with Google" among others.
      loginUrl: "https://accounts.suno.com/sign-in",
    },
    {
      key: "acestep",
      label: "ACE-Step",
      group: "Audio engines",
      url: (s.acestep_api_url && s.acestep_api_url.trim()) || KAGGLE_NOTEBOOK.acestep,
      // No browser login: the Start/Fetch flow authenticates via the kaggle CLI token. The "Open
      // notebook" button opens KAGGLE_NOTEBOOK in the system browser (Google sign-in works there)
      // only when you need the Kaggle web UI — e.g. to set a secret or free a stuck GPU session.
      cliAuth: true,
      loginUrl: KAGGLE_NOTEBOOK.acestep,
    },
    {
      key: "heartmula",
      label: "HeartMuLa",
      group: "Audio engines",
      url: (s.heartmula_api_url && s.heartmula_api_url.trim()) || KAGGLE_NOTEBOOK.heartmula,
      cliAuth: true,
      loginUrl: KAGGLE_NOTEBOOK.heartmula,
    },
    {
      key: "midjourney",
      label: "Midjourney",
      group: "Image engines",
      url: "https://www.midjourney.com/imagine",
      // Redirects into MJ's auth flow (Google / Discord options).
      loginUrl: "https://www.midjourney.com/login",
    },
    {
      key: "comfyui",
      label: "ComfyUI",
      group: "Image engines",
      url: (s.comfyui_api_url && s.comfyui_api_url.trim()) || KAGGLE_NOTEBOOK.comfyui,
      cliAuth: true,
      loginUrl: KAGGLE_NOTEBOOK.comfyui,
    },
    {
      key: "flux",
      label: "FLUX.1",
      group: "Image engines",
      url: (s.flux_api_url && s.flux_api_url.trim()) || KAGGLE_NOTEBOOK.flux,
      cliAuth: true,
      loginUrl: KAGGLE_NOTEBOOK.flux,
    },
  ];
}

// ── Injected automation (first drafts) ────────────────────────────────────────────
// Each returns a JS source string to eval() inside the webview. They post progress/results
// back through window.__bmScrape(key, data) so the app can pick them up via webviewTakeScrape.

// Suno: fast readiness probe, run *before* the (slow, up-to-20s) compose script. Distinguishes
// three situations a caller needs to react to differently, which sunoComposeScript's own single
// "no lyrics field" timeout used to collapse into one dead end:
//   • 'auth-wall'    — the page is showing a sign-in prompt; go run the login flow, don't retry.
//   • 'not-rendered' — the SPA hasn't hydrated the create form yet (slow load, stale bfcache
//                      entry, or a one-off render glitch); worth a page reload + retry.
//   • 'ready'        — either the Custom-mode toggle or a lyrics-ish field already exists.
// Deliberately short (default 6s) so a caller can retry/reload a few times within the budget a
// human would tolerate, rather than sinking a full 20s into every failed attempt.
export function sunoReadyProbeScript({ timeoutMs = 6000 } = {}) {
  const j = JSON.stringify;
  return `(() => {
    const KEY = 'suno:ready';
    const TIMEOUT = ${timeoutMs};
    const started = Date.now();
    let finished = false;
    const done = (p) => { if (finished) return; finished = true; try { window.__bmScrape(KEY, Object.assign({ url: location.href }, p)); } catch (e) {} };

    const visible = (el) => { const r = el.getBoundingClientRect(); return r.width > 1 && r.height > 1; };
    const authWall = () => {
      const txt = (document.body && document.body.innerText || '').slice(0, 3000).toLowerCase();
      return /sign in|log in|create your account|couldn.t sign you in/.test(txt);
    };
    const customBtn = () => [...document.querySelectorAll('button, [role="tab"], [role="switch"], [role="radio"]')]
      .filter(visible).find(b => /^custom(\\s*mode)?$/i.test((b.textContent || '').trim())) || null;
    const anyLyricsHint = () => [...document.querySelectorAll('textarea, [contenteditable="true"], [contenteditable=""]')]
      .filter(visible).some(el => /lyric/i.test((el.getAttribute('placeholder') || '') + ' ' + (el.getAttribute('aria-label') || '') + ' ' + (el.getAttribute('data-testid') || '')));

    const tick = () => {
      if (finished) return;
      if (customBtn() || anyLyricsHint()) return done({ state: 'ready' });
      if (authWall()) return done({ state: 'auth-wall' });
      if (Date.now() - started > TIMEOUT) return done({ state: 'not-rendered' });
      setTimeout(tick, 400);
    };
    tick();
    return true;
  })();`;
}

// Suno: best-effort engine/model-version picker. Suno's create page exposes a small
// button/menu (near the mode toggle) that opens a list of model versions with the newest one
// listed first — this opens it and clicks the first enabled option. Purely additive: if no such
// control is found (Suno changed the markup, or there's nothing to pick), it reports
// `found: false` rather than failing the run — advanced-mode + field-filling still proceed.
//
// NOT run automatically by the generation pipeline (see genPipeline.js): on a free/unpaid Suno
// plan the newest version is locked, so "click the first option" opens the picker and then has
// nothing clickable to select — the menu stays open, the run never reaches Create, and every
// later step (fill/submit) starts failing against a page with a stuck dropdown overlaid on it.
// Left here only as an opt-in Macro Manager action for paid plans that actually want it.
export function sunoEngineSelectScript() {
  return `(() => {
    const KEY = 'suno:engine';
    const visible = (el) => { const r = el.getBoundingClientRect(); return r.width > 1 && r.height > 1; };
    const label = (el) => ((el.textContent || '') + ' ' + (el.getAttribute('aria-label') || '')).trim();

    const trigger = [...document.querySelectorAll('button, [role="button"]')]
      .filter(visible)
      .find(b => /^v\\d/i.test(label(b).trim()) || /model|version/i.test(b.getAttribute('aria-label') || ''));
    if (!trigger) { window.__bmScrape(KEY, { found: false, reason: 'no model/version control on the page' }); return false; }

    trigger.click();
    setTimeout(() => {
      const items = [...document.querySelectorAll('[role="menuitem"], [role="option"], li, button')]
        .filter(el => visible(el) && el !== trigger && /^v\\d/i.test(label(el).trim()));
      if (!items.length) {
        window.__bmScrape(KEY, { found: true, picked: false, reason: 'opened the picker but found no version options' });
        return;
      }
      items[0].click();
      window.__bmScrape(KEY, { found: true, picked: true, label: label(items[0]).slice(0, 40) });
    }, 400);
    return true;
  })();`;
}

// Suno: switch the create page into Custom mode, fill lyrics/style/title, and report what
// happened. Suno's create page is a client-rendered SPA whose fields are a mix of plain
// <textarea>s and rich contenteditable editors depending on the release and on which mode is
// active, so this:
//
//   • polls until the fields exist rather than reading the DOM once — the page routinely
//     needs a few seconds after load (and after the Custom toggle) to render them;
//   • matches fields on every label-ish hint it can reach (placeholder, aria-label,
//     data-testid, aria-labelledby, data-placeholder, and short preceding label text),
//     because none of them alone survives a Suno redesign;
//   • writes through the path each field type actually listens to — React's native value
//     setter for inputs, execCommand('insertText') for contenteditable, since assigning
//     .textContent on a Lexical/ProseMirror editor is silently reverted;
//   • verifies the value stuck by reading it back, and on failure returns the fields it *did*
//     find, so a broken run says what the page looked like instead of just "not found".
export function sunoComposeScript({ lyrics = "", style = "", title = "" }, { timeoutMs = 20000 } = {}) {
  const j = JSON.stringify;
  return `(() => {
    const KEY = 'suno:compose';
    const TIMEOUT = ${timeoutMs};
    const LYRICS = ${j(lyrics)}, STYLE = ${j(style)}, TITLE = ${j(title)};
    let finished = false;
    const done = (p) => {
      if (finished) return; finished = true;
      try { window.__bmScrape(KEY, Object.assign({ url: location.href }, p)); } catch (e) {}
    };

    const visible = (el) => {
      const r = el.getBoundingClientRect();
      if (r.width < 2 || r.height < 2) return false;
      const st = getComputedStyle(el);
      return st.visibility !== 'hidden' && st.display !== 'none' && st.opacity !== '0';
    };

    const isCE = (el) => el.isContentEditable;

    const fields = () => [...document.querySelectorAll(
      'textarea, input[type=text], input:not([type]), [contenteditable=""], [contenteditable="true"]'
    )].filter(visible);

    // Every string on/around a field that might say what it's for.
    const hint = (el) => {
      const bits = [
        el.getAttribute('placeholder'), el.getAttribute('aria-label'),
        el.getAttribute('data-testid'), el.getAttribute('name'), el.id,
        el.getAttribute('data-placeholder'),
      ];
      const by = el.getAttribute('aria-labelledby');
      if (by) by.split(/\\s+/).forEach(id => { const l = document.getElementById(id); if (l) bits.push(l.innerText); });
      // Rich editors hang their placeholder on a sibling node rather than the editor itself.
      let box = el.parentElement, hops = 0;
      while (box && hops++ < 3) {
        bits.push(box.getAttribute('aria-label'), box.getAttribute('data-testid'));
        [...box.children].forEach(c => {
          if (c !== el && !c.contains(el)) {
            const t = (c.innerText || '').trim();
            if (t && t.length < 40) bits.push(t);
          }
        });
        box = box.parentElement;
      }
      return bits.filter(Boolean).join(' ').toLowerCase();
    };

    const area = (el) => { const r = el.getBoundingClientRect(); return r.width * r.height; };

    // Best hint match; ties broken by the bigger field (lyrics boxes are the large ones).
    const pick = (fs, re, exclude) => fs
      .filter(f => !(exclude || []).includes(f) && re.test(hint(f)))
      .sort((a, b) => area(b) - area(a))[0] || null;

    const setNative = (el, val) => {
      const proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      const setter = Object.getOwnPropertyDescriptor(proto, 'value') && Object.getOwnPropertyDescriptor(proto, 'value').set;
      el.focus();
      if (setter) setter.call(el, val); else el.value = val;
      // React tracks the last value it wrote; the native setter above bypasses that, so these
      // events are what make it re-read the field.
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
      return el.value === val;
    };

    const setCE = (el, val) => {
      el.focus();
      const sel = window.getSelection();
      const range = document.createRange();
      range.selectNodeContents(el);
      sel.removeAllRanges(); sel.addRange(range);
      let ok = false;
      // Goes through the editor's own beforeinput/input pipeline, which is the only write a
      // Lexical/ProseMirror/Draft instance will keep.
      try { ok = document.execCommand('insertText', false, val); } catch (e) {}
      if (!ok) {
        el.textContent = val;
        el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: val }));
      }
      return (el.innerText || el.textContent || '').replace(/\\s+/g, ' ').trim().length > 0;
    };

    const setVal = (el, val) => { try { return isCE(el) ? setCE(el, val) : setNative(el, val); } catch (e) { return false; } };
    const readVal = (el) => (isCE(el) ? (el.innerText || '') : (el.value || ''));

    // What we saw, for the failure report.
    const diag = (el) => ({
      tag: el.tagName.toLowerCase(), editable: isCE(el),
      hint: hint(el).slice(0, 120), chars: readVal(el).length,
    });

    // Lyrics only exist in Custom mode; Simple mode instead shows a "Describe your song" box,
    // which is the one field we must never mistake for it.
    const customBtn = () => [...document.querySelectorAll('button, [role="tab"], [role="switch"], [role="radio"]')]
      .filter(visible).find(b => /^custom(\\s*mode)?$/i.test((b.textContent || '').trim())) || null;

    const customOn = (btn) => !!btn && (
      btn.getAttribute('aria-selected') === 'true'
      || btn.getAttribute('aria-checked') === 'true'
      || /^(active|checked|on)$/.test(btn.getAttribute('data-state') || '')
    );

    // Returns true when it just clicked, so the caller can let the re-render happen before
    // reading the DOM — Suno swaps the pane a beat after the toggle, and picking fields on the
    // same tick sees the *old* mode's fields.
    let toggled = false;
    const enableCustom = () => {
      if (toggled) return false;
      const btn = customBtn();
      if (!btn) return false;
      toggled = true;
      if (customOn(btn)) return false;
      btn.click();
      return true;
    };

    // Fields that announce themselves as something other than lyrics. The Simple-mode
    // "Describe your song" textarea is large and editable, so without this the last-resort
    // "biggest editor" pick lands on it and silently fills the wrong box.
    const NOT_LYRICS = /describe|style|genre|vibe|title|name|search|prompt|persona/;

    const started = Date.now();
    let clickedAt = 0;
    const FALLBACK_AFTER = 8000; // only guess at the field once the page has really settled

    const tick = () => {
      if (finished) return;
      if (enableCustom()) { clickedAt = Date.now(); return setTimeout(tick, 500); }

      const fs = fields();
      const elapsed = Date.now() - started;
      // Named match first — this is the only pick we trust immediately.
      let lyricsEl = pick(fs, /lyric/) || pick(fs, /\\bverse\\b|\\bchorus\\b/);
      if (!lyricsEl && elapsed > FALLBACK_AFTER && Date.now() - clickedAt > 2000) {
        lyricsEl = fs
          .filter(f => (isCE(f) || f.tagName === 'TEXTAREA') && !NOT_LYRICS.test(hint(f)))
          .sort((a, b) => area(b) - area(a))[0] || null;
      }

      if (!lyricsEl) {
        if (elapsed < TIMEOUT) return setTimeout(tick, 400);
        const btn = customBtn();
        return done({
          filledLyrics: false, filledStyle: false,
          reason: btn && !customOn(btn)
            ? 'the page is still in Simple mode — the Custom tab did not switch'
            : 'no lyrics field on the page (signed out, or the create form never rendered)',
          customMode: btn ? (customOn(btn) ? 'on' : 'off') : 'no toggle found',
          candidates: fs.map(diag),
        });
      }

      // Tiered so a bare "describe…" box can never outrank a real style field.
      const styleEl = pick(fs, /style|genre/, [lyricsEl]) || pick(fs, /vibe|describe the|sound of/, [lyricsEl]);
      const titleEl = pick(fs, /title/, [lyricsEl, styleEl].filter(Boolean));

      const okL = setVal(lyricsEl, LYRICS);
      const okS = styleEl && STYLE ? setVal(styleEl, STYLE) : false;
      if (titleEl && TITLE) setVal(titleEl, TITLE);

      // The editor may need a frame to commit before the read-back is meaningful.
      setTimeout(() => done({
        filledLyrics: okL && readVal(lyricsEl).length > 0,
        filledStyle: okS,
        lyricsField: diag(lyricsEl),
        styleField: styleEl ? diag(styleEl) : null,
        titleField: titleEl ? diag(titleEl) : null,
      }), 250);
    };
    tick();
    return true;
  })();`;
}

// Suno: click the Create/Compose button once fields are filled. The button carries an icon and
// a credit count alongside its label and is disabled until the form validates, so match on the
// verb appearing near the start rather than anchoring to position 0 (a leading credit badge or
// icon's accessible text can push it a few characters in) and skip disabled buttons.
//
// A bare `btn.click()` used to be trusted blindly: it always reports success even when Suno's
// own handlers never actually reacted to it (a debounced validity check that hadn't caught up
// with the just-typed fields yet, or the click landing on a node mid-transition) — which is how
// this used to report "submitted" and send the pipeline off to poll a generation that never
// started. This now (1) dispatches a fuller pointer/mouse event sequence, closer to what a real
// click produces, since some component libraries key off pointerdown/up rather than just click;
// (2) re-checks ~1.2s later for a visible sign the click actually landed (the button going
// disabled, its label changing, or a "generating/please wait" hint appearing on the page) and
// retries up to twice if nothing changed; (3) only reports `clicked: true` once it either sees
// that sign or has genuinely run out of retries — `verified` distinguishes the two so a caller
// can still proceed on the (rarer) case where Suno gives no visible feedback at all.
export function sunoSubmitScript() {
  return `(() => {
    const KEY = 'suno:submit';
    const visible = (el) => {
      const r = el.getBoundingClientRect();
      if (r.width < 2 || r.height < 2) return false;
      const st = getComputedStyle(el);
      return st.visibility !== 'hidden' && st.display !== 'none' && st.opacity !== '0';
    };
    const enabled = (b) => !b.disabled && b.getAttribute('aria-disabled') !== 'true';
    const label = (b) => ((b.textContent || '') + ' ' + (b.getAttribute('aria-label') || '')).trim();
    const VERB = /\\b(create|compose|generate)\\b/i;
    const candidates = () => [...document.querySelectorAll('button, [role="button"]')].filter(visible);
    // Only look at the first ~30 chars so an unrelated button elsewhere on the page whose text
    // happens to contain one of these words later on doesn't match.
    const findBtn = (btns) => btns.find((b) => VERB.test(label(b).slice(0, 30)));

    const clickHard = (el) => {
      try { el.scrollIntoView({ block: 'center', inline: 'center' }); } catch (e) {}
      const mouse = (type) => el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, view: window }));
      const pointer = (type) => { try { el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, pointerId: 1, isPrimary: true })); } catch (e) {} };
      pointer('pointerdown'); mouse('mousedown');
      pointer('pointerup'); mouse('mouseup');
      if (typeof el.click === 'function') el.click(); else mouse('click');
    };

    const attempt = (n) => {
      const btns = candidates();
      const btn = findBtn(btns);
      if (!btn) {
        window.__bmScrape(KEY, { clicked: false, reason: 'no Create/Compose button on the page', buttons: btns.slice(0, 15).map((b) => label(b).slice(0, 30)).filter(Boolean) });
        return;
      }
      if (!enabled(btn)) {
        // Suno may still be validating the just-typed fields (async moderation/style checks) —
        // worth a couple of short retries before treating "disabled" as final.
        if (n < 3) { setTimeout(() => attempt(n + 1), 700); return; }
        window.__bmScrape(KEY, { clicked: false, reason: 'the Create button is disabled — Suno is rejecting the form (out of credits, a title that is too long/invalid, or a field it still wants)', label: label(btn).slice(0, 60) });
        return;
      }

      clickHard(btn);
      const clickedLabel = label(btn).slice(0, 60);
      setTimeout(() => {
        const stillBtns = candidates();
        const still = findBtn(stillBtns);
        const sameStillClickable = !!still && enabled(still) && label(still).slice(0, 60) === clickedLabel;
        const disabledNow = !!still && !enabled(still);
        const busyHint = /generating|please wait|creating your song|working on it/i.test((document.body.innerText || '').slice(0, 3000));
        if (sameStillClickable && !disabledNow && !busyHint && n < 2) {
          attempt(n + 1); // the click didn't visibly do anything yet — try once more
          return;
        }
        window.__bmScrape(KEY, {
          clicked: true, label: clickedLabel, attempts: n + 1,
          verified: disabledNow || busyHint || !still,
        });
      }, 1200);
    };

    attempt(0);
    return true;
  })();`;
}

// Suno finished clips render as an <audio src> (or a download <a href>) pointing at Suno's CDN.
const SUNO_AUDIO_RE = "/\\.mp3(\\?|$)/i.test(s) || /cdn\\d*\\.suno\\.(ai|com)\\//i.test(s)";

// Suno: snapshot every finished-clip audio URL currently on the page. Called right before
// Create is clicked so runSunoAutomation can tell "the clip that just finished" apart from
// older clips already sitting in the page's history list.
export function sunoAudioSnapshotScript() {
  return `(() => {
    const isAudio = (s) => (${SUNO_AUDIO_RE});
    const urls = new Set();
    document.querySelectorAll('audio[src], audio source[src]').forEach(el => {
      const src = el.src || el.getAttribute('src');
      if (src && isAudio(src)) urls.add(src);
    });
    document.querySelectorAll('a[href]').forEach(a => { if (a.href && isAudio(a.href)) urls.add(a.href); });
    const list = [...urls];
    window.__bmScrape('suno:snapshot', list);
    return list.length;
  })();`;
}

// Suno: single-shot scan for clip(s) whose audio URL wasn't in `knownUrls` (pass [] to just
// grab whatever's already finished) — no internal loop, no internal timeout, reports whatever
// it sees right now and returns. genPipeline.js's pollForClips calls this fresh on every 10s
// tick instead of firing one long-lived in-page polling loop (see sunoScrapeResultScript below)
// and waiting on it: a loop like that lives in the page's JS context, and Suno occasionally
// navigates the create page (e.g. once a clip is queued) partway through a run, which silently
// kills that context — and with it every future __bmScrape call the loop would have made — so
// the app-side poll never gets an answer and eventually times out even though Suno finished
// the clip minutes ago. A single-shot script re-injected every tick has no state to lose: if a
// tick lands mid-navigation it just comes back empty and the next tick tries again.
export function sunoScrapeOnceScript(knownUrls = []) {
  const j = JSON.stringify;
  return `(() => {
    const KEY = 'suno:scan';
    const KNOWN = new Set(${j(knownUrls)});
    const isAudio = (s) => (${SUNO_AUDIO_RE});
    const out = [];
    const seen = new Set();
    document.querySelectorAll('audio[src], audio source[src]').forEach((el) => {
      const src = el.src || el.getAttribute('src');
      if (!src || !isAudio(src) || KNOWN.has(src) || seen.has(src)) return;
      seen.add(src);
      const audioEl = el.tagName === 'AUDIO' ? el : el.closest('audio');
      const duration = audioEl && isFinite(audioEl.duration) && audioEl.duration > 0 ? audioEl.duration : null;
      out.push({ audioUrl: src, duration });
    });
    if (!out.length) {
      document.querySelectorAll('a[href]').forEach((a) => {
        const h = a.href;
        if (h && isAudio(h) && !KNOWN.has(h) && !seen.has(h)) { seen.add(h); out.push({ audioUrl: h, duration: null }); }
      });
    }
    // A failure banner means the run is over — worth reporting so the caller can stop
    // polling instead of waiting out the full budget on a generation that already died.
    const bodyTxt = (document.body && document.body.innerText || '').slice(0, 4000).toLowerCase();
    const failed = /failed to generate|generation failed|something went wrong|out of credits|insufficient credits/.test(bodyTxt);
    window.__bmScrape(KEY, { clips: out, failed, url: location.href });
    return out.length;
  })();`;
}

// Suno: poll the page for clip(s) whose audio URL wasn't in `knownUrls` (pass [] to just grab
// whatever's already finished). Suno clips usually render in 30-90s; the internal poll loop
// below keeps running in the page for up to `timeoutMs` (default ~3.5min) regardless of how the
// caller times its own evalAndTake polling, the same pattern sunoComposeScript uses. Suno
// generates two clips per run, so this collects up to `wantCount` (default 2) new ones before
// reporting — `clips: [{audioUrl, duration}, …]`, newest first — while keeping `audioUrl`/
// `duration` at the top level (the first clip) for any caller that only wants one.
export function sunoScrapeResultScript(knownUrls = [], { timeoutMs = 210000, wantCount = 2 } = {}) {
  const j = JSON.stringify;
  return `(() => {
    const KEY = 'suno:result';
    const KNOWN = new Set(${j(knownUrls)});
    const TIMEOUT = ${timeoutMs};
    const WANT = ${wantCount};
    const started = Date.now();
    let finished = false;
    const done = (p) => { if (finished) return; finished = true; try { window.__bmScrape(KEY, p); } catch (e) {} };
    const isAudio = (s) => (${SUNO_AUDIO_RE});

    const findAll = () => {
      const out = [];
      const seen = new Set();
      const els = [...document.querySelectorAll('audio[src], audio source[src]')];
      for (const el of els) {
        const src = el.src || el.getAttribute('src');
        if (!src || !isAudio(src) || KNOWN.has(src) || seen.has(src)) continue;
        seen.add(src);
        const audioEl = el.tagName === 'AUDIO' ? el : el.closest('audio');
        const duration = audioEl && isFinite(audioEl.duration) && audioEl.duration > 0 ? audioEl.duration : null;
        out.push({ audioUrl: src, duration });
      }
      if (out.length < WANT) {
        for (const a of document.querySelectorAll('a[href]')) {
          const h = a.href;
          if (h && isAudio(h) && !KNOWN.has(h) && !seen.has(h)) { seen.add(h); out.push({ audioUrl: h, duration: null }); }
        }
      }
      return out;
    };

    const tick = () => {
      if (finished) return;
      const found = findAll();
      if (found.length >= WANT || (found.length > 0 && Date.now() - started > TIMEOUT * 0.6)) {
        return done({ clips: found, audioUrl: found[0].audioUrl, duration: found[0].duration });
      }
      if (Date.now() - started > TIMEOUT) {
        if (found.length) return done({ clips: found, audioUrl: found[0].audioUrl, duration: found[0].duration });
        return done({ clips: [], audioUrl: null, reason: 'timed out waiting for a finished clip — Suno may still be rendering, or the run failed. Click "Capture result" once it finishes in the webview.' });
      }
      setTimeout(tick, 3000);
    };
    tick();
    return true;
  })();`;
}

// Midjourney: fill the /imagine prompt bar and submit (Enter). MJ's web app renders the
// prompt box as an input/textarea inside the imagine bar; selectors are defensive.
export function midjourneyImagineScript(prompt) {
  const j = JSON.stringify;
  return `(() => {
    const setVal = (el, val) => {
      const proto = el.tagName === 'TEXTAREA' ? window.HTMLTextAreaElement.prototype : window.HTMLInputElement.prototype;
      const setter = Object.getOwnPropertyDescriptor(proto, 'value')?.set;
      if (setter) setter.call(el, val); else el.value = val;
      el.dispatchEvent(new Event('input', { bubbles: true }));
    };
    const el = document.querySelector('#desktop_input_bar, [id*=imagine] input, [id*=imagine] textarea')
      || [...document.querySelectorAll('input[type=text], textarea')].find(f => /imagine|prompt/i.test((f.placeholder || '') + (f.getAttribute('aria-label') || '')));
    if (!el) { window.__bmScrape('midjourney:imagine', { ok: false, reason: 'prompt bar not found (logged out?)' }); return false; }
    el.focus();
    setVal(el, ${j(prompt)});
    setTimeout(() => {
      const opts = { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true };
      el.dispatchEvent(new KeyboardEvent('keydown', opts));
      el.dispatchEvent(new KeyboardEvent('keyup', opts));
      window.__bmScrape('midjourney:imagine', { ok: true, submitted: true });
    }, 250);
    return true;
  })();`;
}

// Midjourney: scrape currently visible generated image URLs (newest jobs first on /imagine).
export function midjourneyScrapeJobsScript() {
  return `(() => {
    const urls = [...document.querySelectorAll('img')]
      .map(i => i.src).filter(s => /cdn\\.midjourney\\.com/.test(s)).slice(0, 40);
    window.__bmScrape('midjourney:jobs', { count: urls.length, urls });
    return urls.length;
  })();`;
}

// Shared by the channel-switcher scripts below: YouTube's account-menu UI is built from
// Polymer/lit custom elements, each hosting its own (open) shadow root, several layers deep —
// a plain `document.querySelectorAll` only ever sees the outermost light DOM and never reaches
// the actual channel-card elements, which is why a selector that looks right in devtools
// (inspecting one specific element) still returns nothing from a script. This walks every
// shadow root recursively so `sel` is matched wherever it actually lives.
const DEEP_QUERY_JS = `
    function deepAll(sel, root) {
      root = root || document;
      var out = [];
      try { out = out.concat([].slice.call(root.querySelectorAll(sel))); } catch (e) {}
      var all = [];
      try { all = [].slice.call(root.querySelectorAll('*')); } catch (e) {}
      for (var i = 0; i < all.length; i++) {
        if (all[i].shadowRoot) out = out.concat(deepAll(sel, all[i].shadowRoot));
      }
      return out;
    }
`;

// YouTube channel switcher (youtube.com/channel_switcher): scrape every channel card's @handle.
// Matches on href (a card's link is normally `/@handle`) with a text-scan fallback for cards
// that render the handle as plain text instead, since the account-menu markup shifts across
// YouTube releases and pinning to one custom-element tag name breaks the moment it's renamed.
export function youtubeListChannelsScript() {
  return `(() => {
    ${DEEP_QUERY_JS}
    const HANDLE_RE = /@[a-zA-Z0-9._-]{2,30}/;
    const found = new Map(); // handle -> { handle, title, href }

    const anchors = deepAll('a[href]');
    for (const a of anchors) {
      let path = '';
      try { path = new URL(a.getAttribute('href'), location.href).pathname; } catch (e) { continue; }
      const m = path.match(/^\\/(@[a-zA-Z0-9._-]{2,30})/);
      if (!m) continue;
      const handle = m[1];
      if (found.has(handle)) continue;
      const lines = (a.innerText || a.textContent || '').split('\\n').map(s => s.trim()).filter(Boolean);
      const title = lines.find(s => s !== handle && !HANDLE_RE.test(s)) || '';
      found.set(handle, { handle: handle, title: title.slice(0, 80), href: a.href || ('https://www.youtube.com' + path) });
    }

    // Fallback: a card that renders its handle as plain text (no href) next to an avatar.
    if (!found.size) {
      const nodes = deepAll('*');
      for (const el of nodes) {
        const own = [].slice.call(el.childNodes || []).some(n => n.nodeType === 3 && n.textContent.trim());
        if (!own) continue;
        const matches = (el.textContent || '').match(new RegExp(HANDLE_RE.source, 'g')) || [];
        for (const h of matches) if (!found.has(h)) found.set(h, { handle: h, title: '', href: '' });
      }
    }

    const channels = [...found.values()].slice(0, 50);
    window.__bmScrape('youtube:channels', { count: channels.length, channels, url: location.href });
    return channels.length;
  })();`;
}

// YouTube channel switcher: click the channel whose name or @handle matches (case-insensitive
// substring). Pierces shadow roots the same way youtubeListChannelsScript does.
export function youtubeSwitchChannelScript(name) {
  const j = JSON.stringify;
  return `(() => {
    ${DEEP_QUERY_JS}
    const want = ${j(name)}.toLowerCase().replace(/^@/, '');
    const nodes = deepAll('a[href*="/@"], a[href*="/channel/"], ytd-account-item-renderer');
    const hit = nodes.find(el => (el.innerText || el.href || '').toLowerCase().includes(want));
    if (hit) { (hit.tagName === 'A' ? hit : (hit.querySelector('a') || hit)).click(); window.__bmScrape('youtube:switch', { ok: true, clicked: want }); return true; }
    window.__bmScrape('youtube:switch', { ok: false, reason: 'channel not found', wanted: want });
    return false;
  })();`;
}

// Engine tunnels (ACE-Step / HeartMuLa / ComfyUI): same-origin health probe from inside the page.
export function engineHealthScript(siteKey) {
  const j = JSON.stringify;
  const probe = siteKey === "comfyui"
    ? `fetch('/system_stats').then(r => r.json())`
    : `fetch('/query_result', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{"task_id_list":[]}' }).then(r => r.json())`;
  return `(() => {
    const key = ${j(siteKey)} + ':health';
    ${probe}
      .then(data => window.__bmScrape(key, { ok: true, data }))
      .catch(err => window.__bmScrape(key, { ok: false, error: String(err), title: document.title }));
    return true;
  })();`;
}

// Generic: report whether the page looks logged-in (best-effort, per-site heuristics).
export function loginProbeScript(siteKey) {
  const j = JSON.stringify;
  return `(() => {
    const key = ${j(siteKey)};
    const txt = document.body ? document.body.innerText.slice(0, 4000) : '';
    const loggedOut = /sign in|log in|create account|couldn.t sign you in|not secure/i.test(txt);
    window.__bmScrape(key + ':login', { url: location.href, loggedOutHint: loggedOut, title: document.title });
    return !loggedOut;
  })();`;
}
