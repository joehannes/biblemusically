//! Embedded-browser webviews (Tauri unstable multiwebview) + AI macro recorder.
//!
//! We host *native* child webviews inside the main window — not iframes, because
//! YouTube/Suno/Google all send `X-Frame-Options: DENY`. The React "Browser" tab renders a
//! transparent placeholder, measures its on-screen rect, and asks us to position the active
//! child webview over exactly that region (and to hide it when you navigate away).
//!
//! Pages: each open "page" (tab) is its own child webview, keyed by a caller-chosen page id.
//! Only one page is visible at a time; the rest stay hidden but keep their session/DOM state,
//! so switching pages is instant and logins survive.
//!
//! Automation model (Playwright can't drive WebKitGTK): we `eval()` JavaScript into the live
//! webview to click/fill/submit, and injected scripts report scraped data back to Rust through
//! `window.webkit.messageHandlers.bmChannel` — a native WebKit script-message-handler bridge
//! (`install_bridge` below), *not* a network request. This matters because sites like YouTube
//! ship a strict `connect-src` CSP that silently blocks `fetch()`/`XHR` to `http://127.0.0.1`
//! from injected page scripts (they run in the page's own JS world, so they're bound by its
//! CSP just like any other in-page script) — the old design POSTed to a local warp server on
//! 127.0.0.1:3337 and worked on permissive sites but went silently, invisibly dark on YouTube:
//! no network error, no console log the app could see, just zero steps ever arriving. A script
//! message handler is a native GObject signal, not a subresource fetch, so CSP has no say over
//! it. The warp server (still below, on 127.0.0.1:3337) remains for the few endpoints that
//! don't need to survive a strict-CSP page (e.g. the standalone Suno-cookie capture route) and
//! as a fallback path in `init_script`'s `bridgeSend` if the message handler is ever missing.
//! A spoofed Chrome UA is set so Google login is less likely to reject the embedded webview as
//! "not secure".
//!
//! Macro recorder: while `MacroState.recording` is on, an init-script recorder in every page
//! captures clicks / context-menus / pastes / field edits / Enter presses and sends one step
//! per action over the bmChannel bridge. Each step carries a *positional* description of its
//! target (stable attribute selector when available, plus "item N of list L" context computed
//! from repeated siblings) so playback can re-resolve elements by position — e.g. "the newest
//! item in the downloads list" — rather than by a stale name. Rather than have the recorder
//! poll a `/macro/state` endpoint (same CSP problem as above), Rust re-arms it directly: every
//! page's `on_page_load` handler evals `__bmArmRecorder()` on `PageLoadEvent::Finished` whenever
//! a recording is in progress, so a recording survives full navigations without any network
//! round trip.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tauri::webview::PageLoadEvent;
use tauri::{command, AppHandle, LogicalPosition, LogicalSize, Manager, Webview, WebviewBuilder, WebviewUrl, Wry};

#[cfg(target_os = "linux")]
// WebKitGTK traits for the bmChannel bridge and the download hook. Both call sites are already
// `cfg(target_os = "linux")`; gating the import too is what lets the crate compile for Android,
// macOS and Windows at all — the dependency itself is Linux-only in Cargo.toml, so an
// unconditional `use` was a hard compile error everywhere else.
#[cfg(target_os = "linux")]
use webkit2gtk::{DownloadExt as _, UserContentManagerExt as _, WebContextExt as _, WebViewExt as _};

pub const BROWSER_LABEL_PREFIX: &str = "embedded-browser";

const CHROME_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";

/// Recording state + collected steps, shared with the local HTTP server so injected
/// recorder scripts can append steps and query whether a recording is live.
#[derive(Default)]
pub struct MacroState {
    pub recording: AtomicBool,
    pub steps: Mutex<Vec<Value>>,
}

/// A download the frontend has asked us to redirect into an app-chosen folder — set by
/// `webview_arm_download` right before a macro/pipeline step clicks whatever triggers it, and
/// consumed by the first `WebKitDownload` that starts afterwards (see `install_download_hook`).
pub struct PendingDownload {
    pub dir: PathBuf,
    pub filename: Option<String>,
    pub result_key: String,
}

/// Shared state: the live child webviews (created lazily, one per page id), which page is
/// visible, a bucket of scraped payloads delivered via the `/scrape` HTTP route, the macro
/// recorder state (shared with `/macro/*` routes), and the one in-flight armed-download slot.
#[derive(Default)]
pub struct WebviewManager {
    pub pages: Mutex<HashMap<String, Webview<Wry>>>,
    pub active: Mutex<Option<String>>,
    pub scrape: Arc<Mutex<HashMap<String, Value>>>,
    pub macros: Arc<MacroState>,
    pub pending_download: Arc<Mutex<Option<PendingDownload>>>,
    // WebKit's download-started signal fires on the (process-wide, shared-by-every-page)
    // WebContext, not per-webview — hooked once, lazily, off whichever page opens first.
    pub download_hooked: Arc<AtomicBool>,
}

impl WebviewManager {
    /// A live webview handle for cookie-store access — all pages share one cookie store,
    /// so any handle works: the active page if set, else any open page.
    pub fn primary_webview(&self) -> Result<Webview<Wry>, String> {
        let pages = self.pages.lock().map_err(lock_err)?;
        let active = self.active.lock().map_err(lock_err)?;
        if let Some(id) = active.as_ref() {
            if let Some(wv) = pages.get(id) {
                return Ok(wv.clone());
            }
        }
        pages
            .values()
            .next()
            .cloned()
            .ok_or_else(|| "no embedded browser page is open".to_string())
    }
}

