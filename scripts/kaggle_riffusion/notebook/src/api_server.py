"""
FastAPI Server for Riffusion Kaggle Song Studio

Exposes REST API endpoints for remote song generation control.
Designed to run inside a Kaggle notebook with cloudflared tunnel.
"""

import os
import uuid
import asyncio
from pathlib import Path
from typing import Dict, List, Optional, Any
from datetime import datetime

from fastapi import FastAPI, HTTPException, BackgroundTasks, Response
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field
import uvicorn

from .song_arranger import SongArranger, SongArrangement
from .long_form_generator import LongFormGenerator
from .preset_engine import PresetEngine
from .job_queue import JobQueue, JobStatus, Job
from .tunnel_manager import TunnelManager

import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)


# Request/Response Models
class GenerateSongRequest(BaseModel):
    title: str
    lyrics: str
    style_preset: str
    target_duration_minutes: float = Field(ge=5.0, le=10.0, default=5.0)
    seed: Optional[int] = None


class GenerateAlternatesRequest(BaseModel):
    original_job_id: str
    num_variations: int = Field(ge=1, le=5, default=3)
    variation_type: str = "seed"  # "seed" or "prompt_mutation"


class BulkSongRequest(BaseModel):
    songs: List[Dict[str, Any]]


class JobStatusResponse(BaseModel):
    job_id: str
    status: str
    progress: Optional[dict] = None
    error: Optional[str] = None
    created_at: Optional[float] = None
    completed_at: Optional[float] = None


class DownloadResponse(BaseModel):
    job_id: str
    file_path: str
    file_size: int


# FastAPI Application
app = FastAPI(
    title="Riffusion Kaggle Song Studio API",
    description="Remote-controlled AI music generation via Kaggle notebook",
    version="1.0.0"
)

# Enable CORS for all origins (Kaggle tunnel use case)
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Global state
job_queue: Optional[JobQueue] = None
song_arranger: Optional[SongArranger] = None
generator: Optional[LongFormGenerator] = None
preset_engine: Optional[PresetEngine] = None
tunnel_manager: Optional[TunnelManager] = None
api_key: str = ""


def init_app_state(output_dir: str = "/tmp/riffusion_output"):
    """Initialize application state."""
    global job_queue, song_arranger, generator, preset_engine, api_key
    
    api_key = os.environ.get("RIFFUSION_API_KEY", str(uuid.uuid4()))
    logger.info(f"API Key: {api_key}")
    
    preset_engine = PresetEngine()
    song_arranger = SongArranger(preset_engine)
    generator = LongFormGenerator(output_dir=output_dir)
    job_queue = JobQueue()
    
    logger.info("Application state initialized")


@app.on_event("startup")
async def startup_event():
    """Run on server startup."""
    init_app_state()
    logger.info("Server starting up...")


@app.on_event("shutdown")
async def shutdown_event():
    """Run on server shutdown."""
    logger.info("Server shutting down...")


# Health & Info Endpoints
@app.get("/health")
async def health_check():
    """Health check endpoint."""
    return {
        "status": "healthy",
        "timestamp": datetime.now().isoformat(),
        "queue_size": len(job_queue.jobs) if job_queue else 0
    }


@app.get("/presets")
async def list_presets():
    """List available musical style presets."""
    if not preset_engine:
        raise HTTPException(status_code=500, detail="Preset engine not initialized")
    
    presets = preset_engine.list_presets()
    section_types = preset_engine.list_section_types()
    
    return {
        "presets": presets,
        "section_types": section_types
    }


@app.get("/api-key")
async def get_api_key():
    """Get the current API key for client authentication."""
    return {"api_key": api_key}


# Core Generation Endpoints
@app.post("/generate_song", response_model=Dict[str, str])
async def generate_song(request: GenerateSongRequest, background_tasks: BackgroundTasks):
    """
    Submit a new song generation job.
    
    Returns job_id for tracking progress.
    """
    if not job_queue or not song_arranger or not generator:
        raise HTTPException(status_code=500, detail="Service not initialized")
    
    # Validate preset
    if request.style_preset.lower() not in preset_engine.list_presets():
        logger.warning(f"Unknown preset '{request.style_preset}', using default")
    
    # Create job
    job_id = str(uuid.uuid4())
    
    job = Job(
        job_id=job_id,
        title=request.title,
        status=JobStatus.QUEUED,
        created_at=datetime.now().timestamp()
    )
    
    # Add to queue
    job_queue.add_job(job, {
        "title": request.title,
        "lyrics": request.lyrics,
        "style_preset": request.style_preset,
        "target_duration_minutes": request.target_duration_minutes,
        "seed": request.seed
    })
    
    # Start processing in background
    background_tasks.add_task(process_song_job, job_id)
    
    logger.info(f"Created job {job_id}: '{request.title}'")
    
    return {"job_id": job_id, "status": "queued"}


