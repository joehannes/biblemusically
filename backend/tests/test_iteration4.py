"""Iteration 4 backend tests - bulk upload automation.

Covers:
- POST /api/uploads/bulk-from-videos (dupe-skip, matching by language)
- GET  /api/uploads/preflight (need_oauth / ready / pending_uploads)
- POST /api/uploads/ai-enrich (deterministic fallback contains global_description + language hashtag)
- POST /api/channels/connect-all-urls (only NOT-connected channels with resolvable client)
- Mongo _id exclusion across all new responses.
"""
import os
import time
import re
import pytest
import requests
from pathlib import Path
from dotenv import load_dotenv

load_dotenv(Path(__file__).resolve().parents[2] / "frontend" / ".env")
BASE = os.environ['REACT_APP_BACKEND_URL'].rstrip('/') + '/api'
S = requests.Session()
S.headers.update({"Content-Type": "application/json"})

# --- helpers --------------------------------------------------------------

def _no_mongo_id(obj):
    """Recursively assert no `_id` key in any dict in the response payload."""
    if isinstance(obj, dict):
        assert "_id" not in obj, f"Found _id in: {list(obj.keys())}"
        for v in obj.values():
            _no_mongo_id(v)
    elif isinstance(obj, list):
        for v in obj:
            _no_mongo_id(v)


def _create_project(name, languages=None):
    r = S.post(f"{BASE}/projects", json={"name": name, "topic": "TEST_iter4"})
    assert r.status_code == 200, r.text
    pid = r.json()["id"]
    return pid


def _import_songs(pid, items):
    r = S.post(f"{BASE}/projects/{pid}/lyrics-import", json={"items": items})
    assert r.status_code == 200, r.text
    return r.json()["songs"]


def _create_channel(name, language):
    r = S.post(f"{BASE}/channels", json={"name": name, "language": language})
    assert r.status_code == 200, r.text
    return r.json()


def _compose_video_and_wait(sid, timeout=12):
    """Trigger compose-video job and poll until song.status == video_ready."""
    r = S.post(f"{BASE}/songs/{sid}/compose-video")
    assert r.status_code == 200, r.text
    deadline = time.time() + timeout
    while time.time() < deadline:
        sr = S.get(f"{BASE}/songs/{sid}")
        if sr.status_code == 200 and sr.json().get("status") == "video_ready":
            return
        time.sleep(0.6)
    pytest.fail(f"song {sid} did not reach video_ready within {timeout}s — status={sr.json().get('status')}")


def _purge_test_state():
    """Remove TEST_ projects / their songs / TEST_ channels / orphan uploads."""
    # Channels with TEST_ prefix
    chs = S.get(f"{BASE}/channels").json()
    for c in chs:
        if (c.get("name") or "").startswith("TEST_iter4"):
            S.delete(f"{BASE}/channels/{c['id']}")
    # Projects with TEST_iter4
    projs = S.get(f"{BASE}/projects").json()
    for p in projs:
        if (p.get("name") or "").startswith("TEST_iter4"):
            S.delete(f"{BASE}/projects/{p['id']}")
    # Drop any pending uploads referencing no-longer-existing songs/channels
    ups = S.get(f"{BASE}/uploads").json()
    valid_song_ids = set()
    for p in S.get(f"{BASE}/projects").json():
        songs = S.get(f"{BASE}/projects/{p['id']}/songs").json()
        for s in songs:
            valid_song_ids.add(s["id"])
    valid_ch_ids = {c["id"] for c in S.get(f"{BASE}/channels").json()}
    # uploads has no DELETE endpoint by id; we leave them filtered.
    # But we DO need a clean slate for preflight/ai-enrich — drop pending uploads
    # whose channel_id is missing.
    # No HTTP delete for uploads exists, so just record the count and isolate via TEST_ tag if possible.


# Oauth-client list / create / delete for clean preflight tests
def _purge_oauth_clients(label_prefix="TEST_iter4"):
    for c in S.get(f"{BASE}/oauth-clients").json():
        if (c.get("label") or "").startswith(label_prefix):
            S.delete(f"{BASE}/oauth-clients/{c['id']}")


