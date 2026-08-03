//! Noticing that a setup cannot work, and saying so before it is run.
//!
//! # Why this is rules and not a language model
//!
//! Every question here has an arithmetic answer. Does 20 GB of weights fit in 16 GB of VRAM? Does
//! twelve clips at fourteen minutes each fit in what is left of a thirty-hour weekly quota? Is the
//! chosen engine the one whose server is actually running? A model asked these questions would
//! usually get them right, which is the problem — "usually" is the wrong reliability for a warning
//! whose entire value is that it can be trusted, and this one has to fire on the free tier, offline,
//! before any budget is spent. The AI budget in this app is fifty requests a day; spending them
//! re-deriving multiplication would be a poor trade.
//!
//! So the *detection* is deterministic and the *wording* is written out in full. What the app gets
//! from that is a warning that is never wrong, never costs a request, and works on a plane.
//!
//! # What makes a hint worth showing
//!
//! Each one names the thing that will fail, and the specific alternative that would not. A warning
//! without a remedy is just anxiety — "this may not work" tells somebody nothing they can act on,
//! whereas "the 14B needs 24 GB; the 1.3B does the same job on your T4 at 480p" is a decision they
//! can take in one step. Every hint below carries a `fix` for that reason, and the `apply` field
//! carries the preset id when the remedy is something the app can simply do.

use crate::video_models::{self as vm, PresetPack, Tier, VideoPreset};
use serde::Serialize;

/// How much attention a hint deserves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Level {
    /// Worth knowing, nothing is broken.
    Note,
    /// This will work but will cost more than the user probably expects.
    Warn,
    /// This cannot work as configured. Running it wastes the quota it spends before failing.
    Blocker,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hint {
    pub level: Level,
    /// Stable identifier, so the UI can dismiss one kind of hint without dismissing all of them.
    pub code: &'static str,
    /// What is wrong, in one sentence.
    pub problem: String,
    /// What to do instead, naming something specific.
    pub fix: String,
    /// A preset id the app can switch to on the user's behalf, when the remedy is that concrete.
    pub apply: Option<String>,
}

impl Hint {
    fn new(level: Level, code: &'static str, problem: String, fix: String) -> Self {
        Hint { level, code, problem, fix, apply: None }
    }
    fn applying(mut self, preset_id: &str) -> Self { self.apply = Some(preset_id.to_string()); self }
}

/// Everything the advisor needs to know about the current setup.
///
/// Passed in rather than read from the store so the rules stay testable without a database, a Kaggle
/// account or a network — which is what makes it affordable to have this many of them.
#[derive(Debug, Clone)]
pub struct Setup {
    pub tier: Tier,
    /// The preset the user has selected, if any.
    pub preset: Option<String>,
    /// The pack they have selected, if any.
    pub pack: Option<String>,
    /// How many clips they intend to make in this sitting.
    pub planned_clips: u32,
    /// GPU minutes left in the weekly quota, if known. Kaggle gives 30 h per account per week.
    pub quota_minutes_left: Option<u32>,
    /// How many Kaggle accounts are connected — the app rotates when one runs dry.
    pub connected_accounts: u32,
    /// Whether the video server is up and answering.
    pub server_live: bool,
    /// Free disk on the render host, if known. Kaggle gives ~57 GB of working space.
    pub disk_gb_free: Option<f32>,
}

impl Default for Setup {
    fn default() -> Self {
        Setup { tier: Tier::FreeT4, preset: None, pack: None, planned_clips: 1,
                quota_minutes_left: None, connected_accounts: 1, server_live: false,
                disk_gb_free: None }
    }
}

/// Kaggle's free weekly GPU allowance, per account.
pub const WEEKLY_QUOTA_MINUTES: u32 = 30 * 60;

/// Look at a setup and say what will go wrong with it.
///
/// Ordered most severe first, because the UI shows the top of this list and a blocker underneath two
/// notes is a blocker nobody reads.
pub fn advise(s: &Setup) -> Vec<Hint> {
    let mut out = Vec::new();
    let chosen = s.preset.as_deref().and_then(vm::preset);

    check_preset_fits_hardware(s, chosen, &mut out);
    check_pack_fits_hardware(s, &mut out);
    check_quota(s, chosen, &mut out);
    check_disk(s, &mut out);
    check_server(s, &mut out);
    check_accounts(s, &mut out);
    check_tier_is_worth_it(s, chosen, &mut out);

    out.sort_by(|a, b| b.level.cmp(&a.level));
    out
}

