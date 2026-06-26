"""
Preset Engine for Riffusion Song Studio

Manages musical style presets and section-specific prompt modifications.
Loads configuration from YAML files and provides prompt synthesis utilities.
"""

import os
from pathlib import Path
from typing import Dict, List, Optional, Any
from dataclasses import dataclass, field

import yaml

try:
    import yaml
except ImportError:
    raise ImportError("PyYAML not installed. Install with: pip install pyyaml")

import logging

logger = logging.getLogger(__name__)


@dataclass
class StylePreset:
    """Represents a musical style preset."""
    name: str
    base_prompt: str
    negative_prompt: str
    tempo_bpm: int
    key: str
    instrumentation: str = ""


@dataclass
class SectionModifier:
    """Represents modifiers for a song section type."""
    name: str
    energy_level: float
    prompt_additions: str
    duration_ratio: float


@dataclass
class PresetConfig:
    """Complete preset configuration."""
    presets: Dict[str, StylePreset] = field(default_factory=dict)
    section_modifiers: Dict[str, SectionModifier] = field(default_factory=dict)


class PresetEngine:
    """
    Engine for managing and synthesizing musical prompts based on presets.
    
    Loads style presets from YAML configuration and provides methods
    to generate section-specific prompts by combining base styles
    with energy modifiers.
    """
    
    DEFAULT_PRESET_PATH = Path(__file__).parent.parent / "presets" / "styles.yaml"
    
    def __init__(self, preset_file: Optional[str] = None):
        """
        Initialize the preset engine.
        
        Args:
            preset_file: Path to YAML preset file. Uses default if None.
        """
        self.presets: Dict[str, StylePreset] = {}
        self.section_modifiers: Dict[str, SectionModifier] = {}
        
        preset_path = Path(preset_file) if preset_file else self.DEFAULT_PRESET_PATH
        
        if preset_path.exists():
            self.load_presets(str(preset_path))
        else:
            logger.warning(f"Preset file not found: {preset_path}. Using empty presets.")
    
    def load_presets(self, preset_file: str) -> None:
        """
        Load presets from a YAML file.
        
        Args:
            preset_file: Path to YAML configuration file
        """
        try:
            with open(preset_file, 'r', encoding='utf-8') as f:
                config = yaml.safe_load(f)
            
            if not config:
                logger.warning(f"Empty preset file: {preset_file}")
                return
            
            # Load style presets
            presets_data = config.get('presets', {})
            for name, data in presets_data.items():
                self.presets[name] = StylePreset(
                    name=name,
                    base_prompt=data.get('base_prompt', ''),
                    negative_prompt=data.get('negative_prompt', ''),
                    tempo_bpm=data.get('tempo_bpm', 120),
                    key=data.get('key', 'C major'),
                    instrumentation=data.get('instrumentation', '')
                )
            
            # Load section modifiers
            modifiers_data = config.get('section_modifiers', {})
            for name, data in modifiers_data.items():
                self.section_modifiers[name] = SectionModifier(
                    name=name,
                    energy_level=data.get('energy_level', 0.7),
                    prompt_additions=data.get('prompt_additions', ''),
                    duration_ratio=data.get('duration_ratio', 0.2)
                )
            
            logger.info(f"Loaded {len(self.presets)} presets and {len(self.section_modifiers)} section modifiers")
            
        except Exception as e:
            logger.error(f"Failed to load presets from {preset_file}: {e}")
            raise
    
    def get_preset(self, preset_name: str) -> Optional[StylePreset]:
        """
        Get a style preset by name.
        
        Args:
            preset_name: Name of the preset
        
        Returns:
            StylePreset if found, None otherwise
        """
        preset = self.presets.get(preset_name.lower())
        if preset is None:
            logger.warning(f"Preset '{preset_name}' not found. Available: {list(self.presets.keys())}")
        return preset
    
    def get_section_modifier(self, section_type: str) -> SectionModifier:
        """
        Get section modifier by type.
        
        Args:
            section_type: Type of section (intro, verse, chorus, bridge, outro)
        
        Returns:
            SectionModifier for the section type
        """
        modifier = self.section_modifiers.get(section_type.lower())
        if modifier is None:
            logger.warning(f"Section modifier '{section_type}' not found. Using defaults.")
            # Return a default modifier
            return SectionModifier(
                name=section_type,
                energy_level=0.7,
                prompt_additions="",
                duration_ratio=0.2
            )
        return modifier
    
    def synthesize_prompt(
        self,
        preset_name: str,
        section_type: str,
        custom_lyrics_theme: Optional[str] = None
    ) -> tuple[str, str]:
        """
        Synthesize a complete prompt for a song section.
        
        Combines the base style preset with section-specific energy modifiers
        and optional custom theme elements.
        
        Args:
            preset_name: Name of the style preset
            section_type: Type of section (intro, verse, chorus, etc.)
            custom_lyrics_theme: Optional additional theme keywords from lyrics
        
        Returns:
            Tuple of (positive_prompt, negative_prompt)
        """
        preset = self.get_preset(preset_name)
        if preset is None:
            # Fallback to a generic prompt
            base_prompt = "instrumental music, melodic, coherent"
            negative_prompt = "noise, distortion, silence"
        else:
            base_prompt = preset.base_prompt
            negative_prompt = preset.negative_prompt
        
        modifier = self.get_section_modifier(section_type)
        
        # Combine base prompt with section modifiers
        parts = [base_prompt]
        
        if modifier.prompt_additions:
            parts.append(modifier.prompt_additions)
        
        if custom_lyrics_theme:
            parts.append(custom_lyrics_theme)
        
        # Add instrumentation if available
        if preset and preset.instrumentation:
            parts.append(f"featuring {preset.instrumentation}")
        
        positive_prompt = ", ".join(parts)
        
        logger.debug(f"Synthesized prompt for {preset_name}/{section_type}: {positive_prompt[:80]}...")
        
        return positive_prompt, negative_prompt
    
    def list_presets(self) -> List[str]:
        """Get list of available preset names."""
        return list(self.presets.keys())
    
    def list_section_types(self) -> List[str]:
        """Get list of available section types."""
        return list(self.section_modifiers.keys())
    
    def calculate_section_durations(
        self,
        target_duration_seconds: float,
        sections: List[str]
    ) -> Dict[str, float]:
        """
        Calculate duration for each section based on target total duration.
        
        Args:
            target_duration_seconds: Target total song duration
            sections: List of section types in order
        
        Returns:
            Dictionary mapping section index to duration in seconds
        """
        durations = {}
        
        # Get duration ratios for each section
        total_ratio = 0.0
        ratios = []
        for section in sections:
            modifier = self.get_section_modifier(section)
            ratios.append(modifier.duration_ratio)
            total_ratio += modifier.duration_ratio
        
        # Normalize ratios if they don't sum to 1.0
        if total_ratio > 0:
            normalized_ratios = [r / total_ratio for r in ratios]
        else:
            # Equal distribution if no ratios defined
            equal_ratio = 1.0 / len(sections) if sections else 0
            normalized_ratios = [equal_ratio] * len(sections)
        
        # Calculate actual durations
        for i, ratio in enumerate(normalized_ratios):
            durations[i] = target_duration_seconds * ratio
        
        return durations
    
    def get_energy_adjusted_params(
        self,
        preset_name: str,
        section_type: str
    ) -> Dict[str, Any]:
        """
        Get generation parameters adjusted for section energy level.
        
        Args:
            preset_name: Style preset name
            section_type: Section type
        
        Returns:
            Dictionary with adjusted generation parameters
        """
        preset = self.get_preset(preset_name)
        modifier = self.get_section_modifier(section_type)
        
        # Adjust guidance scale based on energy
        # Higher energy = lower guidance (more creative/frenzied)
        base_guidance = 7.0
        energy_adjustment = (1.0 - modifier.energy_level) * 2.0
        guidance_scale = base_guidance + energy_adjustment
        
        # Adjust steps based on energy
        # Higher energy might benefit from more steps for clarity
        base_steps = 50
        steps = int(base_steps + (modifier.energy_level * 10))
        
        return {
            "guidance_scale": round(guidance_scale, 1),
            "num_inference_steps": min(steps, 100),
            "energy_level": modifier.energy_level
        }
