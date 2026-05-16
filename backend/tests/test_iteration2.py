"""Iteration 2 backend tests: Bible sources, AI Composer, OAuth callback, regressions."""
import os, time, pytest, requests

BASE = os.environ['REACT_APP_BACKEND_URL'].rstrip('/') if os.environ.get('REACT_APP_BACKEND_URL') else None
if not BASE:
    with open('/app/frontend/.env') as f:
        for line in f:
            if line.startswith('REACT_APP_BACKEND_URL='):
                BASE = line.split('=', 1)[1].strip().rstrip('/')

API = f"{BASE}/api"
S = requests.Session()
S.headers.update({"Content-Type": "application/json"})


def no_id(d):
    if isinstance(d, dict):
        return "_id" not in d and all(no_id(v) for v in d.values())
    if isinstance(d, list):
        return all(no_id(x) for x in d)
    return True


def get_with_retry(url, params=None, retries=2):
    """Network calls to third-party Bible APIs can flake — retry once."""
    last = None
    for _ in range(retries + 1):
        try:
            r = S.get(url, params=params, timeout=30)
            if r.status_code < 500:
                return r
            last = r
        except requests.RequestException as e:
            last = e
        time.sleep(1.5)
    if isinstance(last, requests.Response):
        return last
    raise last


# ---------- Bible: Translations & Books ----------
def test_bible_translations_dict_keyed_by_language():
    r = S.get(f"{API}/bible/translations")
    assert r.status_code == 200
    j = r.json()
    assert isinstance(j, dict)
    # iter5: "English" was split into "English (modern)" + "English (literal / original-meaning)"
    expected_langs = {"German", "Italian", "Russian", "Spanish", "French", "Portuguese"}
    assert expected_langs.issubset(set(j.keys())), f"missing langs: {expected_langs - set(j.keys())}"
    # At least one English-* group must exist
    assert any(k.startswith("English") for k in j.keys()), f"no English group present; got {list(j.keys())}"
    # Each value must be a non-empty list of translation objects
    for lang, items in j.items():
        assert isinstance(items, list) and len(items) > 0
        for t in items:
            assert "id" in t and "name" in t and "source" in t
    # Spot check key translations (English ids spread across both English-* groups)
    eng_ids = {t["id"] for k, items in j.items() if k.startswith("English") for t in items}
    assert {"web", "kjv", "bbe", "asv"}.issubset(eng_ids)
    assert "deu_l12" in {t["id"] for t in j["German"]}
    assert "rus_syn" in {t["id"] for t in j["Russian"]}
    assert "ita_dio" in {t["id"] for t in j["Italian"]}
    assert no_id(j)


def test_bible_books_17_objects():
    r = S.get(f"{API}/bible/books")
    assert r.status_code == 200
    j = r.json()
    assert isinstance(j, list)
    assert len(j) == 17
    for b in j:
        assert "id" in b and "name" in b and "chapters" in b
        assert isinstance(b["chapters"], int) and b["chapters"] > 0
    ids = {b["id"] for b in j}
    assert {"genesis", "john", "psalms", "revelation"}.issubset(ids)
    assert no_id(j)


# ---------- Bible: Chapter fetch (LIVE third-party) ----------
def test_bible_chapter_web_john_1():
    r = get_with_retry(f"{API}/bible/chapter", params={"translation": "web", "book": "john", "chapter": 1})
    assert r.status_code == 200, r.text[:300]
    j = r.json()
    assert j["translation"] == "web"
    verses = j["verses"]
    assert isinstance(verses, list)
    assert len(verses) == 51, f"expected 51 verses, got {len(verses)}"
    assert verses[0]["verse"] == 1
    assert "beginning" in verses[0]["text"].lower() or "Word" in verses[0]["text"]
    assert no_id(j)


def test_bible_chapter_deu_l12_john_1_german():
    r = get_with_retry(f"{API}/bible/chapter", params={"translation": "deu_l12", "book": "john", "chapter": 1})
    assert r.status_code == 200, r.text[:300]
    j = r.json()
    assert j["translation"] == "deu_l12"
    verses = j["verses"]
    assert isinstance(verses, list)
    assert len(verses) == 51, f"expected 51 German verses, got {len(verses)}"
    assert verses[0]["verse"] == 1
    assert len(verses[0]["text"]) > 0
    assert no_id(j)


