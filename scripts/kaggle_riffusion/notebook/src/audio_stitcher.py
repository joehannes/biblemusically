"""
Audio Stitching Module for Riffusion Song Studio

Handles seamless concatenation of audio clips with crossfades,
beat-matching assistance, and loudness normalization.
"""

import os
import tempfile
from pathlib import Path
from typing import List, Optional, Tuple
from dataclasses import dataclass

try:
    from pydub import AudioSegment
    from pydub.effects import normalize
except ImportError:
    AudioSegment = None  # type: ignore
    normalize = None  # type: ignore

import logging

logger = logging.getLogger(__name__)


@dataclass
class StitchConfig:
    """Configuration for audio stitching operations."""
    crossfade_duration: float = 1.0  # seconds
    normalize_target_dbfs: float = -16.0  # LUFS target for streaming
    fade_out_duration: float = 2.0  # seconds for final fade out
    sample_rate: int = 44100
    channels: int = 2
    format: str = "wav"


def load_audio_clip(file_path: str) -> "AudioSegment":
    """Load an audio clip from file."""
    if AudioSegment is None:
        raise ImportError("pydub not installed. Install with: pip install pydub")
    
    path = Path(file_path)
    if not path.exists():
        raise FileNotFoundError(f"Audio file not found: {file_path}")
    
    # Determine format from extension
    ext = path.suffix.lower().lstrip('.')
    return AudioSegment.from_file(str(path), format=ext or "wav")


def apply_crossfade(
    clip1: "AudioSegment",
    clip2: "AudioSegment",
    crossfade_duration: float = 1.0
) -> "AudioSegment":
    """
    Apply crossfade between two audio clips.
    
    Args:
        clip1: First audio segment
        clip2: Second audio segment  
        crossfade_duration: Duration of crossfade in seconds
    
    Returns:
        Combined audio segment with crossfade
    """
    if AudioSegment is None:
        raise ImportError("pydub not installed")
    
    # Convert crossfade duration to milliseconds
    crossfade_ms = int(crossfade_duration * 1000)
    
    # Ensure crossfade doesn't exceed clip lengths
    crossfade_ms = min(crossfade_ms, len(clip1) - 100, len(clip2) - 100)
    crossfade_ms = max(crossfade_ms, 0)
    
    # Overlay clip2 onto clip1 with fade in/out
    clip2_faded = clip2.fade_in(crossfade_ms).fade_out(crossfade_ms)
    
    # Calculate position to start overlay
    overlay_position = len(clip1) - crossfade_ms
    
    combined = clip1.overlay(clip2_faded, position=int(overlay_position))
    
    logger.debug(f"Crossfaded clips: {len(clip1)}ms + {len(clip2)}ms with {crossfade_ms}ms overlap")
    
    return combined


def concatenate_clips(
    clips: List["AudioSegment"],
    crossfade_duration: float = 1.0
) -> "AudioSegment":
    """
    Concatenate multiple audio clips with crossfades.
    
    Args:
        clips: List of audio segments to concatenate
        crossfade_duration: Duration of crossfade between clips
    
    Returns:
        Single concatenated audio segment
    """
    if not clips:
        raise ValueError("No clips provided for concatenation")
    
    if len(clips) == 1:
        return clips[0]
    
    result = clips[0]
    for i, clip in enumerate(clips[1:], start=1):
        result = apply_crossfade(result, clip, crossfade_duration)
        logger.debug(f"Concatenated clip {i}/{len(clips) - 1}")
    
    return result


def normalize_loudness(
    audio: "AudioSegment",
    target_dbfs: float = -16.0
) -> "AudioSegment":
    """
    Normalize audio loudness to target dBFS.
    
    Args:
        audio: Input audio segment
        target_dbfs: Target loudness in dBFS (default -16 for streaming)
    
    Returns:
        Normalized audio segment
    """
    if normalize is None:
        raise ImportError("pydub.effects.normalize not available")
    
    # Simple peak normalization using pydub
    change_in_dbfs = target_dbfs - audio.dBFS
    normalized = audio.apply_gain(change_in_dbfs)
    
    # Clip to prevent digital clipping
    normalized = normalized.limit(0.0)
    
    logger.info(f"Normalized audio from {audio.dBFS:.1f} dBFS to {target_dbfs:.1f} dBFS")
    
    return normalized


