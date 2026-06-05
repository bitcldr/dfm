//! The output seam between the engine and the terminal.
//!
//! The engine reports progress through an [`OutputSink`] rather than calling
//! [`IoStreams`](crate::iostreams::IoStreams) directly. This keeps engine
//! tests free of real streams (they use [`NullSink`]) while production wires a
//! sink that forwards to `IoStreams`.

/// Receives engine progress events. All methods default to no-ops so a sink
/// can implement only the events it cares about.
pub trait OutputSink {
    /// A new symlink `from -> to` was created.
    fn linked(&mut self, from: &str, to: &str) {
        let _ = (from, to);
    }
    /// A stale symlink was replaced with `from -> to`.
    fn relinked(&mut self, from: &str, to: &str) {
        let _ = (from, to);
    }
    /// An existing symlink already points at the correct target.
    fn link_ok(&mut self, from: &str, to: &str) {
        let _ = (from, to);
    }
    /// A pre-existing file was moved `from -> to` before linking.
    fn backed_up(&mut self, from: &str, to: &str) {
        let _ = (from, to);
    }
    /// A dangling symlink `from -> to` was removed.
    fn removed_dead_link(&mut self, from: &str, to: &str) {
        let _ = (from, to);
    }
    /// A create target already exists and was skipped.
    fn path_exists(&mut self, path: &str) {
        let _ = path;
    }
    /// A directory was created.
    fn created(&mut self, path: &str) {
        let _ = path;
    }
    /// A shell directive entry is about to run. `quiet` mirrors the entry's
    /// own quiet flag (distinct from stream-level quiet).
    fn shell_cmd(&mut self, description: &str, command: &str, quiet: bool) {
        let _ = (description, command, quiet);
    }
}

/// An [`OutputSink`] that discards every event. Used in tests and when the
/// engine runs without a terminal.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSink;

impl OutputSink for NullSink {}

/// Forwards engine progress events to an [`IoStreams`](crate::iostreams::IoStreams),
/// rendering them as styled, quiet-aware terminal output.
pub struct IoStreamsSink<'a> {
    ios: &'a mut crate::iostreams::IoStreams,
}

impl<'a> IoStreamsSink<'a> {
    /// Wrap a mutable [`IoStreams`](crate::iostreams::IoStreams).
    pub fn new(ios: &'a mut crate::iostreams::IoStreams) -> Self {
        Self { ios }
    }
}

impl OutputSink for IoStreamsSink<'_> {
    fn linked(&mut self, from: &str, to: &str) {
        self.ios.linked(from, to);
    }
    fn relinked(&mut self, from: &str, to: &str) {
        self.ios.relinked(from, to);
    }
    fn link_ok(&mut self, from: &str, to: &str) {
        self.ios.link_ok(from, to);
    }
    fn backed_up(&mut self, from: &str, to: &str) {
        self.ios.backed_up(from, to);
    }
    fn removed_dead_link(&mut self, from: &str, to: &str) {
        self.ios.removed_dead_link(from, to);
    }
    fn path_exists(&mut self, path: &str) {
        self.ios.path_exists(path);
    }
    fn created(&mut self, path: &str) {
        self.ios.created(path);
    }
    fn shell_cmd(&mut self, description: &str, command: &str, quiet: bool) {
        self.ios.shell_cmd(description, command, quiet);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_sink_accepts_all_events() {
        let mut s = NullSink;
        s.linked("a", "b");
        s.relinked("a", "b");
        s.link_ok("a", "b");
        s.backed_up("a", "b");
        s.removed_dead_link("a", "b");
        s.path_exists("a");
        s.created("a");
        s.shell_cmd("desc", "cmd", true);
    }
}
