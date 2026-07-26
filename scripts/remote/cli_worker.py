#!/usr/bin/env python3
"""Run one command-line job somewhere that isn't the user's device.

The goal this serves: a phone should be able to drive the whole studio without carrying ffmpeg, and
without pulling media down and pushing it back up again. So a job says *what to run* and *where the
inputs live*; this fetches, runs, uploads, and returns only the small stuff — exit code, stdout tail,
and the URLs of what it produced.

    python3 cli_worker.py job.json
    BM_CLI_JOB_B64=<base64 of the spec> python3 cli_worker.py

Spec:

    {
      "job_id": "…",
      "tool": "ffmpeg",                     # allowlisted below — never a shell string
      "args": ["-i", "{in:source}", "-vf", "scale=1080:1920", "{out:short}"],
      "inputs":  { "source": "https://…/video.mp4" },
      "outputs": { "short":  { "name": "short.mp4", "upload": "https://…/put-url" } },
      "timeout_s": 900,
      "result": { "post_url": "https://…" }   # optional callback
    }

`{in:name}` and `{out:name}` are substituted with local paths after the inputs are fetched. That
indirection is the security boundary: the app never sends a path or a shell line, so a job cannot
reach outside the working directory or chain a second command.

Standard library only — the same reason as render_worker.py: every host already has python3 and
ffmpeg, and a pip install per provider is a failure mode nobody needs.
"""

import base64
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request

# Only these may be executed. An allowlist rather than a blocklist: the caller is the app, but a
# compromised or confused caller must not be able to run an arbitrary binary on someone else's host.
ALLOWED_TOOLS = {
    "ffmpeg": "ffmpeg",
    "ffprobe": "ffprobe",
    "yt-dlp": "yt-dlp",
    "magick": "magick",
    "convert": "convert",
    "python3": "python3",
}

CHUNK = 8 * 1024 * 1024
UA = "BibleMusically-CliWorker/1.0"
PLACEHOLDER = re.compile(r"^\{(in|out):([A-Za-z0-9_.-]+)\}$")


def log(msg):
    print(f"[cli] {msg}", flush=True)


def load_job(argv):
    if len(argv) > 1 and os.path.exists(argv[1]):
        with open(argv[1], "r", encoding="utf-8") as fh:
            return json.load(fh)
    blob = os.environ.get("BM_CLI_JOB_B64", "").strip()
    if blob:
        return json.loads(base64.b64decode(blob + "=" * (-len(blob) % 4)).decode("utf-8"))
    raise SystemExit("no job: pass a job.json path or set BM_CLI_JOB_B64")


def fetch(url, dest, token=None):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(req, timeout=300) as r, open(dest, "wb") as out:
        shutil.copyfileobj(r, out, CHUNK)
    return dest


def put(url, path, token=None):
    """Upload a produced file. PUT because that is what pre-signed URLs expect."""
    size = os.path.getsize(path)
    with open(path, "rb") as fh:
        req = urllib.request.Request(url, data=fh, method="PUT")
        req.add_header("Content-Length", str(size))
        req.add_header("User-Agent", UA)
        if token:
            req.add_header("Authorization", f"Bearer {token}")
        with urllib.request.urlopen(req, timeout=900) as r:
            return r.status


def resolve_args(args, workdir, inputs, outputs):
    """Replace {in:…}/{out:…} with local paths. Anything else passes through untouched."""
    resolved, produced = [], {}
    for a in args:
        m = PLACEHOLDER.match(str(a))
        if not m:
            resolved.append(str(a))
            continue
        kind, name = m.group(1), m.group(2)
        if kind == "in":
            if name not in inputs:
                raise SystemExit(f"job references unknown input '{name}'")
            resolved.append(inputs[name])
        else:
            spec = (outputs or {}).get(name) or {}
            # Basename only: an output must land in the working directory, never at an absolute path.
            filename = os.path.basename(spec.get("name") or f"{name}.out")
            path = os.path.join(workdir, filename)
            produced[name] = path
            resolved.append(path)
    return resolved, produced


