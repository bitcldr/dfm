//! State file location, load, and save.
//!
//! Path resolution honors `XDG_STATE_HOME` (falling back to
//! `~/.local/state/dfm/state.json`). An empty `XDG_STATE_HOME` is treated as
//! unset rather than as the current directory.

use std::path::PathBuf;

use super::error::StateError;
use super::schema::State;

/// Reads environment values; injectable so path-precedence tests need no
/// global `set_var` (which is `unsafe` in edition 2024).
pub trait Env {
    /// Look up an environment variable, returning `None` when unset.
    fn var(&self, key: &str) -> Option<String>;
    /// The current user's home directory.
    fn home_dir(&self) -> Option<PathBuf>;
}

/// The real process environment. Public because it appears as the default
/// type parameter of [`Store`].
pub struct SystemEnv;

impl Env for SystemEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
    fn home_dir(&self) -> Option<PathBuf> {
        // Read $HOME directly (not getpwuid) so test `HOME` overrides apply.
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
    }
}

/// A handle for reading and writing state, parameterized over its environment
/// source for testability.
pub struct Store<E: Env = SystemEnv> {
    env: E,
}

impl Default for Store<SystemEnv> {
    fn default() -> Self {
        Self { env: SystemEnv }
    }
}

impl Store<SystemEnv> {
    /// A store backed by the real process environment.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<E: Env> Store<E> {
    /// Build a store over a custom environment (tests).
    pub fn with_env(env: E) -> Self {
        Self { env }
    }

    /// Resolve the absolute path of the state file, honoring `XDG_STATE_HOME`.
    pub fn path(&self) -> Result<PathBuf, StateError> {
        // An empty XDG_STATE_HOME is treated as unset, not as the cwd.
        if let Some(xdg) = self.env.var("XDG_STATE_HOME").filter(|s| !s.is_empty()) {
            return Ok(PathBuf::from(xdg).join("dfm").join("state.json"));
        }
        let home = self.env.home_dir().ok_or(StateError::NoHome)?;
        Ok(home
            .join(".local")
            .join("state")
            .join("dfm")
            .join("state.json"))
    }

    /// Read the state file. A missing file returns `Ok(None)` so callers can
    /// distinguish "never applied" from "corrupt".
    pub fn load(&self) -> Result<Option<State>, StateError> {
        let p = self.path()?;
        let data = match std::fs::read(&p) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(StateError::Read { path: p, source }),
        };
        let state = serde_json::from_slice(&data)
            .map_err(|source| StateError::Parse { path: p, source })?;
        Ok(Some(state))
    }

    /// Write the state file, creating the parent directory as needed. The JSON
    /// is pretty-printed with 2-space indentation.
    pub fn save(&self, state: &State) -> Result<(), StateError> {
        let p = self.path()?;
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(StateError::Mkdir)?;
        }
        let data = serde_json::to_vec_pretty(state).map_err(StateError::Marshal)?;
        std::fs::write(&p, &data).map_err(|source| StateError::Write { path: p, source })
    }
}

// ── free-function façade over the default-environment store ────────────────

/// The state file path, using the real environment.
pub fn path() -> Result<PathBuf, StateError> {
    Store::new().path()
}

/// Load state using the real environment.
pub fn load() -> Result<Option<State>, StateError> {
    Store::new().load()
}

