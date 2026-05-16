"""Iteration 3 backend tests: OAuth client POOL CRUD, resolution priority, oauth-start/callback integration, regression."""
import os, time, pytest, requests
from urllib.parse import urlparse, parse_qs

BASE = os.environ.get('REACT_APP_BACKEND_URL', '').rstrip('/')
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


# Module-level cleanup helpers
def _clear_oauth_clients():
    clients = S.get(f"{API}/oauth-clients").json()
    for c in clients:
        S.delete(f"{API}/oauth-clients/{c['id']}")


def _clear_legacy_google():
    cur = S.get(f"{API}/settings").json()
    cur["google_client_id"] = ""
    cur["google_client_secret"] = ""
    cur["google_redirect_uri"] = ""
    S.put(f"{API}/settings", json=cur)


@pytest.fixture(autouse=True)
def isolate():
    # before each test
    _clear_oauth_clients()
    yield
    _clear_oauth_clients()


# ---------- 1. List empty + grows ----------
def test_list_initially_empty_then_grows():
    r = S.get(f"{API}/oauth-clients")
    assert r.status_code == 200
    assert r.json() == []
    r2 = S.post(f"{API}/oauth-clients", json={
        "label": "TEST_pool_A", "client_id": "cidA.apps.googleusercontent.com",
        "client_secret": "SECRETA_full_value_xyz", "redirect_uri": "http://localhost/cb",
        "languages": ["English"]})
    assert r2.status_code == 200
    assert no_id(r2.json())
    r3 = S.get(f"{API}/oauth-clients")
    assert len(r3.json()) == 1


# ---------- 2. POST returns masked secret ----------
def test_post_returns_masked_secret():
    body = {"label": "TEST_mask", "client_id": "cidM.apps.googleusercontent.com",
            "client_secret": "SUPERSECRETvalue1234", "redirect_uri": "http://localhost/cb",
            "languages": ["English"]}
    r = S.post(f"{API}/oauth-clients", json=body)
    assert r.status_code == 200
    j = r.json()
    assert "id" in j and j["id"]
    assert j["client_secret"].startswith("•" * 8)
    assert j["client_secret"].endswith("1234")
    assert "SUPERSECRETvalue" not in j["client_secret"]
    # Also list must mask
    listed = S.get(f"{API}/oauth-clients").json()
    assert listed[0]["client_secret"].startswith("•" * 8)


# ---------- 3. PUT preserves secret when masked or empty ----------
def test_put_preserves_secret_when_masked_or_empty():
    cr = S.post(f"{API}/oauth-clients", json={
        "label": "TEST_put", "client_id": "cidP.apps.googleusercontent.com",
        "client_secret": "ORIGINAL_SECRET_LONG_4321", "redirect_uri": "http://localhost/cb",
        "languages": ["English"]}).json()
    oid = cr["id"]
    masked = cr["client_secret"]  # "••••••••4321"

    # Send masked string back along with label/languages update
    upd = S.put(f"{API}/oauth-clients/{oid}", json={
        "label": "TEST_put_upd", "languages": ["French", "German"],
        "client_secret": masked}).json()
    assert upd["label"] == "TEST_put_upd"
    assert upd["languages"] == ["French", "German"]
    assert upd["client_secret"].endswith("4321")

    # Verify oauth-start picks this client for French and uses ORIGINAL plaintext secret behind the scenes
    ch = S.post(f"{API}/channels", json={"name": "TEST_putchan", "language": "French"}).json()
    pick = S.get(f"{API}/channels/{ch['id']}/oauth-client").json()
    assert pick["client"]["id"] == oid
    assert pick["client"]["client_secret"].endswith("4321")
    # cleanup channel
    S.delete(f"{API}/channels/{ch['id']}")


