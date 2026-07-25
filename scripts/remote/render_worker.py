#!/usr/bin/env python3
"""Assemble one video with ffmpeg and upload it to YouTube — on somebody else's computer.

This is the single worker every remote backend runs: Kaggle CPU sessions, GitHub Actions, Modal, or a
plain VPS. Nothing about it is provider-specific, so a job behaves the same wherever it lands and the
app only has to know how to *launch* it.

    python3 render_worker.py job.json            # job spec on disk
    BM_JOB_B64=<base64 of the spec> python3 render_worker.py

Deliberate constraints:
  • Python standard library only. Kaggle, Actions runners and slim containers all have ffmpeg and a
    bare python3; anything else would need a package install step per provider.
  • Assets are fetched by URL, so media travels host-to-host and never through the user's connection.
  • Idempotent on `youtube.existing_video_id`: a retried job never publishes a second video.
  • The result is written to result.json AND printed as one `BM_RESULT {...}` line, because Kaggle
    gives us stdout while Actions and Modal give us a file.

The spec (see src-tauri/commands/remote_render.rs, which writes it):

    {
      "job_id": "…", "song_id": "…", "project_id": "…",
      "video":    {"width":1920,"height":1080,"fps":30,"crf":21,"preset":"veryfast","audio_bitrate":"192k"},
      "audio":    {"url": "https://…/song.mp3"},
      "images":   [{"url": "https://…/01.png", "duration": 24.5}, …],
      "transition": {"name":"fade","duration":0.8},
      "subtitles":  {"srt": "1\\n00:00:01,000 --> …"} | null,
      "youtube":  {"upload": true, "title":"…", "description":"…", "tags":[…],
                   "privacy":"private", "category_id":"10",
                   "client_id":"…", "client_secret":"…", "refresh_token":"…",
                   "existing_video_id": null},
      "result":   {"post_url": "https://…"} | null
    }
"""

import base64
import json
import mimetypes
import os
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request

CHUNK = 8 * 1024 * 1024          # 8 MB resumable upload chunks
UA = "BibleMusically-RemoteRender/1.0"


def log(msg):
    print(f"[render] {msg}", flush=True)


# ── job loading ──────────────────────────────────────────────────────────────

def load_job(argv):
    if len(argv) > 1 and os.path.exists(argv[1]):
        with open(argv[1], "r", encoding="utf-8") as fh:
            return json.load(fh)
    blob = os.environ.get("BM_JOB_B64", "").strip()
    if blob:
        return json.loads(base64.b64decode(blob + "=" * (-len(blob) % 4)).decode("utf-8"))
    raise SystemExit("no job: pass a job.json path or set BM_JOB_B64")


# ── fetching ─────────────────────────────────────────────────────────────────

def fetch(url, dest, token=None):
    """Download one asset. Bearer token support covers private Hugging Face / GitHub assets."""
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(req, timeout=300) as r, open(dest, "wb") as out:
        shutil.copyfileobj(r, out, CHUNK)
    return dest


# ── ffmpeg ───────────────────────────────────────────────────────────────────