@app.post("/generate_alternates", response_model=Dict[str, List[str]])
async def generate_alternates(request: GenerateAlternatesRequest, background_tasks: BackgroundTasks):
    """
    Generate alternate versions of an existing song.
    
    Creates variations by changing seed or mutating prompts.
    """
    if not job_queue:
        raise HTTPException(status_code=500, detail="Service not initialized")
    
    # Find original job
    original_job = job_queue.get_job(request.original_job_id)
    if not original_job:
        raise HTTPException(status_code=404, detail="Original job not found")
    
    if original_job.status != JobStatus.COMPLETED:
        raise HTTPException(
            status_code=400,
            detail=f"Original job not completed. Status: {original_job.status.value}"
        )
    
    # Create alternate jobs
    alternate_ids = []
    
    for i in range(request.num_variations):
        job_id = str(uuid.uuid4())
        
        job = Job(
            job_id=job_id,
            title=f"{original_job.title} (Variant {i + 1})",
            status=JobStatus.QUEUED,
            created_at=datetime.now().timestamp(),
            parent_job_id=request.original_job_id
        )
        
        # Vary parameters based on variation type
        base_params = original_job.params.copy()
        
        if request.variation_type == "seed":
            # Use different seed
            base_params["seed"] = (base_params.get("seed", 42) + (i + 1) * 1000)
        elif request.variation_type == "prompt_mutation":
            # Could add prompt mutations here
            pass
        
        job_queue.add_job(job, base_params)
        background_tasks.add_task(process_song_job, job_id)
        
        alternate_ids.append(job_id)
    
    logger.info(f"Created {len(alternate_ids)} alternate jobs for {request.original_job_id}")
    
    return {"alternate_job_ids": alternate_ids}


@app.post("/generate_bulk", response_model=Dict[str, List[str]])
async def generate_bulk(request: BulkSongRequest, background_tasks: BackgroundTasks):
    """
    Submit multiple songs for bulk generation.
    
    Returns list of job IDs.
    """
    if not job_queue or not song_arranger:
        raise HTTPException(status_code=500, detail="Service not initialized")
    
    job_ids = []
    
    for song_config in request.songs:
        job_id = str(uuid.uuid4())
        
        job = Job(
            job_id=job_id,
            title=song_config.get("title", "Untitled"),
            status=JobStatus.QUEUED,
            created_at=datetime.now().timestamp()
        )
        
        job_queue.add_job(job, song_config)
        background_tasks.add_task(process_song_job, job_id)
        
        job_ids.append(job_id)
    
    logger.info(f"Bulk created {len(job_ids)} jobs")
    
    return {"job_ids": job_ids}


# Status & Retrieval Endpoints
@app.get("/status/{job_id}", response_model=JobStatusResponse)
async def get_status(job_id: str):
    """Get status of a generation job."""
    if not job_queue:
        raise HTTPException(status_code=500, detail="Service not initialized")
    
    job = job_queue.get_job(job_id)
    if not job:
        raise HTTPException(status_code=404, detail="Job not found")
    
    # Get progress from generator checkpoint if available
    progress = None
    if generator and job.status == JobStatus.PROCESSING:
        progress = generator.get_checkpoint_status(job_id)
    
    return JobStatusResponse(
        job_id=job.job_id,
        status=job.status.value,
        progress=progress,
        error=job.error_message,
        created_at=job.created_at,
        completed_at=job.completed_at
    )


@app.get("/download/{job_id}")
async def download_song(job_id: str):
    """Download the final generated song."""
    if not job_queue or not generator:
        raise HTTPException(status_code=500, detail="Service not initialized")
    
    job = job_queue.get_job(job_id)
    if not job:
        raise HTTPException(status_code=404, detail="Job not found")
    
    if job.status != JobStatus.COMPLETED:
        raise HTTPException(
            status_code=400,
            detail=f"Job not completed. Status: {job.status.value}"
        )
    
    if not job.output_file:
        raise HTTPException(status_code=404, detail="Output file not found")
    
    output_path = Path(job.output_file)
    if not output_path.exists():
        raise HTTPException(status_code=404, detail="Output file deleted")
    
    # Return file for download
    from fastapi.responses import FileResponse
    
    return FileResponse(
        path=str(output_path),
        media_type="audio/wav",
        filename=output_path.name,
        headers={
            "Content-Disposition": f'attachment; filename="{output_path.name}"'
        }
    )


