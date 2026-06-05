//! Verifies `IoStreams` routing (data→out, `progress/diagnostics→err_out`) and
//! quiet gating. Uses a shared buffer so the boxed writers can be inspected
//! after the fact.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use dfm::iostreams::{ApplyResult, IoStreams};

/// A cloneable writer backed by a shared `Vec<u8>`.
#[derive(Clone)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl SharedBuf {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }
    fn contents(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Build an `IoStreams` over two inspectable buffers (color off).
fn streams() -> (IoStreams, SharedBuf, SharedBuf) {
    let out = SharedBuf::new();
    let err = SharedBuf::new();
    let ios = IoStreams::with_writers(out.clone(), err.clone(), false, false);
    (ios, out, err)
}

#[test]
fn progress_goes_to_err_out() {
    let (mut ios, out, err) = streams();
    ios.linked("~/.zshrc", "/repo/zshrc");
    assert!(err.contents().contains("Linked"));
    assert!(out.contents().is_empty(), "progress must not touch stdout");
}

#[test]
fn data_goes_to_out() {
    let (mut ios, out, err) = streams();
    ios.profile_list(&["base".to_string(), "macos".to_string()]);
    assert!(out.contents().contains("Profiles:"));
    assert!(out.contents().contains("base"));
    assert!(err.contents().is_empty(), "data must not touch stderr");
}

#[test]
fn diagnostics_go_to_err_out() {
    let (mut ios, out, err) = streams();
    ios.doctor_done(3, 1);
    assert!(err.contents().contains("Doctor:"));
    assert!(out.contents().is_empty());
}

#[test]
fn quiet_suppresses_progress_only() {
    let (mut ios, _out, err) = streams();
    ios.set_quiet(true);
    ios.linked("a", "b"); // progress — suppressed
    ios.done(false, ApplyResult::default()); // progress — suppressed
    assert!(
        err.contents().is_empty(),
        "progress should be silenced when quiet"
    );

    ios.doctor_fail("no state"); // diagnostic — still visible
    assert!(
        err.contents().contains("no state"),
        "diagnostics survive quiet"
    );
}

#[test]
fn quiet_does_not_affect_data() {
    let (mut ios, out, _err) = streams();
    ios.set_quiet(true);
    ios.status_line("State file:", "/x/state.json");
    assert!(
        out.contents().contains("/x/state.json"),
        "data output flows even when quiet"
    );
}

#[test]
fn done_line_has_all_counts() {
    let (mut ios, _out, err) = streams();
    ios.done(
        false,
        ApplyResult {
            links_ok: 2,
            created: 1,
            relinked: 0,
            backed_up: 0,
            shell_run: 3,
            shell_failed: 1,
            cleaned: 0,
            dirs: 4,
        },
    );
    let line = err.contents();
    assert!(line.contains("Done:"));
    for token in [
        "ok",
        "created",
        "relinked",
        "backed up",
        "shell",
        "cleaned",
        "dirs",
    ] {
        assert!(line.contains(token), "summary missing {token:?}: {line}");
    }
    assert!(line.contains("(1 failed)"), "shell failure suffix: {line}");
}

#[test]
fn color_off_emits_no_ansi() {
    let (mut ios, _out, err) = streams();
    ios.linked("a", "b");
    assert!(
        !err.contents().contains('\u{1b}'),
        "no ESC when color disabled"
    );
}

#[test]
fn color_on_emits_ansi() {
    let out = SharedBuf::new();
    let err = SharedBuf::new();
    let mut ios = IoStreams::with_writers(out.clone(), err.clone(), false, true);
    ios.linked("a", "b");
    assert!(
        err.contents().contains('\u{1b}'),
        "ESC present when err color on"
    );
}

#[test]
fn discard_drops_everything() {
    let mut ios = IoStreams::discard();
    // Should not panic and should write nowhere observable.
    ios.linked("a", "b");
    ios.profile_list(&["x".to_string()]);
    ios.doctor_fail("y");
}