/// The one that matters most: a model that cannot load on this hardware.
fn check_preset_fits_hardware(s: &Setup, chosen: Option<&VideoPreset>, out: &mut Vec<Hint>) {
    let Some(p) = chosen else { return };
    let Some(m) = vm::model_of(p) else { return };
    if m.runs_on(s.tier) { return; }

    // Name the specific thing that does the same job here, rather than saying "pick something else".
    let fix = match vm::best_for(p.purpose, s.tier) {
        Some(alt) => {
            let am = vm::model_of(alt).unwrap();
            format!(
                "{} does the same job on this hardware — {}×{}, about {}-{} min a clip. \
                 {} needs {} to load at all.",
                alt.label, alt.width, alt.height,
                am.minutes_per_clip.0, am.minutes_per_clip.1,
                m.label, m.min_tier.label(),
            )
        }
        None => format!("Nothing in the catalogue covers that purpose on {}.", s.tier.label()),
    };
    let hint = Hint::new(
        Level::Blocker, "preset_too_big",
        format!("{} needs about {} GB of VRAM and this setup has {} — it will fail while loading, \
                 after the session has already spent time installing.",
                m.label, m.vram_gb, s.tier.vram_gb()),
        fix,
    );
    out.push(match vm::best_for(p.purpose, s.tier) {
        Some(alt) => hint.applying(alt.id),
        None => hint,
    });
}

fn check_pack_fits_hardware(s: &Setup, out: &mut Vec<Hint>) {
    let Some(pk) = s.pack.as_deref().and_then(vm::pack) else { return };
    let unusable: Vec<&PresetPack> = if pack_runs(pk, s.tier) { Vec::new() } else { vec![pk] };
    if unusable.is_empty() { return; }

    let alternative = vm::packs_for_tier(s.tier).first().map(|p| p.label).unwrap_or("a smaller pack");
    out.push(Hint::new(
        Level::Blocker, "pack_too_big",
        format!("The “{}” package contains presets this hardware cannot load.", pk.label),
        format!("Use “{alternative}” instead, or move this engine to {} or better.",
                Tier::Pro24.label()),
    ));
}

fn pack_runs(pk: &PresetPack, tier: Tier) -> bool {
    pk.presets.iter().all(|pid|
        vm::preset(pid).and_then(vm::model_of).map(|m| m.runs_on(tier)).unwrap_or(false))
}

/// Quota is the constraint people discover last and most expensively.
fn check_quota(s: &Setup, chosen: Option<&VideoPreset>, out: &mut Vec<Hint>) {
    let Some(p) = chosen else { return };
    let Some(m) = vm::model_of(p) else { return };
    let clips = s.planned_clips.max(1);
    let (lo, hi) = (m.minutes_per_clip.0 * clips, m.minutes_per_clip.1 * clips);

    let Some(left) = s.quota_minutes_left else {
        // Even without a live figure, an ask that exceeds a whole week is worth stopping.
        if lo > WEEKLY_QUOTA_MINUTES {
            out.push(cheaper_hint(s, p, m, clips, lo, hi, WEEKLY_QUOTA_MINUTES, "a full weekly quota"));
        }
        return;
    };

    if lo > left {
        out.push(cheaper_hint(s, p, m, clips, lo, hi, left, "what is left this week"));
    } else if hi > left {
        out.push(Hint::new(
            Level::Warn, "quota_tight",
            format!("{clips} clips on {} take {lo}-{hi} min and about {left} min of GPU quota remain \
                     — the slower end of that does not fit.", m.label),
            if s.connected_accounts > 1 {
                format!("The app rotates to another connected account when this one runs dry, so this \
                         will most likely finish. Reduce to {} clips to stay inside one account.",
                        (left / m.minutes_per_clip.1.max(1)).max(1))
            } else {
                "Connect a second free Kaggle account so the app can rotate when this one runs dry, \
                 or generate fewer clips in this sitting.".to_string()
            },
        ));
    }
}

fn cheaper_hint(
    s: &Setup, p: &VideoPreset, m: &vm::VideoModel,
    clips: u32, lo: u32, hi: u32, budget: u32, budget_label: &str,
) -> Hint {
    // The cheapest preset serving the same purpose — which is what "do less" concretely means.
    let cheaper = vm::presets_for(p.purpose).into_iter()
        .filter(|c| vm::model_of(c).map(|cm| cm.runs_on(s.tier)).unwrap_or(false))
        .min_by_key(|c| vm::model_of(c).unwrap().minutes_per_clip.1);

    let fix = match cheaper.filter(|c| c.id != p.id) {
        Some(c) => {
            let cm = vm::model_of(c).unwrap();
            format!("{} covers the same purpose at {}-{} min a clip — {clips} of them is about {} min.",
                    c.label, cm.minutes_per_clip.0, cm.minutes_per_clip.1,
                    cm.minutes_per_clip.1 * clips)
        }
        None => format!("Generate about {} clips instead.",
                        (budget / m.minutes_per_clip.1.max(1)).max(1)),
    };
    let hint = Hint::new(
        Level::Blocker, "quota_exceeded",
        format!("{clips} clips on {} need {lo}-{hi} min of GPU time, more than {budget_label} \
                 ({budget} min).", m.label),
        fix,
    );
    match cheaper.filter(|c| c.id != p.id) { Some(c) => hint.applying(c.id), None => hint }
}