def test_bible_chapter_rus_syn_john_1():
    r = get_with_retry(f"{API}/bible/chapter", params={"translation": "rus_syn", "book": "john", "chapter": 1})
    # Allow upstream 502 to mark as xfail but log
    if r.status_code >= 500:
        pytest.skip(f"helloao upstream {r.status_code} for rus_syn — flaky network")
    assert r.status_code == 200, r.text[:300]
    j = r.json()
    assert len(j["verses"]) == 51, f"expected 51 Russian verses, got {len(j['verses'])}"


def test_bible_chapter_ita_dio_john_1():
    r = get_with_retry(f"{API}/bible/chapter", params={"translation": "ita_dio", "book": "john", "chapter": 1})
    if r.status_code >= 500:
        pytest.skip(f"helloao upstream {r.status_code} for ita_dio — flaky network")
    assert r.status_code == 200, r.text[:300]
    j = r.json()
    assert len(j["verses"]) == 51, f"expected 51 Italian verses, got {len(j['verses'])}"


# ---------- AI Composer config ----------
def test_compose_config_put_and_get():
    payload = {"globalTheme": "TEST_theme_iter2", "languages": ["English", "German"],
               "mj_params": "--ar 16:9 --v 8.1", "extra": {"nested": True}}
    r = S.put(f"{API}/ai/compose-config", json=payload)
    assert r.status_code == 200
    g = S.get(f"{API}/ai/compose-config")
    assert g.status_code == 200
    j = g.json()
    assert j.get("globalTheme") == "TEST_theme_iter2"
    assert j.get("languages") == ["English", "German"]
    assert j.get("extra") == {"nested": True}
    assert no_id(j)


# ---------- AI Composer compose-lyrics (no openrouter key -> mock error path) ----------
def test_compose_lyrics_without_key_returns_200_with_error():
    # Ensure openrouter_api_key is empty
    cur = S.get(f"{API}/settings").json()
    cur["openrouter_api_key"] = ""
    S.put(f"{API}/settings", json=cur)

    payload = {"chapter_text": "In the beginning was the Word.",
               "sections": [], "targets": [{"language": "English", "styles": ["epic cinematic"]}],
               "themes": {"global": "light vs darkness"}, "mj_params": "--ar 16:9 --v 8.1",
               "style_keywords": ["cinematic"]}
    r = S.post(f"{API}/ai/compose-lyrics", json=payload)
    assert r.status_code == 200, f"expected 200 not {r.status_code}: {r.text[:300]}"
    j = r.json()
    assert "error" in j
    assert "items" in j and j["items"] == []
    assert no_id(j)


# ---------- OAuth callback ----------
def test_oauth_callback_missing_code_state_returns_400():
    r = S.get(f"{API}/oauth/callback", allow_redirects=False)
    assert r.status_code == 400
    assert "OAuth error" in r.text or "missing" in r.text.lower()
    assert "text/html" in r.headers.get("content-type", "").lower()


def test_oauth_callback_with_invalid_code_returns_400():
    # Ensure client creds are present so we hit token exchange path
    cur = S.get(f"{API}/settings").json()
    cur["google_client_id"] = "TEST_client_id.apps.googleusercontent.com"
    cur["google_client_secret"] = "TEST_secret"
    cur["google_redirect_uri"] = "http://localhost/oauth/callback"
    S.put(f"{API}/settings", json=cur)
    r = S.get(f"{API}/oauth/callback",
              params={"code": "INVALID_TEST_CODE", "state": "fake-channel-id"},
              allow_redirects=False)
    # Should be 400 (token exchange failed) — not 500
    assert r.status_code == 400, f"expected 400 not {r.status_code}: {r.text[:300]}"
    assert "text/html" in r.headers.get("content-type", "").lower()


# ---------- Regression: graceful fallbacks ----------
@pytest.fixture(scope="module")
def project_id():
    r = S.post(f"{API}/projects", json={"name": "TEST_iter2_proj", "topic": "John 1"})
    pid = r.json()["id"]
    yield pid
    S.delete(f"{API}/projects/{pid}")


