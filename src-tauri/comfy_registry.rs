//! Which ComfyUI graph runs, and with what numbers.
//!
//! The job runner used to choose between two templates with an `if`. That stopped being enough once
//! there were seven, because the choice depends on three things at once — what is being asked for,
//! which model is loaded, and whether this is a draft or a final — and because two of the graphs
//! *cannot* be used with the wrong model at all.
//!
//! # The two hard constraints
//!
//! * **FLUX loads differently.** It ships UNet, CLIP and VAE as separate files, so a graph built on
//!   `CheckpointLoaderSimple` fails on it with a node error that reads like a server problem. FLUX
//!   gets its own graph, always.
//! * **FLUX has no negative prompt.** Trained with guidance distillation, running at CFG 1: there is
//!   nothing for a negative to push away from. So the FLUX graph has no negative text node at all —
//!   a `ConditioningZeroOut` stands in its place, which is what the sampler needs and what an empty
//!   negative honestly *is*. If somebody explicitly asks for strict mode, a different graph brings
//!   guidance back with `DynamicThresholdingFull`, at roughly twice the time.
//!
//! # Draft and final
//!
//! Reviewing fifty images a day at final quality spends the GPU hours the free tier gives you on
//! pictures nobody keeps. A draft is the same graph at few steps; a final is the same graph at full
//! steps. Expressing that as a step profile rather than as separate templates means the draft and the
//! final are the same picture — a different graph would change the composition, which defeats the
//! purpose of reviewing the draft at all.

use serde::Serialize;

/// What the caller wants done. Not a template name — an intention.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Want {
    /// A new picture from a prompt.
    Fresh,
    /// A new picture that keeps a character's face, from a reference image.
    SameCharacter,
    /// Refine a picture that already exists.
    Refine,
    /// Repair one masked region.
    Repair,
    /// Enlarge and put detail back, for print.
    Print,
    /// Product art with the background cut out.
    Transparent,
    /// A picture generated small and then re-sampled larger, for detail a single pass will not give.
    HiRes,
    /// A picture carrying a style LoRA, for a look a prompt alone cannot reach.
    StyleLora,
    /// Drawn rather than photographed — the text encoder stopped a layer early.
    Illustration,
    /// Structure settled before detail, in one graph and at almost no extra cost.
    TwoStage,
    /// Four takes of one idea, to choose between.
    Variations,
    /// Keep a reference's composition, change everything else.
    Compose,
    /// A photo or sketch redrawn in the project's style.
    StyleTransfer,
    /// Wider canvas painted onto an existing picture, rather than cropping it.
    Outpaint,
    /// A 1280x720 thumbnail from art of another shape.
    Thumbnail,
    /// Bilateral symmetry — stained glass, a rose window, an ornamental frame.
    Symmetry,
    /// Two exposures of one idea blended into a single image.
    DoubleExposure,
}

impl Want {
    pub fn parse(raw: &str) -> Want {
        match raw.trim().to_ascii_lowercase().as_str() {
            "character" | "same_character" => Want::SameCharacter,
            "refine" | "img2img" => Want::Refine,
            "repair" | "inpaint" => Want::Repair,
            "print" | "upscale" => Want::Print,
            "transparent" | "product" => Want::Transparent,
            "hires" | "hires_fix" | "detailed" => Want::HiRes,
            "lora" | "style_lora" => Want::StyleLora,
            "illustration" | "clip_skip" | "drawn" => Want::Illustration,
            "two_stage" | "twostage" => Want::TwoStage,
            "variations" | "takes" => Want::Variations,
            "compose" | "controlnet" | "layout" => Want::Compose,
            "style_transfer" | "redraw" => Want::StyleTransfer,
            "outpaint" | "extend" | "widen" => Want::Outpaint,
            "thumbnail" | "thumb" => Want::Thumbnail,
            "symmetry" | "mirror" | "stained_glass" => Want::Symmetry,
            "double_exposure" | "blend" => Want::DoubleExposure,
            _ => Want::Fresh,
        }
    }
}

/// How much time to spend. A draft is for deciding; a final is for keeping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Quality { Draft, Final }