/// Kaggle gives about 57 GB of working space, and the weights are most of it.
fn check_disk(s: &Setup, out: &mut Vec<Hint>) {
    let Some(pk) = s.pack.as_deref() else { return };
    let need = vm::pack_download_gb(pk);
    if need <= 0.0 { return; }
    let Some(free) = s.disk_gb_free else { return };
    if need + 8.0 <= free { return; }
    out.push(Hint::new(
        Level::Blocker, "disk_short",
        format!("This package downloads about {need:.0} GB and only {free:.0} GB is free — the \
                 download will fail partway, which looks like a network error rather than a full disk."),
        "Pick a package with fewer models, or start a fresh session — Kaggle's working directory \
         keeps whatever the last run left behind.".to_string(),
    ));
}

fn check_server(s: &Setup, out: &mut Vec<Hint>) {
    if s.server_live { return; }
    out.push(Hint::new(
        Level::Note, "server_down",
        "The video server is not answering yet.".to_string(),
        "Press Start & connect. First run on an account takes ~10 min: it installs ComfyUI, downloads \
         the package's models, and opens a tunnel.".to_string(),
    ));
}

fn check_accounts(s: &Setup, out: &mut Vec<Hint>) {
    if s.connected_accounts >= 2 || s.tier > Tier::DualT4 { return; }
    out.push(Hint::new(
        Level::Note, "one_account",
        "Only one Kaggle account is connected, and free GPU quota is per account.".to_string(),
        "Connect a second free account in Settings → Kaggle accounts. The app rotates to it \
         automatically when the first is denied a GPU, which is the difference between a run \
         stopping and a run continuing.".to_string(),
    ));
}

/// The other direction: hardware being under-used.
fn check_tier_is_worth_it(s: &Setup, chosen: Option<&VideoPreset>, out: &mut Vec<Hint>) {
    let Some(p) = chosen else { return };
    let Some(m) = vm::model_of(p) else { return };
    if s.tier <= Tier::DualT4 { return; }
    let Some(best) = vm::best_for(p.purpose, s.tier) else { return };
    if best.id == p.id { return; }
    let bm = vm::model_of(best).unwrap();
    if bm.min_tier <= m.min_tier { return; }
    out.push(Hint::new(
        Level::Note, "tier_underused",
        format!("This hardware can run {}, and {} is selected.", bm.label, m.label),
        format!("Switch to “{}” for a noticeably better clip at {}-{} min instead of {}-{}.",
                best.label, bm.minutes_per_clip.0, bm.minutes_per_clip.1,
                m.minutes_per_clip.0, m.minutes_per_clip.1),
    ).applying(best.id));
}

/// The single most useful sentence about a setup, for a status line that has room for one.
pub fn headline(hints: &[Hint]) -> Option<&Hint> { hints.first() }

/// Is this setup runnable at all?
pub fn is_blocked(hints: &[Hint]) -> bool { hints.iter().any(|h| h.level == Level::Blocker) }

#[cfg(test)]
mod tests {
    use super::*;

    fn free(preset: &str) -> Setup {
        Setup { tier: Tier::FreeT4, preset: Some(preset.into()), server_live: true, ..Default::default() }
    }

    #[test]
    fn a_model_too_big_for_the_hardware_is_a_blocker_with_a_named_alternative() {
        let hints = advise(&free("hero_wide_hq"));
        let h = hints.iter().find(|h| h.code == "preset_too_big").expect("should be caught");
        assert_eq!(h.level, Level::Blocker);
        // The remedy has to be something the app can actually do, not advice.
        let alt = h.apply.as_deref().expect("must name a preset to switch to");
        assert!(vm::preset(alt).is_some());
        assert!(vm::model_of(vm::preset(alt).unwrap()).unwrap().runs_on(Tier::FreeT4),
                "the suggested alternative must itself run on this hardware");
    }

    #[test]
    fn a_preset_that_fits_raises_no_blocker() {
        let hints = advise(&free("broll_wide"));
        assert!(!is_blocked(&hints), "{hints:#?}");
    }