/// Save state using the real environment.
pub fn save(state: &State) -> Result<(), StateError> {
    Store::new().save(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::schema::Link;
    use std::collections::HashMap;

    /// A fake environment driven by a map + an explicit home.
    struct FakeEnv {
        vars: HashMap<String, String>,
        home: Option<PathBuf>,
    }

    impl Env for FakeEnv {
        fn var(&self, key: &str) -> Option<String> {
            self.vars.get(key).cloned()
        }
        fn home_dir(&self) -> Option<PathBuf> {
            self.home.clone()
        }
    }

    fn store_with(vars: &[(&str, &str)], home: Option<&str>) -> Store<FakeEnv> {
        Store::with_env(FakeEnv {
            vars: vars
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            home: home.map(PathBuf::from),
        })
    }

    #[test]
    fn path_uses_xdg_state_home_when_set() {
        let s = store_with(&[("XDG_STATE_HOME", "/xdg")], Some("/home/u"));
        assert_eq!(s.path().unwrap(), PathBuf::from("/xdg/dfm/state.json"));
    }

    #[test]
    fn path_falls_back_to_home_when_xdg_unset() {
        let s = store_with(&[], Some("/home/u"));
        assert_eq!(
            s.path().unwrap(),
            PathBuf::from("/home/u/.local/state/dfm/state.json")
        );
    }

    #[test]
    fn path_empty_xdg_falls_through_to_home() {
        // The empty-string trap: empty XDG_STATE_HOME must be treated as unset.
        let s = store_with(&[("XDG_STATE_HOME", "")], Some("/home/u"));
        assert_eq!(
            s.path().unwrap(),
            PathBuf::from("/home/u/.local/state/dfm/state.json")
        );
    }

    #[test]
    fn path_no_home_errors() {
        let s = store_with(&[], None);
        assert!(matches!(s.path(), Err(StateError::NoHome)));
    }

    #[test]
    fn load_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let s = store_with(&[("XDG_STATE_HOME", dir.path().to_str().unwrap())], None);
        assert_eq!(s.load().unwrap(), None);
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let s = store_with(&[("XDG_STATE_HOME", dir.path().to_str().unwrap())], None);
        let state = State {
            last_applied: vec!["base".into(), "macos".into()],
            applied_at: crate::state::schema::epoch_sentinel(),
            links: vec![Link {
                target: "~/.zshrc".into(),
                source: "/repo/zshrc".into(),
            }],
        };
        s.save(&state).unwrap();
        assert_eq!(s.load().unwrap(), Some(state));
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        // nested, nonexistent XDG dir
        let nested = dir.path().join("a").join("b");
        let s = store_with(&[("XDG_STATE_HOME", nested.to_str().unwrap())], None);
        s.save(&State::default()).unwrap();
        assert!(nested.join("dfm").join("state.json").exists());
    }

    #[test]
    fn decode_explicit_null_slices() {
        // An explicit `null` for a slice field must decode as empty, not error.
        let json = r#"{"last_applied":null,"applied_at":"0001-01-01T00:00:00Z","links":null}"#;
        let st: State = serde_json::from_str(json).unwrap();
        assert!(st.last_applied.is_empty());
        assert!(st.links.is_empty());
    }

    #[test]
    fn decode_missing_fields_default() {
        let st: State = serde_json::from_str("{}").unwrap();
        assert!(st.last_applied.is_empty());
        assert!(st.links.is_empty());
        assert_eq!(st.applied_at, crate::state::schema::epoch_sentinel());
    }

    #[test]
    fn load_corrupt_returns_err() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("dfm");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("state.json"), b"{ not json").unwrap();
        let s = store_with(&[("XDG_STATE_HOME", dir.path().to_str().unwrap())], None);
        assert!(matches!(s.load(), Err(StateError::Parse { .. })));
    }

    #[test]
    fn save_uses_two_space_indent() {
        let dir = tempfile::tempdir().unwrap();
        let s = store_with(&[("XDG_STATE_HOME", dir.path().to_str().unwrap())], None);
        s.save(&State::default()).unwrap();
        let raw = std::fs::read_to_string(s.path().unwrap()).unwrap();
        assert!(
            raw.contains("\n  \"last_applied\""),
            "want 2-space indent, got:\n{raw}"
        );
    }

    #[test]
    fn applied_at_rfc3339_roundtrip() {
        use chrono::{TimeZone, Utc};
        let when = Utc
            .with_ymd_and_hms(2026, 6, 4, 12, 30, 45)
            .single()
            .unwrap();
        let state = State {
            applied_at: when,
            ..State::default()
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("2026-06-04T12:30:45Z"),
            "rfc3339 form: {json}"
        );
        let back: State = serde_json::from_str(&json).unwrap();
        assert_eq!(back.applied_at, when);
    }
}
