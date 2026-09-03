#!/usr/bin/env python3
"""Deploy the subscription Worker to Cloudflare.

    python3 deploy.py               # create what is missing, upload, print the URL and keys
    python3 deploy.py --keys-only   # print the keys and URL again, deploy nothing
    python3 deploy.py --rotate-key  # mint a NEW signing key, deploy it, print the public half
    python3 deploy.py --rotate-admin-token   # mint a new admin token, deploy it
    python3 deploy.py --mint-only   # mint a signing key and print it. No network, no credential.

No wrangler: this talks to the REST API, so there is nothing to install and nothing that can be a
different version than last time. Idempotent — running it again updates the script and reuses the KV
namespaces and the signing key it already made.

Python rather than bash on purpose. The first version was a shell script and half of it was fighting
quoting: a JSON body with nested quotes, inside a command substitution, inside a heredoc, in a shell that
parses the whole construct before running any of it. None of that is a real problem to solve, so it is
not worth having.

Secrets: the Cloudflare token is read from server/.secrets (gitignored). The Ed25519 signing key is
generated on the first run — the private half becomes a Worker secret, the public half is printed for
embedding in the app, and neither is committed. `.deploy-state.json` holds them locally and matters more
than it looks: lose it and a new key is generated, at which point every entitlement already on a user's
machine stops verifying.

Rotation is a first-class operation (`--rotate-key`) rather than something to improvise, because the
key in service today is compromised: its private half was committed in v0.88.0 and is still in the
history. See docs/SECURITY-KEY-ROTATION.md.
"""

import json
import os
import pathlib
import secrets
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request

HERE = pathlib.Path(__file__).parent
API = "https://api.cloudflare.com/client/v4"
SCRIPT_NAME = "studio-lightkid"
STATE = HERE / ".deploy-state.json"
# Ed25519 in crypto.subtle needs a compatibility date recent enough to include it.
COMPAT_DATE = "2026-06-01"
NAMESPACES = ["USERS", "LICENCES", "EVENTS", "CONFIG", "REPORTS"]


def token() -> str:
    path = HERE / ".secrets"
    if not path.exists():
        sys.exit("server/.secrets is missing — put the Cloudflare API token in it.")
    return path.read_text().strip()


def call(method: str, path: str, body=None, tok: str = "") -> dict:
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(f"{API}{path}", data=data, method=method)
    req.add_header("Authorization", f"Bearer {tok}")
    if data:
        req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as err:
        # Cloudflare puts the useful part in the body of a 4xx, so read it rather than raising the status.
        try:
            return json.loads(err.read())
        except Exception:
            return {"success": False, "errors": [{"message": f"HTTP {err.code}"}]}


def expect(result: dict, what: str):
    if not result.get("success"):
        sys.exit(f"{what} failed: {json.dumps(result.get('errors'), indent=2)}")
    return result["result"]


