//! Where an engine's server can live, and what it costs to put it there.
//!
//! # Why this is a separate axis from the engine
//!
//! An engine ("ComfyUI for video", "ACE-Step for songs") answers *what* runs. This answers *where*.
//! The two were tangled together because for a long time there was only one where — Kaggle — so the
//! engine and its host were the same object. That stopped being true the moment somebody could point
//! the app at a rented box, and it is why the app already worked with RunPod without anybody
//! noticing: every engine takes a URL, and a URL does not care what is behind it.
//!
//! So most of what follows is not new capability. It is the app finally *saying* what it can already
//! do, with the steps to get there — which is the difference between a feature and a feature
//! somebody can find.
//!
//! # The three shapes, and why they are ranked by machinery rather than price
//!
//! * **Notebook + tunnel** (Kaggle, Lightning) — free, and the most code: quota arithmetic, session
//!   monitors, rotating URLs, account rotation. Everything the app already carries for Kaggle.
//! * **Rented box** (RunPod, Vast, Modal) — hourly, and almost no code: a fixed address that stays
//!   up. Nothing to monitor because nothing rotates.
//! * **Serverless API** (fal.ai, Replicate) — per output, and *negative* code: no ComfyUI, no graph,
//!   no server lifecycle at all.
//!
//! Cheapest to run and cheapest to build are opposite ends of that list. Worth stating plainly,
//! because the intuition that free is simplest is exactly backwards here.
//!
//! Every figure below is a list price checked in August 2026. They move; they are here to make the
//! *ratios* legible, not as quotes.

use serde::Serialize;

/// How a provider is reached, which decides how much of the app has to be involved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Shape {
    /// A free notebook host running our notebook behind a tunnel. Quota, sessions, rotating URLs.
    NotebookTunnel,
    /// An hourly GPU at a fixed address, running the same ComfyUI.
    RentedBox,
    /// A hosted model API. No server to manage; pay per output.
    Serverless,
    /// The user's own machine.
    Local,
}

/// What using it costs, in the form the user actually experiences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Cost {
    /// No money, but a metered allowance.
    Free,
    /// Free monthly credits, then metered.
    Credits,
    /// Billed by the hour the box is up — including while it is idle.
    Hourly,
    /// Billed per generated image or second of video. Nothing when idle.
    PerOutput,
}

#[derive(Debug, Clone, Serialize)]
pub struct Provider {
    pub id: &'static str,
    pub label: &'static str,
    pub shape: Shape,
    pub cost: Cost,
    /// The headline number, written for a human: "~30 h/week", "$0.34/h", "$0.05/s of video".
    pub price: &'static str,
    /// What you get for free before paying anything, or "" when there is no free part.
    pub free_allowance: &'static str,
    /// Can the app drive this end to end, or does the user have to do something by hand each time?
    pub automated: bool,
    /// Which engines this can host. Empty means "any engine that takes a server URL".
    pub engines: &'static [&'static str],
    /// Where to sign up. Shown as a button, so it has to be the page that actually starts signup.
    pub signup_url: &'static str,
    /// Ordered steps from "no account" to "the app can use it".
    pub setup: &'static [&'static str],
    /// Settings keys that must be filled before this can work.
    pub needs: &'static [&'static str],
    /// The honest sentence about when to pick this one.
    pub note: &'static str,
}

