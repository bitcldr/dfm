//! End-to-end engine behavior against a sandboxed filesystem.
//!
//! Each test drives `Engine::apply` over a parsed profile in a tempdir and
//! asserts both the resulting filesystem state and the recorded action tally.

use std::fs;
use std::path::Path;

use dfm::config::parse_str;
use dfm::engine::{ActionKind, Engine};

/// Build a repo dir with the given relative files, return its path.
fn repo_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, contents) in files {
        let p = dir.path().join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, contents).unwrap();
    }
    dir
}

fn read_link(p: &Path) -> String {
    fs::read_link(p).unwrap().to_string_lossy().into_owned()
}

#[test]
fn creates_symlink_into_repo() {
    let repo = repo_with(&[("zshrc", "# zsh")]);
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join(".zshrc");

    let cfg = parse_str(&format!("link:\n  {}: zshrc\n", target.display())).unwrap();
    let mut eng = Engine::new(repo.path());
    let tally = eng.apply(&cfg).unwrap();

    assert_eq!(tally.links_created, 1);
    assert_eq!(
        read_link(&target),
        repo.path().join("zshrc").to_string_lossy()
    );
}

#[test]
fn idempotent_when_already_correct() {
    let repo = repo_with(&[("zshrc", "# zsh")]);
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join(".zshrc");
    let cfg = parse_str(&format!("link:\n  {}: zshrc\n", target.display())).unwrap();

    let mut eng = Engine::new(repo.path());
    eng.apply(&cfg).unwrap();
    // second apply on a fresh engine: should be a no-op
    let mut eng2 = Engine::new(repo.path());
    let tally = eng2.apply(&cfg).unwrap();
    assert_eq!(tally.links_ok, 1, "already-correct link is an ok no-op");
    assert_eq!(tally.links_created, 0);
}

#[test]
fn relinks_stale_symlink_when_relink_true() {
    let repo = repo_with(&[("a", "A"), ("b", "B")]);
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join(".x");
    // pre-existing symlink pointing at the wrong source
    std::os::unix::fs::symlink(repo.path().join("a"), &target).unwrap();

    let cfg = parse_str(&format!(
        "link:\n  {}:\n    path: b\n    relink: true\n",
        target.display()
    ))
    .unwrap();
    let mut eng = Engine::new(repo.path());
    let tally = eng.apply(&cfg).unwrap();

    assert_eq!(tally.links_relinked, 1);
    assert_eq!(read_link(&target), repo.path().join("b").to_string_lossy());
}

#[test]
fn skips_stale_symlink_without_relink() {
    let repo = repo_with(&[("a", "A"), ("b", "B")]);
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join(".x");
    std::os::unix::fs::symlink(repo.path().join("a"), &target).unwrap();

    let cfg = parse_str(&format!("link:\n  {}: b\n", target.display())).unwrap();
    let mut eng = Engine::new(repo.path());
    let tally = eng.apply(&cfg).unwrap();

    assert_eq!(tally.links_relinked, 0);
    assert_eq!(tally.links_created, 0);
    // original link untouched
    assert_eq!(read_link(&target), repo.path().join("a").to_string_lossy());
    // and a skip action was recorded
    assert!(eng.actions.iter().any(|a| a.kind == ActionKind::LinkSkip));
}

#[test]
fn backs_up_non_symlink_target() {
    let repo = repo_with(&[("zshrc", "# new")]);
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join(".zshrc");
    fs::write(&target, b"# pre-existing real file").unwrap();

    let cfg = parse_str(&format!("link:\n  {}: zshrc\n", target.display())).unwrap();
    let mut eng = Engine::new(repo.path()).with_home(home.path());
    eng.set_backup_tag("testtag");
    let tally = eng.apply(&cfg).unwrap();

    assert_eq!(tally.links_backed_up, 1);
    assert_eq!(tally.links_created, 1);
    // target is now a symlink into the repo
    assert_eq!(
        read_link(&target),
        repo.path().join("zshrc").to_string_lossy()
    );
    // the backup exists and preserved the original contents
    let backup_root = home.path().join(".dotfiles-backup").join("testtag");
    let backed = backup_root.join(target.strip_prefix("/").unwrap());
    assert_eq!(
        fs::read_to_string(backed).unwrap(),
        "# pre-existing real file"
    );
}