@app.get("/download_alternates/{job_id}")
async def download_alternates_zip(job_id: str):
    """Download all alternate versions as a ZIP file."""
    if not job_queue:
        raise HTTPException(status_code=500, detail="Service not initialized")
    
    # Find all children of this job
    original_job = job_queue.get_job(job_id)
    if not original_job:
        raise HTTPException(status_code=404, detail="Job not found")
    
    # Find alternate jobs
    alternate_jobs = [
        j for j in job_queue.jobs.values()
        if j.parent_job_id == job_id and j.status == JobStatus.COMPLETED
    ]
    
    if not alternate_jobs:
        raise HTTPException(status_code=404, detail="No alternate versions found")
    
    # Create ZIP file
    import zipfile
    import tempfile
    
    temp_dir = tempfile.mkdtemp()
    zip_path = Path(temp_dir) / f"{original_job.title}_alternates.zip"
    
    with zipfile.ZipFile(zip_path, 'w', zipfile.ZIP_DEFLATED) as zf:
        for alt_job in alternate_jobs:
            if alt_job.output_file and Path(alt_job.output_file).exists():
                zf.write(alt_job.output_file, Path(alt_job.output_file).name)
    
    from fastapi.responses import FileResponse
    
    return FileResponse(
        path=str(zip_path),
        media_type="application/zip",
        filename=f"{original_job.title}_alternates.zip"
    )


# Management Endpoints
@app.post("/cancel/{job_id}")
async def cancel_job(job_id: str):
    """Cancel a queued or processing job."""
    if not job_queue:
        raise HTTPException(status_code=500, detail="Service not initialized")
    
    job = job_queue.get_job(job_id)
    if not job:
        raise HTTPException(status_code=404, detail="Job not found")
    
    if job.status in [JobStatus.COMPLETED, JobStatus.FAILED]:
        raise HTTPException(
            status_code=400,
            detail=f"Cannot cancel job with status: {job.status.value}"
        )
    
    job.status = JobStatus.CANCELLED
    job.completed_at = datetime.now().timestamp()
    
    logger.info(f"Cancelled job {job_id}")
    
    return {"job_id": job_id, "status": "cancelled"}


@app.delete("/cleanup/{job_id}")
async def cleanup_job(job_id: str, keep_final: bool = True):
    """Clean up temporary files for a job."""
    if not generator:
        raise HTTPException(status_code=500, detail="Service not initialized")
    
    try:
        generator.cleanup_job(job_id, keep_final=keep_final)
        return {"job_id": job_id, "cleaned": True}
    except Exception as e:
        logger.error(f"Cleanup failed for {job_id}: {e}")
        raise HTTPException(status_code=500, detail=str(e))


# Background Processing
async def process_song_job(job_id: str):
    """Process a song generation job from the queue."""
    if not job_queue or not song_arranger or not generator:
        logger.error("Service not initialized for job processing")
        return
    
    job = job_queue.get_job(job_id)
    if not job:
        logger.error(f"Job {job_id} not found")
        return
    
    try:
        # Update status
        job.status = JobStatus.PROCESSING
        job.started_at = datetime.now().timestamp()
        
        params = job.params
        
        # Create arrangement
        arrangement = song_arranger.create_arrangement(
            title=params["title"],
            lyrics=params["lyrics"],
            style_preset=params["style_preset"],
            target_duration_minutes=params.get("target_duration_minutes", 5.0)
        )
        
        # Generate song
        output_file = generator.generate_full_song(
            arrangement=arrangement,
            job_id=job_id,
            seed=params.get("seed"),
            resume=False
        )
        
        # Mark complete
        job.status = JobStatus.COMPLETED
        job.output_file = output_file
        job.completed_at = datetime.now().timestamp()
        
        logger.info(f"Job {job_id} completed: {output_file}")
        
    except Exception as e:
        logger.error(f"Job {job_id} failed: {e}")
        job.status = JobStatus.FAILED
        job.error_message = str(e)
        job.completed_at = datetime.now().timestamp()


def start_server(host: str = "0.0.0.0", port: int = 8000):
    """Start the FastAPI server."""
    uvicorn.run(app, host=host, port=port)


if __name__ == "__main__":
    start_server()