fn lock_err<T>(e: std::sync::PoisonError<T>) -> String {
    format!("webview state lock poisoned: {}", e)
}

/// JS injected into every page any embedded webview loads.
///
/// Exposes `window.__bmScrape(key, data)` (ships JSON payloads back to Rust) and installs the
/// macro recorder. Both report through `bridgeSend`, which prefers the native `bmChannel`
/// message handler (see `install_bridge`) — immune to page CSP — and only falls back to the
/// old HTTP POST to 127.0.0.1:3337 if that handler somehow isn't present (non-WebKit builds).
/// The recorder re-arms itself on navigation via Rust's `on_page_load` hook calling
/// `__bmArmRecorder()` directly, rather than polling a `/macro/state` endpoint over the network.
fn init_script() -> &'static str {
    r##"
    (function(){
      if (window.__bmInitDone) return; window.__bmInitDone = true;
      var BASE = 'http://127.0.0.1:3337';
      function post(path, data){
        try {
          fetch(BASE + path, {
            method: 'POST', mode: 'cors', keepalive: true,
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(data)
          });
        } catch (e) { /* best-effort */ }
      }
      // Native WebKit script-message bridge to Rust — not a subresource fetch, so a page's
      // Content-Security-Policy (YouTube's `connect-src` blocks fetches to 127.0.0.1) can't
      // touch it. Falls back to the HTTP POST above only if the handler is unavailable.
      function bridgeSend(kind, payload){
        try {
          if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.bmChannel) {
            var msg = Object.assign({ kind: kind }, payload);
            window.webkit.messageHandlers.bmChannel.postMessage(JSON.stringify(msg));
            return true;
          }
        } catch (e) { /* fall through to HTTP */ }
        if (kind === 'scrape') post('/scrape', payload);
        else if (kind === 'macroStep') post('/macro/step', payload.step);
        return false;
      }
      window.__bmScrape = function(key, data){ bridgeSend('scrape', { key: String(key), data: data }); };

      // ── AJAX activity tracker ────────────────────────────────────────
      // Backs the "network goes quiet" macro wait condition (see macroRecorder.js): tracks
      // in-flight fetch()/XHR calls and the last time one started or settled, so a wait step
      // re-injected on every poll tick (itself stateless — see sunoScrapeOnceScript/
      // waitCheckScript for why) can still read state that accumulated across ticks. Lives on
      // `window` rather than being re-installed per poll so it keeps counting between checks.
      window.__bmNet = { inFlight: 0, lastActivity: Date.now() };
      (function(){
        var net = window.__bmNet;
        var origFetch = window.fetch;
        if (origFetch) {
          window.fetch = function(){
            net.inFlight++; net.lastActivity = Date.now();
            var settle = function(){ net.inFlight = Math.max(0, net.inFlight - 1); net.lastActivity = Date.now(); };
            var p;
            try { p = origFetch.apply(this, arguments); } catch (e) { settle(); throw e; }
            p.then(settle, settle);
            return p;
          };
        }
        var origOpen = window.XMLHttpRequest && XMLHttpRequest.prototype.open;
        var origSend = window.XMLHttpRequest && XMLHttpRequest.prototype.send;
        if (origOpen && origSend) {
          XMLHttpRequest.prototype.open = function(){ this.__bmTracked = true; return origOpen.apply(this, arguments); };
          XMLHttpRequest.prototype.send = function(){
            if (this.__bmTracked) {
              net.inFlight++; net.lastActivity = Date.now();
              var done = function(){ net.inFlight = Math.max(0, net.inFlight - 1); net.lastActivity = Date.now(); };
              this.addEventListener('loadend', done);
            }
            return origSend.apply(this, arguments);
          };
        }
      })();

      // ── Macro recorder ────────────────────────────────────────────────
      function cssEscape(s){ return (window.CSS && CSS.escape) ? CSS.escape(s) : String(s).replace(/([^a-zA-Z0-9_-])/g, '\\$1'); }

      // A short, stable attribute-based selector for an element, if it has one.
      function attrSel(el){
        if (el.id && !/\d{4,}|^:|^radix/.test(el.id)) return '#' + cssEscape(el.id);
        var attrs = ['data-testid', 'name', 'aria-label', 'placeholder', 'title', 'href'];
        for (var i = 0; i < attrs.length; i++) {
          var v = el.getAttribute && el.getAttribute(attrs[i]);
          if (v && v.length < 120) {
            return el.tagName.toLowerCase() + '[' + attrs[i] + '="' + v.replace(/\\/g, '\\\\').replace(/"/g, '\\"') + '"]';
          }
        }
        return null;
      }

      // nth-of-type path from `stopAt` (exclusive; defaults to body) down to `el`.
      // Bails out early onto an attribute anchor when an ancestor has one.
      function nthPath(el, stopAt){
        stopAt = stopAt || document.body;
        var parts = [], cur = el;
        while (cur && cur !== stopAt && cur !== document.documentElement && cur.nodeType === 1) {
          var q = attrSel(cur);
          if (q) { parts.unshift(q); return parts.join(' > '); }
          var idx = 1, sib = cur;
          while ((sib = sib.previousElementSibling)) { if (sib.tagName === cur.tagName) idx++; }
          parts.unshift(cur.tagName.toLowerCase() + ':nth-of-type(' + idx + ')');
          cur = cur.parentElement;
        }
        return parts.join(' > ');
      }

      // If `el` sits inside a repeated-sibling group (a list/grid of similar items), describe
      // it positionally: the list container, the item's index from start AND end, and the path
      // from the item down to `el`. Playback re-resolves by position, so "item 0" replays as
      // "the current first/newest item", not the specific old one.
      function listContext(el){
        var item = el;
        while (item && item !== document.body) {
          var p = item.parentElement;
          if (p) {
            var kids = Array.prototype.slice.call(p.children);
            var similar = kids.filter(function(c){ return c.tagName === item.tagName; });
            if (similar.length >= 3 && item.tagName !== 'DIV' || similar.length >= 4) {
              var index = similar.indexOf(item);
              if (index >= 0) {
                return {
                  listSelector: nthPath(p) || p.tagName.toLowerCase(),
                  itemTag: item.tagName.toLowerCase(),
                  index: index,
                  indexFromEnd: similar.length - 1 - index,
                  count: similar.length,
                  within: item === el ? '' : nthPath(el, item)
                };
              }
            }
          }
          item = item.parentElement;
        }
        return null;
      }

      function describe(el){
        var text = '';
        try { text = (el.innerText || el.value || '').trim().slice(0, 80); } catch (e) {}
        var d = {
          selector: attrSel(el),
          fallback: nthPath(el),
          tag: el.tagName ? el.tagName.toLowerCase() : '',
          text: text,
          list: listContext(el)
        };
        if (!d.selector && text && /^(button|a|summary)$/i.test(d.tag)) {
          d.byText = { tag: d.tag, text: text };
        }
        return d;
      }

      var armed = false;
      function record(step){
        // App-triggered insertions (paste-from-app-clipboard etc.) set this flag so their
        // synthetic input/change events don't get double-recorded as fill steps.
        if (window.__bmSuppressRecord && step.type !== 'nav') return;
        step.t = Date.now();
        step.url = location.href;
        bridgeSend('macroStep', { step: step });
      }

      function interactionTarget(t){
        if (!t || !t.closest) return t;
        return t.closest('a,button,[role="button"],[role="menuitem"],[role="option"],[role="tab"],input,textarea,select,summary,li,[onclick]') || t;
      }

      function arm(){
        if (armed) return; armed = true;
        record({ type: 'nav', href: location.href, title: document.title });
        document.addEventListener('click', function(e){
          var el = interactionTarget(e.target);
          record({ type: 'click', target: describe(el) });
        }, true);
        document.addEventListener('contextmenu', function(e){
          record({ type: 'contextmenu', target: describe(interactionTarget(e.target)) });
        }, true);
        document.addEventListener('paste', function(e){
          var el = document.activeElement || e.target;
          var txt = '';
          try { txt = (e.clipboardData || window.clipboardData).getData('text') || ''; } catch (err) {}
          record({ type: 'paste', target: describe(el), value: txt.slice(0, 4000), param: 'clipboard' });
        }, true);
        document.addEventListener('change', function(e){
          var el = e.target;
          if (el && /^(input|textarea|select)$/i.test(el.tagName)) {
            var v = (el.type === 'password') ? '' : String(el.value || '').slice(0, 4000);
            record({ type: 'fill', target: describe(el), value: v, param: 'text' });
          }
        }, true);
        document.addEventListener('keydown', function(e){
          if (e.key === 'Enter') {
            record({ type: 'enter', target: describe(document.activeElement || document.body) });
          }
        }, true);
      }
      // Called directly by Rust: once from `macro_start` for the page already on screen, and
      // again from every page's `on_page_load` (PageLoadEvent::Finished) hook while a
      // recording is in progress — that's what lets a recording span navigations without a
      // network round trip.
      window.__bmArmRecorder = arm;
      // Exposed so app-triggered special steps (paste-from-app-clipboard, connector slots)
      // can describe the focused element and append steps through the same pipeline.
      window.__bmDescribe = describe;
      window.__bmRecordStep = record;
    })();
    "##
}