/// The graph to run and the numbers to fill it with.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Choice {
    pub template: &'static str,
    /// The file it came from, for the job log — a name in the log is worth an hour of guessing.
    pub name: &'static str,
    pub steps: i64,
    pub cfg: f64,
    /// How much of the original survives. Only meaningful for `Refine` and `Print`.
    pub denoise: f64,
    /// Custom nodes this graph needs. Checked against the server before submitting, because the
    /// alternative is a node error that reads like the server is broken.
    pub needs_nodes: &'static [&'static str],
    /// Said out loud when the choice is not the obvious one.
    pub note: Option<&'static str>,
}

const PHOTOREAL: &str = include_str!("comfy_workflows/photoreal_sdxl.json");
const CHARACTER: &str = include_str!("comfy_workflows/character_ipadapter_sdxl.json");
const FLUX: &str = include_str!("comfy_workflows/flux_dev.json");
const FLUX_STRICT: &str = include_str!("comfy_workflows/flux_strict.json");
const UPSCALE: &str = include_str!("comfy_workflows/upscale_print.json");
const IMG2IMG: &str = include_str!("comfy_workflows/img2img.json");
const INPAINT: &str = include_str!("comfy_workflows/inpaint.json");
const TRANSPARENT: &str = include_str!("comfy_workflows/transparent.json");
const HIRES: &str = include_str!("comfy_workflows/hires_fix.json");
const LORA: &str = include_str!("comfy_workflows/lora_style_sdxl.json");
const ILLUSTRATION: &str = include_str!("comfy_workflows/clip_skip_stylised.json");
const TWO_STAGE: &str = include_str!("comfy_workflows/two_stage_sampler.json");
const VARIATIONS: &str = include_str!("comfy_workflows/batch_variations.json");
const COMPOSE: &str = include_str!("comfy_workflows/controlnet_compose.json");
const STYLE_TRANSFER: &str = include_str!("comfy_workflows/style_transfer.json");
const OUTPAINT: &str = include_str!("comfy_workflows/outpaint_extend.json");
const FLUX_LORA: &str = include_str!("comfy_workflows/flux_lora.json");
const FLUX_IMG2IMG: &str = include_str!("comfy_workflows/flux_img2img.json");
const THUMBNAIL: &str = include_str!("comfy_workflows/thumbnail_1280x720.json");
const SYMMETRY: &str = include_str!("comfy_workflows/symmetry_mirror.json");
const DOUBLE_EXPOSURE: &str = include_str!("comfy_workflows/double_exposure.json");

/// Is this checkpoint a FLUX one? Decided by name, because that is all the server tells us.
pub fn is_flux(ckpt: &str) -> bool {
    let c = ckpt.to_ascii_lowercase();
    c.contains("flux") || c.starts_with("f1")
}

/// Is this a few-step model (Turbo, Lightning, LCM, Hyper)? Those need low steps and low CFG, and
/// giving them thirty steps at CFG 7 produces a burnt image, not a better one.
pub fn is_few_step(ckpt: &str) -> bool {
    let c = ckpt.to_ascii_lowercase();
    ["turbo", "lightning", "lcm", "hyper", "schnell"].iter().any(|k| c.contains(k))
}

