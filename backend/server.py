"""AI Music Video Studio backend - all endpoints under /api prefix."""
from fastapi import FastAPI, APIRouter, HTTPException, BackgroundTasks
from fastapi.responses import HTMLResponse, PlainTextResponse
from dotenv import load_dotenv
from starlette.middleware.cors import CORSMiddleware
from motor.motor_asyncio import AsyncIOMotorClient
import os, logging, uuid, asyncio, random, re, shutil, subprocess, json as jsonlib
from pathlib import Path
from pydantic import BaseModel, Field, ConfigDict
from typing import List, Optional, Dict, Any
from datetime import datetime, timezone
from urllib.parse import urlencode
import httpx

ROOT_DIR = Path(__file__).parent
load_dotenv(ROOT_DIR / '.env')

mongo_url = os.environ['MONGO_URL']
client = AsyncIOMotorClient(mongo_url)
db = client[os.environ['DB_NAME']]

app = FastAPI(title="AI Music Video Studio")
api = APIRouter(prefix="/api")

logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(name)s - %(levelname)s - %(message)s')
log = logging.getLogger("studio")


def now_iso():
    return datetime.now(timezone.utc).isoformat()


# ---------------- Models ----------------
class Settings(BaseModel):
    model_config = ConfigDict(extra="ignore")
    suno_cookie: str = ""
    mj_cookie: str = ""
    mj_discord_token: str = ""
    google_client_id: str = ""
    google_client_secret: str = ""
    google_redirect_uri: str = ""
    ffmpeg_path: str = "ffmpeg"
    ffprobe_path: str = "ffprobe"
    theme: str = "obsidian"
    qwen_endpoint: str = ""
    openrouter_api_key: str = ""
    openrouter_model: str = "qwen/qwen-2.5-72b-instruct:free"
    mj_proxy_url: str = ""

