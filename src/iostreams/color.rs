//! Color-policy resolution: `auto`/`always`/`never` honoring `NO_COLOR`,
//! `TERM=dumb`, `CLICOLOR_FORCE`, and TTY detection.
//!
//! The env lookups are injectable so the precedence rules can be tested
//! without touching the real process env (`set_var` is `unsafe` in
//! edition 2024).

use std::io::IsTerminal;

/// The user-facing `--color` policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorPolicy {
    /// Decide from env + TTY detection.
    #[default]
    Auto,
    /// Force color on.
    Always,
    /// Force color off.
    Never,
}

impl ColorPolicy {
    /// Parse the `--color` flag value; unknown values are ignored (treated as
    /// `Auto`).
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "always" => ColorPolicy::Always,
            "never" => ColorPolicy::Never,
            _ => ColorPolicy::Auto,
        }
    }
}

/// Environment inputs to the color decision; injectable for tests.
pub trait ColorEnv {
    /// Whether an env var is present (any value).
    fn has(&self, key: &str) -> bool;
    /// An env var's value, if set.
    fn get(&self, key: &str) -> Option<String>;
}

/// The real process environment.
pub struct SystemColorEnv;

impl ColorEnv for SystemColorEnv {
    fn has(&self, key: &str) -> bool {
        std::env::var_os(key).is_some()
    }
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// Decide whether a stream should receive ANSI sequences under `Auto`.
///
/// Environment precedence:
/// - `NO_COLOR` present → never (<https://no-color.org/>)
/// - `TERM=dumb` → never
/// - `CLICOLOR_FORCE=1` → always
/// - otherwise → `is_tty`
pub(crate) fn compute_color(env: &impl ColorEnv, is_tty: bool) -> bool {
    if env.has("NO_COLOR") {
        return false;
    }
    if env.get("TERM").as_deref() == Some("dumb") {
        return false;
    }
    if env.get("CLICOLOR_FORCE").as_deref() == Some("1") {
        return true;
    }
    is_tty
}

/// Resolve color for a concrete stream under a policy. `Always`/`Never`
/// override; `Auto` runs [`compute_color`] against the stream's TTY status.
pub(crate) fn resolve<W: IsTerminal>(policy: ColorPolicy, env: &impl ColorEnv, stream: &W) -> bool {
    match policy {
        ColorPolicy::Always => true,
        ColorPolicy::Never => false,
        ColorPolicy::Auto => compute_color(env, stream.is_terminal()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct FakeEnv(HashMap<String, String>);
    impl ColorEnv for FakeEnv {
        fn has(&self, key: &str) -> bool {
            self.0.contains_key(key)
        }
        fn get(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
    }
    fn env(pairs: &[(&str, &str)]) -> FakeEnv {
        FakeEnv(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        )
    }

    #[test]
    fn no_color_disables_even_on_tty() {
        assert!(!compute_color(&env(&[("NO_COLOR", "")]), true));
        assert!(!compute_color(&env(&[("NO_COLOR", "1")]), true));
    }

    #[test]
    fn term_dumb_disables() {
        assert!(!compute_color(&env(&[("TERM", "dumb")]), true));
    }

    #[test]
    fn clicolor_force_enables_off_tty() {
        assert!(compute_color(&env(&[("CLICOLOR_FORCE", "1")]), false));
    }

    #[test]
    fn clicolor_force_non_one_does_not_force() {
        // Only the literal "1" forces; other values fall through to TTY.
        assert!(!compute_color(&env(&[("CLICOLOR_FORCE", "yes")]), false));
    }

    #[test]
    fn falls_back_to_tty() {
        assert!(compute_color(&env(&[]), true));
        assert!(!compute_color(&env(&[]), false));
    }

    #[test]
    fn no_color_beats_clicolor_force() {
        // NO_COLOR is checked first, so it wins over CLICOLOR_FORCE.
        assert!(!compute_color(
            &env(&[("NO_COLOR", "1"), ("CLICOLOR_FORCE", "1")]),
            false
        ));
    }

    #[test]
    fn policy_parse() {
        assert_eq!(ColorPolicy::parse("always"), ColorPolicy::Always);
        assert_eq!(ColorPolicy::parse("never"), ColorPolicy::Never);
        assert_eq!(ColorPolicy::parse("auto"), ColorPolicy::Auto);
        assert_eq!(ColorPolicy::parse("bogus"), ColorPolicy::Auto);
    }
}
