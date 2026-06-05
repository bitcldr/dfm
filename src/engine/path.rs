//! Path expansion and lexical helpers.
//!
//! Provides tilde and `$VAR` expansion plus the lexical path operations used
//! across the engine. `$VAR` expansion is hand-rolled (not `shellexpand`) so
//! `${VAR:-default}` is treated as a single literal variable name — never as
//! shell default-substitution.

use std::path::{Path, PathBuf};

/// An environment + home source, injectable so expansion is testable without
/// mutating the real process env (`set_var` is `unsafe` in edition 2024).
pub trait PathEnv {
    /// Look up an environment variable; returns an empty string when unset, so
    /// undefined names expand to "".
    fn var(&self, name: &str) -> String;
    /// The current user's home directory, if known.
    fn home(&self) -> Option<PathBuf>;
}

/// The real process environment.
pub struct SystemPathEnv;

impl PathEnv for SystemPathEnv {
    fn var(&self, name: &str) -> String {
        std::env::var(name).unwrap_or_default()
    }
    fn home(&self) -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
    }
}

/// Perform tilde + `$VAR` expansion on a path: tilde first, then `$VAR`.
pub fn expand(path: &str, env: &impl PathEnv) -> String {
    expand_env(&expand_home(path, env), env)
}

/// Replace a leading `~` with the user's home directory. `~user` syntax is not
/// supported; it falls back to the current user's HOME.
pub fn expand_home(path: &str, env: &impl PathEnv) -> String {
    let bytes = path.as_bytes();
    if path.is_empty() || bytes[0] != b'~' {
        return path.to_string();
    }

    // `~user/foo` — rare; fall back to HOME of current user for safety.
    if path.len() > 1 && bytes[1] != b'/' {
        let home = env.home().unwrap_or_default();
        let home = home.to_string_lossy();
        return match path.find('/') {
            Some(slash) => format!("{home}{}", &path[slash..]),
            None => home.into_owned(),
        };
    }

    match env.home() {
        Some(home) if !home.as_os_str().is_empty() => {
            format!("{}{}", home.to_string_lossy(), &path[1..])
        }
        _ => path.to_string(),
    }
}

/// Expand `$VAR` and `${VAR}` references. Unknown names expand to empty. `$$`
/// is a literal `$`. `${VAR:-default}` is NOT special — the whole
/// `VAR:-default` is one name (which is undefined, hence empty).
fn expand_env(s: &str, env: &impl PathEnv) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            let (name, width) = parse_var_name(&s[i + 1..]);
            if name.is_empty() {
                // `$` not followed by a name: emit the `$` verbatim and let the
                // next byte be handled on the following iteration.
                out.push('$');
                // Width 0 means we only advanced past `$`; leave the rest of
                // the input for the next loop iteration.
                i += 1;
                continue;
            }
            out.push_str(&env.var(name));
            i += 1 + width;
        } else {
            // push this UTF-8 char whole
            let ch_len = utf8_len(bytes[i]);
            out.push_str(&s[i..i + ch_len]);
            i += ch_len;
        }
    }
    out
}

/// Parse a shell variable name immediately following a `$`. Returns the name
/// and the number of bytes consumed from `rest`. Handles `${...}` braces, `$$`
/// (special var), or a run of alphanumerics/underscore.
fn parse_var_name(rest: &str) -> (&str, usize) {
    let b = rest.as_bytes();
    if b.is_empty() {
        return ("", 0);
    }
    if b[0] == b'{' {
        // ${ ... } — find closing brace; everything inside is the name.
        if let Some(close) = rest.find('}') {
            return (&rest[1..close], close + 1);
        }
        // Unterminated `${` — treat the rest as a (bad) name; return all.
        return (&rest[1..], rest.len());
    }
    if b[0] == b'$' {
        // `$$` — special var named "$" with width 1. We have no such var, so
        // it expands to empty. Preserve the consumed width.
        return ("$", 1);
    }
    // bareword: [A-Za-z0-9_]+
    let mut n = 0;
    while n < b.len() && (b[n].is_ascii_alphanumeric() || b[n] == b'_') {
        n += 1;
    }
    (&rest[..n], n)
}

const fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Lexically clean a path: collapse `.`/`..` and remove duplicate separators.
/// Does not touch the filesystem.
pub fn clean(path: &str) -> PathBuf {
    if path.is_empty() {
        return PathBuf::from(".");
    }
    let is_abs = path.starts_with('/');
    let mut stack: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                match stack.last() {
                    Some(&prev) if prev != ".." => {
                        stack.pop();
                    }
                    _ if is_abs => {} // can't go above root
                    _ => stack.push(".."),
                }
            }
            other => stack.push(other),
        }
    }
    let joined = stack.join("/");
    let result = if is_abs {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    };
    PathBuf::from(result)
}

/// Whether `path` sits inside any of the candidate prefixes. Each prefix is
/// compared against the cleaned path with a trailing separator so `/foo/bar`
/// isn't treated as inside `/foo/ba`.
pub fn is_inside_any(path: &Path, prefixes: &[PathBuf]) -> bool {
    let mut p = clean(&path.to_string_lossy())
        .to_string_lossy()
        .into_owned();
    p.push('/');
    prefixes.iter().any(|pre| {
        let mut pre = clean(&pre.to_string_lossy()).to_string_lossy().into_owned();
        pre.push('/');
        p.starts_with(&pre)
    })
}

