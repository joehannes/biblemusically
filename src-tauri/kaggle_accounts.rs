//! Which Kaggle account an engine should run on.
//!
//! # Why this is a decision rather than a setting
//!
//! Kaggle meters two different things per account, and the app used to model neither. There is the
//! **weekly GPU quota** (~30 h), and there is the **number of GPU sessions an account may run at
//! once**, which is small. An engine is a long-lived server: once started it holds its session until
//! Kaggle's ~9-12 h batch limit. So "run the music engine and the image engine at the same time" is
//! not one account doing two things — on one account it is the second start being refused, or
//! silently taking the first one's slot.
//!
//! The app already had the two halves of the answer and never joined them up. It could rotate to
//! another account, but only *after* a run had failed, and only ever to the **next** one in the list
//! — with no idea whether that account had any quota either. Rotating from an exhausted account to
//! another exhausted account is a fair description of what that did, and it cost eight minutes per
//! attempt to discover.
//!
//! So: ask every connected account what it has left, ask the app which engines are already parked on
//! each, and choose. The choosing is a pure function over those facts, which is why it is separated
//! from the fetching — the policy is the part worth testing, and it needs no network to test.
//!
//! # The rules, and why each one is there
//!
//! * **Sticky first.** An engine that is already assigned to an account stays there when that account
//!   is still viable. Each account owns a *separate kernel* (`<owner>/biblemusically-<engine>-server`)
//!   with its own installed packages and its own downloaded checkpoints, so moving an engine to a
//!   different account is not free — it is a fresh ~8-10 minute cold boot. Never pay that to satisfy
//!   a tie-break.
//! * **Only accounts with useful quota.** A few minutes left is not enough to boot an engine, let
//!   alone serve from it; offering such an account is how somebody spends ten minutes to be told the
//!   quota ran out mid-run.
//! * **Only accounts with a free session slot.** The engine's own current slot does not count against
//!   it, because restarting an engine replaces that session rather than adding one.
//! * **Then the most quota left.** Spreading engines across accounts is the point of having them.
//! * **Never invent.** If nothing could be asked — no network, every token stale — the caller's
//!   current account is returned unchanged. A guess dressed as a choice would send a run to an
//!   account the user never picked, which is precisely the class of bug the identity reconciliation
//!   work was cleaning up.

use serde::Serialize;

/// How many engines the app will park on one account at a time.
///
/// Kaggle's concurrent-GPU-session allowance for a free account is small and has never been formally
/// published; two is what the app can rely on. Being wrong low costs a spread-out engine, being
/// wrong high costs a refused push after an eight-minute wait — so this errs low deliberately.
pub const MAX_ENGINES_PER_ACCOUNT: usize = 2;

/// Below this, starting an engine is not worth the wait.
///
/// A cold engine boot is ~8-10 minutes of install and checkpoint download before it serves anything,
/// and that time is billed against the same quota. An account with less than this cannot finish
/// booting, so sending a run there produces a failure that looks exactly like the one the user is
/// already trying to escape.
pub const MIN_USEFUL_MINUTES: i64 = 20;

/// What is known about one connected account at the moment a start is being decided.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AccountFitness {
    pub username: String,
    pub left_minutes: i64,
    pub allowed_minutes: i64,
    /// When the weekly window resets, as Kaggle stated it.
    pub resets_at: String,
    /// Engines currently assigned to this account, in the app's own bookkeeping.
    pub engines: Vec<String>,
    /// False when Kaggle could not be asked — a stale token, or no network. Distinguished from
    /// "asked, and the answer was zero", because the two call for opposite handling.
    pub quota_known: bool,
    /// Why the quota is unknown, when it is. Empty otherwise.
    pub note: String,
}

impl AccountFitness {
    /// Session slots this account has spare, not counting the engine being started.
    ///
    /// Restarting an engine that already lives here replaces its session instead of adding one, so
    /// its own slot must not be counted against it — otherwise the second start of the same engine
    /// on a two-slot account would be pushed to a different account for no reason, paying a full
    /// cold boot to move somewhere it did not need to go.
    fn slots_free_for(&self, engine: &str) -> usize {
        let taken = self.engines.iter().filter(|e| e.as_str() != engine).count();
        MAX_ENGINES_PER_ACCOUNT.saturating_sub(taken)
    }

