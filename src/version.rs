//! Build-time revision string, embedded by `build.rs`.

/// The revision this binary was built from (a `git describe` string), or
/// `"dev"` when built outside a git checkout.
pub const REVISION: &str = env!("DFM_REVISION");

/// A one-line description including the revision, e.g.
/// `dotfiles manager (v0.3.0-2-gabc1234)`.
#[must_use]
pub fn long_description() -> String {
    format!("dotfiles manager ({REVISION})")
}
