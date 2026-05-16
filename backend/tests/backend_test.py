"""End-to-end backend test suite for AI Music Video Studio."""
import os, time, pytest, requests

BASE = os.environ['REACT_APP_BACKEND_URL'].rstrip('/') if os.environ.get('REACT_APP_BACKEND_URL') else None
if not BASE:
    # Fallback: read from frontend/.env directly
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


# ---------- Health ----------
def test_root():
    r = S.get(f"{API}/")
    assert r.status_code == 200
    j = r.json()
    assert j["status"] == "ok"
    assert no_id(j)


# ---------- Settings ----------
def test_settings_get_default():
    r = S.get(f"{API}/settings")
    assert r.status_code == 200
    j = r.json()
    assert "suno_cookie" in j and "ffmpeg_path" in j
    assert no_id(j)


def test_settings_put_and_persist():
    payload = {"suno_cookie": "TEST_cookie", "mj_cookie": "TEST_mj",
               "google_client_id": "TEST_client_id.apps.googleusercontent.com",
               "google_redirect_uri": "http://localhost/cb",
               "ffmpeg_path": "ffmpeg", "theme": "obsidian"}
    r = S.put(f"{API}/settings", json=payload)
    assert r.status_code == 200
    g = S.get(f"{API}/settings").json()
    assert g["suno_cookie"] == "TEST_cookie"
    assert g["google_client_id"].startswith("TEST_client_id")


def test_settings_test_endpoints():
    r1 = S.post(f"{API}/settings/test-suno"); assert r1.status_code == 200
    assert r1.json()["ok"] is True  # cookie set above
    r2 = S.post(f"{API}/settings/test-mj"); assert r2.status_code == 200
    assert r2.json()["ok"] is True
    r3 = S.post(f"{API}/settings/test-ffmpeg"); assert r3.status_code == 200
    assert "ok" in r3.json()


# ---------- Projects ----------
@pytest.fixture(scope="module")
def project_id():
    r = S.post(f"{API}/projects", json={"name": "TEST_Project", "topic": "biblical themes"})
    assert r.status_code == 200
    j = r.json(); assert no_id(j)
    pid = j["id"]
    yield pid
    S.delete(f"{API}/projects/{pid}")


def test_project_list_and_get(project_id):
    lst = S.get(f"{API}/projects").json()
    assert any(p["id"] == project_id for p in lst)
    assert no_id(lst)
    one = S.get(f"{API}/projects/{project_id}").json()
    assert one["name"] == "TEST_Project"


def test_project_update(project_id):
    r = S.put(f"{API}/projects/{project_id}", json={"topic": "updated topic"})
    assert r.status_code == 200
    assert r.json()["topic"] == "updated topic"


# ---------- Lyrics import using user's exact JSON shape ----------
LYRICS_JSON = {
    "items": [
        {
            "title": "Light of Heaven",
            "language": "English",
            "styles": "cinematic orchestral",
            "lyrics": "Above the clouds I see\nA radiant light calling me\nDarkness fades away\nVictory leads the way",
            "annotations": (
                "[serene celestial sky with soft light beams]\n"
                "Above the clouds I see\n"
                "[radiant divine glow over mountains]\n"
                "A radiant light calling me\n"
                "[tense darkness dissolving into dawn]\n"
                "Darkness fades away\n"
                "[triumphant victory banner rising]\n"
                "Victory leads the way"
            ),
            "image_styles": "oil painting, cinematic"
        }
    ]
}


@pytest.fixture(scope="module")
def song_id(project_id):
    r = S.post(f"{API}/projects/{project_id}/lyrics-import", json=LYRICS_JSON)
    assert r.status_code == 200
    j = r.json()
    assert j["created"] == 1
    assert no_id(j)
    sid = j["songs"][0]["id"]
    # Verify list reflects
    songs = S.get(f"{API}/projects/{project_id}/songs").json()
    assert any(s["id"] == sid for s in songs)
    assert no_id(songs)
    return sid


def wait_job_done(jid, timeout=10):
    for _ in range(int(timeout / 0.5)):
        time.sleep(0.5)
        j = S.get(f"{API}/jobs/{jid}").json()
        if j["status"] in ("done", "failed"):
            return j
    return j


def test_generate_music(song_id):
    r = S.post(f"{API}/songs/{song_id}/generate-music")
    assert r.status_code == 200
    jid = r.json()["id"]
    j = wait_job_done(jid)
    assert j["status"] == "done", j
    song = S.get(f"{API}/songs/{song_id}").json()
    assert song["audio_url"] and song["status"] == "music_ready"


def test_analyze_creates_sections(song_id):
    r = S.post(f"{API}/songs/{song_id}/analyze")
    jid = r.json()["id"]
    j = wait_job_done(jid)
    assert j["status"] == "done"
    secs = S.get(f"{API}/songs/{song_id}/sections").json()
    assert no_id(secs)
    assert len(secs) == 4
    # verify annotation parsing populated image_prompt
    assert all(s["image_prompt"] for s in secs)
    assert secs[0]["line"].startswith("Above the clouds")
    assert "celestial" in secs[0]["image_prompt"].lower()
    # index + start/end monotonic
    for i, s in enumerate(secs):
        assert s["index"] == i
        assert s["end"] > s["start"]
        assert s["mood"]
        assert isinstance(s["effects"], list) and len(s["effects"]) >= 1


def test_update_section(song_id):
    secs = S.get(f"{API}/songs/{song_id}/sections").json()
    sid = secs[0]["id"]
    r = S.put(f"{API}/sections/{sid}", json={"line": "Edited line", "mood": "radiant"})
    assert r.status_code == 200
    assert r.json()["line"] == "Edited line"