    /// Could this account actually carry `engine` right now?
    pub fn can_host(&self, engine: &str) -> bool {
        self.quota_known && self.left_minutes >= MIN_USEFUL_MINUTES && self.slots_free_for(engine) > 0
    }
}

/// Why a particular account was chosen, in the words the interface should use.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Choice {
    /// Stay where this engine already is.
    Keep { username: String, reason: String },
    /// Move it to a different account.
    Switch { username: String, from: String, reason: String },
    /// Nothing is viable, and the reason is worth saying precisely.
    None { reason: String },
}

impl Choice {
    pub fn username(&self) -> Option<&str> {
        match self {
            Choice::Keep { username, .. } | Choice::Switch { username, .. } => Some(username),
            Choice::None { .. } => None,
        }
    }
}

/// Pick the account `engine` should start on.
///
/// `assigned` is the account this engine last ran on, if any; `active` is whoever the machine is
/// signed in as now. Pure on purpose — every input is a fact someone else fetched.
pub fn choose(fits: &[AccountFitness], engine: &str, assigned: &str, active: &str) -> Choice {
    if fits.is_empty() {
        return Choice::None { reason: "No Kaggle account is connected.".into() };
    }

    // Sticky: this engine's own account, when it can still host it. Cheapest possible answer —
    // its checkpoints are already downloaded there.
    if let Some(f) = fits.iter().find(|f| f.username == assigned) {
        if f.can_host(engine) {
            return Choice::Keep {
                username: f.username.clone(),
                reason: format!("{} has {} GPU minutes left and already hosts {}.",
                                f.username, f.left_minutes, engine),
            };
        }
    }

    // Otherwise the account with the most left, among those that can take it. Ties break on the
    // emptiest account and then on name, so the same inputs always give the same answer.
    let best = fits.iter()
        .filter(|f| f.can_host(engine))
        .max_by(|a, b| a.left_minutes.cmp(&b.left_minutes)
            .then_with(|| b.engines.len().cmp(&a.engines.len()))
            .then_with(|| b.username.cmp(&a.username)));

    let Some(best) = best else {
        // Say which wall was hit. "No account available" is true of an exhausted quota and of an
        // account that is merely busy, and those have different answers — wait a week, or stop an
        // engine.
        let any_known = fits.iter().any(|f| f.quota_known);
        let all_busy = fits.iter().filter(|f| f.quota_known)
            .all(|f| f.left_minutes >= MIN_USEFUL_MINUTES && f.slots_free_for(engine) == 0);
        return Choice::None {
            reason: if !any_known {
                "Kaggle could not be asked about any connected account — check the network and that \
                 the stored API tokens are still valid.".into()
            } else if all_busy {
                format!("Every connected account already runs {MAX_ENGINES_PER_ACCOUNT} engines. \
                         Stop one, or connect another free account.")
            } else {
                let soonest = fits.iter().filter(|f| f.quota_known && !f.resets_at.is_empty())
                    .map(|f| f.resets_at.as_str()).min().unwrap_or("");
                format!("Every connected account is out of free GPU time for this week{}.",
                        if soonest.is_empty() { String::new() } else { format!(" (the first resets {soonest})") })
            },
        };
    };

    if best.username == active && (assigned.is_empty() || assigned == active) {
        return Choice::Keep {
            username: best.username.clone(),
            reason: format!("{} has the most GPU time left ({} minutes).", best.username, best.left_minutes),
        };
    }
    Choice::Switch {
        username: best.username.clone(),
        from: if assigned.is_empty() { active.to_string() } else { assigned.to_string() },
        reason: format!("{} has {} GPU minutes left and {} of {} engine slots free.",
                        best.username, best.left_minutes,
                        best.slots_free_for(engine), MAX_ENGINES_PER_ACCOUNT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acct(name: &str, left: i64, engines: &[&str]) -> AccountFitness {
        AccountFitness {
            username: name.into(), left_minutes: left, allowed_minutes: 1800,
            resets_at: "2026-08-22T00:00:00Z".into(),
            engines: engines.iter().map(|s| s.to_string()).collect(),
            quota_known: true, note: String::new(),
        }
    }

    /// The whole point of the feature: a second engine goes to the other account by itself, rather
    /// than being refused a session slot on the first.
    #[test]
    fn a_second_engine_lands_on_the_other_account() {
        let fits = vec![acct("one", 600, &["acestep", "comfyui"]), acct("two", 600, &[])];
        let c = choose(&fits, "heartmula", "", "one");
        assert_eq!(c.username(), Some("two"), "{c:?}");
        assert!(matches!(c, Choice::Switch { .. }));
    }

    /// Moving an engine costs a full cold boot — its checkpoints live on the account it ran on. So a
    /// viable current home always wins, even when another account has more quota.
    #[test]
    fn an_engine_stays_where_its_checkpoints_are() {
        let fits = vec![acct("one", 100, &["heartmula"]), acct("two", 1700, &[])];
        let c = choose(&fits, "heartmula", "one", "one");
        assert_eq!(c.username(), Some("one"), "{c:?}");
        assert!(matches!(c, Choice::Keep { .. }));
    }

    /// …but not when staying would fail: too little quota left to finish booting.
    #[test]
    fn a_nearly_empty_account_is_abandoned_even_if_it_is_home() {
        let fits = vec![acct("one", MIN_USEFUL_MINUTES - 1, &["heartmula"]), acct("two", 500, &[])];
        assert_eq!(choose(&fits, "heartmula", "one", "one").username(), Some("two"));
    }

    /// Restarting an engine must not push it off its own account: its existing session is replaced,
    /// not added to. Counting it would evict the engine from a full account it already lives on.
    #[test]
    fn restarting_an_engine_does_not_count_against_its_own_account() {
        let fits = vec![acct("one", 900, &["heartmula", "acestep"])];
        let c = choose(&fits, "heartmula", "one", "one");
        assert_eq!(c.username(), Some("one"), "{c:?}");
        // A *third*, different engine on the same full account is refused instead.
        assert_eq!(choose(&fits, "comfyui", "", "one").username(), None);
    }

    /// An account Kaggle could not be asked about is never chosen — an unknown is not a zero, and
    /// picking on a guess is how a run lands somewhere the user did not choose.
    #[test]
    fn an_unreachable_account_is_not_guessed_at() {
        let mut dark = acct("two", 0, &[]);
        dark.quota_known = false;
        dark.note = "token rejected".into();
        let fits = vec![acct("one", 900, &[]), dark];
        assert_eq!(choose(&fits, "heartmula", "", "one").username(), Some("one"));
    }

    /// Every account out of quota is a different problem from every account being busy, and the
    /// message has to say which — one is solved by waiting, the other by stopping an engine.
    #[test]
    fn the_refusal_names_the_wall_that_was_hit() {
        let empty = vec![acct("one", 0, &[]), acct("two", 5, &[])];
        match choose(&empty, "heartmula", "", "one") {
            Choice::None { reason } => assert!(reason.contains("out of free GPU time"), "{reason}"),
            other => panic!("expected None, got {other:?}"),
        }
        let busy = vec![acct("one", 900, &["a", "b"]), acct("two", 900, &["c", "d"])];
        match choose(&busy, "heartmula", "", "one") {
            Choice::None { reason } => assert!(reason.contains("Stop one"), "{reason}"),
            other => panic!("expected None, got {other:?}"),
        }
    }

    /// With nothing connected there is no choice to make, and the message must not imply there was.
    #[test]
    fn no_accounts_at_all_says_so() {
        match choose(&[], "heartmula", "", "") {
            Choice::None { reason } => assert!(reason.contains("No Kaggle account"), "{reason}"),
            other => panic!("expected None, got {other:?}"),
        }
    }

    /// The same facts must always produce the same account, or a retry loop could ping-pong an
    /// engine between two equally good accounts, paying a cold boot each time.
    #[test]
    fn equal_accounts_resolve_the_same_way_every_time() {
        let fits = vec![acct("alpha", 900, &[]), acct("beta", 900, &[])];
        let first = choose(&fits, "heartmula", "", "alpha");
        for _ in 0..5 {
            assert_eq!(choose(&fits, "heartmula", "", "alpha"), first);
        }
    }
}
