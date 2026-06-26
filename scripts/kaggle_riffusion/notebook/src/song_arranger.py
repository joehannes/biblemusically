"""
Song Arranger Module for Riffusion Song Studio

The "5-10 Minute Brain" - parses lyrics into song structures,
maps energy levels, and orchestrates long-form generation.
"""

import re
import json
from pathlib import Path
from typing import Dict, List, Optional, Tuple, Any
from dataclasses import dataclass, field, asdict
from enum import Enum
import logging

logger = logging.getLogger(__name__)


class SectionType(str, Enum):
    """Types of song sections."""
    INTRO = "intro"
    VERSE = "verse"
    CHORUS = "chorus"
    BRIDGE = "bridge"
    OUTRO = "outro"
    PRE_CHORUS = "pre_chorus"
    POST_CHORUS = "post_chorus"
    SOLO = "solo"


@dataclass
class SongSection:
    """Represents a single section of a song."""
    section_type: SectionType
    section_number: int
    lyrics: str
    duration_seconds: float
    prompt: str
    negative_prompt: str
    energy_level: float
    clip_count: int  # Number of 5-second clips needed
    status: str = "pending"  # pending, generating, complete, failed
    generated_files: List[str] = field(default_factory=list)
    
    def to_dict(self) -> dict:
        """Convert to dictionary for JSON serialization."""
        return {
            "section_type": self.section_type.value,
            "section_number": self.section_number,
            "lyrics": self.lyrics,
            "duration_seconds": self.duration_seconds,
            "prompt": self.prompt,
            "negative_prompt": self.negative_prompt,
            "energy_level": self.energy_level,
            "clip_count": self.clip_count,
            "status": self.status,
            "generated_files": self.generated_files
        }
    
    @classmethod
    def from_dict(cls, data: dict) -> "SongSection":
        """Create from dictionary."""
        return cls(
            section_type=SectionType(data["section_type"]),
            section_number=data["section_number"],
            lyrics=data.get("lyrics", ""),
            duration_seconds=data["duration_seconds"],
            prompt=data["prompt"],
            negative_prompt=data["negative_prompt"],
            energy_level=data["energy_level"],
            clip_count=data["clip_count"],
            status=data.get("status", "pending"),
            generated_files=data.get("generated_files", [])
        )


@dataclass
class SongArrangement:
    """Complete song arrangement with all sections."""
    title: str
    style_preset: str
    target_duration_minutes: float
    sections: List[SongSection] = field(default_factory=list)
    total_clips: int = 0
    estimated_duration_seconds: float = 0.0
    
    def to_dict(self) -> dict:
        """Convert to dictionary for JSON serialization."""
        return {
            "title": self.title,
            "style_preset": self.style_preset,
            "target_duration_minutes": self.target_duration_minutes,
            "sections": [s.to_dict() for s in self.sections],
            "total_clips": self.total_clips,
            "estimated_duration_seconds": self.estimated_duration_seconds
        }
    
    @classmethod
    def from_dict(cls, data: dict) -> "SongArrangement":
        """Create from dictionary."""
        sections = [SongSection.from_dict(s) for s in data.get("sections", [])]
        return cls(
            title=data["title"],
            style_preset=data["style_preset"],
            target_duration_minutes=data["target_duration_minutes"],
            sections=sections,
            total_clips=data.get("total_clips", 0),
            estimated_duration_seconds=data.get("estimated_duration_seconds", 0.0)
        )