    #[test]
    fn asking_for_more_than_the_quota_is_stopped_before_it_starts() {
        let s = Setup { planned_clips: 40, quota_minutes_left: Some(120), ..free("hero_wide") };
        let hints = advise(&s);
        let h = hints.iter().find(|h| h.code == "quota_exceeded").expect("40 hero clips cannot fit");
        assert_eq!(h.level, Level::Blocker);
        // It must point at something cheaper that serves the same purpose.
        assert!(h.fix.contains("min a clip") || h.fix.contains("clips instead"), "{}", h.fix);
    }

    #[test]
    fn a_tight_but_possible_quota_is_a_warning_not_a_blocker() {
        // 3 clips on Wan 1.3B: 24-48 min. With 30 left, the fast end fits and the slow end does not.
        let s = Setup { planned_clips: 3, quota_minutes_left: Some(30), ..free("hero_wide") };
        let hints = advise(&s);
        assert!(hints.iter().any(|h| h.code == "quota_tight" && h.level == Level::Warn), "{hints:#?}");
    }

    #[test]
    fn a_second_account_changes_the_advice_rather_than_the_verdict() {
        let one = Setup { planned_clips: 3, quota_minutes_left: Some(30), ..free("hero_wide") };
        let two = Setup { connected_accounts: 2, ..one.clone() };
        let fix_one = advise(&one).into_iter().find(|h| h.code == "quota_tight").unwrap().fix;
        let fix_two = advise(&two).into_iter().find(|h| h.code == "quota_tight").unwrap().fix;
        assert!(fix_one.contains("Connect a second"));
        assert!(fix_two.contains("rotates"), "with two accounts it should say it will cope: {fix_two}");
    }

    #[test]
    fn a_full_disk_is_reported_as_a_full_disk() {
        let s = Setup { pack: Some("music_video".into()), disk_gb_free: Some(6.0), ..free("broll_wide") };
        let h = advise(&s).into_iter().find(|h| h.code == "disk_short").expect("6 GB is not enough");
        assert_eq!(h.level, Level::Blocker);
    }

    #[test]
    fn plenty_of_disk_says_nothing() {
        let s = Setup { pack: Some("music_video".into()), disk_gb_free: Some(200.0), ..free("broll_wide") };
        assert!(!advise(&s).iter().any(|h| h.code == "disk_short"));
    }

    #[test]
    fn under_used_hardware_is_pointed_out_but_never_blocks() {
        let s = Setup { tier: Tier::Big40, preset: Some("hero_wide".into()), server_live: true,
                        connected_accounts: 2, ..Default::default() };
        let hints = advise(&s);
        let h = hints.iter().find(|h| h.code == "tier_underused").expect("40 GB can do better");
        assert_eq!(h.level, Level::Note);
        assert!(!is_blocked(&hints));
    }

    #[test]
    fn blockers_sort_above_notes() {
        let s = Setup { planned_clips: 99, quota_minutes_left: Some(10), server_live: false,
                        ..free("hero_wide_hq") };
        let hints = advise(&s);
        assert_eq!(headline(&hints).unwrap().level, Level::Blocker,
                   "the most severe hint must be the one a one-line status shows");
    }

    /// A warning with no remedy is just anxiety.
    #[test]
    fn every_hint_names_something_to_do() {
        let setups = [
            free("hero_wide_hq"),
            Setup { planned_clips: 99, quota_minutes_left: Some(5), ..free("hero_wide") },
            Setup { pack: Some("studio_24gb".into()), ..free("broll_wide") },
            Setup { server_live: false, ..free("broll_wide") },
        ];
        for s in setups {
            for h in advise(&s) {
                assert!(h.fix.len() > 20, "hint '{}' has no usable remedy: {:?}", h.code, h.fix);
                assert!(!h.problem.is_empty());
            }
        }
    }

    /// Whatever the advisor offers to switch to must itself be clean on this hardware — otherwise
    /// taking its advice just produces the next warning.
    #[test]
    fn suggested_presets_are_themselves_runnable() {
        for tier in vm::TIERS {
            for p in vm::PRESETS {
                let s = Setup { tier: *tier, preset: Some(p.id.into()), server_live: true,
                                connected_accounts: 2, ..Default::default() };
                for h in advise(&s) {
                    let Some(alt) = h.apply.as_deref() else { continue };
                    let ap = vm::preset(alt).expect("suggested preset must exist");
                    assert!(vm::model_of(ap).unwrap().runs_on(*tier),
                            "on {} the advisor suggests {alt}, which does not run there", tier.id());
                }
            }
        }
    }
}
