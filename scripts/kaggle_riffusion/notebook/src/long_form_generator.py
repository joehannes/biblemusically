"""
Long Form Generator for Riffusion Song Studio

Handles sequential generation of audio clips for each song section,
manages VRAM, implements checkpointing, and coordinates with the audio stitcher.
"""

import os
import gc
import json
import time
from pathlib import Path
from typing import Dict, List, Optional, Any, Tuple
from dataclasses import dataclass
import logging

try:
    import torch
except ImportError:
    torch = None  # type: ignore

try:
    from diffusers import DiffusionPipeline, DPMSolverMultistepScheduler
except ImportError:
    DiffusionPipeline = None  # type: ignore
    DPMSolverMultistepScheduler = None  # type: ignore

from .song_arranger import SongArrangement, SongSection, SectionType
from .audio_stitcher import stitch_song_sections, StitchConfig

logger = logging.getLogger(__name__)


@dataclass
class GenerationCheckpoint:
    """Checkpoint data for resuming generation."""
    job_id: str
    arrangement_dict: dict
    completed_section_indices: List[int]
    current_section_index: Optional[int]
    generated_files: Dict[int, List[str]]
    error_message: Optional[str]
    created_at: float
    updated_at: float
    
    def to_dict(self) -> dict:
        return {
            "job_id": self.job_id,
            "arrangement_dict": self.arrangement_dict,
            "completed_section_indices": self.completed_section_indices,
            "current_section_index": self.current_section_index,
            "generated_files": self.generated_files,
            "error_message": self.error_message,
            "created_at": self.created_at,
            "updated_at": self.updated_at
        }
    
    @classmethod
    def from_dict(cls, data: dict) -> "GenerationCheckpoint":
        return cls(
            job_id=data["job_id"],
            arrangement_dict=data["arrangement_dict"],
            completed_section_indices=data["completed_section_indices"],
            current_section_index=data.get("current_section_index"),
            generated_files=data.get("generated_files", {}),
            error_message=data.get("error_message"),
            created_at=data["created_at"],
            updated_at=data["updated_at"]
        )