# ---------- 4. DELETE removes + unsets channel ref ----------
def test_delete_unsets_channel_reference():
    c = S.post(f"{API}/oauth-clients", json={
        "label": "TEST_del", "client_id": "cidD.apps.googleusercontent.com",
        "client_secret": "DELSECRETxxxx", "redirect_uri": "http://localhost/cb",
        "languages": ["English"]}).json()
    ch = S.post(f"{API}/channels", json={"name": "TEST_delchan", "language": "English"}).json()
    # bind via oauth-start
    o = S.post(f"{API}/channels/{ch['id']}/oauth-start", json={}).json()
    assert o.get("oauth_client_id") == c["id"]
    # confirm bound
    chan = next(x for x in S.get(f"{API}/channels").json() if x["id"] == ch["id"])
    assert chan.get("oauth_client_id") == c["id"]
    # delete the oauth client
    d = S.delete(f"{API}/oauth-clients/{c['id']}")
    assert d.status_code == 200
    # channel must have oauth_client_id unset (None or missing)
    chan2 = next(x for x in S.get(f"{API}/channels").json() if x["id"] == ch["id"])
    assert not chan2.get("oauth_client_id")
    S.delete(f"{API}/channels/{ch['id']}")


# ---------- 5. Resolution priority ladder ----------
def test_resolution_priority_ladder():
    # Setup: legacy settings has google creds
    cur = S.get(f"{API}/settings").json()
    cur["google_client_id"] = "LEGACY_cidL.apps.googleusercontent.com"
    cur["google_client_secret"] = "LEGACY_secret_ZZZZ"
    cur["google_redirect_uri"] = "http://localhost/legacy"
    S.put(f"{API}/settings", json=cur)

    ch = S.post(f"{API}/channels", json={"name": "TEST_prio", "language": "Spanish"}).json()
    cid = ch["id"]

    # 5a. With no oauth-clients, no language match → legacy fallback
    pick = S.get(f"{API}/channels/{cid}/oauth-client").json()["client"]
    assert pick is not None
    assert pick["id"] == "_legacy"
    assert pick["client_id"] == "LEGACY_cidL.apps.googleusercontent.com"
    # legacy secret is also masked in public response
    assert pick["client_secret"].endswith("ZZZZ")

    # 5b. Add a Spanish-matching client → should override legacy
    sp = S.post(f"{API}/oauth-clients", json={
        "label": "spanish_pool", "client_id": "SP_cid.apps.googleusercontent.com",
        "client_secret": "SP_secret_AAAA", "redirect_uri": "http://localhost/sp",
        "languages": ["Spanish"]}).json()
    pick2 = S.get(f"{API}/channels/{cid}/oauth-client").json()["client"]
    assert pick2["id"] == sp["id"]
    assert pick2["client_id"] == "SP_cid.apps.googleusercontent.com"

    # 5c. Add an explicit channel.oauth_client_id (different client) → should override language match
    other = S.post(f"{API}/oauth-clients", json={
        "label": "other_pool", "client_id": "OT_cid.apps.googleusercontent.com",
        "client_secret": "OT_secret_BBBB", "redirect_uri": "http://localhost/ot",
        "languages": ["German"]}).json()
    # Bind via forced oauth-start
    forced = S.post(f"{API}/channels/{cid}/oauth-start", json={"oauth_client_id": other["id"]}).json()
    assert forced["oauth_client_id"] == other["id"]
    # Now channel.oauth_client_id should outrank language match
    pick3 = S.get(f"{API}/channels/{cid}/oauth-client").json()["client"]
    assert pick3["id"] == other["id"]

    # 5d. Delete other → channel oauth_client_id unset → falls back to Spanish match again
    S.delete(f"{API}/oauth-clients/{other['id']}")
    pick4 = S.get(f"{API}/channels/{cid}/oauth-client").json()["client"]
    assert pick4["id"] == sp["id"]

    # 5e. Delete sp too → no language match → legacy fallback again
    S.delete(f"{API}/oauth-clients/{sp['id']}")
    pick5 = S.get(f"{API}/channels/{cid}/oauth-client").json()["client"]
    assert pick5["id"] == "_legacy"

    # 5f. Clear legacy → null
    _clear_legacy_google()
    pick6 = S.get(f"{API}/channels/{cid}/oauth-client").json()["client"]
    assert pick6 is None

    S.delete(f"{API}/channels/{cid}")