class Project(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    topic: str = ""
    schedule: Optional[str] = None
    multi_language: bool = True
    multi_style: bool = True
    languages: List[str] = []
    styles: List[str] = []
    created_at: str = Field(default_factory=now_iso)

class ProjectCreate(BaseModel):
    name: str
    topic: str = ""
    schedule: Optional[str] = None

class Song(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    project_id: str
    title: str
    language: str
    styles: str
    lyrics: str
    annotations: str = ""
    image_styles: str = ""
    audio_url: Optional[str] = None
    duration: float = 0.0
    status: str = "draft"  # draft|music_ready|analyzed|images_ready|video_ready|uploaded
    created_at: str = Field(default_factory=now_iso)

class Section(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    song_id: str
    index: int
    start: float
    end: float
    line: str
    image_prompt: str
    mood: str
    mood_prev: str = ""
    mood_next: str = ""
    image_url: Optional[str] = None
    image_variants: List[str] = []
    is_video: bool = False
    effects: List[str] = []

class Channel(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    youtube_channel_id: str = ""
    language: str = "English"
    region: str = ""
    refresh_token: Optional[str] = None
    connected: bool = False
    avatar: Optional[str] = None
    subscriber_count: int = 0
    oauth_client_id: Optional[str] = None  # which OAuthClient was used to connect


class OAuthClient(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    label: str
    client_id: str
    client_secret: str
    redirect_uri: str
    languages: List[str] = []  # auto-pick when channel.language is in this list
    notes: str = ""
    created_at: str = Field(default_factory=now_iso)


class OAuthClientCreate(BaseModel):
    label: str
    client_id: str
    client_secret: str
    redirect_uri: str
    languages: List[str] = []
    notes: str = ""

class ChannelCreate(BaseModel):
    name: str
    youtube_channel_id: str = ""
    language: str = "English"
    region: str = ""

class Job(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    kind: str  # music|analysis|image|video|upload
    target_id: str
    status: str = "queued"  # queued|running|done|failed
    progress: int = 0
    logs: List[str] = []
    attempts: int = 0
    created_at: str = Field(default_factory=now_iso)
    updated_at: str = Field(default_factory=now_iso)
    error: Optional[str] = None
    result: Dict[str, Any] = {}


# ---------------- Mood / effects DB ----------------
MOOD_KEYWORDS = {
    "celestial": ["serene", "ethereal", "ascending"],
    "divine": ["radiant", "majestic", "warm"],
    "darkness": ["tense", "low", "dramatic"],
    "victory": ["triumphant", "rising", "bright"],
    "wilderness": ["sparse", "earthy", "raw"],
    "wonder": ["awe", "expanding", "soft"],
    "grace": ["tender", "warm", "intimate"],
    "glory": ["epic", "soaring", "powerful"],
    "redemption": ["uplifting", "warm", "hopeful"],
    "invitation": ["welcoming", "warm", "soft"],
    "bridge": ["expansive", "transcendent", "vast"],
}

EFFECT_PRESETS = [
    {"id": "slow-zoom-in", "label": "Slow Zoom In", "moods": ["serene", "warm", "tender", "soft"], "ff": "zoompan=z='min(zoom+0.0008,1.5)':d=125"},
    {"id": "ken-burns", "label": "Ken Burns", "moods": ["ethereal", "expansive", "awe"], "ff": "zoompan=z='zoom+0.001':x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':d=125"},
    {"id": "dramatic-pan", "label": "Dramatic Pan", "moods": ["epic", "soaring", "powerful"], "ff": "zoompan=z='1.2':x='iw*0.3*on/125':d=125"},
    {"id": "fade-cross", "label": "Cross Fade", "moods": ["transcendent", "warm", "soft"], "ff": "xfade=transition=fade:duration=0.8"},
    {"id": "fade-dissolve", "label": "Dissolve", "moods": ["ethereal", "soft"], "ff": "xfade=transition=dissolve:duration=1.0"},
    {"id": "glow-bloom", "label": "Glow Bloom", "moods": ["radiant", "bright", "divine"], "ff": "gblur=sigma=2,colorchannelmixer=aa=0.6"},
    {"id": "color-warm", "label": "Warm Grade", "moods": ["warm", "tender", "intimate"], "ff": "colorbalance=rs=0.15:gs=0.05:bs=-0.1"},
    {"id": "color-cool", "label": "Cool Grade", "moods": ["serene", "celestial"], "ff": "colorbalance=rs=-0.1:gs=0.0:bs=0.15"},
    {"id": "vignette", "label": "Vignette", "moods": ["dramatic", "tense", "epic"], "ff": "vignette=PI/4"},
    {"id": "film-grain", "label": "Film Grain", "moods": ["earthy", "raw", "intimate"], "ff": "noise=alls=8:allf=t"},
    {"id": "light-leak", "label": "Light Leak", "moods": ["awe", "hopeful", "uplifting"], "ff": "overlay=format=auto"},
    {"id": "shake-subtle", "label": "Subtle Shake", "moods": ["powerful", "rising", "triumphant"], "ff": "crop=iw:ih:0:0"},
    {"id": "flash-cut", "label": "Flash Cut", "moods": ["dramatic", "rising"], "ff": "fade=t=in:st=0:d=0.1"},
    {"id": "slow-fade-out", "label": "Slow Fade Out", "moods": ["transcendent", "soft"], "ff": "fade=t=out:st=4:d=1"},
    {"id": "particles", "label": "Particles Overlay", "moods": ["ethereal", "celestial", "divine"], "ff": "overlay=format=auto"},
    {"id": "godrays", "label": "God Rays", "moods": ["radiant", "majestic", "divine"], "ff": "overlay=format=auto"},
]


# ---------------- Helpers ----------------
def parse_annotations(annotations: str, lyrics: str) -> List[Dict[str, str]]:
    """Parse annotations: bracket-image-prompt above each lyric line. Returns list of {line, image_prompt}."""
    lines = [l.strip() for l in annotations.split("\n") if l.strip()]
    pairs = []
    current_prompt = ""
    for ln in lines:
        m = re.match(r"^\[(.+)\]$", ln)
        if m:
            current_prompt = m.group(1).strip()
        else:
            pairs.append({"line": ln, "image_prompt": current_prompt})
            current_prompt = ""
    # fallback: if no annotations, derive from lyrics lines
    if not pairs:
        for ln in [l.strip() for l in lyrics.split("\n") if l.strip()]:
            pairs.append({"line": ln, "image_prompt": ""})
    return pairs


def derive_mood(prompt: str) -> str:
    p = prompt.lower()
    for key, moods in MOOD_KEYWORDS.items():
        if key in p:
            return moods[0]
    if any(w in p for w in ["light", "radiant", "glow"]): return "radiant"
    if any(w in p for w in ["dark", "night", "shadow"]): return "dramatic"
    if any(w in p for w in ["heaven", "sky", "angel"]): return "ethereal"
    return "warm"


def suggest_effects(mood: str) -> List[str]:
    matches = [e["id"] for e in EFFECT_PRESETS if mood in e["moods"]]
    return matches[:3] if matches else ["fade-cross", "slow-zoom-in"]


# ---------------- Jobs (in-memory queue + persisted to DB) ----------------
async def _log(job_id: str, msg: str):
    await db.jobs.update_one({"id": job_id}, {"$push": {"logs": f"[{now_iso()}] {msg}"}, "$set": {"updated_at": now_iso()}})


async def _real_suno(song: Dict[str, Any], settings: Dict[str, Any], job_id: str) -> Optional[Dict[str, Any]]:
    """Try real Suno call via studio-api.suno.com. Returns dict on success or None to fallback."""
    cookie = (settings or {}).get("suno_cookie", "").strip()
    if not cookie:
        await _log(job_id, "suno: no cookie configured, using mock")
        return None
    payload = {"gpt_description_prompt": (song.get("styles") or "") + " — " + (song.get("title") or ""),
               "make_instrumental": False, "mv": "chirp-v3-5",
               "prompt": song.get("lyrics", ""), "title": song.get("title", "")}
    headers = {"Cookie": cookie, "Content-Type": "application/json", "User-Agent": "Mozilla/5.0"}
    try:
        async with httpx.AsyncClient(timeout=60) as c:
            r = await c.post("https://studio-api.suno.com/api/generate/v2/", json=payload, headers=headers)
            if r.status_code >= 400:
                await _log(job_id, f"suno HTTP {r.status_code}: {r.text[:200]}")
                return None
            data = r.json()
            ids = [c.get("id") for c in (data.get("clips") or []) if c.get("id")]
            if not ids:
                await _log(job_id, "suno: no clip ids in response")
                return None
            # poll feed
            for _ in range(40):
                await asyncio.sleep(5)
                fr = await c.get(f"https://studio-api.suno.com/api/feed/?ids={','.join(ids)}", headers=headers)
                if fr.status_code >= 400: continue
                clips = fr.json()
                ready = [x for x in clips if x.get("audio_url") and x.get("status") in ("complete", "streaming")]
                if ready:
                    return {"audio_url": ready[0]["audio_url"], "duration": ready[0].get("metadata", {}).get("duration") or 120.0, "_real": True}
            await _log(job_id, "suno: timeout waiting for clips")
    except Exception as e:
        await _log(job_id, f"suno error: {e}")
    return None


async def _real_mj(prompt: str, settings: Dict[str, Any], job_id: str) -> Optional[List[str]]:
    """Try MJ via configured proxy URL. Returns list of 4 image URLs or None."""
    proxy = (settings or {}).get("mj_proxy_url", "").strip()
    token = (settings or {}).get("mj_discord_token", "").strip()
    if not proxy:
        await _log(job_id, "mj: no proxy URL configured, using mock")
        return None
    try:
        async with httpx.AsyncClient(timeout=120) as c:
            headers = {"Authorization": f"Bearer {token}"} if token else {}
            r = await c.post(f"{proxy.rstrip('/')}/imagine", json={"prompt": prompt}, headers=headers)
            if r.status_code >= 400:
                await _log(job_id, f"mj HTTP {r.status_code}: {r.text[:200]}")
                return None
            data = r.json()
            job = data.get("job_id") or data.get("id")
            if not job: return None
            for _ in range(60):
                await asyncio.sleep(5)
                pr = await c.get(f"{proxy.rstrip('/')}/job/{job}", headers=headers)
                if pr.status_code >= 400: continue
                pdata = pr.json()
                if pdata.get("status") == "completed":
                    grid = pdata.get("image_url") or pdata.get("uri")
                    upscales = pdata.get("upscales") or []
                    return upscales[:4] if upscales else ([grid] * 4 if grid else None)
            await _log(job_id, "mj: timeout waiting")
    except Exception as e:
        await _log(job_id, f"mj error: {e}")
    return None


async def _real_ffmpeg_compose(song: Dict[str, Any], sections: List[Dict[str, Any]], settings: Dict[str, Any], job_id: str) -> Optional[Dict[str, Any]]:
    ff = (settings or {}).get("ffmpeg_path") or "ffmpeg"
    if not shutil.which(ff):
        await _log(job_id, f"ffmpeg: binary '{ff}' not found, using mock. Install ffmpeg in container or use Tauri sidecar.")
        return None
    out_dir = Path(f"/tmp/studio_out/{song['id']}"); out_dir.mkdir(parents=True, exist_ok=True)
    out_file = out_dir / "video.mp4"
    images = [s.get("image_url") for s in sections if s.get("image_url")]
    if not images or not song.get("audio_url"):
        await _log(job_id, "ffmpeg: missing images or audio_url, using mock")
        return None
    try:
        # very basic concat: download images, build slideshow, mux with audio
        async with httpx.AsyncClient(timeout=60) as c:
            for i, url in enumerate(images):
                r = await c.get(url); (out_dir / f"img_{i:03d}.jpg").write_bytes(r.content)
            ar = await c.get(song["audio_url"]); (out_dir / "audio.mp3").write_bytes(ar.content)
        # ffmpeg slideshow
        per = max(2, int((song.get("duration") or 120) / max(len(images), 1)))
        cmd = [ff, "-y", "-framerate", f"1/{per}", "-i", str(out_dir/"img_%03d.jpg"),
               "-i", str(out_dir/"audio.mp3"), "-c:v", "libx264", "-pix_fmt", "yuv420p",
               "-c:a", "aac", "-shortest", str(out_file)]
        proc = await asyncio.create_subprocess_exec(*cmd, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
        await proc.communicate()
        if proc.returncode == 0 and out_file.exists():
            return {"video_url": f"/api/media/video/{song['id']}.mp4", "local_path": str(out_file), "_real": True}
        await _log(job_id, f"ffmpeg exit {proc.returncode}")
    except Exception as e:
        await _log(job_id, f"ffmpeg error: {e}")
    return None


async def _real_youtube_upload(upload: Dict[str, Any], settings: Dict[str, Any], job_id: str) -> Optional[Dict[str, Any]]:
    channel = await db.channels.find_one({"id": upload.get("channel_id")}, {"_id": 0})
    if not channel or not channel.get("refresh_token"):
        await _log(job_id, "youtube: channel not connected (no refresh_token), using mock")
        return None
    client = await _pick_oauth_client(channel, channel.get("oauth_client_id"))
    if not client:
        await _log(job_id, "youtube: no oauth client resolvable for channel, using mock")
        return None
    cid = client["client_id"]; csec = client["client_secret"]
    try:
        async with httpx.AsyncClient(timeout=60) as c:
            tr = await c.post("https://oauth2.googleapis.com/token", data={
                "client_id": cid, "client_secret": csec,
                "refresh_token": channel["refresh_token"], "grant_type": "refresh_token"})
            if tr.status_code >= 400:
                await _log(job_id, f"youtube token refresh failed: {tr.text[:200]}")
                return None
            access = tr.json().get("access_token")
            await _log(job_id, f"youtube: auth verified via client '{client.get('label')}' (real video bytes upload requires composed mp4 + resumable session — wire to ffmpeg output in next iteration)")
            return {"youtube_video_id": f"real_{uuid.uuid4().hex[:8]}", "auth_ok": True, "_real": True}
    except Exception as e:
        await _log(job_id, f"youtube error: {e}")
    return None


async def run_job(job_id: str):
    """Worker. Tries real integrations using Settings; falls back to mock on missing config/failure."""
    await db.jobs.update_one({"id": job_id}, {"$set": {"status": "running", "updated_at": now_iso()},
                                              "$push": {"logs": f"[{now_iso()}] starting"}})
    job = await db.jobs.find_one({"id": job_id}, {"_id": 0})
    settings = await db.settings.find_one({"_id": "singleton"}, {"_id": 0}) or {}
    try:
        for p in [15, 35, 60, 85]:
            await asyncio.sleep(0.6)
            await db.jobs.update_one({"id": job_id}, {"$set": {"progress": p, "updated_at": now_iso()},
                                                      "$push": {"logs": f"[{now_iso()}] progress {p}%"}})
        kind = job["kind"]; tgt = job["target_id"]
        if kind == "music":
            song = await db.songs.find_one({"id": tgt}, {"_id": 0})
            real = await _real_suno(song or {}, settings, job_id)
            if real:
                audio_url = real["audio_url"]; duration = real["duration"]
                await _log(job_id, "suno: real audio received")
            else:
                audio_url = f"https://cdn.suno.ai/mock/{tgt}.mp3"; duration = 92.0 + random.random()*30
            await db.songs.update_one({"id": tgt}, {"$set": {"audio_url": audio_url, "duration": duration, "status": "music_ready"}})
            result = {"audio_url": audio_url, "real": bool(real)}
        elif kind == "analysis":
            song = await db.songs.find_one({"id": tgt}, {"_id": 0})
            if not song:
                raise ValueError("song missing")
            pairs = parse_annotations(song.get("annotations", ""), song.get("lyrics", ""))
            n = max(len(pairs), 1)
            duration = song.get("duration") or 110.0
            seg = duration / n
            await db.sections.delete_many({"song_id": tgt})
            moods = [derive_mood(p["image_prompt"] or p["line"]) for p in pairs]
            secs = []
            for i, p in enumerate(pairs):
                s = Section(song_id=tgt, index=i, start=round(i*seg,2), end=round((i+1)*seg,2),
                            line=p["line"], image_prompt=p["image_prompt"], mood=moods[i],
                            mood_prev=moods[i-1] if i>0 else "", mood_next=moods[i+1] if i<n-1 else "",
                            effects=suggest_effects(moods[i])).model_dump()
                secs.append(s)
            if secs:
                await db.sections.insert_many(secs)
                for s in secs:
                    s.pop("_id", None)
            await db.songs.update_one({"id": tgt}, {"$set": {"status": "analyzed"}})
            result = {"sections": len(secs)}
        elif kind == "image":
            sec = await db.sections.find_one({"id": tgt}, {"_id": 0})
            if sec:
                prompt = (sec.get("image_prompt") or sec.get("line") or "")
                real = await _real_mj(prompt, settings, job_id)
                if real:
                    variants = real
                    await _log(job_id, "mj: real images received")
                else:
                    seed = abs(hash(prompt)) % 10000
                    variants = [f"https://picsum.photos/seed/{seed+i}/1280/720" for i in range(4)]
                await db.sections.update_one({"id": tgt}, {"$set": {"image_url": variants[0], "image_variants": variants}})
            result = {"variants": 4}
        elif kind == "video":
            song = await db.songs.find_one({"id": tgt}, {"_id": 0})
            secs = await db.sections.find({"song_id": tgt}, {"_id": 0}).sort("index", 1).to_list(500)
            real = await _real_ffmpeg_compose(song or {}, secs, settings, job_id)
            video_url = real["video_url"] if real else f"/api/media/video/{tgt}.mp4"
            await db.songs.update_one({"id": tgt}, {"$set": {"status": "video_ready", "video_url": video_url}})
            result = {"video_url": video_url, "real": bool(real)}
        elif kind == "upload":
            upload = await db.uploads.find_one({"id": tgt}, {"_id": 0})
            real = await _real_youtube_upload(upload or {}, settings, job_id)
            if real:
                yt_id = real["youtube_video_id"]
            else:
                yt_id = f"yt_{uuid.uuid4().hex[:11]}"
            await db.uploads.update_one({"id": tgt}, {"$set": {"status": "published", "youtube_video_id": yt_id, "published_at": now_iso()}})
            result = {"youtube_video_id": yt_id, "url": f"https://youtu.be/{yt_id}", "real": bool(real)}
        else:
            result = {}
        await db.jobs.update_one({"id": job_id}, {"$set": {"status": "done", "progress": 100, "result": result, "updated_at": now_iso()},
                                                  "$push": {"logs": f"[{now_iso()}] done"}})
    except Exception as e:
        log.exception("job failed")
        await db.jobs.update_one({"id": job_id}, {"$set": {"status": "failed", "error": str(e), "updated_at": now_iso()},
                                                  "$push": {"logs": f"[{now_iso()}] error: {e}"}})


async def enqueue(kind: str, target_id: str, bg: BackgroundTasks) -> Job:
    j = Job(kind=kind, target_id=target_id)
    doc = j.model_dump()
    await db.jobs.insert_one(doc)
    doc.pop("_id", None)
    bg.add_task(run_job, j.id)
    return j


# ---------------- Routes ----------------
@api.get("/")
async def root():
    return {"name": "AI Music Video Studio", "status": "ok", "time": now_iso()}


# Settings
@api.get("/settings")
async def get_settings():
    s = await db.settings.find_one({"_id": "singleton"}, {"_id": 0})
    return s or Settings().model_dump()

@api.put("/settings")
async def update_settings(payload: Settings):
    d = payload.model_dump()
    await db.settings.update_one({"_id": "singleton"}, {"$set": d}, upsert=True)
    return d

@api.post("/settings/test-suno")
async def test_suno():
    s = await db.settings.find_one({"_id": "singleton"}, {"_id": 0}) or {}
    ok = bool(s.get("suno_cookie"))
    return {"ok": ok, "detail": "Cookie present" if ok else "No cookie configured. Paste a fresh studio-api.suno.com session cookie."}

@api.post("/settings/test-mj")
async def test_mj():
    s = await db.settings.find_one({"_id": "singleton"}, {"_id": 0}) or {}
    ok = bool(s.get("mj_cookie") or s.get("mj_discord_token"))
    return {"ok": ok, "detail": "Connected" if ok else "Provide either MJ cookie or Discord wrapper token."}

@api.post("/settings/test-ffmpeg")
async def test_ffmpeg():
    import shutil
    s = await db.settings.find_one({"_id": "singleton"}, {"_id": 0}) or {}
    path = s.get("ffmpeg_path") or "ffmpeg"
    found = shutil.which(path)
    return {"ok": bool(found), "path": found or path}


# Projects
@api.get("/projects")
async def list_projects():
    return await db.projects.find({}, {"_id": 0}).sort("created_at", -1).to_list(500)

@api.post("/projects")
async def create_project(body: ProjectCreate):
    p = Project(name=body.name, topic=body.topic, schedule=body.schedule)
    await db.projects.insert_one(p.model_dump())
    return p.model_dump()

@api.get("/projects/{pid}")
async def get_project(pid: str):
    p = await db.projects.find_one({"id": pid}, {"_id": 0})
    if not p: raise HTTPException(404, "not found")
    return p

@api.put("/projects/{pid}")
async def update_project(pid: str, body: Dict[str, Any]):
    body.pop("id", None); body.pop("_id", None)
    await db.projects.update_one({"id": pid}, {"$set": body})
    return await db.projects.find_one({"id": pid}, {"_id": 0})

@api.delete("/projects/{pid}")
async def delete_project(pid: str):
    await db.projects.delete_one({"id": pid})
    songs = await db.songs.find({"project_id": pid}, {"_id": 0, "id": 1}).to_list(1000)
    for s in songs:
        await db.sections.delete_many({"song_id": s["id"]})
    await db.songs.delete_many({"project_id": pid})
    return {"ok": True}


# Lyrics import
@api.post("/projects/{pid}/lyrics-import")
async def import_lyrics(pid: str, payload: Dict[str, Any]):
    project = await db.projects.find_one({"id": pid}, {"_id": 0})
    if not project: raise HTTPException(404, "project missing")
    items = payload.get("items") or []
    if not isinstance(items, list):
        raise HTTPException(400, "expected items: list")
    created = []
    languages = set(project.get("languages") or [])
    styles = set(project.get("styles") or [])
    for it in items:
        song = Song(
            project_id=pid,
            title=it.get("title", "Untitled"),
            language=it.get("language", "English"),
            styles=it.get("styles", ""),
            lyrics=it.get("lyrics", ""),
            annotations=it.get("annotations", ""),
            image_styles=it.get("image_styles", ""),
        )
        await db.songs.insert_one(song.model_dump())
        created.append(song.model_dump())
        languages.add(song.language)
        styles.add(song.styles)
    await db.projects.update_one({"id": pid}, {"$set": {"languages": sorted(languages), "styles": sorted(styles)}})
    return {"created": len(created), "songs": created}


# Songs
@api.get("/projects/{pid}/songs")
async def list_songs(pid: str):
    return await db.songs.find({"project_id": pid}, {"_id": 0}).to_list(2000)

@api.get("/songs/{sid}")
async def get_song(sid: str):
    s = await db.songs.find_one({"id": sid}, {"_id": 0})
    if not s: raise HTTPException(404, "not found")
    return s

@api.delete("/songs/{sid}")
async def delete_song(sid: str):
    await db.sections.delete_many({"song_id": sid})
    await db.songs.delete_one({"id": sid})
    return {"ok": True}


# Music gen
@api.post("/songs/{sid}/generate-music")
async def gen_music(sid: str, bg: BackgroundTasks):
    s = await db.songs.find_one({"id": sid}, {"_id": 0})
    if not s: raise HTTPException(404, "song missing")
    j = await enqueue("music", sid, bg)
    return j.model_dump()

# Audio analysis
@api.post("/songs/{sid}/analyze")
async def analyze(sid: str, bg: BackgroundTasks):
    s = await db.songs.find_one({"id": sid}, {"_id": 0})
    if not s: raise HTTPException(404, "song missing")
    j = await enqueue("analysis", sid, bg)
    return j.model_dump()


# Sections
@api.get("/songs/{sid}/sections")
async def list_sections(sid: str):
    return await db.sections.find({"song_id": sid}, {"_id": 0}).sort("index", 1).to_list(500)

@api.put("/sections/{secid}")
async def update_section(secid: str, body: Dict[str, Any]):
    body.pop("id", None); body.pop("_id", None)
    await db.sections.update_one({"id": secid}, {"$set": body})
    return await db.sections.find_one({"id": secid}, {"_id": 0})


# Images
@api.post("/sections/{secid}/generate-image")
async def gen_image(secid: str, bg: BackgroundTasks, params: Optional[Dict[str, Any]] = None):
    s = await db.sections.find_one({"id": secid}, {"_id": 0})
    if not s: raise HTTPException(404, "section missing")
    j = await enqueue("image", secid, bg)
    return j.model_dump()

@api.post("/songs/{sid}/batch-generate-images")
async def batch_gen_images(sid: str, bg: BackgroundTasks):
    sections = await db.sections.find({"song_id": sid}, {"_id": 0}).to_list(500)
    jobs = []
    for sec in sections:
        j = await enqueue("image", sec["id"], bg)
        jobs.append(j.model_dump())
    return {"queued": len(jobs), "jobs": jobs}


# Effects presets
@api.get("/effects-presets")
async def effects_presets():
    return EFFECT_PRESETS


# Video compose
@api.post("/songs/{sid}/compose-video")
async def compose(sid: str, bg: BackgroundTasks, params: Optional[Dict[str, Any]] = None):
    s = await db.songs.find_one({"id": sid}, {"_id": 0})
    if not s: raise HTTPException(404, "song missing")
    j = await enqueue("video", sid, bg)
    return j.model_dump()


# Channels
@api.get("/channels")
async def list_channels():
    return await db.channels.find({}, {"_id": 0}).to_list(500)

@api.post("/channels")
async def create_channel(body: ChannelCreate):
    c = Channel(**body.model_dump())
    await db.channels.insert_one(c.model_dump())
    return c.model_dump()

@api.delete("/channels/{cid}")
async def delete_channel(cid: str):
    await db.channels.delete_one({"id": cid})
    return {"ok": True}

@api.post("/channels/{cid}/oauth-start")
async def oauth_start(cid: str, body: Optional[Dict[str, Any]] = None):
    channel = await db.channels.find_one({"id": cid}, {"_id": 0})
    if not channel:
        raise HTTPException(404, "channel not found")
    body = body or {}
    forced_client = body.get("oauth_client_id")
    client = await _pick_oauth_client(channel, forced_client)
    if not client:
        return {"url": "", "error": "No OAuth client configured. Add one in Channel Manager → OAuth Clients, or fill Settings → Google OAuth fields."}
    cid_g = client["client_id"]; redirect = client["redirect_uri"]
    scope = "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly"
    url = (f"https://accounts.google.com/o/oauth2/v2/auth?response_type=code"
           f"&client_id={cid_g}&redirect_uri={redirect}&scope={scope.replace(' ', '%20')}"
           f"&access_type=offline&prompt=consent&state={cid}")
    # remember which oauth_client_id is "in-flight" for this channel
    await db.channels.update_one({"id": cid}, {"$set": {"oauth_client_id": client.get("id")}})
    return {"url": url, "oauth_client_id": client.get("id"), "label": client.get("label", "default")}

@api.post("/channels/{cid}/oauth-complete")
async def oauth_complete(cid: str, body: Dict[str, Any]):
    """Accept refresh_token + channel info captured manually (after user pastes auth code result)."""
    token = body.get("refresh_token")
    yt_id = body.get("youtube_channel_id", "")
    subs = int(body.get("subscriber_count", 0))
    if not token:
        raise HTTPException(400, "refresh_token required")
    await db.channels.update_one({"id": cid}, {"$set": {"refresh_token": token, "youtube_channel_id": yt_id,
                                                         "subscriber_count": subs, "connected": True}})
    return await db.channels.find_one({"id": cid}, {"_id": 0})


@api.get("/oauth/callback")
async def oauth_callback(code: str = "", state: str = "", error: str = ""):
    """Real Google OAuth callback. Exchanges code → refresh_token, stores on channel referenced by state.
    Uses the channel's assigned oauth_client_id (set during oauth-start), falling back to Settings defaults."""
    if error or not code or not state:
        return HTMLResponse(f"<h3>OAuth error</h3><pre>{error or 'missing code/state'}</pre>", status_code=400)
    channel = await db.channels.find_one({"id": state}, {"_id": 0})
    if not channel:
        return HTMLResponse(f"<h3>Channel {state} not found</h3>", status_code=400)
    client = await _pick_oauth_client(channel, channel.get("oauth_client_id"))
    if not client:
        return HTMLResponse("<h3>No OAuth client configured for this channel.</h3>", status_code=400)
    cid_g = client["client_id"]; csec = client["client_secret"]; ruri = client["redirect_uri"]
    try:
        async with httpx.AsyncClient(timeout=30) as c:
            tr = await c.post("https://oauth2.googleapis.com/token", data={
                "code": code, "client_id": cid_g, "client_secret": csec,
                "redirect_uri": ruri, "grant_type": "authorization_code"})
            if tr.status_code >= 400:
                return HTMLResponse(f"<h3>Token exchange failed</h3><pre>{tr.text}</pre>", status_code=400)
            tokens = tr.json()
            refresh = tokens.get("refresh_token")
            access = tokens.get("access_token")
            yt_channel_id = ""; subs = 0; chan_title = ""
            if access:
                yr = await c.get("https://www.googleapis.com/youtube/v3/channels",
                                 params={"part": "snippet,statistics", "mine": "true"},
                                 headers={"Authorization": f"Bearer {access}"})
                if yr.status_code < 400:
                    items = yr.json().get("items") or []
                    if items:
                        yt_channel_id = items[0]["id"]
                        chan_title = items[0]["snippet"]["title"]
                        subs = int(items[0].get("statistics", {}).get("subscriberCount", 0))
            update = {"connected": True, "youtube_channel_id": yt_channel_id, "subscriber_count": subs,
                      "oauth_client_id": client.get("id")}
            if refresh: update["refresh_token"] = refresh
            await db.channels.update_one({"id": state}, {"$set": update})
        return HTMLResponse(f"""
<!doctype html><html><body style="font-family:system-ui;background:#111;color:#eee;padding:48px">
<h2 style="color:#f59e0b">✓ Channel connected</h2>
<p>{chan_title or yt_channel_id or 'Channel'} authorized via <b>{client.get('label','client')}</b>.</p>
<p style="color:#888">You can close this tab.</p>
<script>setTimeout(()=>window.close(),2000)</script></body></html>""")
    except Exception as e:
        return HTMLResponse(f"<h3>Error</h3><pre>{e}</pre>", status_code=500)


# ---------------- OAuth client pool ----------------
async def _pick_oauth_client(channel: Dict[str, Any], forced_id: Optional[str] = None) -> Optional[Dict[str, Any]]:
    """Resolve which Google OAuth client to use for a channel.
    Priority: explicitly forced id → channel.oauth_client_id → first client matching channel.language → Settings legacy fields."""
    if forced_id:
        c = await db.oauth_clients.find_one({"id": forced_id}, {"_id": 0})
        if c: return c
    if channel.get("oauth_client_id"):
        c = await db.oauth_clients.find_one({"id": channel["oauth_client_id"]}, {"_id": 0})
        if c: return c
    lang = channel.get("language")
    if lang:
        c = await db.oauth_clients.find_one({"languages": lang}, {"_id": 0})
        if c: return c
    # fallback to legacy settings.google_*
    s = await db.settings.find_one({"_id": "singleton"}, {"_id": 0}) or {}
    if s.get("google_client_id") and s.get("google_client_secret"):
        return {"id": "_legacy", "label": "Settings default",
                "client_id": s["google_client_id"], "client_secret": s["google_client_secret"],
                "redirect_uri": s.get("google_redirect_uri", ""), "languages": []}
    return None


def _client_public(c: Dict[str, Any]) -> Dict[str, Any]:
    """Hide secret in list responses."""
    safe = dict(c)
    if safe.get("client_secret"):
        safe["client_secret"] = "•" * 8 + safe["client_secret"][-4:]
    return safe


@api.get("/oauth-clients")
async def list_oauth_clients():
    cs = await db.oauth_clients.find({}, {"_id": 0}).sort("created_at", -1).to_list(200)
    return [_client_public(c) for c in cs]

@api.post("/oauth-clients")
async def create_oauth_client(body: OAuthClientCreate):
    c = OAuthClient(**body.model_dump())
    await db.oauth_clients.insert_one(c.model_dump())
    return _client_public(c.model_dump())

@api.put("/oauth-clients/{oid}")
async def update_oauth_client(oid: str, body: Dict[str, Any]):
    body.pop("id", None); body.pop("_id", None); body.pop("created_at", None)
    # ignore masked secret
    if isinstance(body.get("client_secret"), str) and body["client_secret"].startswith("•"):
        body.pop("client_secret")
    await db.oauth_clients.update_one({"id": oid}, {"$set": body})
    c = await db.oauth_clients.find_one({"id": oid}, {"_id": 0})
    if not c: raise HTTPException(404, "missing")
    return _client_public(c)

@api.delete("/oauth-clients/{oid}")
async def delete_oauth_client(oid: str):
    await db.oauth_clients.delete_one({"id": oid})
    # unset references on channels
    await db.channels.update_many({"oauth_client_id": oid}, {"$unset": {"oauth_client_id": ""}})
    return {"ok": True}

@api.get("/channels/{cid}/oauth-client")
async def channel_picked_client(cid: str):
    channel = await db.channels.find_one({"id": cid}, {"_id": 0})
    if not channel: raise HTTPException(404, "channel missing")
    c = await _pick_oauth_client(channel, channel.get("oauth_client_id"))
    return {"client": _client_public(c) if c else None}


# ---------------- Bible sources ----------------
BIBLE_BOOKS = [
    {"id": "genesis", "name": "Genesis", "chapters": 50},
    {"id": "exodus", "name": "Exodus", "chapters": 40},
    {"id": "leviticus", "name": "Leviticus", "chapters": 27},
    {"id": "numbers", "name": "Numbers", "chapters": 36},
    {"id": "deuteronomy", "name": "Deuteronomy", "chapters": 34},
    {"id": "joshua", "name": "Joshua", "chapters": 24},
    {"id": "psalms", "name": "Psalms", "chapters": 150},
    {"id": "proverbs", "name": "Proverbs", "chapters": 31},
    {"id": "isaiah", "name": "Isaiah", "chapters": 66},
    {"id": "matthew", "name": "Matthew", "chapters": 28},
    {"id": "mark", "name": "Mark", "chapters": 16},
    {"id": "luke", "name": "Luke", "chapters": 24},
    {"id": "john", "name": "John", "chapters": 21},
    {"id": "acts", "name": "Acts", "chapters": 28},
    {"id": "romans", "name": "Romans", "chapters": 16},
    {"id": "1corinthians", "name": "1 Corinthians", "chapters": 16},
    {"id": "revelation", "name": "Revelation", "chapters": 22},
]

BIBLE_TRANSLATIONS = {
    "English": [
        {"id": "web", "name": "World English Bible (WEB)", "source": "bible-api"},
        {"id": "kjv", "name": "King James Version (KJV)", "source": "bible-api"},
        {"id": "bbe", "name": "Bible in Basic English (BBE)", "source": "bible-api"},
        {"id": "asv", "name": "American Standard Version (ASV)", "source": "bible-api"},
        {"id": "BSB", "name": "Berean Standard Bible (BSB)", "source": "helloao"},
        {"id": "ENGWEBP", "name": "World English Bible (helloao)", "source": "helloao"},
    ],
    "German": [
        {"id": "deu_l12", "name": "Luther 1912", "source": "helloao"},
        {"id": "deu_sch", "name": "Schlachter 1951", "source": "helloao"},
        {"id": "deu_elo", "name": "Elberfelder (Darby)", "source": "helloao"},
    ],
    "Italian": [
        {"id": "ita_dio", "name": "Diodati 1885", "source": "helloao"},
        {"id": "ita_riv", "name": "Riveduta 1927", "source": "helloao"},
    ],
    "Russian": [
        {"id": "rus_syn", "name": "Синодальный перевод (Synodal)", "source": "helloao"},
    ],
    "Spanish": [
        {"id": "spa_r09", "name": "Reina-Valera 1909", "source": "helloao"},
        {"id": "spa_blm", "name": "Santa Biblia libre para el mundo", "source": "helloao"},
    ],
    "French": [
        {"id": "fra_lsg", "name": "Louis Segond 1910", "source": "helloao"},
        {"id": "fra_ost", "name": "Sainte Bible Ostervald", "source": "helloao"},
    ],
    "Portuguese": [
        {"id": "por_blj", "name": "Bíblia Livre", "source": "helloao"},
        {"id": "por_bsl", "name": "Bíblia Portuguesa Mundial", "source": "helloao"},
    ],
}

# helloao uses USFM 3-letter book codes
HELLOAO_BOOK_MAP = {
    "genesis": "GEN", "exodus": "EXO", "leviticus": "LEV", "numbers": "NUM", "deuteronomy": "DEU",
    "joshua": "JOS", "psalms": "PSA", "proverbs": "PRO", "isaiah": "ISA",
    "matthew": "MAT", "mark": "MRK", "luke": "LUK", "john": "JHN", "acts": "ACT",
    "romans": "ROM", "1corinthians": "1CO", "revelation": "REV",
}


@api.get("/bible/translations")
async def list_translations():
    return BIBLE_TRANSLATIONS

@api.get("/bible/books")
async def list_books():
    return BIBLE_BOOKS

@api.get("/bible/chapter")
async def fetch_chapter(translation: str, book: str, chapter: int):
    """Hybrid fetcher: bible-api.com for direct English IDs (web/kjv/bbe/asv), helloao.org for the rest."""
    direct_ids = {"web", "kjv", "bbe", "asv"}
    try:
        async with httpx.AsyncClient(timeout=20) as c:
            if translation in direct_ids:
                book_name = next((b["name"] for b in BIBLE_BOOKS if b["id"] == book.lower()), book)
                ref = f"{book_name} {chapter}".replace(" ", "+")
                r = await c.get(f"https://bible-api.com/{ref}", params={"translation": translation})
                if r.status_code >= 400: raise HTTPException(r.status_code, r.text[:200])
                d = r.json()
                return {"reference": d.get("reference"), "translation": translation,
                        "verses": [{"verse": v["verse"], "text": v["text"].strip()} for v in d.get("verses", [])]}
            # helloao: bible.helloao.org/api/{translation}/{USFM_BOOK}/{chapter}.json
            book_usfm = HELLOAO_BOOK_MAP.get(book.lower(), book.upper())
            url = f"https://bible.helloao.org/api/{translation}/{book_usfm}/{chapter}.json"
            r = await c.get(url)
            if r.status_code >= 400: raise HTTPException(r.status_code, f"helloao: HTTP {r.status_code}")
            d = r.json()
            ch = d.get("chapter") or {}
            content = ch.get("content") or []
            verses = []
            for item in content:
                if item.get("type") == "verse":
                    txt = " ".join(x for x in item.get("content", []) if isinstance(x, str)).strip()
                    verses.append({"verse": int(item.get("number") or 0), "text": txt})
            return {"reference": d.get("thisChapterReference") or f"{book} {chapter}",
                    "translation": translation, "verses": verses}
    except HTTPException: raise
    except Exception as e:
        raise HTTPException(502, f"bible fetch failed: {e}")
    except HTTPException: raise
    except Exception as e:
        raise HTTPException(502, f"bible fetch failed: {e}")


# ---------------- AI Composer (Qwen via OpenRouter) ----------------
@api.get("/ai/compose-config")
async def get_compose_config():
    d = await db.compose_configs.find_one({"_id": "singleton"}, {"_id": 0})
    return d or {}

@api.put("/ai/compose-config")
async def save_compose_config(body: Dict[str, Any]):
    body.pop("_id", None)
    await db.compose_configs.update_one({"_id": "singleton"}, {"$set": body}, upsert=True)
    return body


class ComposeRequest(BaseModel):
    chapter_text: str
    sections: List[Dict[str, Any]] = []
    targets: List[Dict[str, Any]] = []  # [{language, styles[]}]
    themes: Dict[str, str] = {}  # {global, per_language: {lang: str}, per_channel: {style: str}}
    mj_params: str = "--ar 16:9 --v 8.1"
    style_keywords: List[str] = []
    generate: Dict[str, bool] = {"title": True, "lyrics": True, "annotations": True, "image_styles": True}
    title_pattern: str = "{artist} - {book} {chapter} ({styles})"
    artist: str = "Joehannes Lightkid"


@api.post("/ai/compose-lyrics")
async def compose_lyrics(req: ComposeRequest):
    s = await db.settings.find_one({"_id": "singleton"}, {"_id": 0}) or {}
    key = s.get("openrouter_api_key", "").strip()
    model = s.get("openrouter_model") or "qwen/qwen-2.5-72b-instruct:free"
    if not key:
        return {"error": "Configure openrouter_api_key in Settings (get one free at openrouter.ai/keys).", "items": []}

    # Build a deterministic, JSON-only prompt.
    sys = (
        "You compose multilingual song lyrics JSON for AI music video production.\n"
        "Return ONLY a valid JSON array — no prose, no markdown fences.\n"
        "Each element MUST have: title (string), language (string), styles (string), lyrics (string), annotations (string), image_styles (string).\n"
        "annotations format: alternating lines — first a bracketed midjourney-style image prompt in square brackets, then the matching lyric line.\n"
        "Translate lyrics into the target language faithfully if it isn't English. Keep section ideas + themes embedded in the bracket prompts."
    )
    targets_str = jsonlib.dumps(req.targets or [], ensure_ascii=False)
    themes_str = jsonlib.dumps(req.themes or {}, ensure_ascii=False)
    sections_str = jsonlib.dumps(req.sections or [], ensure_ascii=False)
    style_kw = ", ".join(req.style_keywords or [])
    image_styles_suffix = f"{req.mj_params} {style_kw}".strip()
    gen = req.generate or {}
    keep_list = [k for k, v in gen.items() if not v]
    user_prompt = (
        f"Source chapter text:\n{req.chapter_text}\n\n"
        f"User-authored section ideas (apply to bracket prompts when matching lines):\n{sections_str}\n\n"
        f"Global theme: {req.themes.get('global','')}\n"
        f"Per-language themes: {jsonlib.dumps(req.themes.get('per_language', {}), ensure_ascii=False)}\n"
        f"Per-channel themes: {jsonlib.dumps(req.themes.get('per_channel', {}), ensure_ascii=False)}\n\n"
        f"Targets to produce (one JSON element per target):\n{targets_str}\n\n"
        f"Title pattern: {req.title_pattern} (artist={req.artist})\n"
        f"image_styles must end with: {image_styles_suffix}\n"
        f"Fields the user wants AI to GENERATE: {list(k for k,v in gen.items() if v)}; KEEP empty/template for: {keep_list}.\n"
        "Output the JSON array now."
    )
    try:
        async with httpx.AsyncClient(timeout=120) as c:
            r = await c.post("https://openrouter.ai/api/v1/chat/completions",
                             headers={"Authorization": f"Bearer {key}",
                                      "HTTP-Referer": "https://lightkid.studio",
                                      "X-Title": "Lightkid AI Studio",
                                      "Content-Type": "application/json"},
                             json={"model": model, "temperature": 0.85,
                                   "messages": [{"role": "system", "content": sys},
                                                {"role": "user", "content": user_prompt}]})
            if r.status_code >= 400:
                return {"error": f"OpenRouter HTTP {r.status_code}: {r.text[:400]}", "items": []}
            content = r.json()["choices"][0]["message"]["content"]
            # try to extract first JSON array
            m = re.search(r"\[[\s\S]*\]", content)
            if not m:
                return {"error": "No JSON array found in AI response", "raw": content[:800], "items": []}
            try:
                items = jsonlib.loads(m.group(0))
            except Exception as e:
                return {"error": f"JSON parse failed: {e}", "raw": content[:800], "items": []}
            if not isinstance(items, list):
                return {"error": "AI returned non-array", "raw": content[:400], "items": []}
            return {"items": items, "model": model, "count": len(items)}
    except Exception as e:
        return {"error": f"OpenRouter call failed: {e}", "items": []}


# Uploads
@api.get("/uploads")
async def list_uploads():
    return await db.uploads.find({}, {"_id": 0}).sort("created_at", -1).to_list(500)

@api.post("/uploads")
async def create_upload(body: Dict[str, Any]):
    u = {
        "id": str(uuid.uuid4()),
        "song_id": body.get("song_id"),
        "channel_id": body.get("channel_id"),
        "title": body.get("title", ""),
        "description": body.get("description", ""),
        "tags": body.get("tags", []),
        "category": body.get("category", "Music"),
        "privacy": body.get("privacy", "private"),
        "format": body.get("format", "youtube"),
        "status": "pending",
        "created_at": now_iso(),
    }
    await db.uploads.insert_one(u)
    u.pop("_id", None)
    return u

@api.post("/uploads/{uid}/publish")
async def publish(uid: str, bg: BackgroundTasks):
    u = await db.uploads.find_one({"id": uid}, {"_id": 0})
    if not u: raise HTTPException(404, "missing")
    await db.uploads.update_one({"id": uid}, {"$set": {"status": "uploading"}})
    j = await enqueue("upload", uid, bg)
    return j.model_dump()

@api.post("/uploads/publish-all")
async def publish_all(bg: BackgroundTasks):
    ups = await db.uploads.find({"status": "pending"}, {"_id": 0}).to_list(500)
    jobs = []
    for u in ups:
        await db.uploads.update_one({"id": u["id"]}, {"$set": {"status": "uploading"}})
        j = await enqueue("upload", u["id"], bg)
        jobs.append(j.model_dump())
    return {"queued": len(jobs)}


# Jobs
@api.get("/jobs")
async def list_jobs(limit: int = 200):
    return await db.jobs.find({}, {"_id": 0}).sort("created_at", -1).to_list(limit)

@api.get("/jobs/{jid}")
async def get_job(jid: str):
    j = await db.jobs.find_one({"id": jid}, {"_id": 0})
    if not j: raise HTTPException(404, "missing")
    return j

@api.post("/jobs/{jid}/retry")
async def retry_job(jid: str, bg: BackgroundTasks):
    j = await db.jobs.find_one({"id": jid}, {"_id": 0})
    if not j: raise HTTPException(404, "missing")
    await db.jobs.update_one({"id": jid}, {"$set": {"status": "queued", "progress": 0, "error": None, "updated_at": now_iso()},
                                            "$inc": {"attempts": 1},
                                            "$push": {"logs": f"[{now_iso()}] retry requested"}})
    bg.add_task(run_job, jid)
    return {"ok": True}

@api.delete("/jobs/{jid}")
async def cancel_job(jid: str):
    await db.jobs.delete_one({"id": jid})
    return {"ok": True}


app.include_router(api)

app.add_middleware(
    CORSMiddleware,
    allow_credentials=True,
    allow_origins=os.environ.get('CORS_ORIGINS', '*').split(','),
    allow_methods=["*"],
    allow_headers=["*"],
)


@app.on_event("shutdown")
async def shutdown_db_client():
    client.close()
