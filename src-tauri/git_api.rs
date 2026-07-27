//! Git, as operations rather than as command lines.
//!
//! Every git call in this app used to be `git(dir, &["some", "argv"])`, which works perfectly on a
//! desktop and cannot work at all on a phone: Android ships no `git` binary and no shell to run one
//! in. Twenty-six call sites all funnelled through that one function, so the fix is to make the
//! *operation* the interface and let each platform answer it its own way.
//!
//! ```text
//! desktop   shell out to git, exactly as before  — nothing changes, including LFS and credentials
//! mobile    libgit2 through the git2 crate       — pure Rust bindings, no binary, no shell
//! ```
//!
//! # Why a typed API rather than a string translator
//!
//! A "parse the argv and call libgit2" layer would have to understand every flag the app might grow,
//! and would fail at runtime on the one nobody tested. A named operation cannot: `Commit`, `AddAll`,
//! `Status`, `Log`, `Clone`, `Pull`, `Push`, `CheckoutPaths` are what this app actually does, and if a
//! platform cannot do one of them it says so at the point of use, in a sentence.
//!
//! # What is honestly not settled
//!
//! The mobile implementation is written but **not yet verified against a real Android build** — the
//! Android target has never compiled past `ring`'s NDK wiring (see TODOS task 9). `git2` needs the
//! same C toolchain. Rather than pretend, `Backend::describe()` reports which implementation is
//! compiled in, and the mobile paths return a plain explanation rather than a panic if libgit2 is
//! unavailable.

use std::path::Path;

pub type Res<T> = Result<T, String>;

