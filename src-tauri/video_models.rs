//! Which video model to run for which purpose, on a free Kaggle GPU.
//!
//! # The hardware decides almost everything
//!
//! Kaggle's free accelerators are a **T4 (16 GB, Turing)** or a P100. Turing has no bf16 and no fp8
//! arithmetic, which rules out most of what the video-model community publishes as "the low-VRAM
//! option" — those numbers are quoted for Ada and Hopper cards. Two consequences run through this
//! whole file:
//!
//!   * **fp16 weights, never bf16.** A bf16 checkpoint loads on a T4 only by upcasting, which costs
//!     the memory the smaller format was chosen to save. Where a repo ships both, fp16 is the one.
//!   * **fp8 files are a storage format here, not a speed-up.** ComfyUI will load fp8 weights and
//!     dequantise them to fp16 to compute, so the VRAM saving is real and the speed saving is not.
//!     Worth taking for the text encoder, where the memory matters and the compute does not.
//!
//! # What each model is actually for
//!
//! The models differ less in quality than in *throughput*, and throughput is what a free 30 h weekly
//! quota is spent on. A twelve-section music video wants twelve clips; a TikTok cut wants thirty
//! short ones. So the catalogue is organised by what a clip is for, not by leaderboard position:
//!
//!   * **LTX-Video 2B distilled** — the volume workhorse. Distilled to ~8 steps, so it is several
//!     times faster than anything else here and the only one where "generate thirty and keep six" is
//!     affordable. Motion is simple and prompt adherence is loose; for B-roll behind a lyric, that is
//!     not the constraint people imagine it is.
//!   * **Wan 2.1 T2V-1.3B** — the quality option for text-to-video. Genuinely better motion and
//!     prompt adherence than LTX 2B, at several times the cost per clip. Worth it for the shots
//!     somebody will actually look at.
//!   * **Wan 2.2 TI2V-5B** — image-to-video, and the best of these at it. Slowest by a distance on a
//!     T4; budget it for a handful of hero shots rather than a whole video.
//!   * **AnimateDiff v3 on SD 1.5** — cheapest by far, stylised rather than photoreal, and loops
//!     cleanly. The right answer for a repeating texture behind text, and the wrong one for anything
//!     meant to read as footage.
//!
//! # Image-to-video is the one that pays for itself
//!
//! This app already generates a still per section. Animating a still it has already approved is
//! cheaper than generating a clip from scratch, keeps the composition somebody chose, and cannot
//! wander off-prompt — the failure mode text-to-video has and image-to-video structurally does not.
//! That is why `Purpose::AnimateStill` exists as its own thing rather than as a mode of T2V.
//!
//! Stable Video Diffusion is deliberately absent: its repos are gated behind an accepted licence, so
//! it needs a Hugging Face token to download. FLUX already taught this app that a gated model turns
//! a one-click setup into a support conversation, so nothing here is gated.

use serde::Serialize;

/// What a clip is being made for. The catalogue is indexed by this rather than by model name,
/// because the useful question is "what am I making", not "which checkpoint exists".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Purpose {
    /// Lots of short clips, cheaply — background motion behind lyrics, cutaways, filler.
    Broll,
    /// A shot somebody will look at directly. Fewer, slower, better.
    HeroShot,
    /// Motion added to a still this app already generated and somebody already approved.
    AnimateStill,
    /// A short seamless loop, stylised rather than photographic.
    Loop,
}

/// How a model is driven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Drive {
    /// Prompt in, clip out.
    TextToVideo,
    /// Still image (plus prompt) in, clip out.
    ImageToVideo,
}

/// One downloadable weight file, with where it goes in ComfyUI's tree.
#[derive(Debug, Clone, Serialize)]
pub struct Weight {
    pub repo: &'static str,
    pub file: &'static str,
    /// Subdirectory under `ComfyUI/models/`.
    pub dir: &'static str,
    pub approx_gb: f32,
}

