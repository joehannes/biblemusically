"""AI Music Video Studio backend - all endpoints under /api prefix."""
from fastapi import FastAPI, APIRouter, HTTPException, BackgroundTasks
from dotenv import load_dotenv
from starlette.middleware.cors import CORSMiddleware
from motor.motor_asyncio import AsyncIOMotorClient
import os, logging, uuid, asyncio, random, re
from pathlib import Path
from pydantic import BaseModel, Field, ConfigDict
from typing import List, Optional, Dict, Any
from datetime import datetime, timezone

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
async def run_job(job_id: str):
    """Simulate a worker that progresses the job. Mocks Suno/MJ/FFmpeg/Upload."""
    await db.jobs.update_one({"id": job_id}, {"$set": {"status": "running", "updated_at": now_iso()},
                                              "$push": {"logs": f"[{now_iso()}] starting"}})
    job = await db.jobs.find_one({"id": job_id}, {"_id": 0})
    try:
        for p in [15, 35, 60, 85]:
            await asyncio.sleep(0.6)
            await db.jobs.update_one({"id": job_id}, {"$set": {"progress": p, "updated_at": now_iso()},
                                                      "$push": {"logs": f"[{now_iso()}] progress {p}%"}})
        # Side effects per kind
        kind = job["kind"]; tgt = job["target_id"]
        if kind == "music":
            audio_url = f"https://cdn.suno.ai/mock/{tgt}.mp3"
            await db.songs.update_one({"id": tgt}, {"$set": {"audio_url": audio_url, "duration": 92.0 + random.random()*30, "status": "music_ready"}})
            result = {"audio_url": audio_url}
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
                seed = abs(hash(sec["image_prompt"] or sec["line"])) % 10000
                variants = [f"https://picsum.photos/seed/{seed+i}/1280/720" for i in range(4)]
                await db.sections.update_one({"id": tgt}, {"$set": {"image_url": variants[0], "image_variants": variants}})
            result = {"variants": 4}
        elif kind == "video":
            video_url = f"/api/media/video/{tgt}.mp4"
            await db.songs.update_one({"id": tgt}, {"$set": {"status": "video_ready", "video_url": video_url}})
            result = {"video_url": video_url}
        elif kind == "upload":
            yt_id = f"yt_{uuid.uuid4().hex[:11]}"
            await db.uploads.update_one({"id": tgt}, {"$set": {"status": "published", "youtube_video_id": yt_id, "published_at": now_iso()}})
            result = {"youtube_video_id": yt_id, "url": f"https://youtu.be/{yt_id}"}
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
async def oauth_start(cid: str):
    s = await db.settings.find_one({"_id": "singleton"}, {"_id": 0}) or {}
    cid_g = s.get("google_client_id") or ""
    redirect = s.get("google_redirect_uri") or "http://localhost/oauth/callback"
    if not cid_g:
        return {"url": "", "error": "Configure google_client_id in Settings first."}
    scope = "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly"
    url = (f"https://accounts.google.com/o/oauth2/v2/auth?response_type=code"
           f"&client_id={cid_g}&redirect_uri={redirect}&scope={scope.replace(' ', '%20')}"
           f"&access_type=offline&prompt=consent&state={cid}")
    return {"url": url}

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