#[test]
fn creates_directories() {
    let repo = repo_with(&[]);
    let home = tempfile::tempdir().unwrap();
    let d1 = home.path().join("a/b");
    let cfg = parse_str(&format!("create:\n  - {}\n", d1.display())).unwrap();
    let mut eng = Engine::new(repo.path());
    let tally = eng.apply(&cfg).unwrap();
    assert_eq!(tally.created, 1);
    assert!(d1.is_dir());

    // re-apply: idempotent, no new creates
    let mut eng2 = Engine::new(repo.path());
    let tally2 = eng2.apply(&cfg).unwrap();
    assert_eq!(tally2.created, 0);
    assert!(
        eng2.actions
            .iter()
            .any(|a| a.kind == ActionKind::CreateExists)
    );
}

#[test]
fn cleans_dead_link_into_base() {
    let repo = repo_with(&[]);
    let scan = tempfile::tempdir().unwrap();
    // a dangling symlink pointing into the repo (target doesn't exist)
    let dead = scan.path().join("dead");
    std::os::unix::fs::symlink(repo.path().join("gone"), &dead).unwrap();

    let cfg = parse_str(&format!("clean:\n  - {}\n", scan.path().display())).unwrap();
    let mut eng = Engine::new(repo.path());
    let tally = eng.apply(&cfg).unwrap();

    assert_eq!(tally.cleaned, 1);
    assert!(
        !dead.exists() && fs::symlink_metadata(&dead).is_err(),
        "dead link removed"
    );
}

#[test]
fn leaves_dead_link_pointing_outside_base() {
    let repo = repo_with(&[]);
    let other = tempfile::tempdir().unwrap();
    let scan = tempfile::tempdir().unwrap();
    let dead = scan.path().join("dead");
    // dangling link pointing OUTSIDE the repo → must be left alone
    std::os::unix::fs::symlink(other.path().join("gone"), &dead).unwrap();

    let cfg = parse_str(&format!("clean:\n  - {}\n", scan.path().display())).unwrap();
    let mut eng = Engine::new(repo.path());
    let tally = eng.apply(&cfg).unwrap();

    assert_eq!(tally.cleaned, 0, "links outside base dir are not cleaned");
    assert!(fs::symlink_metadata(&dead).is_ok(), "link preserved");
}

#[test]
fn dry_run_plans_without_mutating() {
    let repo = repo_with(&[("zshrc", "# zsh")]);
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join(".zshrc");
    let cfg = parse_str(&format!("link:\n  {}: zshrc\n", target.display())).unwrap();

    let mut eng = Engine::new(repo.path()).dry_run(true);
    let tally = eng.apply(&cfg).unwrap();

    // planned (tally + action recorded) but NOT created on disk
    assert_eq!(tally.links_created, 1);
    assert!(
        fs::symlink_metadata(&target).is_err(),
        "dry-run must not create the link"
    );
    let a = eng
        .actions
        .iter()
        .find(|a| a.kind == ActionKind::LinkCreate)
        .unwrap();
    assert!(a.dry_run, "action marked dry-run");
}

#[test]
fn dry_run_does_not_count_shell_run() {
    let repo = repo_with(&[]);
    let cfg = parse_str("shell:\n  - name: noop\n    script: 'true'\n").unwrap();
    let mut eng = Engine::new(repo.path()).dry_run(true);
    let tally = eng.apply(&cfg).unwrap();
    // The shell action is recorded, but shell_run is NOT incremented in dry-run.
    assert_eq!(tally.shell_run, 0, "dry-run must not count shell_run");
    assert!(eng.actions.iter().any(|a| a.kind == ActionKind::ShellRun));
}