fn parse_url(url: &str) -> Result<url::Url, String> {
    url::Url::parse(url).map_err(|e| format!("Invalid URL '{}': {}", url, e))
}

fn label_for(page_id: &str) -> String {
    let safe: String = page_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    format!("{}-{}", BROWSER_LABEL_PREFIX, safe)
}

fn page_id_or_default(page_id: Option<String>) -> String {
    match page_id {
        Some(id) if !id.trim().is_empty() => id,
        _ => "default".to_string(),
    }
}

/// Child-webview control, in one place because it exists only on desktop.
///
/// Tauri models an embedded page as a *child webview* of the window: `Window::add_child`, and
/// `show`/`hide`/`close`/`set_position`/`set_size` on the resulting `Webview`. None of that exists
/// in the mobile runtime — Android has a single system WebView with an entirely different embedding
/// model — so on mobile these are no-ops and `build_page` refuses with an explanation. The commands
/// stay registered either way, so the frontend gets a clear "not available here" instead of an
/// unknown-command error, and every caller keeps one code path.
#[cfg(desktop)]
fn wv_show(wv: &Webview<Wry>) { let _ = wv.show(); }
#[cfg(not(desktop))]
fn wv_show(_wv: &Webview<Wry>) {}

#[cfg(desktop)]
fn wv_hide(wv: &Webview<Wry>) { let _ = wv.hide(); }
#[cfg(not(desktop))]
fn wv_hide(_wv: &Webview<Wry>) {}