/// A video model, as it exists on a T4.
#[derive(Debug, Clone, Serialize)]
pub struct VideoModel {
    pub id: &'static str,
    pub label: &'static str,
    pub drive: Drive,
    /// Everything that has to be on disk before the graph will run.
    pub weights: &'static [Weight],
    /// Peak VRAM in gigabytes, measured at the preset resolutions below. The T4 has 16.
    pub vram_gb: f32,
    /// Rough minutes for one clip on a T4. Ranges are wide because resolution dominates.
    pub minutes_per_clip: (u32, u32),
    /// The honest sentence about what this model is and is not good at.
    pub character: &'static str,
    pub licence: &'static str,
}

/// A ready-to-run configuration: a model plus the numbers that suit one job.
#[derive(Debug, Clone, Serialize)]
pub struct VideoPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub model: &'static str,
    pub purpose: Purpose,
    pub width: u32,
    pub height: u32,
    /// Frame count. Latent video models want (n × 8 + 1) frames; the values here already satisfy it.
    pub frames: u32,
    pub fps: u32,
    pub steps: u32,
    pub cfg: f32,
    /// Why you would pick this over its neighbours.
    pub use_for: &'static str,
}

impl VideoPreset {
    /// Clip length in seconds, which is what a person is actually choosing.
    pub fn seconds(&self) -> f32 {
        if self.fps == 0 { return 0.0 }
        self.frames as f32 / self.fps as f32
    }
    pub fn is_vertical(&self) -> bool { self.height > self.width }
}

/// A named bundle of presets that go together for one kind of output.
///
/// Packages exist because the useful unit of choice is "I am making a TikTok" rather than "I want
/// 512×768 at 121 frames" — and because a package can be shipped, shared and extended as one thing.
#[derive(Debug, Clone, Serialize)]
pub struct PresetPack {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub presets: &'static [&'static str],
    /// Rough GPU minutes to fill one piece of output with this pack, for quota planning.
    pub typical_minutes: u32,
}

// ── Models ──────────────────────────────────────────────────────────────────

/// Shared text encoder for the Wan family. fp8 purely to save VRAM — see the header.
const UMT5_FP8: Weight = Weight {
    repo: "Comfy-Org/Wan_2.1_ComfyUI_repackaged",
    file: "split_files/text_encoders/umt5_xxl_fp8_e4m3fn_scaled.safetensors",
    dir: "text_encoders", approx_gb: 6.7,
};
const WAN21_VAE: Weight = Weight {
    repo: "Comfy-Org/Wan_2.1_ComfyUI_repackaged",
    file: "split_files/vae/wan_2.1_vae.safetensors",
    dir: "vae", approx_gb: 0.25,
};