pub const PROVIDERS: &[Provider] = &[
    // ── Free, automated ─────────────────────────────────────────────────
    Provider {
        id: "kaggle",
        label: "Kaggle",
        shape: Shape::NotebookTunnel,
        cost: Cost::Free,
        price: "free",
        free_allowance: "~30 GPU-hours per week, per account",
        automated: true,
        engines: &[],
        signup_url: "https://www.kaggle.com/account/login?phase=startRegisterTab",
        setup: &[
            "Create a free Kaggle account (a Google account works).",
            "Verify your phone number — Kaggle will not hand out a GPU until you do.",
            "Open Settings → Account → Create New API Token and save the kaggle.json it downloads.",
            "Paste that file into Settings → Kaggle accounts → Add another account.",
            "Press Start & connect on any engine. The first run takes about ten minutes.",
        ],
        needs: &["kaggle.json"],
        note: "The only free host the app can drive completely on its own, and the reason it works \
               without a card. Quota is per account, so connecting a second one roughly doubles what \
               you can render in a week.",
    },
    Provider {
        id: "lightning",
        label: "Lightning.ai",
        shape: Shape::NotebookTunnel,
        cost: Cost::Free,
        price: "free tier, then hourly",
        free_allowance: "~80 GPU-hours per month",
        automated: false,
        engines: &[],
        signup_url: "https://lightning.ai/sign-up",
        setup: &[
            "Create a free Lightning.ai account.",
            "Start a new Studio and attach a GPU (T4 or A10G).",
            "Upload the engine notebook from Settings → the engine → Open notebook, or clone it.",
            "Run it. It prints a public URL exactly as the Kaggle one does.",
            "Paste that URL into the engine's server-URL field here.",
        ],
        needs: &["server URL"],
        note: "About 80 free hours a month on top of Kaggle's ~30 a week, and the session persists \
               instead of dying at 12 hours. Currently a paste-the-URL provider: the app does not \
               drive it the way it drives Kaggle.",
    },

    // ── Your own hardware ───────────────────────────────────────────────
    Provider {
        id: "local",
        label: "This machine",
        shape: Shape::Local,
        cost: Cost::Free,
        price: "free",
        free_allowance: "whatever your GPU can do",
        automated: false,
        engines: &[],
        signup_url: "https://github.com/comfyanonymous/ComfyUI#installing",
        setup: &[
            "Install ComfyUI locally (or acestep-api for songs).",
            "Start it — ComfyUI listens on 127.0.0.1:8188 by default.",
            "Put http://127.0.0.1:8188 in the engine's server-URL field here.",
        ],
        needs: &["server URL"],
        note: "No quota, no tunnel, no session limit, and nothing leaves the machine. Needs a GPU \
               with enough VRAM for the model — see the tier the engine is set to.",
    },

    // ── Rented, hourly ──────────────────────────────────────────────────
    Provider {
        id: "runpod",
        label: "RunPod",
        shape: Shape::RentedBox,
        cost: Cost::Hourly,
        price: "RTX 4090 from $0.34/h · A100 80 GB ~$1.64/h",
        free_allowance: "",
        automated: false,
        engines: &[],
        signup_url: "https://www.runpod.io/console/signup",
        setup: &[
            "Create a RunPod account and add credit.",
            "Deploy a pod from the ComfyUI template (Community Cloud is the cheaper tier).",
            "Open the pod's HTTP port 8188 and copy its public URL.",
            "Paste that URL into the engine's server-URL field here.",
            "Stop the pod when you are done — an idle pod bills the same as a busy one.",
        ],
        needs: &["server URL", "payment method"],
        note: "The most predictable paid option, and about five times cheaper per video than paying \
               per output. Nothing to integrate — the app has always accepted a URL.",
    },
    Provider {
        id: "vast",
        label: "Vast.ai",
        shape: Shape::RentedBox,
        cost: Cost::Hourly,
        price: "RTX 4090 from $0.29/h · A100 80 GB from $0.50/h",
        free_allowance: "",
        automated: false,
        engines: &[],
        signup_url: "https://cloud.vast.ai/create/",
        setup: &[
            "Create a Vast.ai account and add credit.",
            "Rent an instance with the ComfyUI template, filtering for reliability above 99%.",
            "Copy the instance's public URL for port 8188.",
            "Paste that URL into the engine's server-URL field here.",
        ],
        needs: &["server URL", "payment method"],
        note: "The cheapest real GPU here. It is a peer-to-peer marketplace, so the low tier is \
               interruptible — good for batches of short clips, poor for one long render.",
    },
    Provider {
        id: "modal",
        label: "Modal",
        shape: Shape::RentedBox,
        cost: Cost::Credits,
        price: "per second, T4 through B200",
        free_allowance: "$30 of credits every month",
        automated: false,
        engines: &[],
        signup_url: "https://modal.com/signup",
        setup: &[
            "Create a Modal account — the Starter plan is free and needs no card.",
            "Install the CLI: pip install modal, then run modal setup.",
            "Deploy a ComfyUI app and note the web endpoint it prints.",
            "Paste that endpoint into the engine's server-URL field here.",
        ],
        needs: &["server URL"],
        note: "$30 a month of per-second billing with no session cap is arguably the best free tier \
               available. The app already uses Modal for remote video rendering — this is the same \
               account doing GPU generation as well.",
    },

    // ── Serverless, per output ──────────────────────────────────────────
    Provider {
        id: "fal",
        label: "fal.ai",
        shape: Shape::Serverless,
        cost: Cost::PerOutput,
        price: "video from $0.05/s · images from ~$0.003",
        free_allowance: "",
        automated: true,
        engines: &["video", "images"],
        signup_url: "https://fal.ai/dashboard/keys",
        setup: &[
            "Create a fal.ai account and add credit.",
            "Open Dashboard → Keys and create an API key.",
            "Paste the key into Settings → fal.ai.",
            "That is all — there is no server to start, and nothing to stop afterwards.",
        ],
        needs: &["fal_api_key"],
        note: "No server, no tunnel, no quota, no session. About five times the cost of a rented box \
               per video, and none of the operations. The right answer for anyone who never wants to \
               see the word 'tunnel'.",
    },
    Provider {
        id: "replicate",
        label: "Replicate",
        shape: Shape::Serverless,
        cost: Cost::PerOutput,
        price: "video $0.07–0.25/s",
        free_allowance: "",
        automated: false,
        engines: &["video", "images"],
        signup_url: "https://replicate.com/signin",
        setup: &[
            "Create a Replicate account and add credit.",
            "Copy your API token from the account page.",
            "Paste it into Settings → Replicate.",
        ],
        needs: &["replicate_api_key"],
        note: "The largest model catalogue anywhere, at a worse price than fal.ai for the same \
               models. Worth it when you need something fal does not host.",
    },
];