#[cfg(desktop)]
fn wv_close(wv: Webview<Wry>) { let _ = wv.close(); }
#[cfg(not(desktop))]
fn wv_close(_wv: Webview<Wry>) {}

/// Hide every page except `keep` (pass None to hide all).
fn hide_others(pages: &HashMap<String, Webview<Wry>>, keep: Option<&str>) {
    for (id, wv) in pages.iter() {
        if Some(id.as_str()) != keep {
            wv_hide(wv);
        }
    }
}

/// Where a page's webview sits inside the main window, in logical pixels relative to the
/// window's client area — i.e. exactly the CSS-pixel rect React measures for the placeholder.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    fn position(&self) -> LogicalPosition<f64> {
        LogicalPosition::new(self.x, self.y)
    }

    fn size(&self) -> LogicalSize<f64> {
        LogicalSize::new(self.width.max(1.0), self.height.max(1.0))
    }
}

/// Move/resize a page's webview onto `rect`.
///
/// On Linux this deliberately bypasses Tauri: `Webview::set_position`/`set_size` are silent
/// no-ops there (see `linux_layout`), so GTK does the placement instead.
fn place(app: &AppHandle, wv: &Webview<Wry>, rect: &Rect) {
    #[cfg(target_os = "linux")]
    {
        linux_layout::place(app, wv.label().to_string(), *rect);
    }
    // macOS/Windows: Tauri's own bounds setters work (it is specifically Linux where they are
    // dropped, which is what linux_layout exists for).
    #[cfg(all(desktop, not(target_os = "linux")))]
    {
        let _ = app;
        let _ = wv.set_position(rect.position());
        let _ = wv.set_size(rect.size());
    }
    #[cfg(not(desktop))]
    {
        let _ = (app, wv, rect);
    }
}

/// Wire a freshly built page's webview up to Rust: a native `bmChannel` script-message handler
/// that receives scrape/macro-step payloads straight from page JS (see `init_script`'s
/// `bridgeSend`), sidestepping page CSP entirely since it's a GObject signal, not a subresource
/// fetch. No-ops on non-Linux targets (no embedded-browser support there yet).
#[cfg(target_os = "linux")]
fn install_bridge(
    webview: &Webview<Wry>,
    scrape: Arc<Mutex<HashMap<String, Value>>>,
    macros: Arc<MacroState>,
) {
    let _ = webview.with_webview(move |pw| {
        let raw = pw.inner();
        let Some(manager) = raw.user_content_manager() else {
            return;
        };
        manager.connect_script_message_received(Some("bmChannel"), move |_mgr, msg| {
            let Some(js) = msg.js_value() else { return };
            let Ok(payload) = serde_json::from_str::<Value>(&js.to_string()) else {
                return;
            };
            match payload.get("kind").and_then(Value::as_str) {
                Some("scrape") => {
                    let (Some(key), Some(data)) =
                        (payload.get("key").and_then(Value::as_str), payload.get("data"))
                    else {
                        return;
                    };
                    if let Ok(mut store) = scrape.lock() {
                        store.insert(key.to_string(), data.clone());
                    }
                }
                Some("macroStep") => {
                    if !macros.recording.load(Ordering::SeqCst) {
                        return;
                    }
                    if let Some(step) = payload.get("step") {
                        if let Ok(mut steps) = macros.steps.lock() {
                            if steps.len() < 5000 {
                                steps.push(step.clone());
                            }
                        }
                    }
                }
                _ => {}
            }
        });
        manager.register_script_message_handler("bmChannel");
    });
}

#[cfg(not(target_os = "linux"))]
fn install_bridge(_webview: &Webview<Wry>, _scrape: Arc<Mutex<HashMap<String, Value>>>, _macros: Arc<MacroState>) {}