/// Choose the graph and the numbers.
pub fn choose(want: Want, ckpt: &str, quality: Quality, strict: bool) -> Choice {
    let flux = is_flux(ckpt);
    let few = is_few_step(ckpt);

    // Steps first, because everything else reads better next to it. A few-step model ignores the
    // draft/final distinction — it has one speed and thirty steps would only burn it.
    let steps = match (few, quality) {
        (true, _) => 6,
        (false, Quality::Draft) => 12,
        (false, Quality::Final) => 30,
    };
    let cfg = if flux { 3.5 } else if few { 2.0 } else { 6.5 };

    match want {
        Want::SameCharacter if flux => Choice {
            // IPAdapter's SDXL presets do not load against a FLUX UNet, so asking for a character on
            // FLUX quietly produces somebody else. Better to say so and make a fresh picture than to
            // return a stranger and call it the same person.
            template: FLUX, name: "flux_dev", steps, cfg, denoise: 1.0,
            needs_nodes: &["UNETLoader", "DualCLIPLoader", "FluxGuidance"],
            note: Some("FLUX cannot hold a face from a reference image — the IPAdapter presets are \
                        SDXL-only. This is a fresh picture. For character consistency, choose an \
                        SDXL model such as Juggernaut XL."),
        },
        Want::SameCharacter => Choice {
            template: CHARACTER, name: "character_ipadapter_sdxl", steps, cfg, denoise: 1.0,
            needs_nodes: &["IPAdapterUnifiedLoader", "IPAdapterAdvanced"],
            note: None,
        },
        // Refining on FLUX used to submit the SDXL graph — CheckpointLoaderSimple against a FLUX
        // UNet, which fails with a node error that reads like a broken server. FLUX has its own
        // encode path now, so the picture being refined actually survives.
        Want::Refine if flux => Choice {
            template: FLUX_IMG2IMG, name: "flux_img2img", steps, cfg, denoise: 0.55,
            needs_nodes: &["UNETLoader", "DualCLIPLoader", "VAELoader"],
            note: None,
        },
        Want::Refine => Choice {
            template: IMG2IMG, name: "img2img", steps, cfg,
            // Enough to change the thing that was wrong, little enough to keep the picture.
            denoise: 0.55,
            needs_nodes: &[], note: None,
        },
        Want::Repair => Choice {
            template: INPAINT, name: "inpaint", steps, cfg, denoise: 1.0,
            needs_nodes: &[], note: None,
        },
        Want::Print => Choice {
            template: UPSCALE, name: "upscale_print",
            // A detail pass is never a draft: this runs because something is about to be printed.
            steps: if few { 6 } else { 24 }, cfg,
            // Low, because this is putting detail back into pixels that already exist rather than
            // reinterpreting the picture. Higher and the enlargement becomes a different image.
            denoise: 0.35,
            needs_nodes: &["UpscaleModelLoader", "ImageUpscaleWithModel"],
            note: Some("Enlarging and adding detail. This is a second pass over the picture you \
                        already have, not a new one."),
        },
        Want::Transparent => Choice {
            template: TRANSPARENT, name: "transparent", steps, cfg, denoise: 1.0,
            needs_nodes: &["ImageRemoveBackground+"],
            note: Some("Generated on a flat backdrop and then cut out, because a white rectangle \
                        behind a design prints as a white rectangle on the product."),
        },
        // Two passes over one picture: generate at the asked-for size, enlarge the latent, sample
        // again at low denoise. It is the cheapest real quality gain in ComfyUI — the second pass
        // adds detail the first could not resolve without changing the composition, which is why the
        // denoise is low. FLUX is sent to its own graph instead: a second pass at CFG 1 mostly
        // re-renders what is already there, for twice the time.
        Want::HiRes if flux => Choice {
            template: FLUX, name: "flux_dev", steps, cfg, denoise: 1.0,
            needs_nodes: &["UNETLoader", "DualCLIPLoader", "FluxGuidance"],
            note: Some("FLUX resolves detail in one pass, so a second one costs time without \
                        adding much. This is the ordinary FLUX graph."),
        },
        Want::HiRes => Choice {
            template: HIRES, name: "hires_fix", steps,
            cfg,
            // Enough for the second pass to add detail, little enough to keep the picture it was
            // given. Higher and the enlargement becomes a different image.
            denoise: 0.45,
            needs_nodes: &[],
            note: Some("Two passes: generated, enlarged, then re-sampled for detail. Slower than \
                        one pass and sharper than enlarging afterwards."),
        },
        // A style LoRA is the one way to get a consistent *look* that a prompt cannot describe.
        // SDXL only — FLUX LoRAs load through a different node and a mismatched pair fails with a
        // shape error that reads like a broken server.
        Want::StyleLora if flux => Choice {
            // LoraLoaderModelOnly, because FLUX's text encoders come from DualCLIPLoader rather than
            // a checkpoint — there is no CLIP output for a LoRA to patch, and passing one is a type
            // error. Until this existed, a style LoRA on FLUX was silently ignored.
            template: FLUX_LORA, name: "flux_lora", steps, cfg, denoise: 1.0,
            needs_nodes: &["UNETLoader", "DualCLIPLoader", "LoraLoaderModelOnly"],
            note: Some("FLUX LoRAs patch the model only — the text encoders are loaded separately, \
                        so a LoRA's CLIP half is not applied on FLUX."),
        },
        Want::StyleLora => Choice {
            template: LORA, name: "lora_style_sdxl", steps, cfg, denoise: 1.0,
            needs_nodes: &[],
            note: Some("Carrying the style LoRA named in Settings. An empty or misnamed LoRA is a \
                        node error, not a silent fallback — ComfyUI has no default to reach for."),
        },
        // CLIPSetLastLayer is an SD-family idea. FLUX has no equivalent text-encoder layer to stop
        // at, so these all send a FLUX checkpoint to its own graph with a note rather than failing.
        Want::Illustration if flux => Choice {
            template: FLUX, name: "flux_dev", steps, cfg, denoise: 1.0,
            needs_nodes: &["UNETLoader", "DualCLIPLoader", "FluxGuidance"],
            note: Some("Clip skip is an SD-family control and FLUX has no equivalent, so this is the \
                        ordinary FLUX graph. Choose an SDXL illustration model for a drawn look."),
        },
        Want::Illustration => Choice {
            template: ILLUSTRATION, name: "clip_skip_stylised", steps, cfg, denoise: 1.0,
            needs_nodes: &[],
            note: Some("Text encoder stopped one layer early — what illustration and anime \
                        checkpoints were fine-tuned expecting."),
        },
        Want::TwoStage if flux => Choice {
            template: FLUX, name: "flux_dev", steps, cfg, denoise: 1.0,
            needs_nodes: &["UNETLoader", "DualCLIPLoader", "FluxGuidance"],
            note: Some("Splitting the schedule needs a CFG the two halves can differ on, which FLUX \
                        does not have at guidance 1. This is the ordinary FLUX graph."),
        },
        Want::TwoStage => Choice {
            template: TWO_STAGE, name: "two_stage_sampler", steps, cfg, denoise: 1.0,
            needs_nodes: &[],
            note: Some("Structure settled in the early steps, detail after. Costs almost nothing \
                        over one pass and fixes muddled composition rather than softness."),
        },
        Want::Variations => Choice {
            // Four of them, so a draft is the right default: these exist to be chosen between, and
            // reviewing four finals spends the GPU on three pictures nobody keeps.
            template: VARIATIONS, name: "batch_variations",
            steps: if few { 6 } else { 12 }, cfg, denoise: 1.0,
            needs_nodes: &[],
            note: Some("Four takes of one composition in a single job — cheaper than four jobs, \
                        because the model is loaded once."),
        },
        Want::Compose => Choice {
            template: COMPOSE, name: "controlnet_compose", steps, cfg, denoise: 1.0,
            needs_nodes: &["ControlNetLoader", "ControlNetApply"],
            note: Some("The reference steers layout only — unlike a refine, none of its pixels \
                        survive. The ControlNet named in Settings must match the checkpoint family; \
                        an SD1.5 control against SDXL is a shape error, not a weaker effect."),
        },
        Want::StyleTransfer => Choice {
            template: STYLE_TRANSFER, name: "style_transfer", steps, cfg,
            // High on purpose: a timid value here hands back the photograph it was given.
            denoise: 0.78,
            needs_nodes: &[],
            note: Some("The source is rescaled to the target size first, so a phone photo does not \
                        dictate the output dimensions."),
        },
        Want::Outpaint => Choice {
            template: OUTPAINT, name: "outpaint_extend", steps, cfg, denoise: 1.0,
            needs_nodes: &[],
            note: Some("Canvas added left and right and painted in — the original pixels are \
                        untouched. This is how square art becomes a 16:9 frame without cropping."),
        },
        Want::Thumbnail => Choice {
            template: THUMBNAIL, name: "thumbnail_1280x720", steps, cfg,
            // Low: the art is already right, this is a reframe rather than a reinterpretation.
            denoise: 0.30,
            needs_nodes: &[],
            note: Some("Scaled to cover 1280x720 and centre-cropped rather than squashed."),
        },
        Want::Symmetry => Choice {
            template: SYMMETRY, name: "symmetry_mirror", steps, cfg, denoise: 1.0,
            needs_nodes: &[],
            note: Some("Mirrored in the latent and composited before decoding, so the seam blends \
                        rather than showing as a hard vertical edge."),
        },
        Want::DoubleExposure => Choice {
            template: DOUBLE_EXPOSURE, name: "double_exposure", steps, cfg, denoise: 1.0,
            needs_nodes: &[],
            note: Some("Two samplings of one prompt, screen-blended. Costs two passes."),
        },
        Want::Fresh if flux && strict => Choice {
            template: FLUX_STRICT, name: "flux_strict", steps: steps * 2, cfg: 5.0, denoise: 1.0,
            needs_nodes: &["UNETLoader", "DualCLIPLoader", "DynamicThresholdingFull"],
            note: Some("Strict mode: guidance is switched back on so the \"things to avoid\" list \
                        works on FLUX. Roughly twice the time."),
        },
        Want::Fresh if flux => Choice {
            template: FLUX, name: "flux_dev", steps, cfg, denoise: 1.0,
            needs_nodes: &["UNETLoader", "DualCLIPLoader", "FluxGuidance"],
            note: None,
        },
        Want::Fresh => Choice {
            template: PHOTOREAL, name: "photoreal_sdxl", steps, cfg, denoise: 1.0,
            needs_nodes: &[], note: None,
        },
    }
}