/// Which provider a server URL belongs to, read from its hostname.
///
/// This is the answer to "is RunPod set up?" — nothing records that a pasted URL *is* RunPod, so the
/// host is the only evidence there is. It is evidence rather than a guess for every provider that
/// serves from its own domain, which is all of them except one:
///
/// A `*.trycloudflare.com` address is a quick tunnel, and Kaggle and a Lightning studio open exactly
/// the same kind. So that case returns `Ambiguous` rather than picking one — reporting "Lightning is
/// configured" because Kaggle's tunnel happens to be up would be worse than reporting nothing, since
/// somebody would then stop looking for the thing that is actually missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum UrlOrigin {
    /// Confidently this provider.
    Known(&'static str),
    /// A notebook tunnel. Could be any host running our notebook.
    Ambiguous,
    /// Nothing configured.
    Empty,
    /// A host nothing here recognises — somebody's own box, and none the worse for it.
    Unknown,
}

pub fn provider_of_url(url: &str) -> UrlOrigin {
    let u = url.trim().to_ascii_lowercase();
    if u.is_empty() { return UrlOrigin::Empty; }
    // Take the host without parsing: these arrive hand-pasted and half of them lack a scheme.
    let host = u.trim_start_matches("https://").trim_start_matches("http://")
        .split('/').next().unwrap_or("").split(':').next().unwrap_or("");
    if host.is_empty() { return UrlOrigin::Unknown; }

    if host == "127.0.0.1" || host == "localhost" || host == "0.0.0.0" || host == "[::1]" {
        return UrlOrigin::Known("local");
    }
    // Ambiguous before the specific matches, since a tunnel host can front any of them.
    if host.ends_with("trycloudflare.com") || host.ends_with("lhr.life")
        || host.ends_with("serveo.net") || host.ends_with("gradio.live") {
        return UrlOrigin::Ambiguous;
    }
    for (suffix, id) in [
        ("proxy.runpod.net", "runpod"), ("runpod.io", "runpod"), ("runpod.net", "runpod"),
        ("modal.run", "modal"), ("modal.host", "modal"),
        ("vast.ai", "vast"), ("vastai.io", "vast"),
        ("lightning.ai", "lightning"),
        ("fal.run", "fal"), ("fal.ai", "fal"),
        ("replicate.com", "replicate"), ("replicate.delivery", "replicate"),
    ] {
        if host == suffix || host.ends_with(&format!(".{suffix}")) {
            return UrlOrigin::Known(id);
        }
    }
    UrlOrigin::Unknown
}

