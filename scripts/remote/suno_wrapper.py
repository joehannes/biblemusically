"""Suno wrapper as a Modal web endpoint — the fallback route.

Deploy once, paste the printed URL into Settings → Music engines → Suno wrapper URL:

    pip install modal && modal setup
    modal secret create bm-suno SUNO_COOKIE='<the whole Cookie header from a signed-in suno.com tab>'
    modal deploy scripts/remote/suno_wrapper.py

WHY THIS EXISTS. The app talks to Suno directly, which needs no hosting at all. But Suno has no
official API, so the direct route breaks whenever they change their front end — and it breaks at 3am,
when the cookie expires and nobody is there to paste a new one. With this deployed, the app falls back
to it automatically after an unanswered prompt, so a night's generation is not lost to a dialog nobody
saw.

It also solves a second problem: the cookie lives *here*, as a Modal secret, rather than on the phone.
A phone that never holds the cookie cannot leak it, and re-capturing it means updating one secret rather
than every device.

The API shape deliberately matches the community wrappers (gcui-art/suno-api and the FastAPI one), so
the app's fallback path works against this or against either of those with only a URL change:

    POST /api/custom_generate   { prompt, tags, title, make_instrumental }
    POST /api/generate          { prompt, make_instrumental }          (description mode)
    GET  /api/get?ids=a,b

CAVEATS, the same ones the app states: unofficial, for research; the cookie expires; Suno can
rate-limit or suspend an account. `BM_WORKER_TOKEN` in the same secret is required as a bearer token if
present, so a public URL is not an open relay to your Suno account.
"""

import os
import time

import modal

image = modal.Image.debian_slim(python_version="3.12").pip_install("httpx", "fastapi")

app = modal.App("bm-suno")

# These move. They moved from studio-api.suno.ai to studio-api.prod.suno.com, and they will move again —
# so they are environment variables with today's defaults rather than constants to redeploy over.
API_BASE = os.environ.get("SUNO_API_BASE", "https://studio-api.prod.suno.com")
CLERK_BASE = os.environ.get("SUNO_CLERK_BASE", "https://clerk.suno.com")
CLERK_VERSION = "5.65.0"
# Suno's front end is a browser; a request that does not look like one gets a different answer.
UA = ("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) "
      "Chrome/126.0 Safari/537.36")


def _jwt(client, cookie: str) -> str:
    """Cookie → active session → fresh JWT. Called before every request, which is what keeps a
    session alive indefinitely while the cookie itself is valid."""
    who = client.get(f"{CLERK_BASE}/v1/client?_clerk_js_version={CLERK_VERSION}",
                     headers={"Cookie": cookie, "User-Agent": UA})
    if who.status_code in (401, 403):
        raise RuntimeError("SUNO_COOKIE_EXPIRED")
    body = who.json()
    session = (body.get("response") or {}).get("last_active_session_id") \
        or body.get("last_active_session_id")
    if not session:
        raise RuntimeError("SUNO_COOKIE_EXPIRED")

    token = client.post(
        f"{CLERK_BASE}/v1/client/sessions/{session}/tokens?_clerk_js_version={CLERK_VERSION}",
        headers={"Cookie": cookie, "User-Agent": UA, "Content-Length": "0"},
    )
    if token.status_code in (401, 403):
        raise RuntimeError("SUNO_COOKIE_EXPIRED")
    jwt = token.json().get("jwt")
    if not jwt:
        raise RuntimeError("Suno returned no token for this session")
    return jwt


def _authorised(authorization: str) -> bool:
    expected = os.environ.get("BM_WORKER_TOKEN", "")
    if not expected:
        return True
    return authorization.removeprefix("Bearer ").strip() == expected


@app.function(image=image, secrets=[modal.Secret.from_name("bm-suno", required_keys=["SUNO_COOKIE"])],
              timeout=300)