/// Hook WebKit's download-started signal, once. It fires on the `WebContext`, which every page
/// shares (same as the cookie store — see `WebviewManager::primary_webview`), so hooking it a
/// second time off a later page would double-handle every future download; `download_hooked`
/// guards that.
///
/// A download the frontend never armed via `webview_arm_download` is left completely alone —
/// no destination is set, no signals are answered — so this only ever touches a download a
/// macro/pipeline step is actively waiting on; everything else keeps whatever behavior WebKit
/// already had (nothing visible, absent a destination handler).
#[cfg(target_os = "linux")]
fn install_download_hook(
    webview: &Webview<Wry>,
    pending_download: Arc<Mutex<Option<PendingDownload>>>,
    scrape: Arc<Mutex<HashMap<String, Value>>>,
    download_hooked: Arc<AtomicBool>,
) {
    if download_hooked.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = webview.with_webview(move |pw| {
        let raw = pw.inner();
        let Some(ctx) = raw.context() else { return };
        ctx.connect_download_started(move |_ctx, download| {
            let Some(pending) = pending_download.lock().ok().and_then(|mut g| g.take()) else {
                return; // nothing armed — a plain, unautomated download; don't touch it
            };

            let scrape_dest = scrape.clone();
            let key_dest = pending.result_key.clone();
            let dir = pending.dir.clone();
            let filename = pending.filename.clone();
            download.connect_decide_destination(move |dl, suggested| {
                let name = filename
                    .clone()
                    .filter(|f| !f.trim().is_empty())
                    .unwrap_or_else(|| suggested.to_string());
                let safe_name: String = name
                    .chars()
                    .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' || c == ' ' { c } else { '_' })
                    .collect();
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    if let Ok(mut s) = scrape_dest.lock() {
                        s.insert(key_dest.clone(), json!({ "ok": false, "error": format!("couldn't create '{}': {}", dir.display(), e) }));
                    }
                    return false;
                }
                let dest = dir.join(safe_name.trim());
                let uri = url::Url::from_file_path(&dest)
                    .map(|u| u.to_string())
                    .unwrap_or_else(|_| format!("file://{}", dest.display()));
                dl.set_destination(&uri);
                true
            });

            let scrape_failed = scrape.clone();
            let key_failed = pending.result_key.clone();
            download.connect_failed(move |_dl, err| {
                if let Ok(mut s) = scrape_failed.lock() {
                    s.insert(key_failed.clone(), json!({ "ok": false, "error": err.to_string() }));
                }
            });

            let scrape_finished = scrape.clone();
            let key_finished = pending.result_key.clone();
            download.connect_finished(move |dl| {
                let path = dl.destination().map(|u| {
                    url::Url::parse(&u).ok().and_then(|parsed| parsed.to_file_path().ok())
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| u.to_string())
                });
                if let Ok(mut s) = scrape_finished.lock() {
                    // `failed` may already have written a result for this key by the time
                    // `finished` also fires — don't clobber a real error with a bare "ok: true".
                    if !s.contains_key(&key_finished) {
                        s.insert(key_finished.clone(), json!({ "ok": path.is_some(), "path": path }));
                    }
                }
            });
        });
    });
}

#[cfg(not(target_os = "linux"))]
fn install_download_hook(
    _webview: &Webview<Wry>,
    _pending_download: Arc<Mutex<Option<PendingDownload>>>,
    _scrape: Arc<Mutex<HashMap<String, Value>>>,
    _download_hooked: Arc<AtomicBool>,
) {
}

/// Create a page's child webview at `url`, sized onto `rect`. Caller holds no locks.
/// Mobile: there is no child-webview API to build a page into (see `wv_show`). The command stays
/// registered so the frontend gets this explanation instead of an unknown-command error.
#[cfg(not(desktop))]
fn build_page(
    _app: &AppHandle,
    _id: &str,
    _url: url::Url,
    _rect: &Rect,
    _scrape: Arc<Mutex<HashMap<String, Value>>>,
    _macros: Arc<MacroState>,
    _pending_download: Arc<Mutex<Option<PendingDownload>>>,
    _download_hooked: Arc<AtomicBool>,
) -> Result<Webview<Wry>, String> {
    Err("The embedded browser isn't available on mobile — Android has a single system WebView, with \
         no child-webview API to place pages into. Use the desktop app for Suno/Midjourney sign-in \
         and macro recording."
        .to_string())
}

#[cfg(desktop)]
fn build_page(
    app: &AppHandle,
    id: &str,
    url: url::Url,
    rect: &Rect,
    scrape: Arc<Mutex<HashMap<String, Value>>>,
    macros: Arc<MacroState>,
    pending_download: Arc<Mutex<Option<PendingDownload>>>,
    download_hooked: Arc<AtomicBool>,
) -> Result<Webview<Wry>, String> {
    let window = app
        .get_window("main")
        .ok_or_else(|| "main window not found".to_string())?;

    let label = label_for(id);
    let macros_for_load = macros.clone();
    let builder = WebviewBuilder::new(&label, WebviewUrl::External(url))
        .user_agent(CHROME_UA)
        .initialization_script(init_script())
        // Re-arms the recorder after a full navigation while a recording is in progress —
        // replaces polling a `/macro/state` endpoint (same CSP problem `bridgeSend` works
        // around) with Rust driving the arm call directly.
        .on_page_load(move |webview, payload| {
            if matches!(payload.event(), PageLoadEvent::Finished)
                && macros_for_load.recording.load(Ordering::SeqCst)
            {
                let _ = webview.eval("window.__bmArmRecorder && window.__bmArmRecorder();");
            }
        });

    let webview = window
        .add_child(builder, rect.position(), rect.size())
        .map_err(|e| format!("failed to create embedded webview: {}", e))?;

    // wry parents new webviews onto the window's vertical GtkBox, where they'd be laid out as
    // full-width siblings of the app. Move it into the Fixed where we control its geometry.
    #[cfg(target_os = "linux")]
    linux_layout::adopt(app, label, *rect);

    install_bridge(&webview, scrape.clone(), macros);
    install_download_hook(&webview, pending_download, scrape, download_hooked);

    Ok(webview)
}

/// Position child webviews on Linux, where Tauri can't.
///
/// `Webview::set_position`/`set_size` funnel into wry's `set_bounds`, which only applies bounds
/// when the webview sits in a `GtkFixed` (`is_in_fixed_parent`) or owns an X11 child window
/// (`x11`). On Linux `tauri-runtime-wry` builds child webviews with `build_gtk(default_vbox())`
/// — a `GtkBox` — so neither holds and every rect we send is silently dropped; GTK stacks the
/// pages as full-width vertical siblings of the app, covering the sidebar.
///
/// So we place them ourselves: the window's vbox gets a single `GtkFixed`, the main webview is
/// pinned into it at the window's full size, and each page is parented into the same `Fixed`
/// and moved/resized directly. Widgets carry their Tauri label via `set_widget_name` so we can
/// find them again without holding GTK handles (which are `!Send`) across threads — every entry
/// point hops to the main thread, since GTK may only be touched there.
#[cfg(target_os = "linux")]
mod linux_layout {
    use super::Rect;
    use gtk::glib;
    use gtk::prelude::*;
    use tauri::{AppHandle, Manager};

