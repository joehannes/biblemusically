# The signing key is compromised, and how to replace it

The Ed25519 private key that signs subscription entitlements was committed to this repository and is
still reachable in its history. **It is also still the key in service.** Anyone who has ever cloned
this repo can mint themselves an entitlement — including a lifetime one — that the app verifies
without complaint.

**Minting the replacement needs nothing but this machine.** That was misstated here for a while —
this document used to say rotation needed the Cloudflare account, and it does not. Minting an
Ed25519 pair is local arithmetic over the OS entropy source; the account is needed only to *deploy*
the new private half to the Worker that signs. Minting had inherited the deploy step's requirements
because it sat below `token()` in `deploy.py`'s `main()`, which made a local operation look like a
credentialled one for no reason.

So there are now two ways to mint and neither needs an account: **Account → the signing-key card**
in the app, which keeps the private half in the app's encrypted vault and hands you the exact edit,
or `python3 deploy.py --mint-only` on the command line. The rest was already prepared: the app
accepts more than one key, and CI refuses to carry a credential again.

**Mint it on the machine that will sign with it.** A public key whose private half lives somewhere
that no longer exists is worse than a compromised one — nothing can sign for it, so every entitlement
stops verifying and there is no way back. This is the one part of the procedure that cannot be done
for you by somebody else, and it is why the app grew a button for it rather than a longer runbook.

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

### 1. Mint the new key

Either way works, and both are local — no network, no account, nothing to install.

**In the app.** Account → "This build still trusts the leaked signing key" → *Mint a new key on this
machine*. The card appears **only in a source build** (`npm run tauri dev`), not in a packaged
release: which key a release trusts is not a user's business, and a warning nobody can act on is
alarm rather than information.

The pair is signed-and-verified against itself before either half is shown, so a mismatched pair
cannot reach step 2 — a rotation that shipped one would lock out every user, and silently, since the
app would simply stop believing tokens. The private half goes into the app's vault
(XChaCha20-Poly1305, `vault.rs`) and is shown once, in either encoding: **PKCS#8** for WebCrypto's
`importKey`, which is what the Worker loads, or the **raw 32-byte seed** for anything wanting bare
bytes. The card hands you the exact snippet for step 2.

**On the command line.**

```bash
cd server
python3 deploy.py --mint-only
```

Writes the pair into `.deploy-state.json` (chmod 600, gitignored) and prints the public half.

Whichever you used, the private half now has to reach whatever signs entitlements. For the Cloudflare
Worker that is `python3 deploy.py`, which uploads it as the Worker secret `SIGNING_KEY_PKCS8` — that
step, and only that step, needs `server/.secrets`. Rotate the admin token in the same pass:

```bash
python3 deploy.py --rotate-admin-token
```

The previous private key is discarded and cannot be recovered, which is the point. The new admin
token is printed; anything that used the old one has to be updated.

### 2. Teach the app about it

The app's card gives you this snippet with both values already filled in. By hand, in
`src-tauri/commands/subscription.rs`, `SUBS_PUBLIC_KEYS` becomes two entries:

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

Once this ships, the app's card flips to "the signing key is this machine's own". Until then it says
the rotation is half-done — this machine can sign for a key the shipped build does not trust — which
is the state it is easiest to walk away from and forget about.

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