def test_generate_single_image(song_id):
    secs = S.get(f"{API}/songs/{song_id}/sections").json()
    r = S.post(f"{API}/sections/{secs[0]['id']}/generate-image")
    jid = r.json()["id"]
    wait_job_done(jid)
    secs2 = S.get(f"{API}/songs/{song_id}/sections").json()
    assert secs2[0]["image_url"]
    assert len(secs2[0]["image_variants"]) == 4


def test_batch_generate_images(song_id):
    r = S.post(f"{API}/songs/{song_id}/batch-generate-images")
    assert r.status_code == 200
    jobs = r.json()["jobs"]
    assert len(jobs) == 4
    for j in jobs:
        wait_job_done(j["id"])
    secs = S.get(f"{API}/songs/{song_id}/sections").json()
    assert all(s["image_url"] for s in secs)


def test_effects_presets():
    r = S.get(f"{API}/effects-presets")
    assert r.status_code == 200
    presets = r.json()
    assert len(presets) == 16
    assert all("id" in p and "ff" in p and "moods" in p for p in presets)


def test_compose_video(song_id):
    r = S.post(f"{API}/songs/{song_id}/compose-video")
    jid = r.json()["id"]
    wait_job_done(jid)
    song = S.get(f"{API}/songs/{song_id}").json()
    assert song["status"] == "video_ready"
    assert song.get("video_url", "").endswith(".mp4")


# ---------- Channels ----------
@pytest.fixture(scope="module")
def channel_id():
    r = S.post(f"{API}/channels", json={"name": "TEST_Channel", "language": "English"})
    assert r.status_code == 200
    j = r.json(); assert no_id(j)
    cid = j["id"]
    yield cid
    S.delete(f"{API}/channels/{cid}")


def test_channels_list(channel_id):
    lst = S.get(f"{API}/channels").json()
    assert any(c["id"] == channel_id for c in lst)
    assert no_id(lst)


def test_oauth_start_with_client_id(channel_id):
    r = S.post(f"{API}/channels/{channel_id}/oauth-start")
    assert r.status_code == 200
    j = r.json()
    assert "accounts.google.com" in j["url"]
    assert "TEST_client_id" in j["url"]


def test_oauth_complete(channel_id):
    r = S.post(f"{API}/channels/{channel_id}/oauth-complete",
               json={"refresh_token": "TEST_refresh", "youtube_channel_id": "UC_test", "subscriber_count": 42})
    assert r.status_code == 200
    j = r.json()
    assert j["connected"] is True
    assert j["refresh_token"] == "TEST_refresh"
    assert j["subscriber_count"] == 42
    assert no_id(j)


def test_oauth_complete_missing_token(channel_id):
    r = S.post(f"{API}/channels/{channel_id}/oauth-complete", json={})
    assert r.status_code == 400


# ---------- Uploads ----------
@pytest.fixture(scope="module")
def upload_id(song_id, channel_id):
    r = S.post(f"{API}/uploads", json={"song_id": song_id, "channel_id": channel_id,
                                       "title": "TEST upload", "tags": ["a", "b"]})
    assert r.status_code == 200
    j = r.json(); assert no_id(j); assert j["status"] == "pending"
    return j["id"]


def test_uploads_list(upload_id):
    lst = S.get(f"{API}/uploads").json()
    assert any(u["id"] == upload_id for u in lst)
    assert no_id(lst)


def test_publish_upload(upload_id):
    r = S.post(f"{API}/uploads/{upload_id}/publish")
    assert r.status_code == 200
    jid = r.json()["id"]
    wait_job_done(jid)
    lst = S.get(f"{API}/uploads").json()
    me = next(u for u in lst if u["id"] == upload_id)
    assert me["status"] == "published"
    assert me["youtube_video_id"].startswith("yt_")


def test_publish_all(song_id, channel_id):
    # create two pending uploads
    ids = []
    for i in range(2):
        u = S.post(f"{API}/uploads", json={"song_id": song_id, "channel_id": channel_id,
                                           "title": f"TEST_bulk_{i}"}).json()
        ids.append(u["id"])
    r = S.post(f"{API}/uploads/publish-all")
    assert r.status_code == 200
    assert r.json()["queued"] >= 2
    time.sleep(5)
    lst = {u["id"]: u for u in S.get(f"{API}/uploads").json()}
    for i in ids:
        assert lst[i]["status"] == "published"


# ---------- Jobs ----------
def test_jobs_list_and_reverse_chrono():
    jobs = S.get(f"{API}/jobs").json()
    assert no_id(jobs)
    assert len(jobs) > 0
    times = [j["created_at"] for j in jobs]
    assert times == sorted(times, reverse=True)
    # at least one done with logs
    done = [j for j in jobs if j["status"] == "done"]
    assert done and len(done[0]["logs"]) >= 2


def test_job_retry_and_delete(song_id):
    # create a fresh job
    r = S.post(f"{API}/songs/{song_id}/generate-music")
    jid = r.json()["id"]
    wait_job_done(jid)
    r = S.post(f"{API}/jobs/{jid}/retry")
    assert r.status_code == 200
    time.sleep(4)
    j = S.get(f"{API}/jobs/{jid}").json()
    assert j["attempts"] >= 1
    d = S.delete(f"{API}/jobs/{jid}")
    assert d.status_code == 200
    g = S.get(f"{API}/jobs/{jid}")
    assert g.status_code == 404


# ---------- Error paths ----------
def test_404s():
    assert S.get(f"{API}/projects/nope").status_code == 404
    assert S.get(f"{API}/songs/nope").status_code == 404
    assert S.post(f"{API}/songs/nope/generate-music").status_code == 404