@pytest.fixture(scope="module")
def seeded():
    """Build: project with 2 songs (English, Spanish) both forced to video_ready, plus 2 channels."""
    _purge_test_state()
    _purge_oauth_clients()

    pid = _create_project("TEST_iter4_proj")
    songs = _import_songs(pid, [
        {"title": "TEST_iter4 Song EN", "language": "English", "styles": "lofi",
         "lyrics": "soft piano under city lights"},
        {"title": "TEST_iter4 Song ES", "language": "Spanish", "styles": "bolero",
         "lyrics": "guitarra suave y bossa"},
    ])
    en_song = next(s for s in songs if s["language"] == "English")
    es_song = next(s for s in songs if s["language"] == "Spanish")

    ch_en = _create_channel("TEST_iter4_chan_EN", "English")
    ch_es = _create_channel("TEST_iter4_chan_ES", "Spanish")

    # Trigger compose-video jobs and wait. The mock worker uses ~2.4s/job.
    _compose_video_and_wait(en_song["id"])
    _compose_video_and_wait(es_song["id"])

    yield {
        "pid": pid,
        "en_song": en_song,
        "es_song": es_song,
        "ch_en": ch_en,
        "ch_es": ch_es,
    }

    # Teardown — delete project (cascades nothing automatically, but at least
    # the project + its songs are flushed by manual deletes).
    _purge_test_state()
    _purge_oauth_clients()


# --- 1) bulk-from-videos --------------------------------------------------

class TestBulkFromVideos:
    def test_creates_one_upload_per_song_x_matching_channel_x_format(self, seeded):
        body = {"project_id": seeded["pid"], "formats": ["youtube", "shorts"], "privacy": "public"}
        r = S.post(f"{BASE}/uploads/bulk-from-videos", json=body)
        assert r.status_code == 200, r.text
        data = r.json()
        _no_mongo_id(data)
        assert data["songs"] >= 2
        assert data["channels"] >= 2

        # Verify uploads were persisted with correct shape & matching
        ups = S.get(f"{BASE}/uploads").json()
        _no_mongo_id(ups)
        en_song_id = seeded["en_song"]["id"]
        es_song_id = seeded["es_song"]["id"]
        en_ch_id = seeded["ch_en"]["id"]
        es_ch_id = seeded["ch_es"]["id"]

        # English song must map to OUR English channel and NOT to our Spanish channel
        en_ups = [u for u in ups if u["song_id"] == en_song_id]
        ch_ids_for_en = {u["channel_id"] for u in en_ups}
        assert en_ch_id in ch_ids_for_en
        assert es_ch_id not in ch_ids_for_en, "English song should NOT map to Spanish channel"
        # Both formats present for the EN→EN-channel pair
        fmts_en_pair = {u["format"] for u in en_ups if u["channel_id"] == en_ch_id}
        assert fmts_en_pair == {"youtube", "shorts"}, fmts_en_pair
        # Symmetric check for ES
        es_ups = [u for u in ups if u["song_id"] == es_song_id]
        ch_ids_for_es = {u["channel_id"] for u in es_ups}
        assert es_ch_id in ch_ids_for_es
        assert en_ch_id not in ch_ids_for_es, "Spanish song should NOT map to English channel"
        # Title pulled from song.title; description starts empty; status pending
        for u in en_ups:
            assert u["title"] == seeded["en_song"]["title"]
            assert u["status"] == "pending"
            assert u["privacy"] == "public"
            assert u["tags"] == []

    def test_repeat_call_skips_duplicates(self, seeded):
        body = {"project_id": seeded["pid"], "formats": ["youtube", "shorts"], "privacy": "public"}
        r = S.post(f"{BASE}/uploads/bulk-from-videos", json=body)
        assert r.status_code == 200
        # second call must create zero new rows
        assert r.json()["created"] == 0, r.json()


# --- 2) preflight ---------------------------------------------------------