#[test]
fn shell_runs_and_counts() {
    let repo = repo_with(&[]);
    let marker = repo.path().join("ran");
    let cfg = parse_str(&format!(
        "shell:\n  - name: touch marker\n    script: 'touch {}'\n",
        marker.display()
    ))
    .unwrap();
    let mut eng = Engine::new(repo.path());
    let tally = eng.apply(&cfg).unwrap();
    assert_eq!(tally.shell_run, 1);
    assert_eq!(tally.shell_failed, 0);
    assert!(marker.exists(), "shell command ran in base dir");
}

#[test]
fn shell_failure_is_tallied_not_fatal() {
    let repo = repo_with(&[]);
    let cfg = parse_str("shell:\n  - name: fail\n    script: 'exit 3'\n").unwrap();
    let mut eng = Engine::new(repo.path());
    let tally = eng.apply(&cfg).unwrap();
    assert_eq!(tally.shell_run, 1);
    assert_eq!(tally.shell_failed, 1);
}

#[test]
fn skip_shell_runs_links_but_not_shell() {
    let repo = repo_with(&[("zshrc", "# zsh")]);
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join(".zshrc");
    let marker = repo.path().join("should_not_exist");

    // a profile with BOTH a link and a shell command
    let cfg = parse_str(&format!(
        "link:\n  {}: zshrc\nshell:\n  - name: touch\n    script: 'touch {}'\n",
        target.display(),
        marker.display()
    ))
    .unwrap();

    let mut eng = Engine::new(repo.path()).skip_shell(true);
    let tally = eng.apply(&cfg).unwrap();

    // the link ran...
    assert_eq!(tally.links_created, 1);
    assert_eq!(
        read_link(&target),
        repo.path().join("zshrc").to_string_lossy()
    );
    // ...but the shell directive was skipped entirely: not run, not recorded.
    assert_eq!(tally.shell_run, 0);
    assert!(
        !marker.exists(),
        "shell command must not run under skip_shell"
    );
    assert!(
        !eng.actions.iter().any(|a| a.kind == ActionKind::ShellRun),
        "skipped shell is not recorded as an action"
    );
}

// ── edge / destructive-path coverage ───────────────────────────────────────

#[test]
fn link_create_makes_parent_dir() {
    let repo = repo_with(&[("conf", "x")]);
    let home = tempfile::tempdir().unwrap();
    // target's parent (.config/deep) does not exist yet
    let target = home.path().join(".config/deep/conf");
    let cfg = parse_str(&format!(
        "link:\n  {}:\n    path: conf\n    create: true\n",
        target.display()
    ))
    .unwrap();
    let mut eng = Engine::new(repo.path());
    let tally = eng.apply(&cfg).unwrap();
    assert_eq!(tally.links_created, 1);
    assert!(target.parent().unwrap().is_dir(), "parent dir created");
    assert_eq!(
        read_link(&target),
        repo.path().join("conf").to_string_lossy()
    );
}

#[test]
fn link_ignore_missing_creates_dangling_link_for_absent_source() {
    let repo = repo_with(&[]);
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join(".x");
    // source "gone" does not exist; ignore-missing skips the existence check
    // and creates the link anyway (dotbot-compatible) rather than erroring.
    let cfg = parse_str(&format!(
        "link:\n  {}:\n    path: gone\n    ignore-missing: true\n",
        target.display()
    ))
    .unwrap();
    let mut eng = Engine::new(repo.path());
    let tally = eng.apply(&cfg).unwrap();
    // The symlink is created pointing at the absent source, and nothing errors.
    assert_eq!(tally.links_created, 1);
    assert!(fs::symlink_metadata(&target).is_ok());
}