class LongFormGenerator:
    """
    Generates long-form songs by sequentially creating 5-second Riffusion clips
    and managing the assembly process with checkpointing support.
    """
    
    # Default Riffusion model
    DEFAULT_MODEL = "riffusion/riffusion-v1"
    
    # Generation parameters
    DEFAULT_STEPS = 50
    DEFAULT_GUIDANCE = 7.0
    DEFAULT_SEED = 42
    
    def __init__(
        self,
        output_dir: str = "/tmp/riffusion_output",
        model_name: Optional[str] = None,
        device: Optional[str] = None
    ):
        """
        Initialize the long form generator.
        
        Args:
            output_dir: Directory for generated audio files
            model_name: Riffusion model name/path
            device: Device to run on ('cuda', 'cpu', etc.)
        """
        self.output_dir = Path(output_dir)
        self.output_dir.mkdir(parents=True, exist_ok=True)
        
        self.model_name = model_name or self.DEFAULT_MODEL
        self.device = device or ("cuda" if torch and torch.cuda.is_available() else "cpu")
        
        self.pipeline = None
        self.current_job_id: Optional[str] = None
        self.checkpoint_dir = self.output_dir / "checkpoints"
        self.checkpoint_dir.mkdir(parents=True, exist_ok=True)
        
        logger.info(f"LongFormGenerator initialized on {self.device}, output: {self.output_dir}")
    
    def load_model(self) -> None:
        """Load the Riffusion diffusion pipeline."""
        if DiffusionPipeline is None:
            raise ImportError("diffusers not installed. Install with: pip install diffusers transformers")
        
        if self.pipeline is not None:
            logger.info("Model already loaded")
            return
        
        logger.info(f"Loading Riffusion model: {self.model_name}")
        
        try:
            self.pipeline = DiffusionPipeline.from_pretrained(
                self.model_name,
                use_auth_token=os.environ.get("HUGGINGFACE_TOKEN", None)
            )
            
            # Configure scheduler for better quality
            if DPMSolverMultistepScheduler:
                self.pipeline.scheduler = DPMSolverMultistepScheduler.from_config(
                    self.pipeline.scheduler.config
                )
            
            # Move to device
            self.pipeline = self.pipeline.to(self.device)
            
            # Enable memory optimizations
            if self.device == "cuda" and torch:
                # Enable xformers if available for memory efficiency
                try:
                    self.pipeline.enable_xformers_memory_efficient_attention()
                except Exception:
                    pass
                
                # Enable attention slicing
                try:
                    self.pipeline.enable_attention_slicing()
                except Exception:
                    pass
            
            logger.info("Model loaded successfully")
            
        except Exception as e:
            logger.error(f"Failed to load model: {e}")
            raise
    
    def unload_model(self) -> None:
        """Unload model and clear VRAM."""
        if self.pipeline is not None:
            del self.pipeline
            self.pipeline = None
        
        if torch and self.device == "cuda":
            torch.cuda.empty_cache()
            gc.collect()
        
        logger.info("Model unloaded, VRAM cleared")
    
    def generate_clip(
        self,
        prompt: str,
        negative_prompt: str = "",
        seed: Optional[int] = None,
        duration_seconds: float = 5.0,
        num_clips: int = 1
    ) -> List[str]:
        """
        Generate one or more audio clips using Riffusion.
        
        Args:
            prompt: Positive text prompt
            negative_prompt: Negative text prompt
            seed: Random seed for reproducibility
            duration_seconds: Duration per clip (Riffusion generates ~5s)
            num_clips: Number of clips to generate
        
        Returns:
            List of paths to generated audio files
        """
        if self.pipeline is None:
            self.load_model()
        
        if self.pipeline is None:
            raise RuntimeError("Pipeline not loaded")
        
        seed = seed if seed is not None else self.DEFAULT_SEED
        generator = torch.Generator(device=self.device).manual_seed(seed) if torch else None
        
        generated_files = []
        
        for i in range(num_clips):
            # Use different seed for each clip variation
            clip_seed = seed + i
            clip_generator = torch.Generator(device=self.device).manual_seed(clip_seed) if torch else None
            
            logger.debug(f"Generating clip {i + 1}/{num_clips} with seed {clip_seed}")
            
            try:
                # Run inference
                output = self.pipeline(
                    prompt=prompt,
                    negative_prompt=negative_prompt,
                    generator=clip_generator,
                    num_inference_steps=self.DEFAULT_STEPS,
                    guidance_scale=self.DEFAULT_GUIDANCE,
                    num_images_per_prompt=1
                )
                
                # Save audio (Riffusion outputs spectrogram images that need conversion)
                # For now, we'll save placeholder - actual implementation depends on Riffusion version
                clip_filename = f"clip_{clip_seed}_{int(time.time())}.wav"
                clip_path = str(self.output_dir / clip_filename)
                
                # Note: Actual audio extraction from Riffusion requires additional processing
                # This is a simplified version - production would use riffusion's audio utilities
                self._save_audio_from_output(output, clip_path)
                
                generated_files.append(clip_path)
                logger.info(f"Generated clip: {clip_path}")
                
            except Exception as e:
                logger.error(f"Failed to generate clip: {e}")
                raise
        
        return generated_files
    
    def _save_audio_from_output(self, output: Any, output_path: str) -> None:
        """
        Save audio from Riffusion pipeline output.
        
        Note: This is a placeholder. Actual implementation depends on the specific
        Riffusion pipeline version and output format.
        """
        # Placeholder implementation
        # In production, this would convert spectrogram to audio using Griffin-Lim
        # or load pre-computed audio from the pipeline
        
        # For now, create a minimal WAV file as placeholder
        import wave
        import struct
        
        sample_rate = 44100
        duration = 5.0  # seconds
        n_samples = int(sample_rate * duration)
        
        with wave.open(output_path, 'w') as wav_file:
            wav_file.setnchannels(2)
            wav_file.setsampwidth(2)
            wav_file.setframerate(sample_rate)
            
            # Generate silence as placeholder
            for _ in range(n_samples):
                frame = struct.pack('<h', 0)
                wav_file.writeframes(frame * 2)  # Stereo
        
        logger.warning(f"Created placeholder audio at {output_path} (actual audio extraction not implemented)")
    
    def generate_section(
        self,
        section: SongSection,
        job_id: str,
        seed: Optional[int] = None
    ) -> List[str]:
        """
        Generate all clips for a single song section.
        
        Args:
            section: SongSection to generate
            job_id: Unique job identifier
            seed: Base random seed
        
        Returns:
            List of generated clip file paths
        """
        logger.info(
            f"Generating section: {section.section_type.value} #{section.section_number} "
            f"({section.clip_count} clips)"
        )
        
        section_dir = self.output_dir / job_id / f"section_{section.section_type.value}_{section.section_number}"
        section_dir.mkdir(parents=True, exist_ok=True)
        
        generated_files = []
        
        for i in range(section.clip_count):
            # Vary seed slightly for each clip to add variety
            clip_seed = (seed or self.DEFAULT_SEED) + i * 100
            
            # Generate clip
            clip_files = self.generate_clip(
                prompt=section.prompt,
                negative_prompt=section.negative_prompt,
                seed=clip_seed,
                duration_seconds=5.0,
                num_clips=1
            )
            
            # Move/rename to section directory
            for clip_file in clip_files:
                clip_path = Path(clip_file)
                new_filename = f"clip_{i:03d}{clip_path.suffix}"
                new_path = section_dir / new_filename
                
                if clip_path != new_path:
                    clip_path.rename(new_path)
                
                generated_files.append(str(new_path))
            
            # Clear VRAM between clips if on GPU
            if torch and self.device == "cuda":
                torch.cuda.empty_cache()
        
        section.generated_files = generated_files
        section.status = "complete"
        
        return generated_files
    
    def generate_full_song(
        self,
        arrangement: SongArrangement,
        job_id: str,
        seed: Optional[int] = None,
        resume: bool = False
    ) -> str:
        """
        Generate a complete song from arrangement with checkpointing.
        
        Args:
            arrangement: SongArrangement defining the song structure
            job_id: Unique job identifier
            seed: Base random seed
            resume: Whether to resume from checkpoint
        
        Returns:
            Path to final stitched audio file
        """
        self.current_job_id = job_id
        logger.info(f"Starting full song generation: {job_id} - '{arrangement.title}'")
        
        # Load or create checkpoint
        checkpoint_path = self.checkpoint_dir / f"{job_id}.json"
        
        if resume and checkpoint_path.exists():
            logger.info(f"Resuming from checkpoint: {checkpoint_path}")
            checkpoint = self._load_checkpoint(checkpoint_path)
            completed_indices = set(checkpoint.completed_section_indices)
        else:
            completed_indices = set()
            checkpoint = GenerationCheckpoint(
                job_id=job_id,
                arrangement_dict=arrangement.to_dict(),
                completed_section_indices=[],
                current_section_index=0,
                generated_files={},
                error_message=None,
                created_at=time.time(),
                updated_at=time.time()
            )
        
        all_section_files: Dict[int, List[str]] = checkpoint.generated_files.copy()
        
        try:
            # Ensure model is loaded
            self.load_model()
            
            # Generate each section
            for i, section in enumerate(arrangement.sections):
                if i in completed_indices:
                    logger.info(f"Section {i} already complete, skipping")
                    continue
                
                checkpoint.current_section_index = i
                section.status = "generating"
                
                # Generate section
                section_files = self.generate_section(section, job_id, seed)
                all_section_files[i] = section_files
                
                # Update checkpoint
                checkpoint.completed_section_indices.append(i)
                checkpoint.generated_files = all_section_files
                checkpoint.updated_at = time.time()
                self._save_checkpoint(checkpoint, checkpoint_path)
                
                # Clear VRAM after each section
                self.unload_model()
                
                # Brief pause to allow GPU cooling
                time.sleep(2)
                
                # Reload model for next section (prevents VRAM fragmentation)
                if i < len(arrangement.sections) - 1:
                    self.load_model()
            
            # All sections complete - stitch together
            logger.info("All sections generated, stitching...")
            
            # Flatten all files in order
            ordered_files = []
            for i in range(len(arrangement.sections)):
                ordered_files.extend(all_section_files.get(i, []))
            
            # Stitch sections
            stitch_config = StitchConfig(
                crossfade_duration=1.0,
                normalize_target_dbfs=-16.0,
                fade_out_duration=2.0,
                format="wav"
            )
            
            final_output = str(self.output_dir / job_id / f"{arrangement.title.replace(' ', '_')}_final.wav")
            Path(final_output).parent.mkdir(parents=True, exist_ok=True)
            
            stitched_file = stitch_song_sections(ordered_files, stitch_config, final_output)
            
            # Clean up checkpoint on success
            if checkpoint_path.exists():
                checkpoint_path.unlink()
            
            logger.info(f"Song generation complete: {stitched_file}")
            return stitched_file
            
        except Exception as e:
            logger.error(f"Song generation failed: {e}")
            checkpoint.error_message = str(e)
            checkpoint.updated_at = time.time()
            self._save_checkpoint(checkpoint, checkpoint_path)
            raise
        finally:
            self.unload_model()
            self.current_job_id = None
    
    def _save_checkpoint(self, checkpoint: GenerationCheckpoint, path: Path) -> None:
        """Save checkpoint to JSON file."""
        with open(path, 'w', encoding='utf-8') as f:
            json.dump(checkpoint.to_dict(), f, indent=2)
        logger.debug(f"Checkpoint saved: {path}")
    
    def _load_checkpoint(self, path: Path) -> GenerationCheckpoint:
        """Load checkpoint from JSON file."""
        with open(path, 'r', encoding='utf-8') as f:
            data = json.load(f)
        return GenerationCheckpoint.from_dict(data)
    
    def get_checkpoint_status(self, job_id: str) -> Optional[dict]:
        """
        Get checkpoint status for a job.
        
        Args:
            job_id: Job identifier
        
        Returns:
            Checkpoint status dict or None if no checkpoint exists
        """
        checkpoint_path = self.checkpoint_dir / f"{job_id}.json"
        
        if not checkpoint_path.exists():
            return None
        
        checkpoint = self._load_checkpoint(checkpoint_path)
        
        return {
            "job_id": job_id,
            "completed_sections": len(checkpoint.completed_section_indices),
            "total_sections": len(checkpoint.arrangement_dict.get("sections", [])),
            "current_section": checkpoint.current_section_index,
            "error": checkpoint.error_message,
            "last_updated": checkpoint.updated_at
        }
    
    def cleanup_job(self, job_id: str, keep_final: bool = True) -> None:
        """
        Clean up temporary files for a job.
        
        Args:
            job_id: Job identifier
            keep_final: Whether to keep the final stitched file
        """
        job_dir = self.output_dir / job_id
        
        if not job_dir.exists():
            return
        
        if keep_final:
            # Remove only intermediate files
            for section_dir in job_dir.iterdir():
                if section_dir.is_dir() and section_dir.name.startswith("section_"):
                    import shutil
                    shutil.rmtree(section_dir)
        else:
            # Remove everything
            import shutil
            shutil.rmtree(job_dir)
        
        # Remove checkpoint
        checkpoint_path = self.checkpoint_dir / f"{job_id}.json"
        if checkpoint_path.exists():
            checkpoint_path.unlink()
        
        logger.info(f"Cleaned up job {job_id}")