pub fn provider(id: &str) -> Option<&'static Provider> {
    let id = id.trim().to_ascii_lowercase();
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Providers that cost nothing to try, cheapest-to-start first.
pub fn free_providers() -> Vec<&'static Provider> {
    PROVIDERS.iter().filter(|p| matches!(p.cost, Cost::Free | Cost::Credits)).collect()
}

/// Providers the app can drive end to end, with no manual step per session.
pub fn automated_providers() -> Vec<&'static Provider> {
    PROVIDERS.iter().filter(|p| p.automated).collect()
}

/// Can this provider host this engine?
pub fn hosts(p: &Provider, engine: &str) -> bool {
    p.engines.is_empty() || p.engines.iter().any(|e| *e == engine)
}

/// Providers able to host a given engine.
pub fn for_engine(engine: &str) -> Vec<&'static Provider> {
    PROVIDERS.iter().filter(|p| hosts(p, engine)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every provider has to be actionable: a signup page, steps that start from nothing, and a
    /// sentence saying when to choose it. A catalogue entry without those is an advert.
    #[test]
    fn every_provider_can_actually_be_set_up() {
        for p in PROVIDERS {
            assert!(p.signup_url.starts_with("https://"), "{} has no signup URL", p.id);
            assert!(p.setup.len() >= 3, "{} has too few steps to be followable", p.id);
            assert!(p.note.len() > 40, "{} does not say when to pick it", p.id);
            assert!(!p.needs.is_empty(), "{} claims to need nothing at all", p.id);
            assert!(!p.price.is_empty(), "{} has no price", p.id);
        }
    }

    /// Anything free must say what the allowance is. "Free" with no number is the least useful thing
    /// a page like this can say.
    #[test]
    fn free_means_a_stated_allowance() {
        for p in PROVIDERS.iter().filter(|p| matches!(p.cost, Cost::Free | Cost::Credits)) {
            assert!(!p.free_allowance.is_empty(),
                    "{} is free but does not say how much", p.id);
        }
    }

    /// Paid providers must not claim a free allowance — that is the one direction of error that
    /// costs somebody money they did not expect to spend.
    #[test]
    fn paid_providers_promise_nothing_free() {
        for p in PROVIDERS.iter().filter(|p| matches!(p.cost, Cost::Hourly | Cost::PerOutput)) {
            assert!(p.free_allowance.is_empty(),
                    "{} bills per use but advertises a free allowance", p.id);
        }
    }

    /// At least one free provider must be fully automated, or the app has no out-of-the-box path.
    #[test]
    fn there_is_always_a_free_automated_path() {
        assert!(free_providers().iter().any(|p| p.automated),
                "nothing free can be driven by the app — first run would need manual setup");
    }

    #[test]
    fn ids_are_unique_and_lookups_are_forgiving() {
        let mut ids: Vec<&str> = PROVIDERS.iter().map(|p| p.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(n, ids.len(), "duplicate provider id");
        assert!(provider("  KAGGLE ").is_some(), "lookup should tolerate case and padding");
        assert!(provider("nope").is_none());
    }

    /// A provider with no engine list is a "any engine that takes a URL" provider, and must be able
    /// to host the engines that do.
    #[test]
    fn url_providers_host_every_engine() {
        for engine in ["video", "comfyui", "flux", "heartmula", "acestep"] {
            let hosts_it = for_engine(engine);
            assert!(hosts_it.iter().any(|p| p.id == "kaggle"), "kaggle should host {engine}");
            assert!(hosts_it.iter().any(|p| p.id == "runpod"), "runpod should host {engine}");
        }
        // fal is restricted, and must not claim engines it cannot serve.
        assert!(!hosts(provider("fal").unwrap(), "heartmula"));
        assert!(hosts(provider("fal").unwrap(), "video"));
    }

    /// Reading the provider off a pasted URL is what turns "is RunPod set up?" from a guess into a
    /// fact — nothing else records that a URL is RunPod.
    #[test]
    fn a_url_says_which_provider_it_belongs_to() {
        assert_eq!(provider_of_url("https://abc-8188.proxy.runpod.net"), UrlOrigin::Known("runpod"));
        assert_eq!(provider_of_url("https://me--comfy.modal.run"), UrlOrigin::Known("modal"));
        assert_eq!(provider_of_url("http://12345.vast.ai:8188"), UrlOrigin::Known("vast"));
        assert_eq!(provider_of_url("https://x.lightning.ai/app"), UrlOrigin::Known("lightning"));
        assert_eq!(provider_of_url("http://127.0.0.1:8188"), UrlOrigin::Known("local"));
        assert_eq!(provider_of_url("http://localhost:8188/"), UrlOrigin::Known("local"));
    }

    /// Kaggle and a Lightning studio open the same kind of quick tunnel, so a trycloudflare host
    /// cannot be attributed to either. Saying so beats picking one: a wrong "configured" badge stops
    /// somebody looking for what is actually missing.
    #[test]
    fn a_tunnel_is_reported_as_ambiguous_rather_than_attributed() {
        assert_eq!(provider_of_url("https://angela-craft-icon.trycloudflare.com"), UrlOrigin::Ambiguous);
        assert_eq!(provider_of_url("https://abc.lhr.life"), UrlOrigin::Ambiguous);
    }

    #[test]
    fn an_unrecognised_host_is_not_forced_into_a_provider() {
        assert_eq!(provider_of_url("https://comfy.my-own-server.example"), UrlOrigin::Unknown);
        assert_eq!(provider_of_url(""), UrlOrigin::Empty);
        assert_eq!(provider_of_url("   "), UrlOrigin::Empty);
    }

    /// A hostname that merely *contains* a provider name is not that provider — the check has to be
    /// on the domain boundary, or `runpod.io.evil.example` would authenticate as RunPod.
    #[test]
    fn matching_is_on_the_domain_boundary() {
        assert_eq!(provider_of_url("https://runpod.io.example.com"), UrlOrigin::Unknown);
        assert_eq!(provider_of_url("https://notmodal.run.example.org"), UrlOrigin::Unknown);
        // But a real subdomain does match.
        assert_eq!(provider_of_url("https://a.b.proxy.runpod.net"), UrlOrigin::Known("runpod"));
    }

    /// Every id this can return has to name a real catalogue entry, or the UI matches nothing.
    #[test]
    fn detected_ids_all_exist_in_the_catalogue() {
        for url in ["https://x.proxy.runpod.net", "https://x.modal.run", "https://x.vast.ai",
                    "https://x.lightning.ai", "http://127.0.0.1:8188", "https://x.fal.run",
                    "https://x.replicate.com"] {
            if let UrlOrigin::Known(id) = provider_of_url(url) {
                assert!(provider(id).is_some(), "{url} detected as '{id}', which is not in PROVIDERS");
            } else {
                panic!("{url} should have been recognised");
            }
        }
    }

    /// The ranking the module header argues for: free costs the most machinery, serverless the least.
    #[test]
    fn shapes_line_up_with_how_automated_they_are() {
        let fal = provider("fal").unwrap();
        assert_eq!(fal.shape, Shape::Serverless);
        assert!(fal.automated, "a serverless API needs no per-session step, so it must be automated");
        let vast = provider("vast").unwrap();
        assert_eq!(vast.shape, Shape::RentedBox);
        assert!(!vast.automated, "renting a box is a manual step per session");
    }
}
