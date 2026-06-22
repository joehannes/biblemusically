"""
Job Queue Module for Riffusion Kaggle Song Studio

Manages async job queue with status tracking and persistence.
"""

import threading
from datetime import datetime
from typing import Dict, List, Optional, Any
from dataclasses import dataclass, field
from enum import Enum
import logging

logger = logging.getLogger(__name__)


class JobStatus(str, Enum):
    """Status values for generation jobs."""
    QUEUED = "queued"
    PROCESSING = "processing"
    COMPLETED = "completed"
    FAILED = "failed"
    CANCELLED = "cancelled"


@dataclass
class Job:
    """Represents a song generation job."""
    job_id: str
    title: str
    status: JobStatus = JobStatus.QUEUED
    created_at: float = field(default_factory=lambda: datetime.now().timestamp())
    started_at: Optional[float] = None
    completed_at: Optional[float] = None
    params: Dict[str, Any] = field(default_factory=dict)
    output_file: Optional[str] = None
    error_message: Optional[str] = None
    parent_job_id: Optional[str] = None  # For alternate versions
    
    def to_dict(self) -> dict:
        """Convert to dictionary for serialization."""
        return {
            "job_id": self.job_id,
            "title": self.title,
            "status": self.status.value,
            "created_at": self.created_at,
            "started_at": self.started_at,
            "completed_at": self.completed_at,
            "params": self.params,
            "output_file": self.output_file,
            "error_message": self.error_message,
            "parent_job_id": self.parent_job_id
        }


class JobQueue:
    """
    Thread-safe job queue for managing song generation tasks.
    
    Provides basic queue operations and status tracking.
    In production, could be replaced with Redis/Celery.
    """
    
    def __init__(self, max_concurrent: int = 1):
        """
        Initialize the job queue.
        
        Args:
            max_concurrent: Maximum concurrent jobs (Kaggle limited to 1)
        """
        self.jobs: Dict[str, Job] = {}
        self.queue: List[str] = []  # Job IDs in queue order
        self.max_concurrent = max_concurrent
        self.active_jobs: set = set()
        self._lock = threading.Lock()
        
        logger.info(f"JobQueue initialized with max_concurrent={max_concurrent}")
    
    def add_job(self, job: Job, params: Dict[str, Any]) -> str:
        """
        Add a job to the queue.
        
        Args:
            job: Job object
            params: Generation parameters
        
        Returns:
            Job ID
        """
        with self._lock:
            job.params = params
            self.jobs[job.job_id] = job
            self.queue.append(job.job_id)
            
            logger.debug(f"Added job {job.job_id} to queue")
        
        return job.job_id
    
    def get_job(self, job_id: str) -> Optional[Job]:
        """Get a job by ID."""
        return self.jobs.get(job_id)
    
    def get_next_job(self) -> Optional[Job]:
        """
        Get the next queued job if capacity available.
        
        Returns:
            Next job or None if queue empty or at capacity
        """
        with self._lock:
            if len(self.active_jobs) >= self.max_concurrent:
                return None
            
            while self.queue:
                job_id = self.queue.pop(0)
                job = self.jobs.get(job_id)
                
                if job and job.status == JobStatus.QUEUED:
                    self.active_jobs.add(job_id)
                    return job
            
            return None
    
    def mark_started(self, job_id: str) -> None:
        """Mark a job as started."""
        with self._lock:
            job = self.jobs.get(job_id)
            if job:
                job.status = JobStatus.PROCESSING
                job.started_at = datetime.now().timestamp()
    
    def mark_completed(self, job_id: str, output_file: Optional[str] = None) -> None:
        """Mark a job as completed."""
        with self._lock:
            job = self.jobs.get(job_id)
            if job:
                job.status = JobStatus.COMPLETED
                job.completed_at = datetime.now().timestamp()
                job.output_file = output_file
                
                if job_id in self.active_jobs:
                    self.active_jobs.remove(job_id)
    
    def mark_failed(self, job_id: str, error: str) -> None:
        """Mark a job as failed."""
        with self._lock:
            job = self.jobs.get(job_id)
            if job:
                job.status = JobStatus.FAILED
                job.completed_at = datetime.now().timestamp()
                job.error_message = error
                
                if job_id in self.active_jobs:
                    self.active_jobs.remove(job_id)
    
    def cancel_job(self, job_id: str) -> bool:
        """
        Cancel a queued or processing job.
        
        Returns:
            True if cancelled, False if not cancellable
        """
        with self._lock:
            job = self.jobs.get(job_id)
            if not job:
                return False
            
            if job.status in [JobStatus.COMPLETED, JobStatus.FAILED]:
                return False
            
            job.status = JobStatus.CANCELLED
            job.completed_at = datetime.now().timestamp()
            
            if job_id in self.active_jobs:
                self.active_jobs.remove(job_id)
            
            # Remove from queue if still queued
            if job_id in self.queue:
                self.queue.remove(job_id)
            
            return True
    
    def get_queue_status(self) -> dict:
        """Get current queue status."""
        with self._lock:
            status_counts = {}
            for job in self.jobs.values():
                status = job.status.value
                status_counts[status] = status_counts.get(status, 0) + 1
            
            return {
                "total_jobs": len(self.jobs),
                "queued": len(self.queue),
                "active": len(self.active_jobs),
                "by_status": status_counts
            }
    
    def list_jobs(self, status_filter: Optional[JobStatus] = None) -> List[Job]:
        """List jobs, optionally filtered by status."""
        with self._lock:
            jobs = list(self.jobs.values())
            
            if status_filter:
                jobs = [j for j in jobs if j.status == status_filter]
            
            # Sort by created_at descending
            jobs.sort(key=lambda j: j.created_at, reverse=True)
            
            return jobs
    
    def cleanup_old_jobs(self, max_age_hours: int = 24) -> int:
        """
        Remove old completed/failed/cancelled jobs.
        
        Args:
            max_age_hours: Maximum age in hours
        
        Returns:
            Number of jobs removed
        """
        cutoff = datetime.now().timestamp() - (max_age_hours * 3600)
        removed = 0
        
        with self._lock:
            to_remove = []
            
            for job_id, job in self.jobs.items():
                if job.status in [JobStatus.COMPLETED, JobStatus.FAILED, JobStatus.CANCELLED]:
                    if job.completed_at and job.completed_at < cutoff:
                        to_remove.append(job_id)
            
            for job_id in to_remove:
                del self.jobs[job_id]
                if job_id in self.queue:
                    self.queue.remove(job_id)
                removed += 1
        
        logger.info(f"Cleaned up {removed} old jobs")
        return removed
    
    def __len__(self) -> int:
        """Return total number of jobs."""
        return len(self.jobs)