#[test]
fn link_glob_errors_as_unsupported() {
    let repo = repo_with(&[("a", "x")]);
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join(".x");
    // glob: true with glob chars in the source is an explicit error contract.
    let cfg = parse_str(&format!(
        "link:\n  {}:\n    path: 'conf/*'\n    glob: true\n",
        target.display()
    ))
    .unwrap();
    let mut eng = Engine::new(repo.path());
    let tally = eng.apply(&cfg).unwrap();
    // the entry fails (logged warning), so no link is created
    assert_eq!(tally.links_created, 0);
    assert!(fs::symlink_metadata(&target).is_err());
}

#[test]
fn link_relative_stores_relative_target() {
    let repo = repo_with(&[("conf", "x")]);
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join(".conf");
    let cfg = parse_str(&format!(
        "link:\n  {}:\n    path: conf\n    relative: true\n",
        target.display()
    ))
    .unwrap();
    let mut eng = Engine::new(repo.path());
    let tally = eng.apply(&cfg).unwrap();
    assert_eq!(tally.links_created, 1);
    // the stored link text must be relative, not absolute
    let dest = read_link(&target);
    assert!(
        !dest.starts_with('/'),
        "relative symlink should not be absolute: {dest}"
    );
    // and it must still resolve to the repo file
    assert!(target.exists(), "relative link resolves to the source");
}

#[test]
fn backs_up_existing_directory_target() {
    let repo = repo_with(&[("nvim/init.lua", "-- new")]);
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join(".config/nvim");
    // pre-existing DIRECTORY (not a file) at the target, with content
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("old.lua"), b"-- old").unwrap();

    let cfg = parse_str(&format!(
        "link:\n  {}:\n    path: nvim\n    create: true\n",
        target.display()
    ))
    .unwrap();
    let mut eng = Engine::new(repo.path()).with_home(home.path());
    eng.set_backup_tag("dirtag");
    let tally = eng.apply(&cfg).unwrap();

    assert_eq!(tally.links_backed_up, 1);
    assert_eq!(tally.links_created, 1);
    // target is now a symlink into the repo
    assert_eq!(
        read_link(&target),
        repo.path().join("nvim").to_string_lossy()
    );
    // the backed-up directory kept its contents
    let backed = home
        .path()
        .join(".dotfiles-backup/dirtag")
        .join(target.strip_prefix("/").unwrap());
    assert_eq!(
        fs::read_to_string(backed.join("old.lua")).unwrap(),
        "-- old"
    );
}

#[test]
fn clean_recursive_descends_subdirectories() {
    let repo = repo_with(&[]);
    let scan = tempfile::tempdir().unwrap();
    let sub = scan.path().join("a/b");
    fs::create_dir_all(&sub).unwrap();
    // dead link nested two levels down, pointing into the base repo
    let dead = sub.join("dead");
    std::os::unix::fs::symlink(repo.path().join("gone"), &dead).unwrap();

    let cfg = parse_str(&format!(
        "clean:\n  {}:\n    recursive: true\n",
        scan.path().display()
    ))
    .unwrap();
    let mut eng = Engine::new(repo.path());
    let tally = eng.apply(&cfg).unwrap();
    assert_eq!(
        tally.cleaned, 1,
        "recursive clean reaches nested dead links"
    );
    assert!(fs::symlink_metadata(&dead).is_err());
}

#[test]
fn clean_force_removes_outside_link_when_scan_under_home() {
    // force removes a dead link pointing OUTSIDE the base dir — allowed because
    // the scan dir resolves under the (injected) home. This exercises the
    // actual removal path, not the refusal path.
    let repo = repo_with(&[]);
    let home = tempfile::tempdir().unwrap();
    let scan = home.path().join("scan");
    fs::create_dir_all(&scan).unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let dead = scan.join("dead");
    std::os::unix::fs::symlink(elsewhere.path().join("gone"), &dead).unwrap();

    let cfg = parse_str(&format!("clean:\n  {}:\n    force: true\n", scan.display())).unwrap();
    // inject home so the scan dir counts as "under $HOME" deterministically
    let mut eng = Engine::new(repo.path()).with_home(home.path());
    let tally = eng.apply(&cfg).unwrap();

    assert_eq!(
        tally.cleaned, 1,
        "force removes outside-pointing dead link under home"
    );
    assert!(fs::symlink_metadata(&dead).is_err(), "dead link removed");
}

