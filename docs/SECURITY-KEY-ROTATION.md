# The signing key is compromised, and how to replace it

The Ed25519 private key that signs subscription entitlements was committed to this repository and is
still reachable in its history. **It is also still the key in service.** Anyone who has ever cloned
this repo can mint themselves an entitlement — including a lifetime one — that the app verifies
without complaint.

This is written down because rotating it is a production credential operation that needs the
Cloudflare account, so it cannot be done from a coding session. Everything that *could* be prepared
in advance has been: the app accepts more than one key, `deploy.py` can mint and deploy a new one,
and CI now refuses to carry a credential again.

## What leaked

`server/.deploy-state.json`, added in **v0.88.0** (`012bde5`) and removed in `feee638`. Removing it
changed nothing — the blob is still in the history, and the values in it are still live:

| Value | Status | What it grants |
| --- | --- | --- |
| `private_key` (PKCS#8 Ed25519) | **live** — this is the key the app verifies today | Mint any entitlement, any status, any expiry |
| `admin_token` | **live** unless already rotated | Full access to `/admin` — every account, every licence |
| `public_key` | fine, it is public by design | Nothing |

Verified rather than assumed: the `public_key` in that blob is byte-identical to the key compiled
into `SUBS_PUBLIC_KEYS` in `src-tauri/commands/subscription.rs`.

The Cloudflare API token in `server/.secrets` was **not** committed; only these two.

## Why it had not been rotated

Because doing it naively locks out paying users. An entitlement lasts about a day and the app keeps
accepting a stale one for a further week of grace. The instant the server starts signing with a new
key, every token already on every user's disk stops verifying — including for people who are offline,
which is exactly who the grace window exists for.

So the app now accepts a **list** of keys, each with an optional retirement date. During the overlap
both the old and the new signature verify; after the date, the old one stops being accepted whatever
a token claims. Both properties are tested (`during_the_overlap_both_the_new_and_the_retired_key_verify`,
`past_its_retirement_the_old_key_stops_being_accepted`).

## The procedure

Roughly fifteen minutes, plus a release.

### 1. Rotate the key and the admin token

```bash
cd server
python3 deploy.py --rotate-key --rotate-admin-token
```

This mints a new pair, uploads the private half as the Worker secret `SIGNING_KEY_PKCS8`, keeps the
*previous public* half in the local state file, and prints the new public key. The previous private
key is discarded and cannot be recovered — which is the point.

The new admin token is printed too. Anything that used the old one has to be updated.

### 2. Teach the app about it

In `src-tauri/commands/subscription.rs`, `SUBS_PUBLIC_KEYS` becomes two entries:

```rust
pub const SUBS_PUBLIC_KEYS: &[SubsKey] = &[
    SubsKey { b64: "<the key deploy.py just printed>", accept_until: None },
    // The compromised key. Accepted until <date> so tokens already issued keep working.
    SubsKey { b64: "9-9bAxvvDtG98OKRR8xn3OeOHk0S0aruy4UA8FUmQwY",
              accept_until: Some(1_770_000_000) },   // ← a fortnight from the rotation
];
```

`date -d '+14 days' +%s` gives the number. Exactly one entry may have `accept_until: None`; a test
enforces that, because two live signers is not a rotation.

### 3. Ship a release

Users on an older build verify only the old key, so until they update they can be signed in only by
that key — which the server is no longer using. **The fortnight is the window in which they must
update.** Say so in the release notes; it is the one user-visible consequence.

### 4. After the retirement date

Delete the compromised entry. The test in step 2 keeps passing either way; nothing else changes.

## What this does *not* fix

**The blob is still in the history.** Rotating makes the leaked key worthless, which is the part that
matters. Removing the blob is separate, destructive, and mostly cosmetic once the key is dead:

```bash
git filter-repo --path server/.deploy-state.json --invert-paths   # rewrites every commit id
git push --force --all
```

Every clone breaks, every open PR has to be recreated, and GitHub keeps unreachable blobs accessible
through its API until asked to garbage-collect them — so the rewrite alone does not make the value
private again. **Rotate first; treat the rewrite as tidying, not as a remedy.** If the repository was
public at any point while that commit was reachable, assume the values were scraped.

## What stops it happening again

`npm run audit:secrets` runs in CI beside the other gates. It checks git's *index* — what is actually
tracked — rather than the ignore rules, because an ignore rule does nothing about a file that is
already tracked, which is precisely how this one got in. It refuses a file by name
(`.deploy-state.json`, `.secrets`, `*.key`, `.env`, `kaggle.json`, a service-account key) and by
contents (PKCS#8, PEM, OpenSSH private keys). Verified against the real blob from `012bde5`: it is
caught both ways.

`deploy.py` also refuses to run at all if the state file is tracked, before it reads any credential.