class TestPreflight:
    def test_preflight_no_client_means_need_oauth_with_empty_url(self, seeded):
        # Ensure no oauth client exists that resolves for these channels
        _purge_oauth_clients()
        # Reset legacy settings so no fallback URL is built
        S.put(f"{BASE}/settings", json={"google_client_id": "", "google_client_secret": "",
                                         "google_redirect_uri": ""})
        r = S.get(f"{BASE}/uploads/preflight")
        assert r.status_code == 200, r.text
        data = r.json()
        _no_mongo_id(data)
        assert "need_oauth" in data and "ready" in data and "pending_uploads" in data
        assert data["pending_uploads"] >= 4  # from bulk-from-videos
        # all channels referenced by pending uploads are NOT connected and have no resolvable client
        need = data["need_oauth"]
        assert len(need) >= 2, need
        for n in need:
            # url MUST be empty when no client is resolvable
            assert n["url"] == "", f"expected empty url, got {n}"
            assert n.get("label") in (None, "")
        assert data["ready"] == []

    def test_preflight_populates_url_when_client_matches_language(self, seeded):
        # Add an OAuth client whose languages include English
        r = S.post(f"{BASE}/oauth-clients", json={
            "label": "TEST_iter4_clientEN", "client_id": "cidEN-123",
            "client_secret": "TEST_secretEN", "redirect_uri": "https://example.com/cb",
            "languages": ["English"],
        })
        assert r.status_code == 200, r.text

        pre = S.get(f"{BASE}/uploads/preflight").json()
        _no_mongo_id(pre)
        # find entry for the English channel
        en_entry = next((n for n in pre["need_oauth"]
                         if n["channel_id"] == seeded["ch_en"]["id"]), None)
        es_entry = next((n for n in pre["need_oauth"]
                         if n["channel_id"] == seeded["ch_es"]["id"]), None)
        assert en_entry is not None and es_entry is not None
        # EN got url + label
        assert en_entry["url"].startswith("https://accounts.google.com/o/oauth2/v2/auth")
        assert "client_id=cidEN-123" in en_entry["url"]
        assert f"state={seeded['ch_en']['id']}" in en_entry["url"]
        assert en_entry["label"] == "TEST_iter4_clientEN"
        # ES has no matching client → empty url
        assert es_entry["url"] == ""

    def test_preflight_ready_when_channel_has_refresh_token(self, seeded):
        # Manually mark the English channel "connected" via oauth-complete
        S.post(f"{BASE}/channels/{seeded['ch_en']['id']}/oauth-complete",
               json={"refresh_token": "TEST_iter4_rt", "youtube_channel_id": "UCfake", "subscriber_count": 10})

        pre = S.get(f"{BASE}/uploads/preflight").json()
        _no_mongo_id(pre)
        ready_ids = {r["channel_id"] for r in pre["ready"]}
        need_ids = {n["channel_id"] for n in pre["need_oauth"]}
        assert seeded["ch_en"]["id"] in ready_ids
        assert seeded["ch_en"]["id"] not in need_ids


# --- 3) ai-enrich ---------------------------------------------------------

class TestAiEnrich:
    def test_deterministic_fallback_contains_global_desc_and_lang_hashtag(self, seeded):
        # Ensure no openrouter key so deterministic fallback fires
        S.put(f"{BASE}/settings", json={"openrouter_api_key": ""})

        global_desc = "TEST_ITER4_GLOBAL_DESCRIPTION_TOKEN_xyz"
        r = S.post(f"{BASE}/uploads/ai-enrich",
                   json={"global_description": global_desc, "regenerate": True})
        assert r.status_code == 200, r.text
        data = r.json()
        _no_mongo_id(data)
        assert data["updated"] >= 4
        assert data["total_pending"] >= 4

        ups = S.get(f"{BASE}/uploads").json()
        _no_mongo_id(ups)
        # Pick uploads that belong to our two seeded songs
        en_song_id = seeded["en_song"]["id"]
        es_song_id = seeded["es_song"]["id"]
        ours = [u for u in ups if u["song_id"] in (en_song_id, es_song_id)]
        assert ours, "expected at least our seeded uploads to be present"

        for u in ours:
            # title pulled from song.title
            if u["song_id"] == en_song_id:
                assert u["title"] == seeded["en_song"]["title"]
                assert "#English" in u["description"]
            else:
                assert u["title"] == seeded["es_song"]["title"]
                assert "#Spanish" in u["description"]
            # deterministic-fallback description MUST contain the global description text literally
            assert global_desc in u["description"], u["description"]
            # tags populated (style/language list)
            assert isinstance(u["tags"], list) and len(u["tags"]) > 0

    def test_only_empty_default_preserves_manual_edits(self, seeded):
        # Set only_empty default (regenerate omitted) -> existing description preserved
        ups_before = S.get(f"{BASE}/uploads").json()
        sample = next(u for u in ups_before if u["song_id"] == seeded["en_song"]["id"])
        prev_desc = sample["description"]
        assert prev_desc  # was filled by previous test

        r = S.post(f"{BASE}/uploads/ai-enrich",
                   json={"global_description": "SHOULD_NOT_APPEAR_TOKEN"})
        assert r.status_code == 200, r.text

        ups_after = S.get(f"{BASE}/uploads").json()
        sample_after = next(u for u in ups_after if u["id"] == sample["id"])
        assert sample_after["description"] == prev_desc, "only_empty should preserve existing description"
        assert "SHOULD_NOT_APPEAR_TOKEN" not in sample_after["description"]

    def test_regenerate_force_overwrites(self, seeded):
        new_desc = "TEST_ITER4_NEW_GLOBAL_TOKEN_abc"
        r = S.post(f"{BASE}/uploads/ai-enrich",
                   json={"global_description": new_desc, "regenerate": True})
        assert r.status_code == 200, r.text
        ups = S.get(f"{BASE}/uploads").json()
        ours = [u for u in ups if u["song_id"] in (seeded["en_song"]["id"], seeded["es_song"]["id"])]
        for u in ours:
            assert new_desc in u["description"]