#[test]
fn clean_force_refused_when_scan_outside_home() {
    // The same force clean is refused when the scan dir is NOT under home —
    // the link pointing outside the base is preserved.
    let repo = repo_with(&[]);
    let home = tempfile::tempdir().unwrap();
    let scan = tempfile::tempdir().unwrap(); // sibling of home, not under it
    let elsewhere = tempfile::tempdir().unwrap();
    let dead = scan.path().join("dead");
    std::os::unix::fs::symlink(elsewhere.path().join("gone"), &dead).unwrap();

    let cfg = parse_str(&format!(
        "clean:\n  {}:\n    force: true\n",
        scan.path().display()
    ))
    .unwrap();
    let mut eng = Engine::new(repo.path()).with_home(home.path());
    let tally = eng.apply(&cfg).unwrap();

    assert_eq!(tally.cleaned, 0, "force refused: scan dir is outside home");
    assert!(fs::symlink_metadata(&dead).is_ok(), "link preserved");
}

#[test]
fn clean_force_recursion_is_depth_capped() {
    // A dead link nested deeper than the force depth cap is left alone, even
    // though force+recursive would otherwise remove it. Links above the cap are
    // still removed, proving the cap bounds descent rather than disabling it.
    let repo = repo_with(&[]);
    let home = tempfile::tempdir().unwrap();
    let scan = home.path().join("scan");

    // shallow dead link (depth 1) — within the cap, should be removed
    fs::create_dir_all(&scan).unwrap();
    let shallow = scan.join("shallow");
    std::os::unix::fs::symlink(home.path().join("gone-shallow"), &shallow).unwrap();

    // deep dead link past MAX_FORCE_DEPTH (=5): scan/d1/.../d7/deep
    let mut deep_dir = scan.clone();
    for i in 1..=7 {
        deep_dir = deep_dir.join(format!("d{i}"));
    }
    fs::create_dir_all(&deep_dir).unwrap();
    let deep = deep_dir.join("deep");
    std::os::unix::fs::symlink(home.path().join("gone-deep"), &deep).unwrap();

    let cfg = parse_str(&format!(
        "clean:\n  {}:\n    force: true\n    recursive: true\n",
        scan.display()
    ))
    .unwrap();
    let mut eng = Engine::new(repo.path()).with_home(home.path());
    let tally = eng.apply(&cfg).unwrap();

    assert!(
        fs::symlink_metadata(&shallow).is_err(),
        "shallow dead link removed"
    );
    assert!(
        fs::symlink_metadata(&deep).is_ok(),
        "dead link past depth cap is left alone"
    );
    assert_eq!(tally.cleaned, 1, "only the within-cap link was cleaned");
}

#[test]
fn force_relinks_stale_symlink_like_relink() {
    // M5 contract: `force: true` behaves like `relink: true` for a stale link.
    let repo = repo_with(&[("a", "A"), ("b", "B")]);
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join(".x");
    std::os::unix::fs::symlink(repo.path().join("a"), &target).unwrap();

    let cfg = parse_str(&format!(
        "link:\n  {}:\n    path: b\n    force: true\n",
        target.display()
    ))
    .unwrap();
    let mut eng = Engine::new(repo.path());
    let tally = eng.apply(&cfg).unwrap();
    assert_eq!(
        tally.links_relinked, 1,
        "force alone relinks a stale symlink"
    );
    assert_eq!(read_link(&target), repo.path().join("b").to_string_lossy());
}
