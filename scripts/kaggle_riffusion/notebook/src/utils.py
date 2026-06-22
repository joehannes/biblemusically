"""
Utility functions for Riffusion Kaggle Song Studio
"""

import os
import logging
from pathlib import Path
from typing import Optional, List, Dict, Any
import hashlib

logger = logging.getLogger(__name__)


def setup_logging(level: int = logging.INFO) -> None:
    """
    Configure logging for the application.
    
    Args:
        level: Logging level (default INFO)
    """
    logging.basicConfig(
        level=level,
        format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
        datefmt='%Y-%m-%d %H:%M:%S'
    )


def ensure_directory(path: str, create_parents: bool = True) -> Path:
    """
    Ensure a directory exists, creating it if necessary.
    
    Args:
        path: Directory path
        create_parents: Whether to create parent directories
    
    Returns:
        Path object for the directory
    """
    dir_path = Path(path)
    
    if create_parents:
        dir_path.mkdir(parents=True, exist_ok=True)
    else:
        dir_path.mkdir(exist_ok=True)
    
    return dir_path


def get_file_hash(file_path: str) -> str:
    """
    Calculate MD5 hash of a file.
    
    Args:
        file_path: Path to file
    
    Returns:
        MD5 hash string
    """
    hash_md5 = hashlib.md5()
    
    with open(file_path, 'rb') as f:
        for chunk in iter(lambda: f.read(4096), b""):
            hash_md5.update(chunk)
    
    return hash_md5.hexdigest()


def format_duration(seconds: float) -> str:
    """
    Format duration in human-readable form.
    
    Args:
        seconds: Duration in seconds
    
    Returns:
        Formatted string (e.g., "5m 30s")
    """
    minutes = int(seconds // 60)
    remaining_seconds = int(seconds % 60)
    
    if minutes > 0:
        return f"{minutes}m {remaining_seconds}s"
    else:
        return f"{remaining_seconds}s"


def estimate_generation_time(
    total_clips: int,
    seconds_per_clip: float = 15.0
) -> dict:
    """
    Estimate total generation time based on clip count.
    
    Args:
        total_clips: Number of clips to generate
        seconds_per_clip: Average time per clip (includes inference + overhead)
    
    Returns:
        Dictionary with time estimates
    """
    total_seconds = total_clips * seconds_per_clip
    
    return {
        "total_clips": total_clips,
        "estimated_seconds": total_seconds,
        "estimated_minutes": round(total_seconds / 60, 1),
        "formatted": format_duration(total_seconds)
    }


def check_gpu_availability() -> dict:
    """
    Check GPU availability and configuration.
    
    Returns:
        Dictionary with GPU information
    """
    result = {
        "cuda_available": False,
        "gpu_name": None,
        "gpu_memory_gb": None,
        "device_count": 0
    }
    
    try:
        import torch
        
        result["cuda_available"] = torch.cuda.is_available()
        
        if result["cuda_available"]:
            result["device_count"] = torch.cuda.device_count()
            
            if result["device_count"] > 0:
                result["gpu_name"] = torch.cuda.get_device_name(0)
                
                # Get memory info
                total_memory = torch.cuda.get_device_properties(0).total_memory
                result["gpu_memory_gb"] = round(total_memory / (1024 ** 3), 1)
    
    except ImportError:
        pass
    
    return result


def get_kaggle_environment_info() -> dict:
    """
    Get information about the Kaggle environment.
    
    Returns:
        Dictionary with environment details
    """
    info = {
        "is_kaggle": False,
        "gpu_type": None,
        "ram_gb": None,
        "disk_space_gb": None
    }
    
    # Check if running on Kaggle
    if os.path.exists("/kaggle"):
        info["is_kaggle"] = True
        
        # Try to detect GPU type from environment
        gpu_info = check_gpu_availability()
        if gpu_info["cuda_available"]:
            info["gpu_type"] = gpu_info["gpu_name"]
            info["ram_gb"] = gpu_info["gpu_memory_gb"]
        
        # Estimate disk space
        try:
            import shutil
            total, used, free = shutil.disk_usage("/")
            info["disk_space_gb"] = round(free / (1024 ** 3), 1)
        except Exception:
            pass
    
    return info


def truncate_text(text: str, max_length: int = 100, suffix: str = "...") -> str:
    """
    Truncate text to maximum length.
    
    Args:
        text: Text to truncate
        max_length: Maximum length
        suffix: Suffix to add if truncated
    
    Returns:
        Truncated text
    """
    if len(text) <= max_length:
        return text
    
    return text[:max_length - len(suffix)] + suffix


def sanitize_filename(filename: str) -> str:
    """
    Sanitize a string for use as a filename.
    
    Args:
        filename: Original filename
    
    Returns:
        Sanitized filename
    """
    # Remove or replace problematic characters
    sanitized = filename.strip()
    
    # Replace spaces with underscores
    sanitized = sanitized.replace(' ', '_')
    
    # Remove special characters
    invalid_chars = '<>:"/\\|?*'
    for char in invalid_chars:
        sanitized = sanitized.replace(char, '')
    
    # Limit length
    if len(sanitized) > 100:
        sanitized = sanitized[:100]
    
    return sanitized or "untitled"


def parse_duration_string(duration_str: str) -> Optional[float]:
    """
    Parse a duration string into seconds.
    
    Supports formats like:
    - "5:30" (5 minutes 30 seconds)
    - "5m 30s"
    - "330" (seconds)
    - "5.5" (minutes as float)
    
    Args:
        duration_str: Duration string
    
    Returns:
        Duration in seconds or None if parsing fails
    """
    try:
        # Try MM:SS format
        if ':' in duration_str:
            parts = duration_str.split(':')
            minutes = int(parts[0])
            seconds = int(parts[1]) if len(parts) > 1 else 0
            return minutes * 60 + seconds
        
        # Try "Xm Ys" format
        if 'm' in duration_str.lower():
            parts = duration_str.lower().replace('s', '').split('m')
            minutes = float(parts[0].strip()) if parts[0].strip() else 0
            seconds = float(parts[1].strip()) if len(parts) > 1 and parts[1].strip() else 0
            return minutes * 60 + seconds
        
        # Try plain number (assume seconds if > 60, else minutes)
        value = float(duration_str)
        if value > 60:
            return value  # Already in seconds
        else:
            return value * 60  # Treat as minutes
    
    except (ValueError, IndexError):
        return None


def batch_iterate(items: List[Any], batch_size: int):
    """
    Iterate over items in batches.
    
    Args:
        items: List of items
        batch_size: Size of each batch
    
    Yields:
        Batches of items
    """
    for i in range(0, len(items), batch_size):
        yield items[i:i + batch_size]


class ProgressTracker:
    """Simple progress tracker for long-running operations."""
    
    def __init__(self, total: int, description: str = "Progress"):
        """
        Initialize progress tracker.
        
        Args:
            total: Total number of items
            description: Description of the operation
        """
        self.total = total
        self.current = 0
        self.description = description
    
    def update(self, amount: int = 1) -> None:
        """Update progress by amount."""
        self.current += amount
        self._log()
    
    def _log(self) -> None:
        """Log current progress."""
        percent = (self.current / self.total * 100) if self.total > 0 else 0
        logger.info(f"{self.description}: {self.current}/{self.total} ({percent:.1f}%)")
    
    def is_complete(self) -> bool:
        """Check if progress is complete."""
        return self.current >= self.total
