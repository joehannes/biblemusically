// ─────────────────────────────────────────────────────────────────────────────
// Studio Lightkid — accounts, trials, subscriptions, funnel, reports
//
// One Cloudflare Worker, five KV namespaces, no database. That is not a shortcut: the whole surface here
// is "look up a user, sign a token, count an event", and KV does all three at a scale this will not
// outgrow for years. D1 is deliberately unused — the token for this account has no D1 permission, and
// nothing here needs a join.
//
// THE FREE TIER SHAPES THE DESIGN. KV allows 100k reads but only **1,000 writes a day**. So:
//   • analytics arrive pre-batched — one write per session, plus per-day counters, never one per click;
//   • entitlements are *signed*, not stored per check, so a client verifying its licence hourly costs
//     zero writes and zero reads;
//   • reports are the only unbounded writer, and they are rate-limited per device.
//
// WHY SIGNED ENTITLEMENTS. The app is a desktop program holding the user's own API keys. The server has
// nothing to hold hostage, so enforcement cannot be "ask the server each time" — that is one airplane
// mode away from unlimited. Instead the server signs a short-lived entitlement (Ed25519), the app
// verifies it with an embedded public key, and — the part that actually bites — the app derives its
// project-cache encryption key from the entitlement. Removing the check does not decrypt your work, and
// a fresh trial account derives a different key, which closes the "new trial, reimport my projects"
// route. Difficult for most people. Not impossible for a determined one, and nothing here pretends
// otherwise.
//
// VAT: Lemon Squeezy is the merchant of record. They are the legal seller, they collect and remit EU VAT
// and US sales tax. This server never sees a card and never computes a tax rate.
// ─────────────────────────────────────────────────────────────────────────────

const TRIAL_DAYS = 7;
/** How long a signed entitlement is good for. Short enough that a cancellation bites within a day,
 *  long enough that a week offline does not lock somebody out of work they paid for. */
const ENTITLEMENT_HOURS = 26;
/** Reports per device per day. Generous for a real user, useless as a way to fill the KV quota. */
const REPORT_LIMIT = 40;
/** Concurrent signed-in devices per account. */
const MAX_SESSIONS = 3;
/** A session nobody has used for this long is stale: removed, and its slot freed. Without this, three
 *  reinstalls would lock somebody out of their own licence with no way back. */
const SESSION_STALE_DAYS = 7;

const JSON_HEADERS = {
  "Content-Type": "application/json; charset=utf-8",
  // The desktop app is not a browser origin, and the admin page is served from this same Worker.
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "Content-Type, Authorization",
  "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
};

const json = (body, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: JSON_HEADERS });
const bad = (message, status = 400) => json({ ok: false, error: message }, status);

const nowIso = () => new Date().toISOString();
const dayKey = (d = new Date()) => d.toISOString().slice(0, 10);
const addDays = (days) => new Date(Date.now() + days * 86400_000).toISOString();

// ── Plans and pricing ───────────────────────────────────────────────────────
// Defaults live here so a fresh deployment works before anybody opens the admin page; the admin's saved
// config overrides them. Prices are in cents, because a float price is a rounding bug waiting for a
// customer to find it.
const DEFAULT_CONFIG = {
  currency: "USD",
  plans: {
    monthly:   { label: "Monthly",            cents: 1200,  interval: "month",  months: 1 },
    quarterly: { label: "Three months",       cents: 3000,  interval: "quarter", months: 3 },
    annual:    { label: "Annual",             cents: 9600,  interval: "year",   months: 12 },
    lifetime:  { label: "This version, for life", cents: 24900, interval: "once", months: 0 },
  },
  trial_days: TRIAL_DAYS,
  // A lifetime licence covers the major version it was bought in. A later major version is a new
  // purchase, and the entitlement says which version it covers so the app can be honest about it.
  lifetime_major: 1,

  // Accounts that always hold a full licence, without paying and without a code.
  //
  // The author's own account is here because every alternative is worse: a lifetime record in KV can
  // be wiped by a bad migration, and a promo code can be spent by accident. Checked before any dated
  // state, so it survives an expired trial and a lapsed subscription alike.
  //
  // Compared case-insensitively — an email address is not case-sensitive in its domain, and people
  // type their own address inconsistently.
  permanent_free: ["johannes.neugschwentner@gmail.com"],

  // A reusable promo code granting the lifetime licence for the current major version.
  //
  // Unlike a `gift:` record, which is minted per recipient and dies on first use, this one is a
  // constant — so it can be handed out deliberately, and used on a fresh machine without minting
  // anything first. One redemption per account, tracked at `promo:<code>:<email>`, so a leak costs
  // one licence per address rather than unlimited licences.
  //
  // Override it in production with the PROMO_CODE secret (`wrangler secret put PROMO_CODE`); the
  // constant is the fallback so a self-hosted deploy works out of the box.
  promo_code: "BM1-ZR6JN-QE4FD-P7DAB",
  hotjar_site_id: "",
  signups_open: true,
  message: "",
};

async function config(env) {
  const stored = await env.CONFIG.get("config", { type: "json" });
  return { ...DEFAULT_CONFIG, ...(stored || {}), plans: { ...DEFAULT_CONFIG.plans, ...(stored?.plans || {}) } };
}

// ── Signing ─────────────────────────────────────────────────────────────────

