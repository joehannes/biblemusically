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
}

impl Want {
    pub fn parse(raw: &str) -> Want {
        match raw.trim().to_ascii_lowercase().as_str() {
            "character" | "same_character" => Want::SameCharacter,
            "refine" | "img2img" => Want::Refine,
            "repair" | "inpaint" => Want::Repair,
            "print" | "upscale" => Want::Print,
            "transparent" | "product" => Want::Transparent,
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
        for t in ["__CFG__", "__IPWEIGHT__", "__DENOISE__"] {
            s = s.replace(t, "1.0");
        }
        for (t, v) in [("__CKPT__", "m.safetensors"), ("__PROMPT__", "a lamp"),
                       ("__NEGATIVE__", "nothing"), ("__SAMPLER__", "euler"),
                       ("__SCHEDULER__", "normal"), ("__REF_IMAGE__", "ref.png"),
                       ("__INPUT_IMAGE__", "in.png"), ("__UPSCALER__", "up.pth")] {
            s = s.replace(t, v);
        }
        serde_json::from_str(&s).unwrap_or_else(|e| panic!("template is not valid JSON: {e}\n{s}"))
    }

    fn every_choice() -> Vec<Choice> {
        let mut out = Vec::new();
        for want in [Want::Fresh, Want::SameCharacter, Want::Refine, Want::Repair,
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