def build_video(job, workdir):
    """Stills + audio → one H.264 file. Crossfades when asked for, hard cuts otherwise."""
    video = job.get("video") or {}
    w = int(video.get("width", 1920))
    h = int(video.get("height", 1080))
    fps = int(video.get("fps", 30))
    crf = str(video.get("crf", 21))
    preset = video.get("preset", "veryfast")
    abitrate = video.get("audio_bitrate", "192k")
    xfade = (job.get("transition") or {}).get("name", "fade")
    xdur = float((job.get("transition") or {}).get("duration", 0) or 0)

    token = job.get("assets_token")
    audio_src = (job.get("audio") or {}).get("url") or (job.get("audio") or {}).get("path")
    if not audio_src:
        raise SystemExit("job has no audio")
    audio = os.path.join(workdir, "audio" + (os.path.splitext(urllib.parse.urlparse(audio_src).path)[1] or ".mp3"))
    if audio_src.startswith("http"):
        log(f"fetching audio {audio_src}")
        fetch(audio_src, audio, token)
    else:
        shutil.copyfile(audio_src, audio)

    images = job.get("images") or []
    if not images:
        raise SystemExit("job has no images")
    local = []
    for idx, img in enumerate(images):
        src = img.get("url") or img.get("path")
        ext = os.path.splitext(urllib.parse.urlparse(src).path)[1] or ".png"
        dest = os.path.join(workdir, f"img{idx:03d}{ext}")
        if src.startswith("http"):
            log(f"fetching image {idx + 1}/{len(images)}")
            fetch(src, dest, token)
        else:
            shutil.copyfile(src, dest)
        local.append({"path": dest, "duration": float(img.get("duration") or 0)})

    total = probe_duration(audio)
    # Any image without an explicit duration shares what is left of the track evenly, so the video
    # always ends with the audio instead of freezing or cutting the last section short.
    unset = [i for i in local if i["duration"] <= 0]
    if unset:
        used = sum(i["duration"] for i in local if i["duration"] > 0)
        share = max(1.0, (total - used) / len(unset))
        for i in unset:
            i["duration"] = share

    out = os.path.join(workdir, "out.mp4")
    scale = (f"scale={w}:{h}:force_original_aspect_ratio=increase,"
             f"crop={w}:{h},setsar=1,fps={fps},format=yuv420p")

    if xdur > 0 and len(local) > 1:
        # xfade chain: each image is held for its duration plus the overlap it gives to the next one.
        cmd = ["ffmpeg", "-y"]
        for i in local:
            cmd += ["-loop", "1", "-t", f"{i['duration'] + xdur:.3f}", "-i", i["path"]]
        cmd += ["-i", audio]
        parts = [f"[{n}:v]{scale}[v{n}]" for n in range(len(local))]
        prev, offset = "[v0]", 0.0
        for n in range(1, len(local)):
            offset += local[n - 1]["duration"]
            label = f"[x{n}]"
            parts.append(f"{prev}[v{n}]xfade=transition={xfade}:duration={xdur:.3f}:offset={offset:.3f}{label}")
            prev = label
        filt = ";".join(parts)
        cmd += ["-filter_complex", filt, "-map", prev, "-map", f"{len(local)}:a"]
    else:
        # Concat demuxer: no re-timing maths, and identical output for the single-image case.
        listfile = os.path.join(workdir, "images.txt")
        with open(listfile, "w", encoding="utf-8") as fh:
            for i in local:
                fh.write(f"file '{i['path']}'\nduration {i['duration']:.3f}\n")
            fh.write(f"file '{local[-1]['path']}'\n")   # concat needs the last frame twice
        cmd = ["ffmpeg", "-y", "-f", "concat", "-safe", "0", "-i", listfile, "-i", audio,
               "-vf", scale, "-map", "0:v", "-map", "1:a"]

    srt = (job.get("subtitles") or {}).get("srt")
    if srt:
        srt_path = os.path.join(workdir, "subs.srt")
        with open(srt_path, "w", encoding="utf-8") as fh:
            fh.write(srt)
        # Burned in, because YouTube captions would need a second API call and a second quota hit.
        idx = cmd.index("-vf") if "-vf" in cmd else -1
        if idx >= 0:
            cmd[idx + 1] = cmd[idx + 1] + f",subtitles={srt_path}"
        else:
            cmd += ["-vf", f"subtitles={srt_path}"]

    cmd += ["-c:v", "libx264", "-preset", preset, "-crf", crf, "-pix_fmt", "yuv420p",
            "-c:a", "aac", "-b:a", abitrate, "-shortest", "-movflags", "+faststart", out]

    log(f"encoding {len(local)} images over {total:.1f}s of audio")
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        tail = (proc.stderr or "")[-1500:]
        raise SystemExit(f"ffmpeg failed ({proc.returncode}):\n{tail}")
    log(f"encoded {os.path.getsize(out) / 1e6:.1f} MB")
    return out


def probe_duration(path):
    """Audio length in seconds; 180s is a safe fallback if ffprobe is unavailable."""
    try:
        p = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration",
                            "-of", "default=nw=1:nk=1", path], capture_output=True, text=True)
        return float(p.stdout.strip())
    except Exception:
        return 180.0


# ── YouTube ──────────────────────────────────────────────────────────────────

def post_form(url, fields):
    body = urllib.parse.urlencode(fields).encode()
    req = urllib.request.Request(url, data=body, headers={
        "Content-Type": "application/x-www-form-urlencoded", "User-Agent": UA})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.loads(r.read().decode())


def access_token(yt):
    tok = post_form("https://oauth2.googleapis.com/token", {
        "client_id": yt["client_id"],
        "client_secret": yt["client_secret"],
        "refresh_token": yt["refresh_token"],
        "grant_type": "refresh_token",
    })
    if "access_token" not in tok:
        raise SystemExit(f"could not refresh the YouTube token: {tok}")
    return tok["access_token"]