pub const MODELS: &[VideoModel] = &[
    VideoModel {
        id: "ltx2b",
        label: "LTX-Video 2B (distilled)",
        drive: Drive::TextToVideo,
        weights: &[
            Weight { repo: "Lightricks/LTX-Video", file: "ltxv-2b-0.9.8-distilled-fp8.safetensors",
                     dir: "checkpoints", approx_gb: 4.46 },
            Weight { repo: "comfyanonymous/flux_text_encoders",
                     file: "t5xxl_fp8_e4m3fn_scaled.safetensors", dir: "text_encoders", approx_gb: 4.9 },
        ],
        vram_gb: 9.0,
        minutes_per_clip: (2, 5),
        character: "Fastest by a wide margin — distilled to about eight steps. Motion is simple and it \
                    follows a prompt loosely, which is why it earns its place on volume rather than on \
                    any single clip.",
        licence: "LTX-Video Open Weights (permissive, commercial use allowed under 10M revenue)",
    },
    VideoModel {
        id: "wan21_t2v_1_3b",
        label: "Wan 2.1 T2V 1.3B",
        drive: Drive::TextToVideo,
        weights: &[
            // fp16 and not bf16: a T4 cannot compute in bf16, so the bf16 file of identical size
            // would be upcast on load and buy nothing.
            Weight { repo: "Comfy-Org/Wan_2.1_ComfyUI_repackaged",
                     file: "split_files/diffusion_models/wan2.1_t2v_1.3B_fp16.safetensors",
                     dir: "diffusion_models", approx_gb: 2.84 },
            UMT5_FP8, WAN21_VAE,
        ],
        vram_gb: 12.0,
        minutes_per_clip: (8, 16),
        character: "Clearly better motion and prompt adherence than LTX at this size, at several times \
                    the cost per clip. The one to spend quota on when the shot carries meaning.",
        licence: "Apache-2.0",
    },
    VideoModel {
        id: "wan22_ti2v_5b",
        label: "Wan 2.2 TI2V 5B (image-to-video)",
        drive: Drive::ImageToVideo,
        weights: &[
            Weight { repo: "QuantStack/Wan2.2-TI2V-5B-GGUF", file: "Wan2.2-TI2V-5B-Q5_K_M.gguf",
                     dir: "unet", approx_gb: 4.0 },
            UMT5_FP8,
            Weight { repo: "Comfy-Org/Wan_2.2_ComfyUI_Repackaged",
                     file: "split_files/vae/wan2.2_vae.safetensors", dir: "vae", approx_gb: 1.4 },
        ],
        vram_gb: 13.0,
        minutes_per_clip: (18, 35),
        character: "The best image-to-video here, and the slowest thing in the catalogue on a T4. \
                    Budget it for a few hero shots, not for a whole video.",
        licence: "Apache-2.0",
    },
    VideoModel {
        id: "animatediff_sd15",
        label: "AnimateDiff v3 on SD 1.5",
        drive: Drive::TextToVideo,
        weights: &[
            Weight { repo: "guoyww/animatediff", file: "v3_sd15_mm.ckpt",
                     dir: "animatediff_models", approx_gb: 1.8 },
            Weight { repo: "runwayml/stable-diffusion-v1-5", file: "v1-5-pruned-emaonly.safetensors",
                     dir: "checkpoints", approx_gb: 4.0 },
        ],
        vram_gb: 6.0,
        minutes_per_clip: (1, 3),
        character: "Cheapest and most stylised — it reads as animation, not as footage. Loops cleanly, \
                    which none of the others do, so it is the right answer for a repeating texture and \
                    the wrong one for anything meant to look filmed.",
        licence: "CreativeML Open RAIL-M (SD 1.5) + Apache-2.0 (motion module)",
    },
];

// ── Presets ─────────────────────────────────────────────────────────────────
//
// Resolutions are the ones each model was trained around. Pushing a video model off its training
// resolution degrades it far more sharply than it does an image model — a 16:9 clip from a model
// trained square does not come out wider, it comes out wrong — so the vertical presets use the
// model's own portrait resolution rather than a cropped landscape one.

pub const PRESETS: &[VideoPreset] = &[
    VideoPreset {
        id: "broll_wide", label: "B-roll · wide 16:9", model: "ltx2b", purpose: Purpose::Broll,
        width: 768, height: 512, frames: 121, fps: 24, steps: 8, cfg: 1.0,
        use_for: "Background motion behind lyrics. Cheap enough to generate several and keep one.",
    },
    VideoPreset {
        id: "broll_vertical", label: "B-roll · vertical 9:16", model: "ltx2b", purpose: Purpose::Broll,
        width: 512, height: 768, frames: 121, fps: 24, steps: 8, cfg: 1.0,
        use_for: "TikTok / Reels / Shorts filler at the aspect the platform actually wants.",
    },
    VideoPreset {
        id: "hero_wide", label: "Hero shot · wide 16:9", model: "wan21_t2v_1_3b", purpose: Purpose::HeroShot,
        width: 832, height: 480, frames: 81, fps: 16, steps: 25, cfg: 6.0,
        use_for: "The shot the video opens on, or the one under the chorus.",
    },
    VideoPreset {
        id: "hero_vertical", label: "Hero shot · vertical 9:16", model: "wan21_t2v_1_3b", purpose: Purpose::HeroShot,
        width: 480, height: 832, frames: 81, fps: 16, steps: 25, cfg: 6.0,
        use_for: "The opening three seconds of a short, where retention is decided.",
    },
    VideoPreset {
        id: "animate_wide", label: "Animate a still · wide", model: "wan22_ti2v_5b", purpose: Purpose::AnimateStill,
        width: 1280, height: 704, frames: 81, fps: 16, steps: 20, cfg: 5.0,
        use_for: "Give motion to an image this project already made, keeping its composition.",
    },
    VideoPreset {
        id: "animate_vertical", label: "Animate a still · vertical", model: "wan22_ti2v_5b", purpose: Purpose::AnimateStill,
        width: 704, height: 1280, frames: 81, fps: 16, steps: 20, cfg: 5.0,
        use_for: "Turn a vertical cover image into a moving opening shot.",
    },
    VideoPreset {
        id: "loop_texture", label: "Loop · stylised texture", model: "animatediff_sd15", purpose: Purpose::Loop,
        width: 512, height: 512, frames: 16, fps: 8, steps: 20, cfg: 7.5,
        use_for: "A seamless repeating background behind text. Not meant to read as footage.",
    },
];

