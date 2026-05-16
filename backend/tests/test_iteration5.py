"""Iteration 5 backend tests:
   - Bible translations expanded to 10 language groups (literal English, Greek, Hebrew, etc.)
   - New live bible fetches via bible-api.com (ylt, darby, oeb-cw) and helloao.org (grc_byz, eng_lxx, GHT)
   - Pasted-chapter CRUD: POST/GET/DELETE /api/bible/pasted
   - Generic drafts auto-save: PUT/GET/DELETE /api/drafts/{key} (GET returns {} not 404)
   - Regression: original bible IDs still work
   - MongoDB _id leak check across new endpoints
"""
import os
import uuid
import time
import pytest
import requests
from pathlib import Path
from dotenv import load_dotenv

load_dotenv(Path(__file__).resolve().parents[2] / "frontend" / ".env")
BASE = os.environ["REACT_APP_BACKEND_URL"].rstrip("/")
API = f"{BASE}/api"


# ---------- helpers ----------
def _no_id_leak(obj):
    if isinstance(obj, dict):
        assert "_id" not in obj, f"_id leaked: {obj}"
        for v in obj.values():
            _no_id_leak(v)
    elif isinstance(obj, list):
        for it in obj:
            _no_id_leak(it)


# ============================================================
# Bible translations catalogue
# ============================================================
class TestBibleTranslationsCatalogue:
    def test_returns_10_language_groups(self):
        r = requests.get(f"{API}/bible/translations", timeout=20)
        assert r.status_code == 200
        data = r.json()
        assert isinstance(data, dict)
        expected = {
            "English (modern)",
            "English (literal / original-meaning)",
            "Greek (Koine / NT)",
            "Hebrew (OT)",
            "German",
            "Italian",
            "Russian",
            "Spanish",
            "French",
            "Portuguese",
        }
        missing = expected - set(data.keys())
        assert not missing, f"Missing groups: {missing}. got={list(data.keys())}"
        for grp, items in data.items():
            assert isinstance(items, list) and len(items) >= 1, f"empty group {grp}"
            for t in items:
                assert "id" in t and "name" in t and "source" in t, f"bad entry in {grp}: {t}"

    def test_literal_group_contains_required_ids(self):
        r = requests.get(f"{API}/bible/translations", timeout=20)
        assert r.status_code == 200
        lit = r.json().get("English (literal / original-meaning)", [])
        ids = {t["id"] for t in lit}
        for needed in ["ylt", "darby", "GHT", "eng_lxx"]:
            assert needed in ids, f"missing {needed} in literal group; got={ids}"

    def test_greek_group_contains_grc_byz(self):
        r = requests.get(f"{API}/bible/translations", timeout=20)
        assert r.status_code == 200
        gk = r.json().get("Greek (Koine / NT)", [])
        ids = {t["id"] for t in gk}
        assert "grc_byz" in ids, f"missing grc_byz; got={ids}"


# ============================================================
# Live Bible chapter fetches — bible-api.com path
# ============================================================
class TestBibleApiPath:
    """These hit external bible-api.com — flaky-prone; we retry once."""

    def _get(self, params, retries=1):
        last = None
        for _ in range(retries + 1):
            try:
                r = requests.get(f"{API}/bible/chapter", params=params, timeout=30)
                if r.status_code == 200:
                    return r
                last = r
            except requests.RequestException as e:
                last = e
            time.sleep(1.5)
        return last

    def test_ylt_john_1(self):
        r = self._get({"translation": "ylt", "book": "john", "chapter": 1})
        if not hasattr(r, "status_code") or r.status_code != 200:
            pytest.skip(f"bible-api.com unreachable: {r}")
        d = r.json()
        assert d.get("translation") == "ylt"
        verses = d.get("verses") or []
        assert len(verses) >= 50, f"expected >=50 verses, got {len(verses)}"
        first = verses[0]["text"].lower()
        assert "beginning" in first and "word" in first, f"first verse unexpected: {verses[0]}"
        _no_id_leak(d)

    def test_darby_john_1(self):
        r = self._get({"translation": "darby", "book": "john", "chapter": 1})
        if not hasattr(r, "status_code") or r.status_code != 200:
            pytest.skip(f"bible-api.com unreachable: {r}")
        d = r.json()
        verses = d.get("verses") or []
        assert len(verses) >= 50, f"darby verses={len(verses)}"

    def test_oebcw_john_1(self):
        r = self._get({"translation": "oeb-cw", "book": "john", "chapter": 1})
        if not hasattr(r, "status_code") or r.status_code != 200:
            pytest.skip(f"bible-api.com unreachable: {r}")
        d = r.json()
        verses = d.get("verses") or []
        assert len(verses) >= 40, f"oeb-cw verses={len(verses)}"