# ---------- 6. oauth-start auto-pick by language ----------
def test_oauth_start_auto_picks_by_language():
    _clear_legacy_google()
    de = S.post(f"{API}/oauth-clients", json={
        "label": "german_pool", "client_id": "DE_cid_zzz.apps.googleusercontent.com",
        "client_secret": "DE_sec_xyz", "redirect_uri": "http://localhost/de",
        "languages": ["German"]}).json()
    ch = S.post(f"{API}/channels", json={"name": "TEST_autoG", "language": "German"}).json()
    r = S.post(f"{API}/channels/{ch['id']}/oauth-start", json={}).json()
    assert "url" in r and r["url"]
    qs = parse_qs(urlparse(r["url"]).query)
    assert qs.get("client_id", [""])[0] == "DE_cid_zzz.apps.googleusercontent.com"
    assert r["oauth_client_id"] == de["id"]
    assert r["label"] == "german_pool"
    S.delete(f"{API}/channels/{ch['id']}")


# ---------- 7. oauth-start with forced oauth_client_id overrides language ----------
def test_oauth_start_forced_overrides_language():
    _clear_legacy_google()
    en = S.post(f"{API}/oauth-clients", json={
        "label": "eng_pool", "client_id": "EN_cid.apps.googleusercontent.com",
        "client_secret": "EN_sec_xyz", "redirect_uri": "http://localhost/en",
        "languages": ["English"]}).json()
    fr = S.post(f"{API}/oauth-clients", json={
        "label": "fr_pool", "client_id": "FR_cid.apps.googleusercontent.com",
        "client_secret": "FR_sec_xyz", "redirect_uri": "http://localhost/fr",
        "languages": ["French"]}).json()
    ch = S.post(f"{API}/channels", json={"name": "TEST_forced", "language": "English"}).json()
    # Force FR client despite English channel
    r = S.post(f"{API}/channels/{ch['id']}/oauth-start", json={"oauth_client_id": fr["id"]}).json()
    qs = parse_qs(urlparse(r["url"]).query)
    assert qs.get("client_id", [""])[0] == "FR_cid.apps.googleusercontent.com"
    assert r["oauth_client_id"] == fr["id"]
    S.delete(f"{API}/channels/{ch['id']}")


# ---------- 8. oauth/callback uses channel's stored client; invalid code → 400 HTML ----------
def test_oauth_callback_invalid_code_uses_channel_client_returns_400():
    _clear_legacy_google()
    pool = S.post(f"{API}/oauth-clients", json={
        "label": "cb_pool", "client_id": "CB_cid.apps.googleusercontent.com",
        "client_secret": "CB_sec_xyz", "redirect_uri": "http://localhost/cb",
        "languages": ["English"]}).json()
    ch = S.post(f"{API}/channels", json={"name": "TEST_cb", "language": "English"}).json()
    # bind via oauth-start so channel.oauth_client_id is set
    S.post(f"{API}/channels/{ch['id']}/oauth-start", json={})
    r = S.get(f"{API}/oauth/callback",
              params={"code": "INVALID_CODE_TEST", "state": ch["id"]},
              allow_redirects=False)
    assert r.status_code == 400, f"expected 400, got {r.status_code}: {r.text[:200]}"
    assert "text/html" in r.headers.get("content-type", "").lower()
    S.delete(f"{API}/channels/{ch['id']}")


def test_oauth_callback_missing_returns_400():
    r = S.get(f"{API}/oauth/callback", allow_redirects=False)
    assert r.status_code == 400
    assert "text/html" in r.headers.get("content-type", "").lower()


# ---------- 9. YouTube upload worker fallback paths ----------
def _wait_job(jid, timeout=20):
    for _ in range(timeout):
        time.sleep(1)
        j = S.get(f"{API}/jobs/{jid}").json()
        if j["status"] in ("done", "failed"):
            return j
    return j