def upload(path, yt):
    """Resumable upload. Returns the video id."""
    token = access_token(yt)
    meta = {
        "snippet": {
            "title": (yt.get("title") or "Untitled")[:100],
            "description": (yt.get("description") or "")[:5000],
            "tags": (yt.get("tags") or [])[:30],
            "categoryId": str(yt.get("category_id") or "10"),
        },
        "status": {
            "privacyStatus": yt.get("privacy") or "private",
            "selfDeclaredMadeForKids": bool(yt.get("made_for_kids", False)),
        },
    }
    size = os.path.getsize(path)
    init = urllib.request.Request(
        "https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&part=snippet,status",
        data=json.dumps(meta).encode(),
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json; charset=UTF-8",
            "X-Upload-Content-Length": str(size),
            "X-Upload-Content-Type": mimetypes.guess_type(path)[0] or "video/mp4",
            "User-Agent": UA,
        })
    with urllib.request.urlopen(init, timeout=120) as r:
        session = r.headers.get("Location")
    if not session:
        raise SystemExit("YouTube did not return a resumable upload URL")

    sent = 0
    with open(path, "rb") as fh:
        while sent < size:
            chunk = fh.read(CHUNK)
            if not chunk:
                break
            end = sent + len(chunk) - 1
            req = urllib.request.Request(session, data=chunk, method="PUT", headers={
                "Content-Length": str(len(chunk)),
                "Content-Range": f"bytes {sent}-{end}/{size}",
                "User-Agent": UA,
            })
            try:
                with urllib.request.urlopen(req, timeout=600) as r:
                    body = json.loads(r.read().decode() or "{}")
                    log(f"upload complete ({size / 1e6:.1f} MB)")
                    return body.get("id", "")
            except urllib.error.HTTPError as err:
                # 308 is the normal "keep going" answer between chunks.
                if err.code in (308, 200, 201):
                    sent = end + 1
                    log(f"uploaded {sent * 100 // size}%")
                    continue
                raise SystemExit(f"upload failed HTTP {err.code}: {err.read()[:400]!r}")
    raise SystemExit("upload ended without a video id")


# ── result reporting ─────────────────────────────────────────────────────────

def report(job, result, workdir):
    payload = {"job_id": job.get("job_id"), "song_id": job.get("song_id"), **result}
    text = json.dumps(payload)
    print(f"BM_RESULT {text}", flush=True)
    for target in [os.path.join(workdir, "result.json"), "result.json",
                   "/kaggle/working/result.json" if os.path.isdir("/kaggle/working") else None]:
        if not target:
            continue
        try:
            with open(target, "w", encoding="utf-8") as fh:
                fh.write(text)
        except OSError:
            pass
    sink = (job.get("result") or {}).get("post_url")
    if sink:
        try:
            req = urllib.request.Request(sink, data=text.encode(),
                                         headers={"Content-Type": "application/json", "User-Agent": UA})
            urllib.request.urlopen(req, timeout=60).read()
        except Exception as err:            # the run still succeeded; only the callback failed
            log(f"result callback failed: {err}")


def main():
    job = load_job(sys.argv)
    workdir = tempfile.mkdtemp(prefix="bm-render-")
    keep = os.environ.get("BM_KEEP_OUTPUT") == "1" or os.path.isdir("/kaggle/working")
    try:
        video = build_video(job, workdir)
        yt = job.get("youtube") or {}
        result = {"status": "rendered", "bytes": os.path.getsize(video)}

        if yt.get("existing_video_id"):
            # Idempotency: something already published this song. Never do it twice.
            log(f"song already published as {yt['existing_video_id']} — skipping upload")
            result.update(status="skipped_upload", video_id=yt["existing_video_id"])
        elif yt.get("upload") and yt.get("refresh_token"):
            vid = upload(video, yt)
            result.update(status="uploaded", video_id=vid,
                          url=f"https://www.youtube.com/watch?v={vid}" if vid else "")
        else:
            log("upload not requested — leaving the file for review")

        if keep:
            final = os.path.join("/kaggle/working" if os.path.isdir("/kaggle/working") else ".",
                                 f"{job.get('song_id', 'render')}.mp4")
            try:
                shutil.copyfile(video, final)
                result["local_path"] = final
            except OSError as err:
                log(f"could not keep the output: {err}")

        report(job, result, workdir)
    except SystemExit as err:
        report(job, {"status": "failed", "error": str(err)}, workdir)
        raise
    except Exception as err:                # noqa: BLE001 — any failure must still be reported
        report(job, {"status": "failed", "error": f"{type(err).__name__}: {err}"}, workdir)
        raise
    finally:
        if not keep:
            shutil.rmtree(workdir, ignore_errors=True)


if __name__ == "__main__":
    main()