/// What this app asks git to do. Nothing here is a flag; everything is an intention.
#[derive(Debug, Clone, PartialEq)]
pub enum Op<'a> {
    /// Create a repository, with the identity this app commits under.
    Init,
    /// Stage everything.
    AddAll,
    /// Stage one path.
    Add(&'a str),
    /// Commit what is staged. `None` when there was nothing to commit.
    Commit(&'a str),
    /// Porcelain status, one line per changed path.
    Status,
    /// `--cached --name-only`: what a commit would include.
    Staged,
    /// The short hash of HEAD.
    Head,
    /// The last `n` commits, as `hash\tiso-date\tsubject`.
    Log(usize),
    /// Clone `url` into `dir/name`.
    Clone { url: &'a str, name: &'a str },
    Pull,
    Push { remote: &'a str, branch: &'a str },
    /// Restore paths from a commit — the rewind path.
    CheckoutPaths { commit: &'a str, paths: &'a [&'a str] },
    /// Take one side of a conflicted file.
    TakeSide { ours: bool, file: &'a str },
    /// The contents of a file at a stage or a commit (`git show <rev>:<file>`).
    Show { rev: &'a str, file: &'a str },
    RemoteGet,
    RemoteSet(&'a str),
    /// How many commits are waiting to be pushed.
    Ahead,
}

/// Which implementation this build carries.
pub fn describe() -> &'static str {
    #[cfg(desktop)]
    { "the git command on this machine" }
    #[cfg(all(not(desktop), feature = "mobile-git"))]
    { "libgit2, built into the app" }
    #[cfg(all(not(desktop), not(feature = "mobile-git")))]
    { "nothing — this build has no git" }
}

/// Run one operation.
pub fn run(dir: &Path, op: Op<'_>) -> Res<String> {
    #[cfg(desktop)]
    { desktop::run(dir, op) }
    #[cfg(all(not(desktop), feature = "mobile-git"))]
    { mobile::run(dir, op) }
    #[cfg(all(not(desktop), not(feature = "mobile-git")))]
    {
        let _ = (dir, op);
        Err("This build has no git, so project history is not available here. The JSON files are \
             still the real data and still sync.".into())
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Desktop: the command line, unchanged
// ────────────────────────────────────────────────────────────────────────────

#[cfg(desktop)]
mod desktop {
    use super::*;

    /// The argv one operation becomes. Pure, so the translation is testable on any platform.
    pub fn argv<'a>(op: &Op<'a>) -> Vec<Vec<String>> {
        let one = |v: Vec<&str>| vec![v.into_iter().map(String::from).collect::<Vec<_>>()];
        match op {
            // Identity is set at init rather than relied on from the machine: a user with no global
            // git config otherwise gets "Please tell me who you are" on their first save.
            Op::Init => vec![
                vec!["init".into()],
                vec!["config".into(), "user.name".into(), "Studio Lightkid".into()],
                vec!["config".into(), "user.email".into(), "studio@lightkid.local".into()],
            ],
            Op::AddAll => one(vec!["add", "-A"]),
            Op::Add(path) => one(vec!["add", "--", path]),
            Op::Commit(message) => one(vec!["commit", "-m", message]),
            Op::Status => one(vec!["status", "--porcelain"]),
            Op::Staged => one(vec!["diff", "--cached", "--name-only"]),
            Op::Head => one(vec!["rev-parse", "--short", "HEAD"]),
            Op::Log(n) => vec![vec![
                "log".into(), format!("-{}", n.max(&1)),
                "--pretty=format:%h\t%aI\t%s".into(),
            ]],
            Op::Clone { url, name } => one(vec!["clone", url, name]),
            Op::Pull => one(vec!["pull", "--no-rebase", "--no-edit"]),
            Op::Push { remote, branch } => one(vec!["push", "-u", remote, branch]),
            Op::CheckoutPaths { commit, paths } => {
                let mut v: Vec<String> = vec!["checkout".into(), (*commit).into(), "--".into()];
                v.extend(paths.iter().map(|p| (*p).to_string()));
                vec![v]
            }
            Op::TakeSide { ours, file } => one(vec![
                "checkout", if *ours { "--ours" } else { "--theirs" }, "--", file]),
            Op::Show { rev, file } => vec![vec!["show".into(), format!("{rev}:{file}")]],
            Op::RemoteGet => one(vec!["remote", "get-url", "origin"]),
            Op::RemoteSet(url) => one(vec!["remote", "set-url", "origin", url]),
            Op::Ahead => one(vec!["rev-list", "--count", "@{u}..HEAD"]),
        }
    }

    pub fn run(dir: &Path, op: Op<'_>) -> Res<String> {
        let mut last = String::new();
        for args in argv(&op) {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            match crate::project_sync::git(dir, &refs) {
                Ok(out) => last = out,
                // `init` sets identity afterwards, and a failure there must not fail the init.
                Err(err) if matches!(op, Op::Init) && refs.first().copied() == Some("config") => {
                    let _ = err;
                }
                Err(err) => return Err(err),
            }
        }
        Ok(last)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Mobile: libgit2
// ────────────────────────────────────────────────────────────────────────────

#[cfg(all(not(desktop), feature = "mobile-git"))]
mod mobile {
    use super::*;
    use git2::{Repository, Signature};

    fn open(dir: &Path) -> Res<Repository> {
        Repository::open(dir).map_err(|e| format!("this folder is not a repository: {e}"))
    }

    fn who() -> Res<Signature<'static>> {
        Signature::now("Studio Lightkid", "studio@lightkid.local").map_err(|e| e.to_string())
    }

    pub fn run(dir: &Path, op: Op<'_>) -> Res<String> {
        match op {
            Op::Init => { Repository::init(dir).map_err(|e| e.to_string())?; Ok(String::new()) }
            Op::AddAll => {
                let repo = open(dir)?;
                let mut index = repo.index().map_err(|e| e.to_string())?;
                index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
                    .map_err(|e| e.to_string())?;
                index.write().map_err(|e| e.to_string())?;
                Ok(String::new())
            }
            Op::Add(path) => {
                let repo = open(dir)?;
                let mut index = repo.index().map_err(|e| e.to_string())?;
                index.add_path(Path::new(path)).map_err(|e| e.to_string())?;
                index.write().map_err(|e| e.to_string())?;
                Ok(String::new())
            }
            Op::Commit(message) => {
                let repo = open(dir)?;
                let mut index = repo.index().map_err(|e| e.to_string())?;
                let tree_id = index.write_tree().map_err(|e| e.to_string())?;
                let tree = repo.find_tree(tree_id).map_err(|e| e.to_string())?;
                let sig = who()?;
                let parents: Vec<git2::Commit> = match repo.head().ok().and_then(|h| h.target()) {
                    Some(oid) => vec![repo.find_commit(oid).map_err(|e| e.to_string())?],
                    None => Vec::new(),
                };
                let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
                let oid = repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
                    .map_err(|e| e.to_string())?;
                Ok(oid.to_string())
            }
            Op::Status => {
                let repo = open(dir)?;
                let statuses = repo.statuses(None).map_err(|e| e.to_string())?;
                Ok(statuses.iter()
                    .filter_map(|s| s.path().map(|p| format!(" M {p}")))
                    .collect::<Vec<_>>().join("\n"))
            }
            Op::Staged => {
                let repo = open(dir)?;
                let head = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
                let diff = repo.diff_tree_to_index(head.as_ref(), None, None)
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                diff.foreach(&mut |delta, _| {
                    if let Some(p) = delta.new_file().path() { out.push(p.display().to_string()); }
                    true
                }, None, None, None).map_err(|e| e.to_string())?;
                Ok(out.join("\n"))
            }
            Op::Head => {
                let repo = open(dir)?;
                let oid = repo.head().ok().and_then(|h| h.target())
                    .ok_or("this repository has no commits yet")?;
                Ok(oid.to_string()[..7].to_string())
            }
            Op::Log(n) => {
                let repo = open(dir)?;
                let mut walk = repo.revwalk().map_err(|e| e.to_string())?;
                walk.push_head().map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                for oid in walk.take(n.max(1)) {
                    let Ok(oid) = oid else { continue };
                    let Ok(commit) = repo.find_commit(oid) else { continue };
                    let when = chrono::DateTime::from_timestamp(commit.time().seconds(), 0)
                        .map(|d| d.to_rfc3339()).unwrap_or_default();
                    out.push(format!("{}\t{when}\t{}", &oid.to_string()[..7],
                                     commit.summary().unwrap_or("")));
                }
                Ok(out.join("\n"))
            }
            Op::Clone { url, name } => {
                git2::build::RepoBuilder::new().clone(url, &dir.join(name))
                    .map_err(|e| e.to_string())?;
                Ok(String::new())
            }
            Op::Pull => Err("Pulling on a phone is not supported yet — open the project on a \
                             desktop to merge someone else's changes.".into()),
            Op::Push { remote, branch } => {
                let repo = open(dir)?;
                let mut r = repo.find_remote(remote).map_err(|e| e.to_string())?;
                r.push(&[&format!("refs/heads/{branch}:refs/heads/{branch}")], None)
                    .map_err(|e| e.to_string())?;
                Ok(String::new())
            }
            Op::CheckoutPaths { commit, paths } => {
                let repo = open(dir)?;
                let object = repo.revparse_single(commit).map_err(|e| e.to_string())?;
                let mut builder = git2::build::CheckoutBuilder::new();
                builder.force();
                for p in paths { builder.path(p); }
                repo.checkout_tree(&object, Some(&mut builder)).map_err(|e| e.to_string())?;
                Ok(String::new())
            }
            Op::TakeSide { ours, file } => {
                let repo = open(dir)?;
                let mut index = repo.index().map_err(|e| e.to_string())?;
                let stage = if ours { 2 } else { 3 };
                let entry = index.get_path(Path::new(file), stage)
                    .ok_or("that file is not conflicted")?;
                let blob = repo.find_blob(entry.id).map_err(|e| e.to_string())?;
                std::fs::write(dir.join(file), blob.content()).map_err(|e| e.to_string())?;
                index.add_path(Path::new(file)).map_err(|e| e.to_string())?;
                index.write().map_err(|e| e.to_string())?;
                Ok(String::new())
            }
            Op::Show { rev, file } => {
                let repo = open(dir)?;
                let object = repo.revparse_single(&format!("{rev}:{file}"))
                    .map_err(|e| e.to_string())?;
                let blob = object.as_blob().ok_or("that path is not a file")?;
                Ok(String::from_utf8_lossy(blob.content()).to_string())
            }
            Op::RemoteGet => {
                let repo = open(dir)?;
                Ok(repo.find_remote("origin").map_err(|e| e.to_string())?
                    .url().unwrap_or("").to_string())
            }
            Op::RemoteSet(url) => {
                let repo = open(dir)?;
                repo.remote_set_url("origin", url).map_err(|e| e.to_string())?;
                Ok(String::new())
            }
            Op::Ahead => {
                let repo = open(dir)?;
                let local = repo.head().ok().and_then(|h| h.target()).ok_or("no commits yet")?;
                let upstream = repo.revparse_single("@{u}").ok().map(|o| o.id());
                match upstream {
                    Some(up) => {
                        let (ahead, _) = repo.graph_ahead_behind(local, up)
                            .map_err(|e| e.to_string())?;
                        Ok(ahead.to_string())
                    }
                    None => Ok("0".into()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(desktop)]
    #[test]
    fn every_operation_becomes_the_command_it_always_was() {
        // This is a refactor, so the desktop behaviour must be byte-identical to what these call
        // sites sent before — a subtly different `git add` is a subtly different history.
        use desktop::argv;
        let flat = |op: Op| argv(&op).into_iter().map(|v| v.join(" ")).collect::<Vec<_>>();

        assert_eq!(flat(Op::AddAll), vec!["add -A"]);
        assert_eq!(flat(Op::Add("data/songs.json")), vec!["add -- data/songs.json"]);
        assert_eq!(flat(Op::Commit("data: a message")), vec!["commit -m data: a message"]);
        assert_eq!(flat(Op::Status), vec!["status --porcelain"]);
        assert_eq!(flat(Op::Staged), vec!["diff --cached --name-only"]);
        assert_eq!(flat(Op::Head), vec!["rev-parse --short HEAD"]);
        assert_eq!(flat(Op::Pull), vec!["pull --no-rebase --no-edit"]);
        assert_eq!(flat(Op::RemoteGet), vec!["remote get-url origin"]);
        assert_eq!(flat(Op::Ahead), vec!["rev-list --count @{u}..HEAD"]);
        assert_eq!(flat(Op::TakeSide { ours: true, file: "a.json" }),
                   vec!["checkout --ours -- a.json"]);
        assert_eq!(flat(Op::TakeSide { ours: false, file: "a.json" }),
                   vec!["checkout --theirs -- a.json"]);
        assert_eq!(flat(Op::Show { rev: ":2", file: "a.json" }), vec!["show :2:a.json"]);
        assert_eq!(flat(Op::CheckoutPaths { commit: "abc123", paths: &["."] }),
                   vec!["checkout abc123 -- ."]);
    }

    #[cfg(desktop)]
    #[test]
    fn init_sets_an_identity_so_a_first_save_cannot_fail_for_lack_of_one() {
        // A user with no global git config otherwise meets "Please tell me who you are" on their
        // very first save, which is a terrible first experience of a feature that is meant to be
        // invisible.
        let steps = desktop::argv(&Op::Init);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0], vec!["init"]);
        assert!(steps[1].contains(&"user.name".to_string()));
        assert!(steps[2].contains(&"user.email".to_string()));
    }

    #[cfg(desktop)]
    #[test]
    fn a_log_asks_for_a_parseable_shape_and_never_for_zero_commits() {
        let steps = desktop::argv(&Op::Log(5));
        assert!(steps[0].contains(&"-5".to_string()));
        assert!(steps[0].iter().any(|a| a.contains("%h\t%aI\t%s")), "{steps:?}");
        // `-0` would silently return everything.
        assert!(desktop::argv(&Op::Log(0))[0].contains(&"-1".to_string()));
    }

    #[test]
    fn the_build_says_which_git_it_has_rather_than_assuming() {
        let d = describe();
        assert!(!d.is_empty());
        #[cfg(desktop)]
        assert!(d.contains("command"));
    }

    #[cfg(desktop)]
    #[test]
    fn a_real_repository_survives_the_whole_round_trip() {
        // The operations are only worth anything if they compose: init, add, commit, log, head.
        let dir = std::env::temp_dir().join(format!("lightkid-git-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        if !crate::project_sync::git_available() { return; }

        run(&dir, Op::Init).expect("init");
        std::fs::write(dir.join("a.json"), b"{}").unwrap();
        run(&dir, Op::Add("a.json")).expect("add");
        assert!(run(&dir, Op::Staged).unwrap().contains("a.json"));
        run(&dir, Op::Commit("data: the first save")).expect("commit");

        let head = run(&dir, Op::Head).expect("head");
        assert!(head.trim().len() >= 7, "{head:?}");
        let log = run(&dir, Op::Log(5)).expect("log");
        assert!(log.contains("the first save"), "{log}");
        assert_eq!(log.lines().count(), 1);
        // A clean tree reports nothing, which is what the autosave sweep checks.
        assert!(run(&dir, Op::Status).unwrap().trim().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