/** Base64url without padding — what fits in a token without escaping. */
function b64u(bytes) {
  let s = "";
  const arr = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  for (const b of arr) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function signingKey(env) {
  // The private key is a Worker secret, raw Ed25519 seed as base64url. Generated by deploy.sh, never in
  // the repo and never returned by any endpoint.
  const raw = Uint8Array.from(atob(env.SIGNING_KEY_PKCS8.replace(/-/g, "+").replace(/_/g, "/")),
                              (c) => c.charCodeAt(0));
  return crypto.subtle.importKey("pkcs8", raw, { name: "Ed25519" }, false, ["sign"]);
}

/**
 * Mint an entitlement.
 *
 * The payload is deliberately small and self-describing: the app must be able to decide what to allow
 * without asking anything else, including while offline.
 */
async function mintEntitlement(env, user, cfg) {
  const state = entitlementState(user, cfg);
  const payload = {
    sub: user.id,
    email: user.email,
    username: user.username || "",
    // "trial" | "active" | "lifetime" | "expired" | "blocked"
    status: state.status,
    plan: user.plan || "",
    // What the app actually gates on. Kept explicit rather than derived from `status`, so adding a
    // capability later does not need a client release to interpret it.
    features: state.features,
    trial_ends: user.trial_ends || "",
    period_ends: user.period_ends || "",
    lifetime_major: user.lifetime_major || 0,
    iat: Math.floor(Date.now() / 1000),
    exp: Math.floor(Date.now() / 1000) + ENTITLEMENT_HOURS * 3600,
    // Ties the cache key to this account. A new trial account cannot open the old account's projects,
    // which is the whole point of the restriction.
    vault: user.vault_salt,
  };
  const body = b64u(new TextEncoder().encode(JSON.stringify(payload)));
  const key = await signingKey(env);
  const sig = await crypto.subtle.sign({ name: "Ed25519" }, key,
                                       new TextEncoder().encode(body));
  return { token: `${body}.${b64u(sig)}`, payload };
}

/**
 * What this user is entitled to, right now.
 *
 * One function so the client, the admin page and the webhook can never disagree about who is paid up.
 */
function entitlementState(user, cfg) {
  const full = {
    read: true, generate: true, publish: true, export: true, save_copies: true, remote_sync: true,
  };
  // A trial can do everything except take its work out of the app. That is the one restriction that
  // stops "new trial account, reimport my projects" from being a way to never pay.
  const trial = {
    read: true, generate: true, publish: true, export: false, save_copies: false, remote_sync: false,
  };
  const none = {
    read: true, generate: false, publish: false, export: false, save_copies: false, remote_sync: false,
  };

  if (user.blocked) return { status: "blocked", features: { ...none, read: false } };

  // Named accounts hold a full licence unconditionally. Deliberately ahead of every dated check, so
  // an expired trial or a lapsed card cannot lock the author out of their own app. `blocked` still
  // wins, because an account being on this list is not a reason to keep serving a compromised one.
  const freeList = (cfg.permanent_free || []).map((a) => String(a).trim().toLowerCase());
  if (user.email && freeList.includes(String(user.email).trim().toLowerCase())) {
    return { status: "lifetime", features: full };
  }

  if (user.lifetime_major && user.lifetime_major >= (cfg.lifetime_major || 1)) {
    return { status: "lifetime", features: full };
  }
  if (user.period_ends && user.period_ends > nowIso()) return { status: "active", features: full };
  if (user.trial_ends && user.trial_ends > nowIso()) return { status: "trial", features: trial };
  return { status: "expired", features: none };
}

// ── Sessions ────────────────────────────────────────────────────────────────
// Three at once, because a licence is for a person and not for a shared login. The awkward part is not
// the limit but the way out of it: somebody who reinstalls three times must not be locked out of what
// they paid for. So sessions expire on their own after a week of not being used, and the user can end
// specific ones — shown by approximate location, since "Vienna, yesterday" is recognisable in a way a
// device id is not.
//
// Location comes from Cloudflare's own request properties. No IP address is stored: the city and country
// are enough to recognise your own laptop, and an IP is a liability rather than a feature.

function place(request) {
  const cf = request.cf || {};
  const parts = [cf.city, cf.country].filter(Boolean);
  return parts.length ? parts.join(", ") : "somewhere";
}

const isStale = (session) =>
  !session.last_seen || session.last_seen < new Date(Date.now() - SESSION_STALE_DAYS * 86400_000).toISOString();

/** Drop stale sessions and return what is left. Called on every sign-in, so it needs no scheduled job. */
function pruneSessions(user) {
  user.sessions = (user.sessions || []).filter((s) => !isStale(s));
  return user.sessions;
}

/**
 * Register a device, or refuse because three are already in use.
 *
 * Refusing returns the list so the app can offer to end one — a limit with no way to act on it is just a
 * wall.
 */
function claimSession(user, device, request) {
  const sessions = pruneSessions(user);
  const existing = sessions.find((s) => s.device === device);
  if (existing) {
    // Already ours: refresh it. This is also what keeps a machine in daily use from going stale.
    existing.last_seen = nowIso();
    existing.place = place(request);
    return { ok: true, sessions };
  }
  if (sessions.length >= MAX_SESSIONS) {
    return {
      ok: false,
      reason: `This licence is signed in on ${sessions.length} devices, which is the limit. End one to ` +
              `continue — or leave it a week and it frees itself.`,
      sessions,
    };
  }
  sessions.push({
    device, place: place(request), first_seen: nowIso(), last_seen: nowIso(),
  });
  return { ok: true, sessions };
}

// ── Identity ────────────────────────────────────────────────────────────────

/**
 * Verify a Google ID token.
 *
 * Google's own tokeninfo endpoint rather than local JWKS verification: one request, no key rotation to
 * track, and this is not a hot path — an account is created once and an entitlement lasts a day.
 */
async function verifyGoogle(idToken) {
  const r = await fetch(`https://oauth2.googleapis.com/tokeninfo?id_token=${encodeURIComponent(idToken)}`);
  if (!r.ok) return null;
  const info = await r.json();
  if (!info.email || info.email_verified === "false") return null;
  return { email: String(info.email).toLowerCase(), name: info.name || "", sub: info.sub };
}

const userKey = (email) => `user:${String(email).toLowerCase()}`;

async function getUser(env, email) {
  return await env.USERS.get(userKey(email), { type: "json" });
}

async function putUser(env, user) {
  user.updated_at = nowIso();
  await env.USERS.put(userKey(user.email), JSON.stringify(user));
  return user;
}

// ── Routes ──────────────────────────────────────────────────────────────────

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const path = url.pathname.replace(/\/+$/, "") || "/";

    if (request.method === "OPTIONS") return new Response(null, { headers: JSON_HEADERS });

    try {
      // ── Public ──────────────────────────────────────────────────────────
      if (path === "/") {
        return new Response(SITE_HTML, { headers: { "Content-Type": "text/html; charset=utf-8" } });
      }

      // A short path worth saying out loud. The workers.dev subdomain is fixed to the account name
      // and cannot be shortened, so until a real domain is attached this is the shortest thing that
      // can be read down a phone: ".../get". Any ?ref= is carried through, so a share link still
      // credits whoever sent it.
      if (path === "/get" || path === "/download") {
        const ref = (url.searchParams.get("ref") || "").trim().slice(0, 40);
        await bump(env, "visit_get");
        const target = ref ? `/?ref=${encodeURIComponent(ref)}#get` : "/#get";
        return Response.redirect(new URL(target, url).toString(), 302);
      }
      if (path === "/health") return json({ ok: true, service: "studio-lightkid", now: nowIso() });

      // The terms as a readable page rather than only as JSON for the app. Rendered client-side from the
      // same Markdown the app shows, so there is one copy of the text and no way for them to drift.
      if (path === "/terms") {
        const md = (await env.CONFIG.get("terms")) || "The terms have not been published yet.";
        return new Response(TERMS_PAGE.replace("__MARKDOWN__", JSON.stringify(md)),
                            { headers: { "Content-Type": "text/html; charset=utf-8" } });
      }

      if (path === "/v1/pricing") {
        const cfg = await config(env);
        return json({
          ok: true, currency: cfg.currency, plans: cfg.plans, trial_days: cfg.trial_days,
          signups_open: cfg.signups_open, message: cfg.message,
          lifetime_major: cfg.lifetime_major,
          hotjar_site_id: cfg.hotjar_site_id,
        });
      }

      if (path === "/v1/terms") {
        const md = await env.CONFIG.get("terms");
        return json({ ok: true, markdown: md || "" });
      }

      // Sign in (and create the account on first sight). Google proves who they are; we store no
      // password, so there is nothing here to leak that Google has not already trusted them with.
      if (path === "/v1/auth/google" && request.method === "POST") {
        const body = await request.json().catch(() => ({}));
        const cfg = await config(env);
        const who = await verifyGoogle(body.id_token || "");
        if (!who) return bad("Google did not recognise that sign-in.", 401);

        let user = await getUser(env, who.email);
        if (!user) {
          if (!cfg.signups_open) return bad("New accounts are paused right now.", 403);
          user = {
            id: crypto.randomUUID(),
            email: who.email,
            name: who.name,
            username: (body.username || "").trim().toLowerCase().slice(0, 32),
            created_at: nowIso(),
            trial_ends: addDays(cfg.trial_days),
            plan: "", period_ends: "", lifetime_major: 0,
            blocked: false, notes: "",
            // Random per account, and what the app's cache key is derived from. A new trial account
            // therefore cannot open an old account's cached projects.
            vault_salt: b64u(crypto.getRandomValues(new Uint8Array(16))),
            devices: [],
            referred_by: (body.referral || "").trim().slice(0, 40),
          };
          await putUser(env, user);
          await bump(env, "signup");
          if (user.referred_by) await bump(env, "signup_referred");
        }
        if (body.username && !user.username) {
          user.username = String(body.username).trim().toLowerCase().slice(0, 32);
          await putUser(env, user);
        }
        // Three concurrent devices. Checked here rather than at every request: signing in is the moment
        // a slot is taken, and a per-request check would cost a KV write per call.
        const device = String(body.device_id || "").slice(0, 64);
        if (device) {
          const claim = claimSession(user, device, request);
          await putUser(env, user);
          if (!claim.ok) {
            await bump(env, "session_limit_hit");
            return json({
              ok: false, error: claim.reason, code: "session_limit",
              sessions: claim.sessions.map((s) => ({
                device: s.device.slice(0, 8), place: s.place, last_seen: s.last_seen,
                // So the app can say "this one" rather than making somebody guess which is theirs.
                this_device: s.device === device,
              })),
            }, 409);
          }
        }
        const ent = await mintEntitlement(env, user, cfg);
        return json({ ok: true, entitlement: ent.token, state: ent.payload,
                      sessions: (user.sessions || []).length, max_sessions: MAX_SESSIONS });
      }

      // Refresh the entitlement. No write, so a client checking hourly costs nothing.
      if (path === "/v1/entitlement" && request.method === "POST") {
        const body = await request.json().catch(() => ({}));
        const user = await getUser(env, body.email || "");
        if (!user) return bad("No such account.", 404);
        const cfg = await config(env);
        const ent = await mintEntitlement(env, user, cfg);
        return json({ ok: true, entitlement: ent.token, state: ent.payload });
      }

      // ── Lemon Squeezy webhook ───────────────────────────────────────────
      // Signed with HMAC-SHA256 over the raw body. Verified before anything is trusted: an unverified
      // webhook is an open door to free subscriptions.
      if (path === "/v1/webhook/lemonsqueezy" && request.method === "POST") {
        const raw = await request.text();
        const given = request.headers.get("X-Signature") || "";
        if (!(await verifyHmac(env.LS_WEBHOOK_SECRET, raw, given))) {
          return bad("bad signature", 401);
        }
        const evt = JSON.parse(raw);
        const name = evt?.meta?.event_name || "";
        const attrs = evt?.data?.attributes || {};
        const email = String(attrs.user_email || evt?.meta?.custom_data?.email || "").toLowerCase();
        if (!email) return json({ ok: true, ignored: "no email on the event" });

        let user = await getUser(env, email);
        if (!user) {
          // Somebody paid before signing in. Create them so the money is never lost, and they inherit
          // the entitlement the moment they sign in.
          user = {
            id: crypto.randomUUID(), email, name: attrs.user_name || "", username: "",
            created_at: nowIso(), trial_ends: "", plan: "", period_ends: "", lifetime_major: 0,
            blocked: false, notes: "created by a payment before sign-in",
            vault_salt: b64u(crypto.getRandomValues(new Uint8Array(16))), devices: [],
          };
        }

        const variant = String(evt?.meta?.custom_data?.plan || attrs.variant_name || "").toLowerCase();
        if (name === "order_created" && variant.includes("life")) {
          const cfg = await config(env);
          user.lifetime_major = cfg.lifetime_major || 1;
          user.plan = "lifetime";
          await bump(env, "purchase_lifetime");
        } else if (["subscription_created", "subscription_updated", "subscription_resumed",
                    "subscription_payment_success"].includes(name)) {
          user.plan = variant || user.plan || "monthly";
          // Trust their renewal date rather than computing one: they own the billing clock.
          user.period_ends = attrs.renews_at || attrs.ends_at || addDays(31);
          await bump(env, "subscription_active");
        } else if (["subscription_cancelled", "subscription_expired", "subscription_paused"].includes(name)) {
          // Cancelled is not immediately off: they paid to the end of the period.
          user.period_ends = attrs.ends_at || nowIso();
          await bump(env, "subscription_cancelled");
        }
        user.last_event = name;
        await putUser(env, user);
        return json({ ok: true, applied: name });
      }

      // ── Funnel events ───────────────────────────────────────────────────
      // Pre-batched by the client: one request per session, and only per-day counters are written. That
      // is what keeps a hundred users inside 1,000 writes a day.
      if (path === "/v1/events" && request.method === "POST") {
        const body = await request.json().catch(() => ({}));
        const events = Array.isArray(body.events) ? body.events.slice(0, 200) : [];
        if (!events.length) return json({ ok: true, counted: 0 });

        const day = dayKey();
        const counters = (await env.EVENTS.get(`count:${day}`, { type: "json" })) || {};
        for (const e of events) {
          const name = String(e.name || "").slice(0, 48);
          if (!name) continue;
          counters[name] = (counters[name] || 0) + (Number(e.n) || 1);
        }
        // One write for the day's counters…
        await env.EVENTS.put(`count:${day}`, JSON.stringify(counters), { expirationTtl: 86400 * 400 });
        // …and one for the session's own path, which is what answers "where did they stop".
        if (body.session) {
          await env.EVENTS.put(
            `session:${day}:${String(body.session).slice(0, 40)}`,
            JSON.stringify({
              at: nowIso(), status: body.status || "", plan: body.plan || "",
              version: body.version || "", platform: body.platform || "",
              path: events.map((e) => e.name).slice(0, 120),
            }),
            { expirationTtl: 86400 * 90 },
          );
        }
        return json({ ok: true, counted: events.length });
      }

      // ── Bug reports and feedback ────────────────────────────────────────
      if (path === "/v1/reports" && request.method === "POST") {
        const body = await request.json().catch(() => ({}));
        const device = String(body.device_id || "anon").slice(0, 64);
        const day = dayKey();
        const seenKey = `rate:${device}:${day}`;
        const seen = Number((await env.REPORTS.get(seenKey)) || 0);
        if (seen >= REPORT_LIMIT) return json({ ok: true, throttled: true });

        // Recurring crashes are counted, not re-filed. Twenty copies of one stack trace is noise that
        // buries the report that matters.
        const fingerprint = (body.fingerprint || body.message || "").slice(0, 200);
        const fpKey = `fp:${await sha256hex(fingerprint)}`;
        const existing = (await env.REPORTS.get(fpKey, { type: "json" })) || null;
        if (existing && body.kind === "error") {
          existing.count = (existing.count || 1) + 1;
          existing.last_seen = nowIso();
          if (body.comment) existing.comments = [...(existing.comments || []), body.comment].slice(-20);
          await env.REPORTS.put(fpKey, JSON.stringify(existing));
          await env.REPORTS.put(seenKey, String(seen + 1), { expirationTtl: 86400 });
          return json({ ok: true, deduplicated: true, count: existing.count });
        }

        const report = {
          id: crypto.randomUUID(),
          kind: String(body.kind || "bug").slice(0, 24),
          title: String(body.title || "").slice(0, 200),
          message: String(body.message || "").slice(0, 8000),
          comment: String(body.comment || "").slice(0, 4000),
          stack: String(body.stack || "").slice(0, 6000),
          version: String(body.version || "").slice(0, 32),
          platform: String(body.platform || "").slice(0, 64),
          email: String(body.email || "").slice(0, 120).toLowerCase(),
          status: "new",
          count: 1,
          created_at: nowIso(),
          last_seen: nowIso(),
        };
        await env.REPORTS.put(fpKey, JSON.stringify(report));
        await env.REPORTS.put(`report:${report.created_at}:${report.id}`, JSON.stringify(report),
                              { expirationTtl: 86400 * 400 });
        await env.REPORTS.put(seenKey, String(seen + 1), { expirationTtl: 86400 });
        await bump(env, `report_${report.kind}`);
        return json({ ok: true, id: report.id });
      }

      // ── Gifts and referrals ─────────────────────────────────────────────
      if (path === "/v1/redeem" && request.method === "POST") {
        const body = await request.json().catch(() => ({}));
        const code = String(body.code || "").trim().toUpperCase();
        const cfgEarly = await config(env);

        // The reusable promo code, tried before the single-use gift records. It grants the same
        // lifetime licence a purchase does, once per account: the guard is a marker keyed by code
        // *and* email, so the same person cannot mint themselves a second one, and a leaked code
        // costs one licence per address rather than an unlimited number.
        const promo = String(env.PROMO_CODE || cfgEarly.promo_code || "").trim().toUpperCase();
        if (promo && code === promo) {
          const user = await getUser(env, body.email || "");
          if (!user) return bad("Sign in first, then redeem.", 404);
          const seen = `promo:${promo}:${String(user.email).toLowerCase()}`;
          if (await env.LICENCES.get(seen)) {
            return bad("You have already used that code on this account.", 409);
          }
          user.lifetime_major = cfgEarly.lifetime_major || 1;
          user.plan = "lifetime";
          await env.LICENCES.put(seen, nowIso());
          await putUser(env, user);
          await bump(env, "promo_redeemed");
          const ent = await mintEntitlement(env, user, cfgEarly);
          return json({ ok: true, entitlement: ent.token, state: ent.payload });
        }

        const licence = await env.LICENCES.get(`gift:${code}`, { type: "json" });
        if (!licence) return bad("That code is not one of ours.", 404);
        if (licence.redeemed_by) return bad("That gift has already been redeemed.", 409);
        const user = await getUser(env, body.email || "");
        if (!user) return bad("Sign in first, then redeem.", 404);

        const cfg = await config(env);
        if (licence.plan === "lifetime") {
          user.lifetime_major = cfg.lifetime_major || 1;
          user.plan = "lifetime";
        } else {
          const months = cfg.plans[licence.plan]?.months || 1;
          const from = user.period_ends && user.period_ends > nowIso()
            ? new Date(user.period_ends) : new Date();
          // Extend rather than replace: a gift on top of a paid month adds to it.
          user.period_ends = new Date(from.getTime() + months * 30 * 86400_000).toISOString();
          user.plan = licence.plan;
        }
        licence.redeemed_by = user.email;
        licence.redeemed_at = nowIso();
        await env.LICENCES.put(`gift:${code}`, JSON.stringify(licence));
        await putUser(env, user);
        await bump(env, "gift_redeemed");
        const ent = await mintEntitlement(env, user, cfg);
        return json({ ok: true, entitlement: ent.token, state: ent.payload });
      }

      if (path === "/v1/referral" && request.method === "POST") {
        // A referral code is just the referrer's username; nothing to store until somebody signs up
        // with it, which `/v1/auth/google` records.
        const body = await request.json().catch(() => ({}));
        const user = await getUser(env, body.email || "");
        if (!user) return bad("No such account.", 404);
        const code = (user.username || user.id).slice(0, 32);
        return json({
          ok: true, code,
          share_url: `https://studio-lightkid.pages.dev/?r=${encodeURIComponent(code)}`,
          note: "Anyone who signs up with this gets the normal trial; you are credited in the stats.",
        });
      }

      // ── Sessions ────────────────────────────────────────────────────────
      if (path === "/v1/sessions" && request.method === "POST") {
        const body = await request.json().catch(() => ({}));
        const user = await getUser(env, body.email || "");
        if (!user) return bad("No such account.", 404);
        const mine = String(body.device_id || "");

        if (body.end === "others") {
          user.sessions = (user.sessions || []).filter((s) => s.device === mine);
          await putUser(env, user);
          await bump(env, "session_ended_others");
        } else if (body.end) {
          // Ending a specific one, chosen from the list by its short id.
          const target = String(body.end);
          user.sessions = (user.sessions || []).filter((s) => !s.device.startsWith(target));
          await putUser(env, user);
          await bump(env, "session_ended_one");
        } else {
          // Just looking: prune while we are here, so the list is honest about what is really in use.
          pruneSessions(user);
          await putUser(env, user);
        }
        return json({
          ok: true, max: MAX_SESSIONS, stale_after_days: SESSION_STALE_DAYS,
          sessions: (user.sessions || []).map((s) => ({
            device: s.device.slice(0, 8), place: s.place,
            last_seen: s.last_seen, first_seen: s.first_seen,
            this_device: s.device === mine,
          })),
        });
      }

      // A manifest in the shape Tauri's own updater plugin expects, so an in-place update can be turned
      // on later by adding a signing key at build time. Until then the app downloads the installer and
      // hands it to the operating system, which needs no key and no trust decision from the user.
      if (path === "/v1/latest.json") {
        const rel = await releases(env);
        if (!rel.latest) return json({ ok: false, error: "no release yet" }, 404);
        const target = url.searchParams.get("target") || "linux-x86_64";
        const asset = (rel.latest.assets || {})[target.startsWith("darwin") ? "macos"
          : target.startsWith("windows") ? "windows" : "linux"];
        return json({
          version: rel.latest.version,
          notes: rel.latest.notes,
          pub_date: rel.at,
          platforms: asset ? { [target]: { signature: "", url: `${url.origin}/v1/download?platform=linux` } } : {},
          note: "Signatures are empty until a build signing key exists; the app does not use this yet.",
        });
      }

      // ── Updates ─────────────────────────────────────────────────────────
      // The app asks this every 15 minutes. Answered from a cached copy of the GitHub release list, so a
      // thousand installs polling all day costs a handful of GitHub calls rather than a rate-limit ban.
      if (path === "/v1/updates") {
        const current = url.searchParams.get("version") || "0.0.0";
        const rel = await releases(env);
        if (!rel.latest) return json({ ok: true, checked: nowIso(), update: null, note: rel.note || "" });

        const cmp = compareVersions(rel.latest.version, current);
        const sameMajor = major(rel.latest.version) === major(current);
        return json({
          ok: true,
          checked: nowIso(),
          current,
          latest: rel.latest.version,
          // A minor or patch release is an *update*: same major, safe to apply, covered by every licence.
          update: cmp > 0 && sameMajor ? {
            version: rel.latest.version, notes: rel.latest.notes, assets: rel.latest.assets,
          } : null,
          // A new major version is an *upgrade*: a separate purchase, so it is announced rather than
          // offered, and never applied on its own.
          upgrade: cmp > 0 && !sameMajor ? {
            version: rel.latest.version, notes: rel.latest.notes,
            image: rel.latest.image || "",
            major: major(rel.latest.version),
          } : null,
        });
      }

      // Issue a one-time download password. Trial and paid alike — the download is not the product, the
      // licence is, and making people fight for the installer helps nobody.
      if (path === "/v1/download/otp" && request.method === "POST") {
        const body = await request.json().catch(() => ({}));
        const email = String(body.email || "").toLowerCase();
        const user = email ? await getUser(env, email) : null;
        // No account yet is fine: a trial download is exactly how somebody gets an account.
        const code = otpCode();
        const rel = await releases(env);
        await env.LICENCES.put(`otp:${code}`, JSON.stringify({
          email, created_at: nowIso(), version: rel.latest?.version || "",
          status: user ? entitlementState(user, await config(env)).status : "anonymous",
        // Fifteen minutes: long enough to walk to another machine, short enough that a code in a chat
        // log is worthless by the time anyone finds it.
        }), { expirationTtl: 900 });
        await bump(env, "download_otp");
        return json({
          ok: true, code, expires_in: 900,
          version: rel.latest?.version || "",
          platforms: Object.keys(rel.latest?.assets || {}),
          url: `${url.origin}/v1/download?code=${code}`,
        });
      }

      // Redeem it. The Worker fetches the asset from the *private* repo with its own token and streams it
      // back, which is the only way a private release is downloadable without handing out a GitHub token.
      if (path === "/v1/download") {
        const code = (url.searchParams.get("code") || "").toUpperCase();
        const platform = url.searchParams.get("platform") || "linux";
        const key = `otp:${code}`;
        const ticket = await env.LICENCES.get(key, { type: "json" });
        if (!ticket) return bad("That download code has expired. Ask for a new one.", 410);

        const rel = await releases(env);
        const asset = (rel.latest?.assets || {})[platform];
        if (!asset) return bad(`No build for ${platform} in ${rel.latest?.version || "the latest release"}.`, 404);
        if (!env.GITHUB_TOKEN) return bad("Downloads are not configured yet.", 503);

        // One use. Deleting first means a retry after a failed transfer needs a new code — annoying, and
        // much better than a code that quietly becomes a public mirror.
        await env.LICENCES.delete(key);
        await bump(env, `download_${platform}`);

        const upstream = await fetch(asset.url, {
          headers: {
            "Authorization": `Bearer ${env.GITHUB_TOKEN}`,
            "Accept": "application/octet-stream",
            "User-Agent": "studio-lightkid-worker",
          },
        });
        if (!upstream.ok) return bad(`The build could not be fetched (${upstream.status}).`, 502);
        return new Response(upstream.body, {
          headers: {
            "Content-Type": "application/octet-stream",
            "Content-Disposition": `attachment; filename="${asset.name}"`,
          },
        });
      }

      // ── Admin ───────────────────────────────────────────────────────────
      if (path === "/admin" || path === "/admin/") {
        return new Response(ADMIN_HTML, {
          headers: { "Content-Type": "text/html; charset=utf-8" },
        });
      }
      if (path.startsWith("/admin/api/")) {
        if (!adminOk(request, env)) return bad("not you", 401);
        return await adminApi(path.slice("/admin/api/".length), request, env);
      }

      return bad("no such endpoint", 404);
    } catch (err) {
      // Never leak a stack to a caller; keep it where the admin can read it.
      await env.REPORTS.put(`server:${nowIso()}`, JSON.stringify({
        at: nowIso(), path, message: String(err), stack: String(err?.stack || ""),
      }), { expirationTtl: 86400 * 30 }).catch(() => {});
      return bad("something went wrong on our side", 500);
    }
  },
};