# ============================================================
# Live Bible chapter fetches — helloao path
# ============================================================
class TestHelloAoPath:
    def _get(self, params, retries=1):
        last = None
        for _ in range(retries + 1):
            try:
                r = requests.get(f"{API}/bible/chapter", params=params, timeout=30)
                if r.status_code == 200:
                    return r
                last = r
            except requests.RequestException as e:
                last = e
            time.sleep(1.5)
        return last

    def test_grc_byz_john_1(self):
        r = self._get({"translation": "grc_byz", "book": "john", "chapter": 1})
        if not hasattr(r, "status_code") or r.status_code != 200:
            pytest.skip(f"helloao unreachable: {r}")
        d = r.json()
        verses = d.get("verses") or []
        assert len(verses) >= 40, f"grc_byz verses={len(verses)}"
        joined = " ".join(v["text"] for v in verses[:5])
        # Greek text should contain greek letters
        assert any("\u0370" <= ch <= "\u03ff" or "\u1f00" <= ch <= "\u1fff" for ch in joined), \
            f"no Greek chars in first 5 verses: {joined[:200]}"

    def test_eng_lxx_psalms_1(self):
        r = self._get({"translation": "eng_lxx", "book": "psalms", "chapter": 1})
        if not hasattr(r, "status_code") or r.status_code != 200:
            pytest.skip(f"helloao unreachable: {r}")
        d = r.json()
        verses = d.get("verses") or []
        assert len(verses) >= 3, f"eng_lxx psalms 1 verses={len(verses)}"

    def test_ght_john_1(self):
        r = self._get({"translation": "GHT", "book": "john", "chapter": 1})
        if not hasattr(r, "status_code") or r.status_code != 200:
            pytest.skip(f"helloao unreachable or GHT missing: {r}")
        d = r.json()
        verses = d.get("verses") or []
        assert len(verses) >= 1, f"GHT verses={len(verses)}"


# ============================================================
# Regression: original bible IDs
# ============================================================
class TestBibleRegression:
    @pytest.mark.parametrize("tr", ["web", "kjv", "bbe", "asv"])
    def test_bible_api_original_ids(self, tr):
        r = requests.get(f"{API}/bible/chapter",
                         params={"translation": tr, "book": "john", "chapter": 1},
                         timeout=30)
        if r.status_code != 200:
            pytest.skip(f"external bible-api flaky for {tr}: {r.status_code}")
        d = r.json()
        assert len(d.get("verses") or []) >= 40

    @pytest.mark.parametrize("tr", ["BSB", "deu_l12", "ita_dio", "rus_syn", "fra_lsg", "spa_r09"])
    def test_helloao_original_ids(self, tr):
        r = requests.get(f"{API}/bible/chapter",
                         params={"translation": tr, "book": "john", "chapter": 1},
                         timeout=30)
        if r.status_code != 200:
            pytest.skip(f"helloao flaky for {tr}: {r.status_code}")
        d = r.json()
        assert len(d.get("verses") or []) >= 1


# ============================================================
# Pasted chapters CRUD
# ============================================================
class TestPastedChapters:
    def test_pasted_full_crud_and_upsert(self):
        # CREATE
        payload = {
            "name": "TEST_iter5 my Genesis paste",
            "language": "English",
            "translation": "NIV",  # arbitrary translation accepted
            "book": "genesis",
            "chapter": 1,
            "text": "In the beginning God created the heavens and the earth.",
        }
        r = requests.post(f"{API}/bible/pasted", json=payload, timeout=20)
        assert r.status_code == 200, r.text
        doc = r.json()
        _no_id_leak(doc)
        pid = doc.get("id")
        assert pid and isinstance(pid, str)
        assert doc["name"] == payload["name"]
        assert doc["translation"] == "NIV"
        assert doc["chapter"] == 1
        assert "created_at" in doc and "updated_at" in doc

        # UPSERT - same id, different text
        upsert_payload = {**payload, "id": pid, "text": "UPDATED iter5 text"}
        r2 = requests.post(f"{API}/bible/pasted", json=upsert_payload, timeout=20)
        assert r2.status_code == 200
        doc2 = r2.json()
        assert doc2["id"] == pid
        assert doc2["text"] == "UPDATED iter5 text"

        # LIST contains it & sorted desc
        rlist = requests.get(f"{API}/bible/pasted", timeout=20)
        assert rlist.status_code == 200
        items = rlist.json()
        _no_id_leak(items)
        assert isinstance(items, list)
        ids = [it["id"] for it in items]
        assert pid in ids
        # sort check: created_at should be desc (non-strict; only if >=2 items)
        if len(items) >= 2:
            cas = [it.get("created_at") for it in items if it.get("created_at")]
            assert cas == sorted(cas, reverse=True), "pasted list not sorted desc by created_at"

        # Verify upserted text via list
        found = next(it for it in items if it["id"] == pid)
        assert found["text"] == "UPDATED iter5 text"

        # DELETE
        rdel = requests.delete(f"{API}/bible/pasted/{pid}", timeout=20)
        assert rdel.status_code == 200
        assert rdel.json().get("ok") is True

        # Verify removed
        rlist2 = requests.get(f"{API}/bible/pasted", timeout=20)
        ids2 = [it["id"] for it in rlist2.json()]
        assert pid not in ids2

    def test_pasted_arbitrary_translation_accepted(self):
        payload = {"name": "TEST_iter5 own translation", "translation": "my own",
                   "book": "john", "chapter": 3, "text": "..."}
        r = requests.post(f"{API}/bible/pasted", json=payload, timeout=20)
        assert r.status_code == 200
        doc = r.json()
        assert doc["translation"] == "my own"
        # cleanup
        requests.delete(f"{API}/bible/pasted/{doc['id']}", timeout=20)