class SongArranger:
    """
    The brain behind 5-10 minute song generation.
    
    Parses raw lyrics into structured song arrangements,
    maps energy levels to sections, and calculates clip requirements.
    """
    
    # Riffusion generates ~5 second clips
    CLIP_DURATION_SECONDS = 5.0
    
    # Common structural tags in lyrics
    SECTION_PATTERNS = {
        SectionType.INTRO: re.compile(r'\[?(intro|introduction)\]?', re.IGNORECASE),
        SectionType.VERSE: re.compile(r'\[?(verse|v)[\s]*(\d+)?\]?', re.IGNORECASE),
        SectionType.CHORUS: re.compile(r'\[?(chorus|c|refrain)\]?', re.IGNORECASE),
        SectionType.BRIDGE: re.compile(r'\[?(bridge|b)\]?', re.IGNORECASE),
        SectionType.OUTRO: re.compile(r'\[?(outro|ending|finale)\]?', re.IGNORECASE),
        SectionType.PRE_CHORUS: re.compile(r'\[?(pre[- ]?chorus|pre)[- ]?(chorus)?\]?', re.IGNORECASE),
        SectionType.POST_CHORUS: re.compile(r'\[?(post[- ]?chorus|post)[- ]?(chorus)?\]?', re.IGNORECASE),
        SectionType.SOLO: re.compile(r'\[?(solo|instrumental|break)\]?', re.IGNORECASE),
    }
    
    def __init__(self, preset_engine=None):
        """
        Initialize the song arranger.
        
        Args:
            preset_engine: PresetEngine instance for prompt synthesis
        """
        self.preset_engine = preset_engine
    
    def parse_lyrics(self, lyrics: str) -> List[Tuple[SectionType, int, str]]:
        """
        Parse raw lyrics into structured sections.
        
        Args:
            lyrics: Raw lyrics text, optionally with section tags
        
        Returns:
            List of (section_type, section_number, lyrics) tuples
        """
        sections = []
        current_section = SectionType.VERSE
        current_number = 1
        current_lyrics = []
        
        lines = lyrics.strip().split('\n')
        
        for line in lines:
            line_stripped = line.strip()
            
            # Check if this line is a section tag
            matched_section = None
            match_num = 1
            
            for section_type, pattern in self.SECTION_PATTERNS.items():
                match = pattern.match(line_stripped)
                if match:
                    matched_section = section_type
                    # Try to extract section number if present
                    num_match = pattern.search(line_stripped)
                    if num_match and len(num_match.groups()) >= 2 and num_match.group(2):
                        try:
                            match_num = int(num_match.group(2))
                        except (ValueError, IndexError):
                            match_num = 1
                    break
            
            if matched_section:
                # Save previous section if it has content
                if current_lyrics:
                    sections.append((
                        current_section,
                        current_number,
                        '\n'.join(current_lyrics).strip()
                    ))
                
                # Start new section
                current_section = matched_section
                current_number = match_num
                current_lyrics = []
            else:
                current_lyrics.append(line)
        
        # Don't forget the last section
        if current_lyrics:
            sections.append((
                current_section,
                current_number,
                '\n'.join(current_lyrics).strip()
            ))
        
        # If no sections were found, split evenly
        if not sections and lyrics.strip():
            logger.info("No section tags found, splitting lyrics evenly")
            sections = self._split_lyrics_evenly(lyrics)
        
        logger.info(f"Parsed {len(sections)} sections from lyrics")
        return sections
    
    def _split_lyrics_evenly(self, lyrics: str, target_sections: int = 6) -> List[Tuple[SectionType, int, str]]:
        """
        Split lyrics evenly into logical sections when no tags are present.
        
        Creates a standard song structure: Intro, Verse 1, Chorus, Verse 2, Chorus, Outro
        """
        lines = lyrics.strip().split('\n')
        total_lines = len(lines)
        
        if total_lines == 0:
            return []
        
        lines_per_section = max(1, total_lines // target_sections)
        
        sections = []
        standard_structure = [
            (SectionType.INTRO, 1),
            (SectionType.VERSE, 1),
            (SectionType.CHORUS, 1),
            (SectionType.VERSE, 2),
            (SectionType.CHORUS, 2),
            (SectionType.OUTRO, 1)
        ]
        
        line_idx = 0
        for section_type, number in standard_structure:
            if line_idx >= total_lines:
                break
            
            end_idx = min(line_idx + lines_per_section, total_lines)
            section_lyrics = '\n'.join(lines[line_idx:end_idx])
            
            if section_lyrics.strip():
                sections.append((section_type, number, section_lyrics.strip()))
            
            line_idx = end_idx
        
        return sections
    
    def analyze_lyric_energy(self, lyrics: str) -> float:
        """
        Analyze lyrics to estimate energy level based on simple heuristics.
        
        Args:
            lyrics: Lyrics text for the section
        
        Returns:
            Energy level between 0.0 and 1.0
        """
        if not lyrics:
            return 0.5
        
        # Simple heuristics
        energy_indicators = {
            'high': ['love', 'fire', 'burn', 'fly', 'rise', 'light', 'power', 'strong', 'free', 'dream'],
            'low': ['quiet', 'soft', 'still', 'night', 'sleep', 'calm', 'peace', 'slow', 'gentle']
        }
        
        lyrics_lower = lyrics.lower()
        words = lyrics_lower.split()
        
        high_count = sum(1 for word in words if any(ind in word for ind in energy_indicators['high']))
        low_count = sum(1 for word in words if any(ind in word for ind in energy_indicators['low']))
        
        # Base energy on ratio
        total = high_count + low_count
        if total == 0:
            return 0.5
        
        base_energy = 0.5 + (high_count - low_count) / (total * 2)
        return max(0.0, min(1.0, base_energy))
    
    def create_arrangement(
        self,
        title: str,
        lyrics: str,
        style_preset: str,
        target_duration_minutes: float = 5.0
    ) -> SongArrangement:
        """
        Create a complete song arrangement from lyrics.
        
        Args:
            title: Song title
            lyrics: Raw or tagged lyrics
            style_preset: Name of the style preset to use
            target_duration_minutes: Target song duration (5-10 minutes)
        
        Returns:
            Complete SongArrangement object
        """
        # Clamp duration to 5-10 minutes
        target_duration_minutes = max(5.0, min(10.0, target_duration_minutes))
        target_duration_seconds = target_duration_minutes * 60
        
        # Parse lyrics into sections
        parsed_sections = self.parse_lyrics(lyrics)
        
        # Calculate duration per section
        section_durations = self._calculate_section_durations(
            target_duration_seconds,
            [s[0] for s in parsed_sections]
        )
        
        # Create SongSection objects
        song_sections = []
        total_clips = 0
        
        for i, (section_type, section_num, section_lyrics) in enumerate(parsed_sections):
            duration = section_durations.get(i, 30.0)  # Default 30s per section
            
            # Calculate number of 5-second clips needed
            clip_count = max(1, int(duration / self.CLIP_DURATION_SECONDS))
            
            # Adjust duration based on actual clip count
            adjusted_duration = clip_count * self.CLIP_DURATION_SECONDS
            
            # Synthesize prompt for this section
            prompt, negative_prompt = self._synthesize_section_prompt(
                style_preset,
                section_type,
                section_lyrics
            )
            
            # Analyze lyric energy
            energy_level = self.analyze_lyric_energy(section_lyrics)
            
            song_section = SongSection(
                section_type=section_type,
                section_number=section_num,
                lyrics=section_lyrics,
                duration_seconds=adjusted_duration,
                prompt=prompt,
                negative_prompt=negative_prompt,
                energy_level=energy_level,
                clip_count=clip_count
            )
            
            song_sections.append(song_section)
            total_clips += clip_count
        
        arrangement = SongArrangement(
            title=title,
            style_preset=style_preset,
            target_duration_minutes=target_duration_minutes,
            sections=song_sections,
            total_clips=total_clips,
            estimated_duration_seconds=sum(s.duration_seconds for s in song_sections)
        )
        
        logger.info(
            f"Created arrangement '{title}': {len(song_sections)} sections, "
            f"{total_clips} clips, ~{arrangement.estimated_duration_seconds:.0f}s"
        )
        
        return arrangement
    
    def _calculate_section_durations(
        self,
        total_duration: float,
        section_types: List[SectionType]
    ) -> Dict[int, float]:
        """
        Calculate duration for each section based on type ratios.
        
        Standard ratios:
        - Intro: 10%
        - Verse: 25%
        - Chorus: 25%
        - Bridge: 15%
        - Outro: 10%
        - Pre/Post Chorus: 7.5%
        """
        default_ratios = {
            SectionType.INTRO: 0.10,
            SectionType.VERSE: 0.25,
            SectionType.CHORUS: 0.25,
            SectionType.BRIDGE: 0.15,
            SectionType.OUTRO: 0.10,
            SectionType.PRE_CHORUS: 0.075,
            SectionType.POST_CHORUS: 0.075,
            SectionType.SOLO: 0.10
        }
        
        # Calculate total ratio weight
        total_ratio = sum(default_ratios.get(st, 0.2) for st in section_types)
        
        if total_ratio == 0:
            # Equal distribution
            equal_dur = total_duration / len(section_types) if section_types else 0
            return {i: equal_dur for i in range(len(section_types))}
        
        # Distribute duration proportionally
        durations = {}
        for i, section_type in enumerate(section_types):
            ratio = default_ratios.get(section_type, 0.2)
            durations[i] = (ratio / total_ratio) * total_duration
        
        return durations
    
    def _synthesize_section_prompt(
        self,
        style_preset: str,
        section_type: SectionType,
        lyrics: str
    ) -> Tuple[str, str]:
        """
        Synthesize prompts for a section using the preset engine.
        
        Args:
            style_preset: Style preset name
            section_type: Type of section
            lyrics: Section lyrics for theme extraction
        
        Returns:
            Tuple of (positive_prompt, negative_prompt)
        """
        if self.preset_engine:
            # Extract theme keywords from lyrics
            theme_keywords = self._extract_theme_keywords(lyrics)
            
            return self.preset_engine.synthesize_prompt(
                style_preset,
                section_type.value,
                theme_keywords
            )
        else:
            # Fallback prompts
            base_prompts = {
                SectionType.INTRO: "atmospheric introduction, building mood",
                SectionType.VERSE: "steady rhythm, narrative flow",
                SectionType.CHORUS: "energetic, anthemic, full instrumentation",
                SectionType.BRIDGE: "transitional, building tension",
                SectionType.OUTRO: "resolving, fading conclusion",
            }
            
            prompt = base_prompts.get(section_type, "melodic instrumental")
            negative_prompt = "noise, distortion, silence, artifacts"
            
            return f"{style_preset}, {prompt}", negative_prompt
    
    def _extract_theme_keywords(self, lyrics: str, max_keywords: int = 3) -> str:
        """
        Extract key thematic words from lyrics for prompt enhancement.
        
        Args:
            lyrics: Section lyrics
            max_keywords: Maximum number of keywords to extract
        
        Returns:
            Comma-separated keywords string
        """
        if not lyrics:
            return ""
        
        # Simple keyword extraction - take unique nouns/adjectives
        # In production, could use NLP library
        words = lyrics.lower().split()
        
        # Filter common words
        stop_words = {'the', 'a', 'an', 'and', 'or', 'but', 'in', 'on', 'at', 'to', 'for', 'of', 'with', 'by'}
        significant_words = [w for w in words if w not in stop_words and len(w) > 3]
        
        # Get unique words preserving order
        seen = set()
        keywords = []
        for word in significant_words:
            clean_word = ''.join(c for c in word if c.isalpha())
            if clean_word and clean_word not in seen:
                seen.add(clean_word)
                keywords.append(clean_word)
                if len(keywords) >= max_keywords:
                    break
        
        return ', '.join(keywords) if keywords else ""
    
    def save_arrangement(self, arrangement: SongArrangement, filepath: str) -> None:
        """Save arrangement to JSON file for checkpointing."""
        with open(filepath, 'w', encoding='utf-8') as f:
            json.dump(arrangement.to_dict(), f, indent=2)
        logger.debug(f"Saved arrangement to {filepath}")
    
    def load_arrangement(self, filepath: str) -> SongArrangement:
        """Load arrangement from JSON file."""
        with open(filepath, 'r', encoding='utf-8') as f:
            data = json.load(f)
        return SongArrangement.from_dict(data)
    
    def get_progress(self, arrangement: SongArrangement) -> dict:
        """
        Get generation progress for an arrangement.
        
        Returns:
            Dictionary with progress information
        """
        total = len(arrangement.sections)
        complete = sum(1 for s in arrangement.sections if s.status == "complete")
        current = None
        
        for i, section in enumerate(arrangement.sections):
            if section.status not in ["complete", "pending"]:
                current = {
                    "index": i,
                    "type": section.section_type.value,
                    "number": section.section_number,
                    "status": section.status
                }
                break
        
        return {
            "total_sections": total,
            "completed_sections": complete,
            "current_section": current,
            "percent_complete": (complete / total * 100) if total > 0 else 0
        }