def test_upload_fallback_when_no_refresh_token():
    _clear_legacy_google()
    ch = S.post(f"{API}/channels", json={"name": "TEST_uplA", "language": "English"}).json()
    proj = S.post(f"{API}/projects", json={"name": "TEST_uplprojA"}).json()
    song = S.post(f"{API}/projects/{proj['id']}/lyrics-import", json={
        "items": [{"title": "T", "language": "English", "styles": "epic",
                   "lyrics": "a\nb", "annotations": "", "image_styles": ""}]}).json()["songs"][0]
    up = S.post(f"{API}/uploads", json={"song_id": song["id"], "channel_id": ch["id"],
                                        "title": "TEST_uplA", "privacy": "private"}).json()
    p = S.post(f"{API}/uploads/{up['id']}/publish").json()
    j = _wait_job(p["id"])
    assert j["status"] == "done"
    assert j["result"]["real"] is False
    assert j["result"]["youtube_video_id"].startswith("yt_")
    S.delete(f"{API}/projects/{proj['id']}")
    S.delete(f"{API}/channels/{ch['id']}")


def test_upload_fallback_when_refresh_token_but_no_resolvable_client():
    _clear_legacy_google()
    ch = S.post(f"{API}/channels", json={"name": "TEST_uplB", "language": "ZZZ_no_match_lang"}).json()
    # Set refresh_token via oauth-complete
    S.post(f"{API}/channels/{ch['id']}/oauth-complete",
           json={"refresh_token": "fake_refresh_token_value", "youtube_channel_id": "UCxxx", "subscriber_count": 0})
    proj = S.post(f"{API}/projects", json={"name": "TEST_uplprojB"}).json()
    song = S.post(f"{API}/projects/{proj['id']}/lyrics-import", json={
        "items": [{"title": "T", "language": "ZZZ_no_match_lang", "styles": "epic",
                   "lyrics": "a\nb", "annotations": "", "image_styles": ""}]}).json()["songs"][0]
    up = S.post(f"{API}/uploads", json={"song_id": song["id"], "channel_id": ch["id"],
                                        "title": "TEST_uplB", "privacy": "private"}).json()
    p = S.post(f"{API}/uploads/{up['id']}/publish").json()
    j = _wait_job(p["id"])
    assert j["status"] == "done"
    assert j["result"]["real"] is False
    assert j["result"]["youtube_video_id"].startswith("yt_")
    S.delete(f"{API}/projects/{proj['id']}")
    S.delete(f"{API}/channels/{ch['id']}")


# ---------- 10. Plaintext secret never leaks ----------
def test_secrets_never_leak_in_list_or_update():
    c = S.post(f"{API}/oauth-clients", json={
        "label": "TEST_leak", "client_id": "LK_cid.apps.googleusercontent.com",
        "client_secret": "PLAINTEXT_LEAK_CANARY_9999", "redirect_uri": "http://localhost/x",
        "languages": ["English"]}).json()
    assert "PLAINTEXT_LEAK_CANARY" not in c["client_secret"]
    lst = S.get(f"{API}/oauth-clients").json()
    for item in lst:
        assert "PLAINTEXT_LEAK_CANARY" not in (item.get("client_secret") or "")
    upd = S.put(f"{API}/oauth-clients/{c['id']}", json={"label": "TEST_leak2"}).json()
    assert "PLAINTEXT_LEAK_CANARY" not in upd["client_secret"]
    # /channels/{cid}/oauth-client SHOULD return plaintext? per code _client_public masks too — verify masked
    ch = S.post(f"{API}/channels", json={"name": "TEST_leakchan", "language": "English"}).json()
    pick = S.get(f"{API}/channels/{ch['id']}/oauth-client").json()["client"]
    assert "PLAINTEXT_LEAK_CANARY" not in pick["client_secret"]
    assert pick["client_secret"].endswith("9999")
    S.delete(f"{API}/channels/{ch['id']}")
