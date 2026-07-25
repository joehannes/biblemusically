"""Render worker as a Modal web endpoint.

Deploy once, then paste the printed URL into Settings → Remote rendering → Modal endpoint:

    pip install modal && modal setup
    modal deploy scripts/remote/modal_app.py

Why Modal for this: containers exist only while a render runs, so 750 videos a month costs
~50–100 core-hours ≈ $2.50–$5 at the 2026 CPU rate — inside the $30/month free credits, with no idle
machine to pay for. Concurrency is elastic, so a backlog clears in parallel instead of in sequence.

The endpoint accepts the same job spec every other backend gets (see render_worker.py) and returns
the worker's result JSON. `BM_WORKER_TOKEN` — set as a Modal secret — is required as a bearer token if
present, so a public URL cannot be used by anyone else's jobs.
"""

import json
import os
import subprocess
import tempfile

import modal

# ffmpeg is the only thing the base image lacks; the worker itself is stdlib-only.
image = (
    modal.Image.debian_slim(python_version="3.12")
    .apt_install("ffmpeg")
    .add_local_file(
        os.path.join(os.path.dirname(__file__), "render_worker.py"),
        remote_path="/app/render_worker.py",
    )
)

app = modal.App("bm-render")


@app.function(
    image=image,
    cpu=2.0,
    memory=4096,
    timeout=60 * 60,          # a 10-minute 1080p render finishes far inside this
    max_containers=10,        # clears a day's backlog in parallel without runaway spend
)
def render(job: dict) -> dict:
    """Run one job. Returns the worker's result dict."""
    with tempfile.TemporaryDirectory() as work:
        spec = os.path.join(work, "job.json")
        with open(spec, "w", encoding="utf-8") as fh:
            json.dump(job, fh)
        proc = subprocess.run(
            ["python3", "/app/render_worker.py", spec],
            capture_output=True, text=True, cwd=work,
        )
        # The worker prints exactly one BM_RESULT line, whether it succeeded or failed.
        for line in reversed((proc.stdout or "").splitlines()):
            if line.startswith("BM_RESULT "):
                return json.loads(line[len("BM_RESULT "):])
        return {
            "status": "failed",
            "error": f"worker exited {proc.returncode} without a result",
            "stderr": (proc.stderr or "")[-1500:],
        }


@app.function(image=image, secrets=[modal.Secret.from_name("bm-render", required_keys=[])])
@modal.fastapi_endpoint(method="POST", docs=False)
def submit(job: dict, authorization: str = ""):
    """HTTP entry point the app POSTs to. Spawns the render and returns immediately."""
    expected = os.environ.get("BM_WORKER_TOKEN", "")
    if expected and authorization.removeprefix("Bearer ").strip() != expected:
        return {"status": "rejected", "error": "bad or missing bearer token"}
    call = render.spawn(job)
    return {"status": "accepted", "call_id": call.object_id, "job_id": job.get("job_id")}