@modal.fastapi_endpoint(method="POST", docs=False)
def custom_generate(body: dict, authorization: str = ""):
    """Custom mode: `prompt` is the lyrics, `tags` is the style."""
    if not _authorised(authorization):
        return {"error": "bad or missing bearer token"}
    import httpx
    cookie = os.environ["SUNO_COOKIE"]
    with httpx.Client(timeout=60) as client:
        try:
            jwt = _jwt(client, cookie)
        except RuntimeError as err:
            # Said plainly, because the app distinguishes this from a transient failure and asks for a
            # fresh cookie only when it is really this.
            return {"error": str(err)}
        r = client.post(
            f"{API_BASE}/api/generate/v2/",
            headers={"Authorization": f"Bearer {jwt}", "User-Agent": UA},
            json={
                "prompt": body.get("prompt", ""),
                "tags": body.get("tags", ""),
                "title": body.get("title", ""),
                "mv": body.get("mv", "chirp-v4"),
                # A real boolean: the string "false" is truthy on the far side and silently produces an
                # instrumental.
                "make_instrumental": bool(body.get("make_instrumental", False)),
                "continue_clip_id": None,
                "continue_at": None,
            },
        )
        if r.status_code in (401, 403):
            return {"error": "SUNO_COOKIE_EXPIRED"}
        if r.status_code >= 300:
            return {"error": f"Suno refused the request ({r.status_code})", "detail": r.text[:400]}
        return r.json()


@app.function(image=image, secrets=[modal.Secret.from_name("bm-suno", required_keys=["SUNO_COOKIE"])],
              timeout=300)
@modal.fastapi_endpoint(method="POST", docs=False)
def generate(body: dict, authorization: str = ""):
    """Description mode: Suno writes the lyrics from `prompt`."""
    if not _authorised(authorization):
        return {"error": "bad or missing bearer token"}
    import httpx
    cookie = os.environ["SUNO_COOKIE"]
    with httpx.Client(timeout=60) as client:
        try:
            jwt = _jwt(client, cookie)
        except RuntimeError as err:
            return {"error": str(err)}
        r = client.post(
            f"{API_BASE}/api/generate/v2/",
            headers={"Authorization": f"Bearer {jwt}", "User-Agent": UA},
            json={
                "gpt_description_prompt": body.get("prompt", ""),
                # Present but empty, which is what the endpoint wants in this mode.
                "prompt": "",
                "tags": body.get("tags", ""),
                "mv": body.get("mv", "chirp-v4"),
                "make_instrumental": bool(body.get("make_instrumental", False)),
            },
        )
        if r.status_code in (401, 403):
            return {"error": "SUNO_COOKIE_EXPIRED"}
        if r.status_code >= 300:
            return {"error": f"Suno refused the request ({r.status_code})", "detail": r.text[:400]}
        return r.json()


@app.function(image=image, secrets=[modal.Secret.from_name("bm-suno", required_keys=["SUNO_COOKIE"])],
              timeout=300)
@modal.fastapi_endpoint(method="GET", docs=False)
def get(ids: str = "", authorization: str = ""):
    """Poll clips. `ids` is comma-separated, matching the community wrappers."""
    if not _authorised(authorization):
        return {"error": "bad or missing bearer token"}
    if not ids:
        return {"error": "no ids"}
    import httpx
    cookie = os.environ["SUNO_COOKIE"]
    with httpx.Client(timeout=60) as client:
        try:
            jwt = _jwt(client, cookie)
        except RuntimeError as err:
            return {"error": str(err)}
        r = client.get(f"{API_BASE}/api/feed/?ids={ids}",
                       headers={"Authorization": f"Bearer {jwt}", "User-Agent": UA})
        if r.status_code in (401, 403):
            return {"error": "SUNO_COOKIE_EXPIRED"}
        return r.json()


@app.function(image=image, secrets=[modal.Secret.from_name("bm-suno", required_keys=["SUNO_COOKIE"])])
@modal.fastapi_endpoint(method="GET", docs=False)
def health(authorization: str = ""):
    """Is the stored cookie still good? Lets the app check without spending a generation."""
    if not _authorised(authorization):
        return {"error": "bad or missing bearer token"}
    import httpx
    with httpx.Client(timeout=30) as client:
        try:
            _jwt(client, os.environ["SUNO_COOKIE"])
            return {"ok": True, "checked_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
        except Exception as err:  # noqa: BLE001 — the whole point is to report any failure as a string
            return {"ok": False, "error": str(err)}