// ── Releases and versions ───────────────────────────────────────────────────

/** Major version as a number. `major("2.3.4") === 2`. */
function major(v) { return Number(String(v).split(".")[0]) || 0; }

/** -1, 0 or 1. Compares numerically per part, so 0.10.0 is correctly newer than 0.9.0. */
function compareVersions(a, b) {
  const pa = String(a).replace(/^v/, "").split(".").map(Number);
  const pb = String(b).replace(/^v/, "").split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    const x = pa[i] || 0, y = pb[i] || 0;
    if (x !== y) return x > y ? 1 : -1;
  }
  return 0;
}

/**
 * The newest release and its assets, cached for ten minutes.
 *
 * Cached because the app polls every fifteen minutes: without this, a hundred installs would spend
 * GitHub's rate limit by lunchtime and every one of them would see "no update available", which is the
 * worst possible failure — indistinguishable from being up to date.
 */
async function releases(env) {
  const cached = await env.CONFIG.get("releases", { type: "json" });
  if (cached && cached.at > new Date(Date.now() - 600_000).toISOString()) return cached;
  if (!env.GITHUB_REPO || !env.GITHUB_TOKEN) {
    return { at: nowIso(), latest: null, note: "No repository configured for updates yet." };
  }
  const r = await fetch(`https://api.github.com/repos/${env.GITHUB_REPO}/releases?per_page=5`, {
    headers: {
      "Authorization": `Bearer ${env.GITHUB_TOKEN}`,
      "Accept": "application/vnd.github+json",
      "User-Agent": "studio-lightkid-worker",
    },
  });
  if (!r.ok) {
    // Serve the stale copy rather than nothing: an old answer is far better than a wrong one.
    return cached || { at: nowIso(), latest: null, note: `GitHub answered ${r.status}` };
  }
  const list = await r.json();
  const published = list.filter((x) => !x.draft);
  const newest = published[0];
  const out = { at: nowIso(), latest: null };
  if (newest) {
    // Match assets to platforms by extension, which is what the build workflow produces.
    const assets = {};
    for (const a of newest.assets || []) {
      const n = a.name.toLowerCase();
      const platform = n.endsWith(".deb") ? "linux"
        : n.endsWith(".appimage") ? "linux-appimage"
        : n.endsWith(".rpm") ? "linux-rpm"
        : n.endsWith(".apk") ? "android"
        : n.endsWith(".dmg") ? "macos"
        : n.endsWith(".exe") || n.endsWith(".msi") ? "windows"
        : null;
      if (platform) assets[platform] = { name: a.name, url: a.url, size: a.size };
    }
    out.latest = {
      version: String(newest.tag_name || "").replace(/^v/, ""),
      notes: String(newest.body || "").slice(0, 4000),
      image: (newest.body || "").match(/!\[[^\]]*\]\(([^)]+)\)/)?.[1] || "",
      assets,
    };
  }
  await env.CONFIG.put("releases", JSON.stringify(out), { expirationTtl: 3600 });
  return out;
}