// ── Packages ────────────────────────────────────────────────────────────────

pub const PACKS: &[PresetPack] = &[
    PresetPack {
        id: "shorts",
        label: "Shorts & TikTok",
        description: "Vertical throughout, short clips, cheap enough to over-generate and cut. One \
                      strong opening shot and a pile of filler behind it — which is the shape a short \
                      actually has.",
        presets: &["hero_vertical", "broll_vertical", "loop_texture"],
        typical_minutes: 30,
    },
    PresetPack {
        id: "music_video",
        label: "Music video (wide)",
        description: "One hero shot per chorus, LTX B-roll for the verses, and stills animated where a \
                      picture already exists that is worth keeping.",
        presets: &["hero_wide", "broll_wide", "animate_wide"],
        typical_minutes: 90,
    },
    PresetPack {
        id: "cheap_draft",
        label: "Draft pass (quota-light)",
        description: "Everything on the fastest model, to settle timing and cuts before spending real \
                      GPU hours. Re-run the chosen shots on a heavier preset afterwards.",
        presets: &["broll_wide", "broll_vertical", "loop_texture"],
        typical_minutes: 12,
    },
];

// ── Lookups ─────────────────────────────────────────────────────────────────

pub fn model(id: &str) -> Option<&'static VideoModel> { MODELS.iter().find(|m| m.id == id) }
pub fn preset(id: &str) -> Option<&'static VideoPreset> { PRESETS.iter().find(|p| p.id == id) }
pub fn pack(id: &str) -> Option<&'static PresetPack> { PACKS.iter().find(|p| p.id == id) }

/// The model a preset drives, resolved.
pub fn model_of(p: &VideoPreset) -> Option<&'static VideoModel> { model(p.model) }

/// Every weight a pack needs downloaded, de-duplicated.
///
/// The notebook uses this to fetch exactly what the chosen package requires — downloading the whole
/// catalogue would be ~30 GB and most of a session's time before anything renders.
pub fn weights_for_pack(pack_id: &str) -> Vec<&'static Weight> {
    let Some(p) = pack(pack_id) else { return Vec::new() };
    let mut out: Vec<&'static Weight> = Vec::new();
    for pid in p.presets {
        let Some(m) = preset(pid).and_then(model_of) else { continue };
        for w in m.weights {
            if !out.iter().any(|e| e.repo == w.repo && e.file == w.file) { out.push(w); }
        }
    }
    out
}

/// Total download size for a pack, in gigabytes — the number that decides whether a session spends
/// its first twenty minutes downloading.
pub fn pack_download_gb(pack_id: &str) -> f32 {
    weights_for_pack(pack_id).iter().map(|w| w.approx_gb).sum()
}