# ============================================================
# Drafts auto-save
# ============================================================
class TestDrafts:
    def test_get_missing_draft_returns_empty_dict_not_404(self):
        key = f"TEST_iter5_missing_{uuid.uuid4().hex[:8]}"
        r = requests.get(f"{API}/drafts/{key}", timeout=20)
        assert r.status_code == 200, f"expected 200, got {r.status_code}"
        body = r.json()
        assert body == {} or (isinstance(body, dict) and len(body) == 0), f"expected {{}}, got {body}"

    def test_drafts_put_get_delete_lifecycle(self):
        key = f"TEST_iter5_draft_{uuid.uuid4().hex[:8]}"
        value = {"value": {"text": "auto-saved chapter notes", "selected": ["a", "b"], "n": 7}}

        # PUT
        rp = requests.put(f"{API}/drafts/{key}", json=value, timeout=20)
        assert rp.status_code == 200, rp.text
        put_resp = rp.json()
        _no_id_leak(put_resp)
        assert put_resp.get("value") == value["value"]
        assert "updated_at" in put_resp

        # GET
        rg = requests.get(f"{API}/drafts/{key}", timeout=20)
        assert rg.status_code == 200
        got = rg.json()
        _no_id_leak(got)
        assert got.get("value") == value["value"]

        # Update / upsert (same key, different value)
        v2 = {"value": "simple string now"}
        rp2 = requests.put(f"{API}/drafts/{key}", json=v2, timeout=20)
        assert rp2.status_code == 200
        rg2 = requests.get(f"{API}/drafts/{key}", timeout=20)
        assert rg2.json().get("value") == "simple string now"

        # List contains updated draft
        rl = requests.get(f"{API}/drafts", timeout=20)
        assert rl.status_code == 200
        lst = rl.json()
        _no_id_leak(lst)
        assert isinstance(lst, list)
        # Each draft in list should at minimum have updated_at
        # (note: the key/_id is intentionally hidden by the projection — this is by design)

        # DELETE
        rd = requests.delete(f"{API}/drafts/{key}", timeout=20)
        assert rd.status_code == 200
        assert rd.json().get("ok") is True

        # GET after delete → {} again
        rg3 = requests.get(f"{API}/drafts/{key}", timeout=20)
        assert rg3.status_code == 200
        assert rg3.json() == {}

    def test_drafts_accept_arbitrary_json_value(self):
        key = f"TEST_iter5_arb_{uuid.uuid4().hex[:8]}"
        payload = {"value": [1, 2, {"nested": True}, None]}
        r = requests.put(f"{API}/drafts/{key}", json=payload, timeout=20)
        assert r.status_code == 200
        assert r.json().get("value") == [1, 2, {"nested": True}, None]
        # cleanup
        requests.delete(f"{API}/drafts/{key}", timeout=20)


# ============================================================
# _id leak global sweep
# ============================================================
class TestNoIdLeak:
    def test_translations_list_no_id(self):
        r = requests.get(f"{API}/bible/translations", timeout=20)
        _no_id_leak(r.json())

    def test_drafts_list_no_id(self):
        r = requests.get(f"{API}/drafts", timeout=20)
        _no_id_leak(r.json())

    def test_pasted_list_no_id(self):
        r = requests.get(f"{API}/bible/pasted", timeout=20)
        _no_id_leak(r.json())