/** A code somebody can read out over the phone: no I, O, 0 or 1. */
function otpCode() {
  const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
  const rand = crypto.getRandomValues(new Uint8Array(8));
  return [...rand].map((b) => alphabet[b % alphabet.length]).join("").replace(/(.{4})(?=.)/g, "$1-");
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/** One counter per event per day. Cheap, and enough for every question the admin page asks. */
async function bump(env, name) {
  const day = dayKey();
  const key = `count:${day}`;
  const counters = (await env.EVENTS.get(key, { type: "json" })) || {};
  counters[name] = (counters[name] || 0) + 1;
  await env.EVENTS.put(key, JSON.stringify(counters), { expirationTtl: 86400 * 400 });
}

async function sha256hex(text) {
  const buf = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("").slice(0, 40);
}

/** Constant-time-ish HMAC check for the payment webhook. */
async function verifyHmac(secret, raw, givenHex) {
  if (!secret || !givenHex) return false;
  const key = await crypto.subtle.importKey("raw", new TextEncoder().encode(secret),
                                            { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  const mac = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(raw));
  const expected = [...new Uint8Array(mac)].map((b) => b.toString(16).padStart(2, "0")).join("");
  if (expected.length !== givenHex.length) return false;
  let diff = 0;
  for (let i = 0; i < expected.length; i++) diff |= expected.charCodeAt(i) ^ givenHex.charCodeAt(i);
  return diff === 0;
}

function adminOk(request, env) {
  const given = (request.headers.get("Authorization") || "").replace(/^Bearer\s+/i, "").trim();
  return Boolean(env.ADMIN_TOKEN) && given === env.ADMIN_TOKEN;
}

async function adminApi(route, request, env) {
  const body = request.method === "POST" ? await request.json().catch(() => ({})) : {};

  if (route === "overview") {
    // The last 30 days of counters, plus the funnel questions asked directly.
    const days = [];
    for (let i = 0; i < 30; i++) {
      const d = dayKey(new Date(Date.now() - i * 86400_000));
      const c = await env.EVENTS.get(`count:${d}`, { type: "json" });
      if (c) days.push({ day: d, counters: c });
    }
    const users = await env.USERS.list({ limit: 1000 });
    let trial = 0, active = 0, lifetime = 0, expired = 0, blocked = 0;
    const cfg = await config(env);
    const rows = [];
    for (const k of users.keys) {
      const u = await env.USERS.get(k.name, { type: "json" });
      if (!u) continue;
      const st = entitlementState(u, cfg).status;
      if (st === "trial") trial++;
      else if (st === "active") active++;
      else if (st === "lifetime") lifetime++;
      else if (st === "blocked") blocked++;
      else expired++;
      rows.push({
        email: u.email, username: u.username, status: st, plan: u.plan,
        sessions: (u.sessions || []).length,
        places: (u.sessions || []).map((x) => x.place).join(" · "),
        trial_ends: u.trial_ends, period_ends: u.period_ends,
        devices: (u.devices || []).length, created_at: u.created_at, notes: u.notes || "",
      });
    }
    return json({ ok: true, totals: { trial, active, lifetime, expired, blocked, all: rows.length },
                  days, users: rows.sort((a, b) => (b.created_at || "").localeCompare(a.created_at || "")) });
  }

  if (route === "sessions") {
    // The paths people actually took, newest first — this is the "where do they hit subscribe" view.
    const list = await env.EVENTS.list({ prefix: "session:", limit: 200 });
    const out = [];
    for (const k of list.keys.reverse()) {
      const s = await env.EVENTS.get(k.name, { type: "json" });
      if (s) out.push({ key: k.name, ...s });
    }
    return json({ ok: true, sessions: out });
  }

  if (route === "reports") {
    const list = await env.REPORTS.list({ prefix: "report:", limit: 200 });
    const out = [];
    for (const k of list.keys.reverse()) {
      const r = await env.REPORTS.get(k.name, { type: "json" });
      if (r) out.push(r);
    }
    return json({ ok: true, reports: out });
  }

  if (route === "config" && request.method === "POST") {
    const cfg = { ...(await config(env)), ...body };
    await env.CONFIG.put("config", JSON.stringify(cfg));
    return json({ ok: true, config: cfg });
  }
  if (route === "config") return json({ ok: true, config: await config(env) });

  if (route === "terms" && request.method === "POST") {
    await env.CONFIG.put("terms", String(body.markdown || ""));
    return json({ ok: true, saved: true });
  }

  // Every correction you might need at 2am: grant, extend, block, unblock, note.
  if (route === "user" && request.method === "POST") {
    const user = await getUser(env, body.email || "");
    if (!user) return bad("No such account.", 404);
    const cfg = await config(env);
    if (body.grant_months) {
      const from = user.period_ends && user.period_ends > nowIso() ? new Date(user.period_ends) : new Date();
      user.period_ends = new Date(from.getTime() + Number(body.grant_months) * 30 * 86400_000).toISOString();
      user.plan = user.plan || "granted";
    }
    if (body.grant_lifetime) { user.lifetime_major = cfg.lifetime_major || 1; user.plan = "lifetime"; }
    if (body.extend_trial_days) {
      const from = user.trial_ends && user.trial_ends > nowIso() ? new Date(user.trial_ends) : new Date();
      user.trial_ends = new Date(from.getTime() + Number(body.extend_trial_days) * 86400_000).toISOString();
    }
    if (body.blocked !== undefined) user.blocked = Boolean(body.blocked);
    if (body.notes !== undefined) user.notes = String(body.notes).slice(0, 2000);
    if (body.revoke) { user.period_ends = nowIso(); user.lifetime_major = 0; user.plan = ""; }
    await putUser(env, user);
    return json({ ok: true, user, state: entitlementState(user, cfg) });
  }

  if (route === "gift" && request.method === "POST") {
    // A gift code is a licence nobody has claimed yet. Readable on purpose: these get typed by hand.
    const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";   // no I/O/0/1 — they get misread
    const rand = crypto.getRandomValues(new Uint8Array(12));
    const code = [...rand].map((b) => alphabet[b % alphabet.length]).join("").replace(/(.{4})(?=.)/g, "$1-");
    const licence = {
      code, plan: String(body.plan || "monthly"), note: String(body.note || "").slice(0, 200),
      created_at: nowIso(), redeemed_by: "", redeemed_at: "",
    };
    await env.LICENCES.put(`gift:${code}`, JSON.stringify(licence));
    return json({ ok: true, licence });
  }

  if (route === "gifts") {
    const list = await env.LICENCES.list({ prefix: "gift:", limit: 300 });
    const out = [];
    for (const k of list.keys) {
      const g = await env.LICENCES.get(k.name, { type: "json" });
      if (g) out.push(g);
    }
    return json({ ok: true, gifts: out });
  }

  return bad("no such admin route", 404);
}

// The admin page is injected here at deploy time from server/admin/index.html, so it is one artifact to
// deploy and there is no second origin to configure or keep in sync.
const ADMIN_HTML = "__ADMIN_HTML__";

// The public site, inlined the same way and for the same reason: one artifact, same origin as the
// download endpoint it calls.
const SITE_HTML = "__SITE_HTML__";

/** A minimal Markdown renderer for the terms page. Enough for headings, lists, tables, bold and links —
 *  which is all the terms use, and far less than a library would drag in for one page. */
const TERMS_PAGE = `<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>Terms &amp; Privacy</title>
<style>
:root{--bg:#0d1017;--text:#e8eaef;--dim:#98a1b0;--line:#242b38;--accent:#8ad4ff}
@media(prefers-color-scheme:light){:root{--bg:#fbfcfe;--text:#141821;--dim:#5f6875;--line:#e4e8ee}}
body{margin:0;background:var(--bg);color:var(--text);font:16px/1.7 ui-sans-serif,system-ui,sans-serif}
main{max-width:760px;margin:0 auto;padding:40px 20px 80px}
h1{font-size:30px;letter-spacing:-.02em}h2{margin-top:34px;font-size:20px}h3{font-size:16px}
table{border-collapse:collapse;width:100%;font-size:14px;margin:12px 0}
th,td{border-bottom:1px solid var(--line);padding:6px 8px;text-align:left;vertical-align:top}
code{background:rgba(138,212,255,.12);padding:1px 5px;border-radius:4px;font-size:.9em}
a{color:var(--accent)}em{color:var(--dim)}hr{border:0;border-top:1px solid var(--line);margin:28px 0}
</style></head><body><main id="c">Loading…</main><script>
const md = __MARKDOWN__;
const esc = (s) => s.replace(/[<>&]/g, (c) => ({"<":"&lt;",">":"&gt;","&":"&amp;"}[c]));
const inline = (t) => esc(t)
  .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
  .replace(/\*([^*]+)\*/g, "<em>$1</em>")
  .replace(/\`([^\`]+)\`/g, "<code>$1</code>")
  .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>');
const out = []; const lines = md.split("\n"); let i = 0;
while (i < lines.length) {
  const l = lines[i];
  if (/^#{1,4}\s/.test(l)) { const n = l.match(/^#+/)[0].length;
    out.push("<h" + n + ">" + inline(l.replace(/^#+\s*/, "")) + "</h" + n + ">"); i++; continue; }
  if (/^---+$/.test(l.trim())) { out.push("<hr>"); i++; continue; }
  if (l.includes("|") && (lines[i+1] || "").includes("-")) {
    const cells = (r) => r.replace(/^\||\|$/g, "").split("|").map((c) => c.trim());
    const head = cells(l); i += 2; const rows = [];
    while (i < lines.length && lines[i].includes("|")) { rows.push(cells(lines[i])); i++; }
    out.push("<table><thead><tr>" + head.map((h) => "<th>" + inline(h) + "</th>").join("") +
      "</tr></thead><tbody>" + rows.map((r) => "<tr>" + r.map((c) => "<td>" + inline(c) + "</td>").join("") +
      "</tr>").join("") + "</tbody></table>"); continue; }
  if (/^\s*[-*]\s/.test(l)) { const items = [];
    while (i < lines.length && /^\s*[-*]\s/.test(lines[i])) { items.push(lines[i].replace(/^\s*[-*]\s/, "")); i++; }
    out.push("<ul>" + items.map((x) => "<li>" + inline(x) + "</li>").join("") + "</ul>"); continue; }
  if (!l.trim()) { i++; continue; }
  const para = []; while (i < lines.length && lines[i].trim() && !/^#|^\s*[-*]\s|^---+$/.test(lines[i])) { para.push(lines[i]); i++; }
  out.push("<p>" + inline(para.join(" ")) + "</p>");
}
document.getElementById("c").innerHTML = out.join("\n");
</script></body></html>`;