# --- 4) connect-all-urls --------------------------------------------------

class TestConnectAllUrls:
    def test_lists_only_not_connected_channels_with_resolvable_client(self, seeded):
        # EN was connected in TestPreflight.test_preflight_ready_when_channel_has_refresh_token
        # ES has no resolvable client at this point — should be excluded
        # Add an ES client to make it resolvable
        r = S.post(f"{BASE}/oauth-clients", json={
            "label": "TEST_iter4_clientES", "client_id": "cidES-456",
            "client_secret": "TEST_secretES", "redirect_uri": "https://example.com/cb",
            "languages": ["Spanish"],
        })
        assert r.status_code == 200, r.text

        r = S.post(f"{BASE}/channels/connect-all-urls")
        assert r.status_code == 200, r.text
        data = r.json()
        _no_mongo_id(data)
        assert "items" in data
        ch_ids = {it["channel_id"] for it in data["items"]}
        # connected EN must NOT appear; un-connected ES with a matching client MUST appear
        assert seeded["ch_en"]["id"] not in ch_ids
        assert seeded["ch_es"]["id"] in ch_ids
        es_item = next(it for it in data["items"] if it["channel_id"] == seeded["ch_es"]["id"])
        assert es_item["url"].startswith("https://accounts.google.com/o/oauth2/v2/auth")
        assert "client_id=cidES-456" in es_item["url"]
        assert f"state={seeded['ch_es']['id']}" in es_item["url"]
        assert es_item["label"] == "TEST_iter4_clientES"
        assert es_item["language"] == "Spanish"

    def test_connect_all_excludes_channels_without_resolvable_client(self, seeded):
        # Create a new channel in a language with NO matching client
        ch = _create_channel("TEST_iter4_chan_FR", "French")
        # Reset legacy settings so no fallback URL is built
        S.put(f"{BASE}/settings", json={"google_client_id": "", "google_client_secret": "",
                                         "google_redirect_uri": ""})
        r = S.post(f"{BASE}/channels/connect-all-urls")
        data = r.json()
        ch_ids = {it["channel_id"] for it in data["items"]}
        assert ch["id"] not in ch_ids


# --- 5) regression smoke (key prior endpoints) ---------------------------

class TestRegression:
    def test_root(self):
        r = S.get(f"{BASE}/")
        assert r.status_code == 200

    def test_bible_translations(self):
        r = S.get(f"{BASE}/bible/translations")
        assert r.status_code == 200
        _no_mongo_id(r.json())

    def test_compose_config(self):
        r = S.get(f"{BASE}/ai/compose-config")
        assert r.status_code == 200
        _no_mongo_id(r.json())

    def test_oauth_clients_list_no_mongo_id(self):
        r = S.get(f"{BASE}/oauth-clients")
        assert r.status_code == 200
        _no_mongo_id(r.json())

    def test_channels_list_no_mongo_id(self):
        r = S.get(f"{BASE}/channels")
        assert r.status_code == 200
        _no_mongo_id(r.json())

    def test_uploads_list_no_mongo_id(self):
        r = S.get(f"{BASE}/uploads")
        assert r.status_code == 200
        _no_mongo_id(r.json())

    def test_jobs_list_no_mongo_id(self):
        r = S.get(f"{BASE}/jobs")
        assert r.status_code == 200
        _no_mongo_id(r.json())
