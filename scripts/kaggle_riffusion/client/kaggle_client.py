"""
Kaggle Riffusion Client SDK

Python client library for interacting with the Riffusion Kaggle Song Studio API.
Designed for use in the external desktop/mobile app.
"""

import time
import requests
from pathlib import Path
from typing import Dict, List, Optional, Any, Callable
from dataclasses import dataclass
import logging

logger = logging.getLogger(__name__)


@dataclass
class SongConfig:
    """Configuration for a song generation request."""
    title: str
    lyrics: str
    style: str
    duration_minutes: float = 5.0
    seed: Optional[int] = None


class RiffusionSongClient:
    """
    Client SDK for the Riffusion Kaggle Song Studio API.
    
    Provides methods for submitting songs, checking status,
    and downloading results.
    """
    
    def __init__(self, tunnel_url: str, api_key: Optional[str] = None):
        """
        Initialize the client.
        
        Args:
            tunnel_url: Public URL of the Kaggle tunnel (e.g., https://xxx.trycloudflare.com)
            api_key: Optional API key for authentication
        """
        self.base_url = tunnel_url.rstrip('/')
        self.api_key = api_key
        self.session = requests.Session()
        
        if api_key:
            self.session.headers['X-API-Key'] = api_key
        
        logger.info(f"RiffusionSongClient initialized for {self.base_url}")
    
    def _request(self, method: str, endpoint: str, **kwargs) -> requests.Response:
        """Make an API request with error handling."""
        url = f"{self.base_url}{endpoint}"
        
        try:
            response = self.session.request(method, url, timeout=300, **kwargs)
            response.raise_for_status()
            return response
        except requests.exceptions.Timeout:
            logger.error(f"Request timeout for {endpoint}")
            raise
        except requests.exceptions.RequestException as e:
            logger.error(f"Request failed: {e}")
            raise
    
    # === Single Song Operations ===
    
    def submit_song(
        self,
        title: str,
        lyrics: str,
        style: str,
        duration_mins: float = 5.0,
        seed: Optional[int] = None
    ) -> str:
        """
        Submit a single song for generation.
        
        Args:
            title: Song title
            lyrics: Song lyrics (can include section tags like [Verse], [Chorus])
            style: Style preset name
            duration_mins: Target duration in minutes (5.0-10.0)
            seed: Optional random seed for reproducibility
        
        Returns:
            Job ID for tracking
        """
        payload = {
            "title": title,
            "lyrics": lyrics,
            "style_preset": style,
            "target_duration_minutes": duration_mins,
            "seed": seed
        }
        
        response = self._request("POST", "/generate_song", json=payload)
        result = response.json()
        
        job_id = result.get("job_id")
        logger.info(f"Submitted song '{title}' - Job ID: {job_id}")
        
        return job_id
    
    def get_status(self, job_id: str) -> dict:
        """
        Get the status of a generation job.
        
        Args:
            job_id: Job identifier
        
        Returns:
            Status dictionary with progress information
        """
        response = self._request("GET", f"/status/{job_id}")
        return response.json()
    
    def poll_until_complete(
        self,
        job_id: str,
        callback: Optional[Callable[[dict], None]] = None,
        poll_interval: float = 10.0,
        timeout_seconds: float = 3600.0
    ) -> dict:
        """
        Poll job status until completion.
        
        Args:
            job_id: Job identifier
            callback: Optional callback function called on each status update
            poll_interval: Seconds between polls
            timeout_seconds: Maximum wait time
        
        Returns:
            Final status dictionary
        
        Raises:
            TimeoutError: If timeout is exceeded
        """
        start_time = time.time()
        
        while True:
            # Check timeout
            elapsed = time.time() - start_time
            if elapsed > timeout_seconds:
                raise TimeoutError(f"Job {job_id} did not complete within {timeout_seconds}s")
            
            # Get status
            status = self.get_status(job_id)
            
            # Call callback if provided
            if callback:
                callback(status)
            
            # Check if terminal state
            if status["status"] in ["completed", "failed", "cancelled"]:
                logger.info(f"Job {job_id} finished with status: {status['status']}")
                return status
            
            # Wait before next poll
            logger.debug(f"Job {job_id} status: {status['status']} - waiting {poll_interval}s")
            time.sleep(poll_interval)
    
    def download_song(self, job_id: str, save_path: str) -> str:
        """
        Download the completed song file.
        
        Args:
            job_id: Job identifier
            save_path: Local path to save the file
        
        Returns:
            Path to saved file
        """
        # Ensure parent directory exists
        save_path = Path(save_path)
        save_path.parent.mkdir(parents=True, exist_ok=True)
        
        # Download file
        response = self._request("GET", f"/download/{job_id}")
        
        # Save to file
        with open(save_path, 'wb') as f:
            f.write(response.content)
        
        logger.info(f"Downloaded song {job_id} to {save_path}")
        
        return str(save_path)
    
    # === Bulk Operations ===
    
    def submit_bulk_songs(self, song_configs: List[Dict[str, Any]]) -> List[str]:
        """
        Submit multiple songs for bulk generation.
        
        Args:
            song_configs: List of song configuration dictionaries
        
        Returns:
            List of job IDs
        """
        payload = {"songs": song_configs}
        
        response = self._request("POST", "/generate_bulk", json=payload)
        result = response.json()
        
        job_ids = result.get("job_ids", [])
        logger.info(f"Submitted {len(job_ids)} bulk songs")
        
        return job_ids
    
    def poll_bulk_jobs(
        self,
        job_ids: List[str],
        callback: Optional[Callable[[str, dict], None]] = None
    ) -> Dict[str, dict]:
        """
        Poll multiple jobs until all complete.
        
        Args:
            job_ids: List of job IDs
            callback: Optional callback(job_id, status) on each update
        
        Returns:
            Dictionary mapping job_id to final status
        """
        results = {}
        pending = set(job_ids)
        
        while pending:
            for job_id in list(pending):
                try:
                    status = self.get_status(job_id)
                    
                    if callback:
                        callback(job_id, status)
                    
                    if status["status"] in ["completed", "failed", "cancelled"]:
                        results[job_id] = status
                        pending.remove(job_id)
                
                except Exception as e:
                    logger.error(f"Error polling {job_id}: {e}")
            
            if pending:
                time.sleep(10)
        
        return results
    
    # === Alternate Versions ===
    
    def request_alternates(
        self,
        original_job_id: str,
        count: int = 3,
        variation_type: str = "seed"
    ) -> List[str]:
        """
        Generate alternate versions of an existing song.
        
        Args:
            original_job_id: ID of the original completed job
            count: Number of variations to generate
            variation_type: "seed" or "prompt_mutation"
        
        Returns:
            List of alternate job IDs
        """
        payload = {
            "original_job_id": original_job_id,
            "num_variations": count,
            "variation_type": variation_type
        }
        
        response = self._request("POST", "/generate_alternates", json=payload)
        result = response.json()
        
        alt_ids = result.get("alternate_job_ids", [])
        logger.info(f"Requested {len(alt_ids)} alternates for {original_job_id}")
        
        return alt_ids
    
    def download_alternates_zip(self, job_id: str, save_path: str) -> str:
        """
        Download all alternate versions as a ZIP file.
        
        Args:
            job_id: Original job ID
            save_path: Local path to save the ZIP file
        
        Returns:
            Path to saved ZIP file
        """
        save_path = Path(save_path)
        save_path.parent.mkdir(parents=True, exist_ok=True)
        
        response = self._request("GET", f"/download_alternates/{job_id}")
        
        with open(save_path, 'wb') as f:
            f.write(response.content)
        
        logger.info(f"Downloaded alternates ZIP for {job_id} to {save_path}")
        
        return str(save_path)
    
    # === Utility Methods ===
    
    def health_check(self) -> dict:
        """Check API health status."""
        response = self._request("GET", "/health")
        return response.json()
    
    def list_presets(self) -> List[str]:
        """Get list of available style presets."""
        response = self._request("GET", "/presets")
        return response.json().get("presets", [])
    
    def cancel_job(self, job_id: str) -> bool:
        """Cancel a queued or processing job."""
        response = self._request("POST", f"/cancel/{job_id}")
        return response.status_code == 200
    
    def cleanup_job(self, job_id: str, keep_final: bool = True) -> bool:
        """Clean up temporary files for a job."""
        response = self._request(
            "DELETE",
            f"/cleanup/{job_id}",
            params={"keep_final": keep_final}
        )
        return response.json().get("cleaned", False)


def create_client_from_env() -> RiffusionSongClient:
    """
    Create a client from environment variables.
    
    Expects:
        RIFFUSION_TUNNEL_URL: The public tunnel URL
        RIFFUSION_API_KEY: Optional API key
    
    Returns:
        Configured RiffusionSongClient instance
    """
    import os
    
    tunnel_url = os.environ.get("RIFFUSION_TUNNEL_URL")
    api_key = os.environ.get("RIFFUSION_API_KEY")
    
    if not tunnel_url:
        raise ValueError(
            "RIFFUSION_TUNNEL_URL environment variable not set. "
            "Please set it to your Kaggle tunnel URL."
        )
    
    return RiffusionSongClient(tunnel_url, api_key)


# Example usage
if __name__ == "__main__":
    # This would be used from the external app
    print("RiffusionSongClient SDK")
    print("=" * 40)
    print("\nUsage:")
    print("  from kaggle_client import RiffusionSongClient")
    print("  client = RiffusionSongClient('https://your-tunnel.trycloudflare.com')")
    print("  job_id = client.submit_song('My Song', '[Verse]...', 'lofi_hiphop', 5.0)")
    print("  status = client.poll_until_complete(job_id)")
    print("  client.download_song(job_id, './output/song.wav')")