@pytest.fixture(scope="module")
def song_id(project_id):
    payload = {"items": [{"title": "TEST iter2 song", "language": "English",
                          "styles": "epic cinematic",
                          "lyrics": "In the beginning was the Word\nAnd the Word was with God",
                          "annotations": "[radiant light over dark cosmos]\nIn the beginning was the Word\n[divine warm glow]\nAnd the Word was with God",
                          "image_styles": "--ar 16:9 --v 8.1"}]}
    r = S.post(f"{API}/projects/{project_id}/lyrics-import", json=payload)
    return r.json()["songs"][0]["id"]


def _wait_job(jid, timeout=15):
    for _ in range(timeout):
        time.sleep(1)
        j = S.get(f"{API}/jobs/{jid}").json()
        if j["status"] in ("done", "failed"):
            return j
    return j


def test_regression_music_falls_back_to_mock(song_id):
    # Clear suno cookie
    cur = S.get(f"{API}/settings").json()
    cur["suno_cookie"] = ""
    S.put(f"{API}/settings", json=cur)
    r = S.post(f"{API}/songs/{song_id}/generate-music")
    jid = r.json()["id"]
    j = _wait_job(jid)
    assert j["status"] == "done"
    assert j["result"].get("real") is False
    assert "mock" in j["result"].get("audio_url", "")


def test_regression_analyze_and_image_fallback(song_id):
    # analyze
    r = S.post(f"{API}/songs/{song_id}/analyze")
    j = _wait_job(r.json()["id"])
    assert j["status"] == "done"
    secs = S.get(f"{API}/songs/{song_id}/sections").json()
    assert len(secs) >= 2
    # image fallback (no mj_proxy_url)
    cur = S.get(f"{API}/settings").json()
    cur["mj_proxy_url"] = ""
    S.put(f"{API}/settings", json=cur)
    r = S.post(f"{API}/sections/{secs[0]['id']}/generate-image")
    j = _wait_job(r.json()["id"])
    assert j["status"] == "done"
    updated = S.get(f"{API}/songs/{song_id}/sections").json()
    assert "picsum.photos" in updated[0]["image_url"]
    assert len(updated[0]["image_variants"]) == 4


def test_regression_compose_video_fallback(song_id):
    r = S.post(f"{API}/songs/{song_id}/compose-video")
    j = _wait_job(r.json()["id"])
    assert j["status"] == "done"
    assert "/api/media/video/" in j["result"]["video_url"]


def test_regression_channels_and_upload(song_id):
    cr = S.post(f"{API}/channels", json={"name": "TEST_iter2_chan", "language": "English"})
    assert cr.status_code == 200
    cid = cr.json()["id"]
    # oauth-start URL builder
    cur = S.get(f"{API}/settings").json()
    cur["google_client_id"] = "TEST_iter2_clientid.apps.googleusercontent.com"
    S.put(f"{API}/settings", json=cur)
    o = S.post(f"{API}/channels/{cid}/oauth-start").json()
    assert "accounts.google.com" in o.get("url", "")
    # upload
    up = S.post(f"{API}/uploads", json={"song_id": song_id, "channel_id": cid,
                                        "title": "TEST iter2 upload", "privacy": "private"}).json()
    p = S.post(f"{API}/uploads/{up['id']}/publish")
    j = _wait_job(p.json()["id"])
    assert j["status"] == "done"
    assert j["result"]["real"] is False
    assert j["result"]["youtube_video_id"].startswith("yt_")
    # cleanup
    S.delete(f"{API}/channels/{cid}")


def test_no_id_global_check():
    """Spot-check _id exclusion across multiple endpoints."""
    endpoints = ["/", "/settings", "/projects", "/channels", "/uploads", "/jobs",
                 "/bible/translations", "/bible/books", "/effects-presets", "/ai/compose-config"]
    for ep in endpoints:
        r = S.get(f"{API}{ep}")
        assert r.status_code == 200, f"{ep} -> {r.status_code}"
        assert no_id(r.json()), f"_id leak in {ep}"