    const MAIN_NAME: &str = "bm-main-webview";

    fn fixed_in(vbox: &gtk::Box) -> Option<gtk::Fixed> {
        vbox.children().into_iter().find_map(|c| c.downcast::<gtk::Fixed>().ok())
    }

    /// Swap the window's vbox contents for a single `GtkFixed` holding the app's webview.
    /// Idempotent — every entry point calls it so ordering never matters.
    ///
    /// `GtkFixed` rather than `GtkOverlay`: a `WebKitWebView` renders through its own native
    /// GdkWindow and simply does not paint as an overlay child — `get-child-position` returned
    /// the right rect and the page stayed invisible. wry itself only ever parents webviews into
    /// a `GtkBox`, a `GtkFixed`, or a plain container, and sizes `GtkFixed` children exactly the
    /// way we do below, so `GtkFixed` is the configuration WebKitGTK is actually exercised in.
    fn ensure(app: &AppHandle) -> Result<(gtk::Box, gtk::Fixed), String> {
        let window = app.get_window("main").ok_or("main window not found")?;
        let vbox = window.default_vbox().map_err(|e| e.to_string())?;
        if let Some(f) = fixed_in(&vbox) {
            return Ok((vbox, f));
        }

        let fixed = gtk::Fixed::new();
        // Whatever is in the vbox at this point is the app's own webview.
        for child in vbox.children() {
            vbox.remove(&child);
            child.set_widget_name(MAIN_NAME);
            fixed.put(&child, 0, 0);
            child.show_all();
        }
        vbox.pack_start(&fixed, true, true, 0);
        fixed.show();

        // A Fixed gives each child exactly its size request, so the app's webview needs one —
        // and it must track the window. Reading vbox.allocation() here would yield 1x1, because
        // at startup the window isn't realized yet; that is what previously pinned the app at
        // 1x1 and rendered a blank window. Instead we follow the vbox's real allocation, and
        // apply the request from an idle callback: calling set_size_request *during* a
        // size-allocate is ignored by GTK, which is why the earlier fallback never recovered.
        let f = fixed.clone();
        vbox.connect_size_allocate(move |_, alloc| {
            let (w, h) = (alloc.width(), alloc.height());
            let f = f.clone();
            glib::idle_add_local_once(move || fit_main(&f, w, h));
        });

        Ok((vbox, fixed))
    }

    /// Keep the app's webview filling the window.
    fn fit_main(fixed: &gtk::Fixed, w: i32, h: i32) {
        if w < 2 || h < 2 {
            return;
        }
        if let Some(main) = fixed.children().into_iter().find(|c| c.widget_name() == MAIN_NAME) {
            let a = main.allocation();
            if a.width() != w || a.height() != h {
                main.set_size_request(w, h);
            }
        }
    }

    fn apply(fixed: &gtk::Fixed, label: &str, rect: Rect) {
        if let Some(w) = fixed.children().into_iter().find(|c| c.widget_name() == label) {
            w.set_size_request(rect.width.max(1.0) as i32, rect.height.max(1.0) as i32);
            fixed.move_(&w, rect.x as i32, rect.y as i32);
        }
    }

    fn on_main<F: FnOnce(AppHandle) + Send + 'static>(app: &AppHandle, f: F) {
        let a = app.clone();
        let _ = app.run_on_main_thread(move || f(a));
    }


    /// Install the overlay at startup. Re-parenting a `WebKitWebView` re-realizes it, which is
    /// free before the frontend has loaded but would blow away app state later.
    pub fn install(app: &AppHandle) {
        on_main(app, |a| {
            let _ = ensure(&a);
        });
    }

    /// Adopt a freshly built page webview out of the vbox and into the Fixed, tagged with its
    /// label so `place` can find it again. wry parents new webviews onto the vbox, so the new
    /// widget is whatever sits there that isn't our Fixed.
    pub fn adopt(app: &AppHandle, label: String, rect: Rect) {
        on_main(app, move |a| {
            let Ok((vbox, fixed)) = ensure(&a) else { return };
            let strays: Vec<gtk::Widget> = vbox
                .children()
                .into_iter()
                .filter(|c| c.downcast_ref::<gtk::Fixed>().is_none())
                .collect();
            for w in strays {
                vbox.remove(&w);
                w.set_widget_name(&label);
                fixed.put(&w, rect.x as i32, rect.y as i32);
                // show_all, not show: re-parenting unrealized the WebKitWebView, and show()
                // reveals only the outer widget. wry's own set_visible uses show_all too.
                w.show_all();
            }
            apply(&fixed, &label, rect);
        });
    }

    pub fn place(app: &AppHandle, label: String, rect: Rect) {
        on_main(app, move |a| {
            let Ok((_, fixed)) = ensure(&a) else { return };
            apply(&fixed, &label, rect);
        });
    }
}

/// Restructure the main window so embedded pages can be positioned (Linux); no-op elsewhere.
/// Call once at startup, before the frontend loads.
pub fn install_layout(app: &AppHandle) {
    #[cfg(target_os = "linux")]
    linux_layout::install(app);
    #[cfg(not(target_os = "linux"))]
    let _ = app;
}

