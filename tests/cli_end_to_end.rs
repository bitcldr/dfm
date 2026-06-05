//! End-to-end CLI tests driving the compiled `dfm` binary.
//!
//! Each test runs against a sandboxed repo + HOME and uses a profile WITHOUT
//! shell directives, so nothing executes against the real system.

use std::fs;
use std::path::Path;
use std::process::Command;

use assert_cmd::cargo::CommandCargoExt;

/// A sandbox with a repo (source files + one shell-free profile) and a HOME.
struct Sandbox {
    _dir: tempfile::TempDir,
    repo: std::path::PathBuf,
    home: std::path::PathBuf,
    state: std::path::PathBuf,
}

fn sandbox() -> Sandbox {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let home = dir.path().join("home");
    let state = dir.path().join("state");
    fs::create_dir_all(repo.join("profiles")).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::write(repo.join("zshrc"), b"# zshrc").unwrap();
    fs::create_dir_all(repo.join("nvim")).unwrap();
    fs::write(repo.join("nvim/init.lua"), b"-- init").unwrap();
    fs::write(
        repo.join("profiles/test.conf.yaml"),
        "defaults:\n  link:\n    relink: true\ncreate:\n  - ~/.config\nlink:\n  ~/.zshrc: zshrc\n  ~/.config/nvim: nvim\n",
    )
    .unwrap();
    Sandbox {
        _dir: dir,
        repo,
        home,
        state,
    }
}

/// A `dfm` command wired to the sandbox env.
fn dfm(sb: &Sandbox) -> Command {
    let mut cmd = Command::cargo_bin("dfm").unwrap();
    cmd.env("HOME", &sb.home)
        .env("XDG_STATE_HOME", &sb.state)
        .env_remove("NO_COLOR")
        .arg("-C")
        .arg(&sb.repo);
    cmd
}

fn read_link(p: &Path) -> String {
    fs::read_link(p).unwrap().to_string_lossy().into_owned()
}

#[test]
fn apply_creates_links_and_state() {
    let sb = sandbox();
    let out = dfm(&sb).args(["apply", "test"]).output().unwrap();
    assert!(
        out.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // symlinks exist and point into the repo
    assert_eq!(
        read_link(&sb.home.join(".zshrc")),
        sb.repo.join("zshrc").to_string_lossy()
    );
    assert_eq!(
        read_link(&sb.home.join(".config/nvim")),
        sb.repo.join("nvim").to_string_lossy()
    );

    // state file written
    assert!(sb.state.join("dfm/state.json").exists());
}

#[test]
fn diff_writes_to_stdout_and_changes_nothing() {
    let sb = sandbox();
    let out = dfm(&sb).args(["diff", "test"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Links to create"), "diff stdout: {stdout}");
    // diff must not create anything
    assert!(fs::symlink_metadata(sb.home.join(".zshrc")).is_err());
}

#[test]
fn doctor_passes_after_apply_and_fails_on_drift() {
    let sb = sandbox();
    dfm(&sb).args(["apply", "test"]).output().unwrap();

    // clean doctor → exit 0
    let ok = dfm(&sb).arg("doctor").output().unwrap();
    assert!(ok.status.success(), "doctor should pass after apply");

    // break a link → doctor exits non-zero
    fs::remove_file(sb.home.join(".zshrc")).unwrap();
    let bad = dfm(&sb).arg("doctor").output().unwrap();
    assert!(
        !bad.status.success(),
        "doctor should fail on a missing link"
    );
}

#[test]
fn status_reports_last_applied() {
    let sb = sandbox();
    dfm(&sb).args(["apply", "test"]).output().unwrap();
    let out = dfm(&sb).arg("status").output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Last applied:"), "status stdout: {stdout}");
    assert!(stdout.contains("test"));
}

#[test]
fn list_outputs_profile_names_to_stdout() {
    let sb = sandbox();
    let out = dfm(&sb).arg("list").output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("test"));
}

#[test]
fn completion_emits_script_to_stdout() {
    let sb = sandbox();
    for shell in ["bash", "zsh", "fish"] {
        let out = dfm(&sb).args(["completion", shell]).output().unwrap();
        assert!(out.status.success(), "completion {shell} failed");
        assert!(
            !out.stdout.is_empty(),
            "completion {shell} produced no script"
        );
    }
    // unknown shell → error
    let bad = dfm(&sb).args(["completion", "elvish"]).output().unwrap();
    assert!(!bad.status.success());
}

#[test]
fn dry_run_apply_changes_nothing() {
    let sb = sandbox();
    let out = dfm(&sb)
        .args(["apply", "--dry-run", "test"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        fs::symlink_metadata(sb.home.join(".zshrc")).is_err(),
        "dry-run must not link"
    );
    // no state file written on dry-run
    assert!(!sb.state.join("dfm/state.json").exists());
}

#[test]
fn no_shell_applies_links_but_skips_shell() {
    let sb = sandbox();
    // a profile that links AND runs a shell command writing a marker
    let marker = sb.repo.join("shell_ran");
    fs::write(
        sb.repo.join("profiles/withshell.conf.yaml"),
        format!(
            "link:\n  ~/.zshrc: zshrc\nshell:\n  - name: marker\n    script: 'touch {}'\n",
            marker.display()
        ),
    )
    .unwrap();

    let out = dfm(&sb)
        .args(["apply", "--no-shell", "withshell"])
        .output()
        .unwrap();
    assert!(out.status.success());

    // link created, shell skipped
    assert!(
        fs::symlink_metadata(sb.home.join(".zshrc")).is_ok(),
        "link should be applied"
    );
    assert!(!marker.exists(), "--no-shell must not run shell directives");
}

#[test]
fn missing_profile_is_an_error() {
    let sb = sandbox();
    let out = dfm(&sb).args(["apply", "nonexistent"]).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not found"));
}

#[test]
fn exit_codes_match_contract() {
    let sb = sandbox();
    // --help → 0
    assert!(dfm(&sb).arg("--help").output().unwrap().status.success());
    // bare (no subcommand) → non-zero
    assert!(
        !Command::cargo_bin("dfm")
            .unwrap()
            .output()
            .unwrap()
            .status
            .success()
    );
    // unknown flag → non-zero
    assert!(!dfm(&sb).arg("--nope").output().unwrap().status.success());
}

#[test]
fn verbose_and_quiet_are_mutually_exclusive() {
    let sb = sandbox();
    let out = dfm(&sb)
        .args(["--verbose", "--quiet", "list"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("mutually exclusive"));
}