def apply_final_fade_out(
    audio: "AudioSegment",
    fade_duration: float = 2.0
) -> "AudioSegment":
    """
    Apply a fade-out effect to the end of an audio track.
    
    Args:
        audio: Input audio segment
        fade_duration: Duration of fade-out in seconds
    
    Returns:
        Audio segment with fade-out applied
    """
    fade_ms = int(fade_duration * 1000)
    fade_ms = min(fade_ms, len(audio) - 100)  # Don't fade entire track
    
    return audio.fade_out(fade_ms)


def stitch_song_sections(
    section_files: List[str],
    config: Optional[StitchConfig] = None,
    output_path: Optional[str] = None
) -> str:
    """
    Stitch together song sections into a complete song.
    
    Args:
        section_files: List of paths to section audio files (in order)
        config: Stitching configuration
        output_path: Optional output file path. If None, uses temp file.
    
    Returns:
        Path to the stitched audio file
    """
    if AudioSegment is None:
        raise ImportError("pydub not installed. Install with: pip install pydub")
    
    config = config or StitchConfig()
    
    if not section_files:
        raise ValueError("No section files provided")
    
    logger.info(f"Stitching {len(section_files)} sections...")
    
    # Load all clips
    clips = []
    for i, file_path in enumerate(section_files):
        try:
            clip = load_audio_clip(file_path)
            
            # Ensure consistent sample rate and channels
            clip = clip.set_frame_rate(config.sample_rate)
            clip = clip.set_channels(config.channels)
            
            clips.append(clip)
            logger.debug(f"Loaded section {i + 1}: {len(clip) / 1000:.1f}s")
        except Exception as e:
            logger.error(f"Failed to load section {i + 1} ({file_path}): {e}")
            raise
    
    # Concatenate with crossfades
    combined = concatenate_clips(clips, config.crossfade_duration)
    
    # Normalize loudness
    normalized = normalize_loudness(combined, config.normalize_target_dbfs)
    
    # Apply final fade out
    final = apply_final_fade_out(normalized, config.fade_out_duration)
    
    # Determine output path
    if output_path is None:
        fd, output_path = tempfile.mkstemp(suffix=f".{config.format}")
        os.close(fd)
    
    # Export final audio
    final.export(
        output_path,
        format=config.format,
        parameters=["-q:a", "0"] if config.format == "mp3" else []
    )
    
    total_duration = len(final) / 1000
    logger.info(f"Stitched song complete: {total_duration:.1f}s saved to {output_path}")
    
    return output_path


def estimate_total_duration(
    section_durations: List[float],
    crossfade_duration: float = 1.0
) -> float:
    """
    Estimate total duration after stitching with crossfades.
    
    Args:
        section_durations: List of section durations in seconds
        crossfade_duration: Crossfade duration between sections
    
    Returns:
        Estimated total duration in seconds
    """
    if not section_durations:
        return 0.0
    
    total = sum(section_durations)
    # Subtract overlap time (one less crossfade than sections)
    overlap = crossfade_duration * (len(section_durations) - 1)
    
    return total - overlap


def get_audio_info(file_path: str) -> dict:
    """
    Get information about an audio file.
    
    Args:
        file_path: Path to audio file
    
    Returns:
        Dictionary with audio metadata
    """
    if AudioSegment is None:
        raise ImportError("pydub not installed")
    
    audio = load_audio_clip(file_path)
    
    return {
        "duration_seconds": len(audio) / 1000,
        "sample_rate": audio.frame_rate,
        "channels": audio.channels,
        "bit_depth": audio.sample_width * 8,
        "dBFS": audio.dBFS,
        "file_size_bytes": os.path.getsize(file_path)
    }