/// Compute a relative path from `base` to `target`. Returns `None` when no
/// relative path exists.
pub fn rel(base: &Path, target: &Path) -> Option<PathBuf> {
    pathdiff::diff_paths(target, base)
}

/// Join a base directory with a relative source. Absolute sources are returned
/// verbatim.
pub fn resolve_base(base_dir: &Path, source: &str) -> PathBuf {
    let p = Path::new(source);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    // Join onto the base directory, then clean the result.
    let joined = base_dir.join(source);
    clean(&joined.to_string_lossy())
}

/// Strip a single leading `.` from a path's basename: `~/.vim` → `vim`.
pub fn default_source(target: &str) -> String {
    let base = Path::new(target)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if base.len() > 1 && base.starts_with('.') {
        base[1..].to_string()
    } else {
        base
    }
}

/// Whether a string contains glob metacharacters (`*`, `?`, `[`).
pub fn has_glob_chars(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '*' | '?' | '['))
}

/// Drop a leading `/` so an absolute path can be re-rooted under a backup dir
/// without `PathBuf::join`'s absolute-component reset.
pub fn strip_root(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    PathBuf::from(s.strip_prefix('/').unwrap_or(&s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct FakeEnv {
        vars: HashMap<String, String>,
        home: Option<PathBuf>,
    }
    impl PathEnv for FakeEnv {
        fn var(&self, name: &str) -> String {
            self.vars.get(name).cloned().unwrap_or_default()
        }
        fn home(&self) -> Option<PathBuf> {
            self.home.clone()
        }
    }
    fn env(vars: &[(&str, &str)], home: Option<&str>) -> FakeEnv {
        FakeEnv {
            vars: vars
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            home: home.map(PathBuf::from),
        }
    }

    #[test]
    fn tilde_expands_to_home() {
        let e = env(&[], Some("/home/u"));
        assert_eq!(expand_home("~/.zshrc", &e), "/home/u/.zshrc");
        assert_eq!(expand_home("~", &e), "/home/u");
    }

    #[test]
    fn tilde_user_falls_back_to_home() {
        let e = env(&[], Some("/home/u"));
        assert_eq!(expand_home("~bob/x", &e), "/home/u/x");
    }

    #[test]
    fn no_tilde_unchanged() {
        let e = env(&[], Some("/home/u"));
        assert_eq!(expand_home("/etc/x", &e), "/etc/x");
        assert_eq!(expand_home("rel/path", &e), "rel/path");
    }

    #[test]
    fn dollar_var_expands() {
        let e = env(&[("FOO", "bar")], Some("/h"));
        assert_eq!(expand_env("$FOO/x", &e), "bar/x");
        assert_eq!(expand_env("${FOO}x", &e), "barx");
    }

    #[test]
    fn unknown_var_is_empty() {
        let e = env(&[], Some("/h"));
        assert_eq!(expand_env("$NOPE/x", &e), "/x");
    }

    #[test]
    fn var_default_syntax_is_literal_not_shell() {
        // `${VAR:-default}` is not shell default-substitution; the whole
        // "VAR:-default" is one (undefined) name → empty.
        let e = env(&[("VAR", "v")], Some("/h"));
        assert_eq!(expand_env("${VAR:-default}", &e), "");
    }

    #[test]
    fn full_expand_combines_tilde_and_var() {
        let e = env(&[("SUB", "cfg")], Some("/home/u"));
        assert_eq!(expand("~/$SUB/file", &e), "/home/u/cfg/file");
    }

    #[test]
    fn clean_collapses() {
        assert_eq!(clean("/a/b/../c"), PathBuf::from("/a/c"));
        assert_eq!(clean("a//b/./c"), PathBuf::from("a/b/c"));
        assert_eq!(clean("/../x"), PathBuf::from("/x"));
        assert_eq!(clean(""), PathBuf::from("."));
    }

    #[test]
    fn inside_any_respects_boundaries() {
        let prefixes = vec![PathBuf::from("/foo/ba")];
        assert!(!is_inside_any(Path::new("/foo/bar"), &prefixes));
        assert!(is_inside_any(Path::new("/foo/ba/x"), &prefixes));
    }

    #[test]
    fn default_source_strips_leading_dot() {
        assert_eq!(default_source("~/.vim"), "vim");
        assert_eq!(default_source("~/.config/nvim"), "nvim");
        assert_eq!(default_source("/x/plain"), "plain");
    }

    #[test]
    fn strip_root_drops_leading_slash() {
        assert_eq!(strip_root(Path::new("/a/b")), PathBuf::from("a/b"));
        assert_eq!(strip_root(Path::new("rel")), PathBuf::from("rel"));
    }

    #[test]
    fn resolve_base_joins_relative_keeps_absolute() {
        let base = Path::new("/repo");
        assert_eq!(
            resolve_base(base, "config/nvim"),
            PathBuf::from("/repo/config/nvim")
        );
        assert_eq!(resolve_base(base, "/abs/path"), PathBuf::from("/abs/path"));
    }
}