/// Open the page's webview at `url` over `rect`, creating it if this is the first time we see
/// `page_id` and navigating it otherwise. Hides every other page and makes this one active.
#[command]
pub async fn webview_open(
    app: AppHandle,
    mgr: tauri::State<'_, WebviewManager>,
    page_id: Option<String>,
    url: String,
    rect: Rect,
) -> Result<(), String> {
    let id = page_id_or_default(page_id);
    let target = parse_url(&url)?;

    {
        let pages = mgr.pages.lock().map_err(lock_err)?;
        if let Some(wv) = pages.get(&id) {
            place(&app, wv, &rect);
            let _ = wv.navigate(target);
            wv_show(wv);
            hide_others(&pages, Some(&id));
            *mgr.active.lock().map_err(lock_err)? = Some(id);
            return Ok(());
        }
    }

    // Built outside the lock: `add_child` runs on the main thread and can block.
    let webview = build_page(&app, &id, target, &rect, mgr.scrape.clone(), mgr.macros.clone(), mgr.pending_download.clone(), mgr.download_hooked.clone())?;

    let mut pages = mgr.pages.lock().map_err(lock_err)?;
    hide_others(&pages, None);
    wv_show(&webview);
    pages.insert(id.clone(), webview);
    *mgr.active.lock().map_err(lock_err)? = Some(id);
    Ok(())
}

/// Show a page (tab switch) over `rect` without navigating it, so its DOM/session state is
/// preserved.
///
/// `url` makes this an ensure-and-show: when the page has no webview yet — a cold app start
/// where the React tab list was restored from localStorage, or a tab the backend never saw —
/// it gets created at `url` instead of erroring. Without it the frontend had to call this
/// first, let it throw, and retry via `webview_open`, which logged a spurious
/// "page '…' is not open" on every start. Omitting `url` keeps the strict behaviour.
#[command]
pub async fn webview_show_page(
    app: AppHandle,
    mgr: tauri::State<'_, WebviewManager>,
    page_id: String,
    rect: Rect,
    url: Option<String>,
) -> Result<(), String> {
    {
        let pages = mgr.pages.lock().map_err(lock_err)?;
        if let Some(wv) = pages.get(&page_id) {
            place(&app, wv, &rect);
            wv_show(wv);
            hide_others(&pages, Some(&page_id));
            *mgr.active.lock().map_err(lock_err)? = Some(page_id);
            return Ok(());
        }
    }

    let target = match url {
        Some(u) => parse_url(&u)?,
        None => return Err(format!("page '{}' is not open", page_id)),
    };

    let webview = build_page(&app, &page_id, target, &rect, mgr.scrape.clone(), mgr.macros.clone(), mgr.pending_download.clone(), mgr.download_hooked.clone())?;

    let mut pages = mgr.pages.lock().map_err(lock_err)?;
    hide_others(&pages, None);
    wv_show(&webview);
    pages.insert(page_id.clone(), webview);
    *mgr.active.lock().map_err(lock_err)? = Some(page_id);
    Ok(())
}

/// Close a page's webview entirely (destroys its DOM; session cookies persist in the profile).
#[command]
pub async fn webview_close_page(
    mgr: tauri::State<'_, WebviewManager>,
    page_id: String,
) -> Result<(), String> {
    let removed = {
        let mut pages = mgr.pages.lock().map_err(lock_err)?;
        pages.remove(&page_id)
    };
    if let Some(wv) = removed {
        wv_close(wv);
    }
    let mut active = mgr.active.lock().map_err(lock_err)?;
    if active.as_deref() == Some(page_id.as_str()) {
        *active = None;
    }
    Ok(())
}

/// List open pages: `[{ id, url, active }]`.
#[command]
pub async fn webview_list_pages(mgr: tauri::State<'_, WebviewManager>) -> Result<Vec<Value>, String> {
    let pages = mgr.pages.lock().map_err(lock_err)?;
    let active = mgr.active.lock().map_err(lock_err)?.clone();
    Ok(pages
        .iter()
        .map(|(id, wv)| {
            json!({
                "id": id,
                "url": wv.url().map(|u| u.to_string()).unwrap_or_default(),
                "active": active.as_deref() == Some(id.as_str()),
            })
        })
        .collect())
}

/// Reposition/resize the active page to track the React placeholder (resize / layout change).
#[command]
pub async fn webview_set_rect(
    app: AppHandle,
    mgr: tauri::State<'_, WebviewManager>,
    rect: Rect,
) -> Result<(), String> {
    let pages = mgr.pages.lock().map_err(lock_err)?;
    if let Some(id) = mgr.active.lock().map_err(lock_err)?.as_ref() {
        if let Some(wv) = pages.get(id) {
            place(&app, wv, &rect);
        }
    }
    Ok(())
}

/// Hide all pages (when the user leaves the Browser tab). They keep their session/state.
#[command]
pub async fn webview_hide(mgr: tauri::State<'_, WebviewManager>) -> Result<(), String> {
    let pages = mgr.pages.lock().map_err(lock_err)?;
    hide_others(&pages, None);
    Ok(())
}