def make_signing_key() -> tuple[str, str]:
    """Ed25519, split into the two encodings each side needs.

    PKCS#8 for WebCrypto in the Worker; the raw 32 bytes for verification in the app, which is the last
    32 bytes of the SubjectPublicKeyInfo.
    """
    import base64

    with tempfile.TemporaryDirectory() as d:
        key = os.path.join(d, "k.pem")
        subprocess.run(["openssl", "genpkey", "-algorithm", "ed25519", "-out", key],
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        der = subprocess.run(["openssl", "pkey", "-in", key, "-outform", "DER"],
                             check=True, capture_output=True).stdout
        pub = subprocess.run(["openssl", "pkey", "-in", key, "-pubout", "-outform", "DER"],
                             check=True, capture_output=True).stdout
    b64u = lambda b: base64.urlsafe_b64encode(b).decode().rstrip("=")
    return b64u(der), b64u(pub[-32:])


def state() -> dict:
    if STATE.exists():
        return json.loads(STATE.read_text())
    private, public = make_signing_key()
    s = {"private_key": private, "public_key": public, "admin_token": secrets.token_urlsafe(32)}
    STATE.write_text(json.dumps(s, indent=2))
    STATE.chmod(0o600)
    print("generated a signing key and an admin token")
    return s


def rotate_signing_key() -> str:
    """Mint a new signing key, keep the previous public half, and return the new public half.

    The previous PUBLIC key is kept in the state file — not out of sentiment, but because the app has
    to keep accepting it for a fortnight or the rotation locks out every user holding a token issued a
    minute before the switch (see SUBS_PUBLIC_KEYS in commands/subscription.rs). The previous PRIVATE
    key is discarded here and cannot be recovered, which is the point of rotating.
    """
    s = state()
    private, public = make_signing_key()
    s["previous_public_key"] = s.get("public_key", "")
    s["previous_key_retired_at"] = ""      # filled in by whoever sets accept_until in the app
    s["private_key"] = private
    s["public_key"] = public
    STATE.write_text(json.dumps(s, indent=2))
    STATE.chmod(0o600)
    return public


def rotate_admin_token() -> str:
    s = state()
    s["admin_token"] = secrets.token_urlsafe(32)
    STATE.write_text(json.dumps(s, indent=2))
    STATE.chmod(0o600)
    return s["admin_token"]


def refuse_if_tracked():
    """Stop if the state file is tracked by git.

    This is not hypothetical: `.deploy-state.json` — private signing key and admin token — was
    committed in v0.88.0 and is still reachable in the history. It is gitignored now, but an ignore
    rule does nothing for a file that is already tracked, so the check is on `git ls-files` rather
    than on the ignore rules.
    """
    try:
        tracked = subprocess.run(["git", "ls-files", "--error-unmatch", str(STATE)],
                                 cwd=HERE, capture_output=True).returncode == 0
    except FileNotFoundError:
        return          # no git here; nothing to protect against
    if tracked:
        sys.exit(f"{STATE.name} is tracked by git. Run `git rm --cached {STATE.name}` before "
                 f"deploying — every key it holds is public the moment it is pushed.")


def bundle() -> bytes:
    """Inline the admin page into the Worker.

    One artifact to deploy, and no second origin that can drift out of step with the API it calls.
    """
    worker = (HERE / "worker.js").read_text()
    admin = (HERE / "admin" / "index.html").read_text()
    site = (HERE / "site" / "index.html").read_text()
    worker = worker.replace('"__ADMIN_HTML__"', json.dumps(admin))
    worker = worker.replace('"__SITE_HTML__"', json.dumps(site))
    print(f"bundled worker + admin ({len(worker) // 1024} KB)")
    return worker.encode()


def upload(account: str, tok: str, script: bytes, metadata: dict):
    """Multipart PUT, assembled by hand — the stdlib has no multipart writer and this needs two parts."""
    boundary = "----slk" + secrets.token_hex(8)
    parts = []

    def part(name: str, payload: bytes, filename: str, ctype: str):
        head = (f"--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; "
                f"filename=\"{filename}\"\r\nContent-Type: {ctype}\r\n\r\n")
        parts.append(head.encode() + payload + b"\r\n")

    part("metadata", json.dumps(metadata).encode(), "metadata.json", "application/json")
    part("worker.js", script, "worker.js", "application/javascript+module")
    body = b"".join(parts) + f"--{boundary}--\r\n".encode()

    req = urllib.request.Request(
        f"{API}/accounts/{account}/workers/scripts/{SCRIPT_NAME}", data=body, method="PUT")
    req.add_header("Authorization", f"Bearer {tok}")
    req.add_header("Content-Type", f"multipart/form-data; boundary={boundary}")
    try:
        with urllib.request.urlopen(req, timeout=120) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as err:
        try:
            return json.loads(err.read())
        except Exception:
            return {"success": False, "errors": [{"message": f"HTTP {err.code}"}]}


def main():
    refuse_if_tracked()

    # Minting is local: an Ed25519 pair out of `secrets`, written to the state file. It needed the
    # Cloudflare token only because it sat below `token()` in this function and so inherited the
    # deploy step's requirements — which is why rotating the compromised key looked like it was
    # gated on a credential when it never was. `--mint-only` is the mint with nothing attached, so
    # the key can be made on the machine that will hold its private half.
    if "--mint-only" in sys.argv:
        new_public = rotate_signing_key()
        print(f"minted a new signing key.\n"
              f"  public half  {new_public}\n"
              f"  private half stays in {STATE.name} (chmod 600, gitignored)\n\n"
              f"Add the public half to SUBS_PUBLIC_KEYS in src-tauri/commands/subscription.rs, above\n"
              f"the key it replaces, and give that one an accept_until about a fortnight out. Then\n"
              f"deploy so the server signs with the new private half:  python3 deploy.py\n")
        return

    tok = token()

    rotated_key = "--rotate-key" in sys.argv
    if rotated_key:
        new_public = rotate_signing_key()
        print(f"minted a new signing key; its public half is {new_public}")
    if "--rotate-admin-token" in sys.argv:
        rotate_admin_token()
        print("minted a new admin token")

    keys = state()
    accounts = expect(call("GET", "/accounts", tok=tok), "reading the account")
    account = accounts[0]["id"]
    print(f"account: {account[:8]}…")

    if "--keys-only" not in sys.argv:
        # KV namespaces, split by lifetime rather than convenience: users are permanent, events expire,
        # reports expire on a different clock, licences are permanent but rarely read, config is tiny
        # and read constantly.
        existing = {n["title"]: n["id"] for n in
                    expect(call("GET", f"/accounts/{account}/storage/kv/namespaces?per_page=100", tok=tok),
                           "listing KV")}
        ids = {}
        for name in NAMESPACES:
            title = f"slk-{name.lower()}"
            if title in existing:
                ids[name] = existing[title]
                print(f"reusing KV {title}")
            else:
                made = expect(call("POST", f"/accounts/{account}/storage/kv/namespaces",
                                   {"title": title}, tok), f"creating KV {title}")
                ids[name] = made["id"]
                print(f"created KV {title}")

        bindings = [{"type": "kv_namespace", "name": n, "namespace_id": ids[n]} for n in NAMESPACES]
        bindings += [
            # secret_text means the value cannot be read back out through the API afterwards.
            {"type": "secret_text", "name": "SIGNING_KEY_PKCS8", "text": keys["private_key"]},
            {"type": "secret_text", "name": "ADMIN_TOKEN", "text": keys["admin_token"]},
            {"type": "secret_text", "name": "LS_WEBHOOK_SECRET",
             "text": os.environ.get("LS_WEBHOOK_SECRET", "not-set-yet")},
            # A GitHub token with `contents:read` on the private repo. The Worker uses it to read the
            # release list and to stream release assets back to a user holding a valid OTP — which is
            # the only way a private repo's builds are downloadable without handing out a token.
            {"type": "secret_text", "name": "GITHUB_TOKEN",
             "text": os.environ.get("GITHUB_TOKEN", "")},
            {"type": "plain_text", "name": "GITHUB_REPO",
             "text": os.environ.get("GITHUB_REPO", "joehannes/biblemusically")},
        ]
        metadata = {"main_module": "worker.js", "compatibility_date": COMPAT_DATE, "bindings": bindings}

        print("uploading…")
        result = upload(account, tok, bundle(), metadata)
        expect(result, "deploying the Worker")
        print("deployed")

        call("POST", f"/accounts/{account}/workers/scripts/{SCRIPT_NAME}/subdomain",
             {"enabled": True}, tok)

    sub = call("GET", f"/accounts/{account}/workers/subdomain", tok=tok).get("result", {}).get("subdomain", "")
    base = f"https://{SCRIPT_NAME}.{sub}.workers.dev" if sub else "(no workers.dev subdomain yet)"
    print("\n" + "─" * 62)
    print(f" URL:          {base}")
    print(f" admin:        {base}/admin")
    print(f" admin token:  {keys['admin_token']}")
    print(f" public key:   {keys['public_key']}")
    if keys.get("previous_public_key"):
        print(f" previous key: {keys['previous_public_key']}  (must stay accepted for a fortnight)")
    print("─" * 62)
    if rotated_key:
        print(" The new key is live on the server. NOTHING VERIFIES IT YET — do this now:")
        print("   1. In src-tauri/commands/subscription.rs, put the new key at the top of")
        print("      SUBS_PUBLIC_KEYS with `accept_until: None`.")
        print("   2. Give the entry that was on top `accept_until: Some(<unix, a fortnight out>)`,")
        print("      so tokens already on users' machines keep working while they update.")
        print("   3. Ship a release, then delete the old entry after that date.")
        print("  Until step 3 reaches users, anyone still on an older build cannot be signed in by")
        print("  the new key — which is what the fortnight is for.")
    else:
        print(" Embed the public key in the app (SUBS_PUBLIC_KEYS in commands/subscription.rs).")
    print(" Before selling: LS_WEBHOOK_SECRET='…' python3 deploy.py")


if __name__ == "__main__":
    main()