/// Does this graph use a negative prompt at all?
///
/// Asked so the job log can say the "avoid" text was not sent, rather than letting somebody believe
/// a restraint was applied when the model never saw it.
pub fn honours_negative(choice: &Choice) -> bool {
    choice.name != "flux_dev"
}

/// Which of the nodes this graph needs are missing from the server.
///
/// The check exists because a missing custom node produces a `/prompt` rejection that names an
/// internal class and reads like the server is broken. Naming the node and what installs it turns a
/// dead end into a five-minute fix.
pub fn missing_nodes(choice: &Choice, installed: &[String]) -> Vec<&'static str> {
    choice.needs_nodes.iter().copied()
        .filter(|n| !installed.iter().any(|i| i == n))
        .collect()
}

/// What to tell somebody whose server is missing a node.
pub fn missing_node_advice(missing: &[&str]) -> String {
    let pack = |node: &str| match node {
        "IPAdapterUnifiedLoader" | "IPAdapterAdvanced" => "ComfyUI_IPAdapter_plus",
        "DynamicThresholdingFull" => "sd-dynamic-thresholding",
        "ImageRemoveBackground+" => "ComfyUI_essentials",
        "UpscaleModelLoader" | "ImageUpscaleWithModel" => "built in — check the server is up to date",
        _ => "a custom node pack",
    };
    let lines: Vec<String> = missing.iter()
        .map(|n| format!("  • {n} — from {}", pack(n)))
        .collect();
    format!(
        "This ComfyUI server is missing {} node(s) this needs:\n{}\nInstall them through the ComfyUI \
         Manager and start the server again. Nothing was submitted, so nothing was wasted.",
        missing.len(), lines.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Fill every placeholder, so a template can be parsed as the JSON it becomes at runtime.
    fn filled(template: &str) -> Value {
        let mut s = template.to_string();
        for t in ["__WIDTH__", "__HEIGHT__", "__BATCH__", "__SEED__", "__STEPS__"] {
            s = s.replace(t, "8");
        }
        for t in ["__CFG__", "__IPWEIGHT__", "__DENOISE__", "__LORA_STRENGTH__"] {
            s = s.replace(t, "1.0");
        }
        for (t, v) in [("__CKPT__", "m.safetensors"), ("__PROMPT__", "a lamp"),
                       ("__NEGATIVE__", "nothing"), ("__SAMPLER__", "euler"),
                       ("__SCHEDULER__", "normal"), ("__REF_IMAGE__", "ref.png"),
                       ("__INPUT_IMAGE__", "in.png"), ("__UPSCALER__", "up.pth"),
                       ("__LORA__", "style.safetensors"),
                       ("__CONTROLNET__", "canny-sdxl.safetensors")] {
            s = s.replace(t, v);
        }
        serde_json::from_str(&s).unwrap_or_else(|e| panic!("template is not valid JSON: {e}\n{s}"))
    }

    fn every_choice() -> Vec<Choice> {
        let mut out = Vec::new();
        for want in [Want::Fresh, Want::SameCharacter, Want::Refine, Want::Repair, Want::HiRes, Want::StyleLora,
                     Want::Illustration, Want::TwoStage, Want::Variations, Want::Compose, Want::StyleTransfer,
                     Want::Outpaint, Want::Thumbnail, Want::Symmetry, Want::DoubleExposure,
                     Want::Print, Want::Transparent] {
            for ckpt in ["juggernautXL_v9.safetensors", "flux1-dev.safetensors",
                         "sdxl_turbo.safetensors"] {
                for quality in [Quality::Draft, Quality::Final] {
                    for strict in [false, true] {
                        out.push(choose(want, ckpt, quality, strict));
                    }
                }
            }
        }
        out
    }

    #[test]
    fn every_graph_the_registry_can_choose_is_valid_json_once_filled() {
        // A template that only fails at submit time fails after the user has waited for a queue.
        for choice in every_choice() {
            let graph = filled(choice.template);
            let obj = graph.as_object().expect("a graph is an object of nodes");
            assert!(obj.values().any(|n| n["class_type"] == "SaveImage"),
                    "{} never saves anything", choice.name);
            for (id, node) in obj {
                if id.starts_with('_') { continue; }
                assert!(node["class_type"].is_string(), "{}: node {id} has no class_type", choice.name);
            }
        }
    }

    #[test]
    fn no_template_leaves_a_placeholder_behind() {
        // A stray __TOKEN__ reaches ComfyUI as a literal string and produces a picture of nothing,
        // or a parse error, depending on where it landed.
        for choice in every_choice() {
            let text = serde_json::to_string(&filled(choice.template)).unwrap();
            assert!(!text.contains("__"), "{} still contains a placeholder: {text}", choice.name);
        }
    }

    #[test]
    fn flux_never_receives_a_negative_prompt() {
        // The trap this whole module exists around. FLUX runs at CFG 1 with no classifier-free
        // guidance, so a negative prompt is silently ignored — and "silently" is the problem.
        let choice = choose(Want::Fresh, "flux1-dev.safetensors", Quality::Final, false);
        assert_eq!(choice.name, "flux_dev");
        assert!(!honours_negative(&choice));
        let graph = filled(choice.template);
        let negatives: Vec<&Value> = graph.as_object().unwrap().values()
            .filter(|n| n["class_type"] == "CLIPTextEncode")
            .collect();
        assert_eq!(negatives.len(), 1, "FLUX gets one text node — the positive one");
        // The sampler still needs *something* on its negative input; zeroed conditioning is what an
        // empty negative honestly is.
        assert!(graph.as_object().unwrap().values()
            .any(|n| n["class_type"] == "ConditioningZeroOut"));
    }

    #[test]
    fn strict_mode_brings_guidance_back_and_charges_for_it() {
        let plain = choose(Want::Fresh, "flux1-dev.safetensors", Quality::Final, false);
        let strict = choose(Want::Fresh, "flux1-dev.safetensors", Quality::Final, true);
        assert_eq!(strict.name, "flux_strict");
        assert!(honours_negative(&strict));
        assert_eq!(strict.steps, plain.steps * 2, "roughly twice the time, and it says so");
        assert!(strict.note.unwrap().contains("twice"));
        assert!(strict.needs_nodes.contains(&"DynamicThresholdingFull"));
        // Strict is meaningless on a model that already has guidance, so it changes nothing there.
        let sdxl = choose(Want::Fresh, "juggernautXL_v9.safetensors", Quality::Final, true);
        assert_eq!(sdxl.name, "photoreal_sdxl");
    }

    #[test]
    fn asking_flux_for_a_character_says_it_cannot_rather_than_returning_a_stranger() {
        // IPAdapter's presets are SDXL-only. Running the character graph against a FLUX UNet does
        // not fail loudly — it produces somebody else, which is worse.
        let choice = choose(Want::SameCharacter, "flux1-dev.safetensors", Quality::Final, false);
        assert_eq!(choice.name, "flux_dev");
        let note = choice.note.expect("this substitution must be explained");
        assert!(note.contains("cannot hold a face"), "{note}");
        assert!(note.contains("Juggernaut") || note.contains("SDXL"), "point at one that can: {note}");

        // On SDXL it is the character graph, with no apology needed.
        let sdxl = choose(Want::SameCharacter, "juggernautXL_v9.safetensors", Quality::Final, false);
        assert_eq!(sdxl.name, "character_ipadapter_sdxl");
        assert!(sdxl.note.is_none());
    }

    #[test]
    fn a_draft_and_a_final_are_the_same_picture_at_different_step_counts() {
        // If they used different graphs the composition would change, and then reviewing the draft
        // would tell you nothing about the final.
        let draft = choose(Want::Fresh, "juggernautXL_v9.safetensors", Quality::Draft, false);
        let final_ = choose(Want::Fresh, "juggernautXL_v9.safetensors", Quality::Final, false);
        assert_eq!(draft.template, final_.template);
        assert_eq!(draft.cfg, final_.cfg);
        assert!(draft.steps < final_.steps);
    }

    #[test]
    fn a_few_step_model_is_never_given_thirty_steps() {
        // Thirty steps at CFG 7 on a Turbo model produces a burnt image, not a better one.
        for ckpt in ["sdxl_turbo.safetensors", "dreamshaperXL_lightning.safetensors",
                     "flux1-schnell.safetensors"] {
            assert!(is_few_step(ckpt), "{ckpt}");
            let c = choose(Want::Fresh, ckpt, Quality::Final, false);
            assert!(c.steps <= 8, "{ckpt} got {} steps", c.steps);
            assert!(c.cfg <= 3.6, "{ckpt} got cfg {}", c.cfg);
        }
        assert!(!is_few_step("juggernautXL_v9.safetensors"));
    }

    #[test]
    fn a_print_pass_keeps_the_picture_it_was_given() {
        // Low denoise on purpose: this puts detail back into pixels that exist. Higher and the
        // enlargement becomes a different image, which is not what "upscale this for print" means.
        let c = choose(Want::Print, "juggernautXL_v9.safetensors", Quality::Final, false);
        assert_eq!(c.name, "upscale_print");
        assert!(c.denoise < 0.5, "denoise {} would reinterpret rather than enlarge", c.denoise);
        assert!(c.note.unwrap().contains("not a new one"));
        // And it never runs as a draft — a detail pass happens because something is being printed.
        let draft = choose(Want::Print, "juggernautXL_v9.safetensors", Quality::Draft, false);
        assert_eq!(draft.steps, c.steps);
    }

    #[test]
    fn a_missing_custom_node_is_named_along_with_what_installs_it() {
        // The bare failure is a /prompt rejection naming an internal class, which reads like the
        // server is broken rather than like a five-minute fix.
        let choice = choose(Want::SameCharacter, "juggernautXL_v9.safetensors", Quality::Final, false);
        let installed = vec!["KSampler".to_string(), "CLIPTextEncode".to_string()];
        let missing = missing_nodes(&choice, &installed);
        assert_eq!(missing, vec!["IPAdapterUnifiedLoader", "IPAdapterAdvanced"]);

        let advice = missing_node_advice(&missing);
        assert!(advice.contains("ComfyUI_IPAdapter_plus"), "{advice}");
        assert!(advice.contains("nothing was wasted"), "{advice}");

        // A server that has them reports nothing missing.
        let full: Vec<String> = choice.needs_nodes.iter().map(|s| s.to_string()).collect();
        assert!(missing_nodes(&choice, &full).is_empty());
        // A graph that needs nothing special is never blocked.
        assert!(missing_nodes(&choose(Want::Fresh, "sdxl.safetensors", Quality::Final, false), &[]).is_empty());
    }

    #[test]
    fn the_words_a_user_types_reach_the_right_graph() {
        assert_eq!(Want::parse("inpaint"), Want::Repair);
        assert_eq!(Want::parse("img2img"), Want::Refine);
        assert_eq!(Want::parse("product"), Want::Transparent);
        assert_eq!(Want::parse("upscale"), Want::Print);
        assert_eq!(Want::parse("character"), Want::SameCharacter);
        // Anything unrecognised makes a picture rather than failing.
        assert_eq!(Want::parse("something else"), Want::Fresh);
    }
}