/// Resolve the page a command targets: the explicit `page_id`, else the active page.
fn resolve<'a>(
    pages: &'a HashMap<String, Webview<Wry>>,
    active: &Option<String>,
    page_id: &Option<String>,
) -> Result<&'a Webview<Wry>, String> {
    let id = page_id
        .as_deref()
        .or(active.as_deref())
        .ok_or_else(|| "no embedded page is open".to_string())?;
    pages
        .get(id)
        .ok_or_else(|| format!("page '{}' is not open", id))
}

/// Navigate a page (active by default) to a new URL (no reposition).
#[command]
pub async fn webview_navigate(
    mgr: tauri::State<'_, WebviewManager>,
    url: String,
    page_id: Option<String>,
) -> Result<(), String> {
    let target = parse_url(&url)?;
    let pages = mgr.pages.lock().map_err(lock_err)?;
    let active = mgr.active.lock().map_err(lock_err)?;
    resolve(&pages, &active, &page_id)?
        .navigate(target)
        .map_err(|e| e.to_string())
}

/// Run JavaScript inside a page (fire-and-forget). Automation actions — click/fill/submit —
/// go through here; results come back via `window.__bmScrape` → `webview_take_scrape`.
#[command]
pub async fn webview_eval(
    mgr: tauri::State<'_, WebviewManager>,
    js: String,
    page_id: Option<String>,
) -> Result<(), String> {
    let pages = mgr.pages.lock().map_err(lock_err)?;
    let active = mgr.active.lock().map_err(lock_err)?;
    resolve(&pages, &active, &page_id)?
        .eval(&js)
        .map_err(|e| e.to_string())
}

/// Current URL of a page (so the UI can show it / decide login state).
#[command]
pub async fn webview_current_url(
    mgr: tauri::State<'_, WebviewManager>,
    page_id: Option<String>,
) -> Result<String, String> {
    let pages = mgr.pages.lock().map_err(lock_err)?;
    let active = mgr.active.lock().map_err(lock_err)?;
    match resolve(&pages, &active, &page_id) {
        Ok(wv) => Ok(wv.url().map(|u| u.to_string()).unwrap_or_default()),
        Err(_) => Ok(String::new()),
    }
}

/// Pull (and clear) a scraped payload delivered by an injected script under `key`.
#[command]
pub async fn webview_take_scrape(
    mgr: tauri::State<'_, WebviewManager>,
    key: String,
) -> Result<Option<Value>, String> {
    let mut store = mgr.scrape.lock().map_err(lock_err)?;
    Ok(store.remove(&key))
}

/// Arm the next WebKit download to land under `~/Documents/BibleMusically/MacroDownloads/
/// <folder_name>/<filename or the browser's own suggested name>` instead of wherever WebKit
/// would otherwise put it, tagged with `result_key` so the caller can poll
/// `webview_take_scrape(result_key)` for `{ ok: true, path }` or `{ ok: false, error }`.
///
/// One-shot and consumed by whichever `WebKitDownload` starts next (see `install_download_hook`
/// in this file), so a macro/pipeline "download" step must call this immediately before the
/// click that actually triggers the download — not any earlier, or a download some other page
/// or step causes in between would eat the armed slot instead.
#[tauri::command]
pub async fn webview_arm_download(
    mgr: tauri::State<'_, WebviewManager>,
    folder_name: String,
    filename: Option<String>,
    result_key: String,
) -> Result<(), String> {
    let base = dirs::document_dir().ok_or_else(|| "Could not resolve the Documents folder".to_string())?;
    let safe_folder: String = folder_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
        .collect();
    let safe_folder = safe_folder.trim();
    let dir = base
        .join("BibleMusically")
        .join("MacroDownloads")
        .join(if safe_folder.is_empty() { "macro-download" } else { safe_folder });
    *mgr.pending_download.lock().map_err(lock_err)? = Some(PendingDownload { dir, filename, result_key });
    Ok(())
}

// ── Macro recorder commands ──────────────────────────────────────────────────

/// Start recording: clears previous steps and arms the recorder in the active page
/// immediately (pages loaded from now on arm themselves via `/macro/state`).
#[command]
pub async fn macro_start(mgr: tauri::State<'_, WebviewManager>) -> Result<(), String> {
    mgr.macros.steps.lock().map_err(lock_err)?.clear();
    mgr.macros.recording.store(true, Ordering::SeqCst);
    // Arm the already-loaded active page without waiting for a navigation.
    let pages = mgr.pages.lock().map_err(lock_err)?;
    let active = mgr.active.lock().map_err(lock_err)?;
    if let Ok(wv) = resolve(&pages, &active, &None) {
        let _ = wv.eval("window.__bmArmRecorder && window.__bmArmRecorder();");
    }
    Ok(())
}

/// Stop recording and return the captured steps (ordered).
#[command]
pub async fn macro_stop(mgr: tauri::State<'_, WebviewManager>) -> Result<Vec<Value>, String> {
    mgr.macros.recording.store(false, Ordering::SeqCst);
    let mut steps = mgr.macros.steps.lock().map_err(lock_err)?;
    Ok(std::mem::take(&mut *steps))
}

/// Recorder status: `{ recording, steps }` (step count so the UI can show live progress).
#[command]
pub async fn macro_status(mgr: tauri::State<'_, WebviewManager>) -> Result<Value, String> {
    let steps = mgr.macros.steps.lock().map_err(lock_err)?;
    Ok(json!({
        "recording": mgr.macros.recording.load(Ordering::SeqCst),
        "steps": steps.len(),
    }))
}