def main():
    job = load_job(sys.argv)
    tool = job.get("tool", "")
    if tool not in ALLOWED_TOOLS:
        raise SystemExit(f"tool '{tool}' is not allowed (allowed: {', '.join(sorted(ALLOWED_TOOLS))})")
    binary = shutil.which(ALLOWED_TOOLS[tool])
    if not binary:
        raise SystemExit(f"{tool} is not installed on this host")

    workdir = tempfile.mkdtemp(prefix="bm-cli-")
    token = job.get("assets_token")
    result = {"job_id": job.get("job_id"), "tool": tool}
    try:
        # 1. Inputs.
        local_inputs = {}
        for name, url in (job.get("inputs") or {}).items():
            dest = os.path.join(workdir, os.path.basename(name) or name)
            if str(url).startswith("http"):
                log(f"fetching {name}")
                fetch(url, dest, token)
            else:
                shutil.copyfile(url, dest)
            local_inputs[name] = dest

        # 2. Run.
        args, produced = resolve_args(job.get("args") or [], workdir, local_inputs, job.get("outputs"))
        log(f"running {tool} with {len(args)} args")
        proc = subprocess.run(
            [binary, *args],
            capture_output=True, text=True, cwd=workdir,
            timeout=int(job.get("timeout_s") or 900),
        )
        result["exit_code"] = proc.returncode
        # Only the tail: a full ffmpeg log is noise, and the useful part is always at the end.
        result["stdout"] = (proc.stdout or "")[-2000:]
        result["stderr"] = (proc.stderr or "")[-2000:]
        if proc.returncode != 0:
            result["status"] = "failed"
            return finish(job, result, workdir)

        # 3. Outputs.
        files = {}
        for name, path in produced.items():
            if not os.path.exists(path):
                result.setdefault("missing", []).append(name)
                continue
            entry = {"name": os.path.basename(path), "bytes": os.path.getsize(path)}
            spec = (job.get("outputs") or {}).get(name) or {}
            if spec.get("upload"):
                log(f"uploading {name} ({entry['bytes'] / 1e6:.1f} MB)")
                entry["upload_status"] = put(spec["upload"], path, token)
                entry["url"] = spec.get("public_url") or spec["upload"].split("?")[0]
            elif entry["bytes"] <= 256 * 1024:
                # Small results (a JSON probe, a caption file) ride back inline — cheaper than a
                # round trip through storage for a few kilobytes.
                with open(path, "rb") as fh:
                    entry["inline_base64"] = base64.b64encode(fh.read()).decode()
            else:
                keep = os.path.join(os.getcwd(), os.path.basename(path))
                try:
                    shutil.copyfile(path, keep)
                    entry["local_path"] = keep
                except OSError as err:
                    entry["error"] = f"could not keep the file: {err}"
            files[name] = entry
        result["files"] = files
        result["status"] = "ok"
        return finish(job, result, workdir)

    except subprocess.TimeoutExpired:
        result.update(status="failed", error=f"{tool} exceeded the {job.get('timeout_s', 900)}s timeout")
        return finish(job, result, workdir)
    except SystemExit as err:
        result.update(status="failed", error=str(err))
        finish(job, result, workdir)
        raise
    except Exception as err:  # noqa: BLE001 — any failure must still be reported
        result.update(status="failed", error=f"{type(err).__name__}: {err}")
        finish(job, result, workdir)
        raise


def finish(job, result, workdir):
    text = json.dumps(result)
    print(f"BM_CLI_RESULT {text}", flush=True)
    for target in ["result.json", "/kaggle/working/result.json" if os.path.isdir("/kaggle/working") else None]:
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
        except Exception as err:  # noqa: BLE001
            log(f"result callback failed: {err}")
    shutil.rmtree(workdir, ignore_errors=True)
    return 0 if result.get("status") == "ok" else 1


if __name__ == "__main__":
    sys.exit(main() or 0)