/// The presets that suit a purpose, cheapest first.
pub fn presets_for(purpose: Purpose) -> Vec<&'static VideoPreset> {
    let mut v: Vec<&'static VideoPreset> = PRESETS.iter().filter(|p| p.purpose == purpose).collect();
    v.sort_by_key(|p| model_of(p).map(|m| m.minutes_per_clip.0).unwrap_or(u32::MAX));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The T4 has 16 GB and no bf16. Both halves of that are load-bearing.
    #[test]
    fn every_model_fits_a_free_kaggle_t4() {
        for m in MODELS {
            assert!(m.vram_gb <= 14.0,
                    "{} claims {} GB — a T4 has 16 and ComfyUI needs headroom", m.id, m.vram_gb);
            for w in m.weights {
                assert!(!w.file.contains("bf16"),
                        "{} references {}: Turing cannot compute in bf16, so this is upcast on load \
                         and costs the memory the format was chosen to save", m.id, w.file);
            }
        }
    }

    /// Latent video models encode in groups of 8 frames plus a keyframe. An off-grid count is either
    /// rejected outright or silently truncated, and a silently shorter clip is worse than an error.
    #[test]
    fn frame_counts_sit_on_the_latent_grid() {
        for p in PRESETS {
            if p.model == "animatediff_sd15" { continue } // SD1.5 motion modules are not latent-video
            assert_eq!((p.frames - 1) % 8, 0,
                       "{} asks for {} frames, which is not 8n+1", p.id, p.frames);
        }
    }

    #[test]
    fn every_preset_names_a_real_model_and_every_pack_a_real_preset() {
        for p in PRESETS {
            assert!(model_of(p).is_some(), "preset {} names unknown model {}", p.id, p.model);
        }
        for pk in PACKS {
            assert!(!pk.presets.is_empty(), "pack {} is empty", pk.id);
            for pid in pk.presets {
                assert!(preset(pid).is_some(), "pack {} names unknown preset {pid}", pk.id);
            }
        }
    }

    /// A preset that animates an existing still must be driven by an image-to-video model — a T2V
    /// model handed an image ignores it and invents a different scene, which looks like a bug in the
    /// app rather than a mismatched preset.
    #[test]
    fn animating_a_still_uses_an_image_to_video_model() {
        for p in presets_for(Purpose::AnimateStill) {
            assert_eq!(model_of(p).unwrap().drive, Drive::ImageToVideo,
                       "{} animates a still but drives a text-to-video model", p.id);
        }
    }

    #[test]
    fn vertical_presets_are_actually_vertical() {
        for p in PRESETS.iter().filter(|p| p.id.contains("vertical")) {
            assert!(p.is_vertical(), "{} is named vertical but is {}x{}", p.id, p.width, p.height);
        }
    }

    #[test]
    fn clip_lengths_are_short_enough_to_be_worth_regenerating() {
        for p in PRESETS {
            let s = p.seconds();
            assert!(s > 0.5 && s <= 8.0, "{} is {s}s — outside the useful range for a cut", p.id);
        }
    }

    /// A pack's download has to fit a session with time left to render in it.
    #[test]
    fn a_pack_downloads_in_a_sensible_amount_of_time() {
        for pk in PACKS {
            let gb = pack_download_gb(pk.id);
            assert!(gb > 0.0, "{} downloads nothing", pk.id);
            assert!(gb < 25.0, "{} pulls {gb} GB before anything renders", pk.id);
        }
    }

    /// The draft pack exists to be cheap. If it stopped being the cheapest it would have no reason to.
    #[test]
    fn the_draft_pack_is_the_cheapest_one() {
        let draft = PACKS.iter().find(|p| p.id == "cheap_draft").unwrap();
        for pk in PACKS.iter().filter(|p| p.id != "cheap_draft") {
            assert!(draft.typical_minutes < pk.typical_minutes,
                    "draft ({}) is not cheaper than {} ({})", draft.typical_minutes, pk.id, pk.typical_minutes);
        }
    }

    #[test]
    fn purpose_lookup_is_ordered_cheapest_first() {
        let broll = presets_for(Purpose::Broll);
        assert!(!broll.is_empty());
        let mins: Vec<u32> = broll.iter().map(|p| model_of(p).unwrap().minutes_per_clip.0).collect();
        assert!(mins.windows(2).all(|w| w[0] <= w[1]), "not cheapest-first: {mins:?}");
    }
}
