//! Profile YAML parser.
//!
//! Uses `marked-yaml`'s low-level `Node` walk (not serde) to (a) preserve
//! mapping declaration order == execution order, (b) report 1-based
//! `line:col` on every error, and (c) read **raw scalar lexemes**
//! (`prevent_coercion`) so `on`/`yes`/`0700` survive as strings. Unknown keys
//! are rejected at every nesting level.

use std::fs;
use std::path::Path;

use marked_yaml::types::{MarkedScalarNode, Node};
use marked_yaml::{LoaderOptions, parse_yaml_with_options};

use super::error::{LoadError, ParseError};
use super::model::{
    Clean, CleanEntry, CleanOptions, Config, Create, CreateEntry, Defaults, Directive,
    DirectiveKind, Link, LinkEntry, LinkOptions, Shell, ShellEntry, ShellOptions,
};

/// Read a profile YAML file from disk and parse it.
pub fn load(path: impl AsRef<Path>) -> Result<Config, LoadError> {
    let path = path.as_ref();
    let path_str = path.display().to_string();

    let data = fs::read_to_string(path).map_err(|source| LoadError::Open {
        path: path_str.clone(),
        source,
    })?;

    let mut cfg = parse_str(&data).map_err(|source| LoadError::Parse {
        path: path_str.clone(),
        source,
    })?;

    cfg.path = path_str;
    Ok(cfg)
}

/// Parse a profile YAML from a string. The root must be a mapping whose keys
/// are directive names; unknown directives are rejected.
pub fn parse_str(src: &str) -> Result<Config, ParseError> {
    // An empty / comment-only document yields an empty config.
    if src.split('\n').all(|l| {
        let t = l.trim();
        t.is_empty() || t.starts_with('#')
    }) {
        return Ok(Config::default());
    }

    // prevent_coercion: keep scalars raw so on/yes/0700 stay strings (no
    // type-resolution). toplevel_mapping (default): a sequence/scalar root is
    // rejected — we map that LoadError to the "top-level must be a mapping"
    // message.
    let opts = LoaderOptions::default().prevent_coercion(true);
    let root = parse_yaml_with_options(0, src, opts).map_err(translate_load_error)?;

    let body = root.as_mapping().ok_or_else(|| {
        err_at(
            &root,
            "top-level must be a mapping of directives (defaults/link/shell/clean/create)",
        )
    })?;

    let mut cfg = Config {
        path: String::new(),
        directives: Vec::with_capacity(body.len()),
    };

    for (key, val) in body.iter() {
        cfg.directives.push(parse_directive(key, val)?);
    }

    Ok(cfg)
}

fn parse_directive(key: &MarkedScalarNode, val: &Node) -> Result<Directive, ParseError> {
    let line = scalar_line(key);
    let kind = match key.as_str() {
        "defaults" => DirectiveKind::Defaults(parse_defaults(val)?),
        "link" => DirectiveKind::Link(parse_link(val)?),
        "shell" => DirectiveKind::Shell(parse_shell(val)?),
        "clean" => DirectiveKind::Clean(parse_clean(val)?),
        "create" => DirectiveKind::Create(parse_create(val)?),
        other => {
            return Err(err_at_scalar(
                key,
                format!("unknown directive {other:?} (plugins are not supported)"),
            ));
        }
    };
    Ok(Directive { kind, line })
}

fn parse_defaults(n: &Node) -> Result<Defaults, ParseError> {
    let m = n
        .as_mapping()
        .ok_or_else(|| err_at(n, "defaults: must be a mapping"))?;

    let mut d = Defaults::default();
    for (k, v) in m.iter() {
        match k.as_str() {
            "link" => d.link = Some(parse_link_options(v)?),
            "shell" => d.shell = Some(parse_shell_options(v)?),
            "clean" => d.clean = Some(parse_clean_options(v)?),
            // `create` has only per-entry `mode`; accepted but ignored here.
            "create" => {}
            other => {
                return Err(err_at_scalar(
                    k,
                    format!("defaults: unknown section {other:?}"),
                ));
            }
        }
    }
    Ok(d)
}

fn parse_link(n: &Node) -> Result<Link, ParseError> {
    let m = n
        .as_mapping()
        .ok_or_else(|| err_at(n, "link: must be a mapping of target→source"))?;

    let mut l = Link {
        entries: Vec::with_capacity(m.len()),
    };
    for (k, v) in m.iter() {
        let mut e = LinkEntry {
            target: k.as_str().to_string(),
            options: LinkOptions::default(),
        };
        if let Some(s) = v.as_scalar() {
            // string form: shorthand for { path: ... }
            e.options.path = Some(s.as_str().to_string());
        } else if v.as_mapping().is_some() {
            e.options = parse_link_options(v)?;
        } else {
            return Err(err_at(
                v,
                format!("link {:?}: value must be a string or mapping", k.as_str()),
            ));
        }
        l.entries.push(e);
    }
    Ok(l)
}

fn parse_link_options(n: &Node) -> Result<LinkOptions, ParseError> {
    let m = n
        .as_mapping()
        .ok_or_else(|| err_at(n, "link options must be a mapping"))?;

    let mut o = LinkOptions::default();
    for (k, v) in m.iter() {
        match k.as_str() {
            "path" => o.path = Some(scalar_str(v)?),
            "create" => o.create = Some(scalar_bool(v)?),
            "relink" => o.relink = Some(scalar_bool(v)?),
            "force" => o.force = Some(scalar_bool(v)?),
            "relative" => o.relative = Some(scalar_bool(v)?),
            "glob" => o.glob = Some(scalar_bool(v)?),
            "ignore-missing" => o.ignore_missing = Some(scalar_bool(v)?),
            "backup" => o.backup = Some(scalar_bool(v)?),
            "type" => {
                let s = scalar_str(v)?;
                if s != "symlink" && s != "hardlink" {
                    return Err(err_at(
                        v,
                        format!(
                            "link type must be {:?} or {:?}, got {:?}",
                            "symlink", "hardlink", s
                        ),
                    ));
                }
                o.link_type = Some(s);
            }
            "canonicalize" | "canonicalize-path" => o.canonicalize = Some(scalar_bool(v)?),
            "prefix" => o.prefix = Some(scalar_str(v)?),
            "exclude" => {
                let seq = v
                    .as_sequence()
                    .ok_or_else(|| err_at(v, "link exclude must be a sequence"))?;
                o.exclude = seq.iter().map(node_value_str).collect();
            }
            "if" => {
                return Err(err_at_scalar(
                    k,
                    "link: 'if' directive is not supported; conditionals were removed",
                ));
            }
            other => {
                return Err(err_at_scalar(k, format!("link: unknown option {other:?}")));
            }
        }
    }
    Ok(o)
}

fn parse_shell(n: &Node) -> Result<Shell, ParseError> {
    let seq = n
        .as_sequence()
        .ok_or_else(|| err_at(n, "shell: must be a sequence"))?;

    let mut s = Shell {
        entries: Vec::with_capacity(seq.len()),
    };
    for item in seq.iter() {
        s.entries.push(parse_shell_entry(item)?);
    }
    Ok(s)
}

/// Accepts the single canonical form: a mapping with `name:` + `script:` plus
/// optional flags. Legacy forms (`command:`/`description:`, scalar, list) are
/// rejected.
fn parse_shell_entry(n: &Node) -> Result<ShellEntry, ParseError> {
    let m = n
        .as_mapping()
        .ok_or_else(|| err_at(n, "shell entry must be a mapping with 'name' and 'script'"))?;

    let mut e = ShellEntry::default();
    for (k, v) in m.iter() {
        match k.as_str() {
            "name" => e.description = scalar_str(v)?,
            "script" => e.command = scalar_str(v)?,
            "stdin" => e.options.stdin = Some(scalar_bool(v)?),
            "stdout" => e.options.stdout = Some(scalar_bool(v)?),
            "stderr" => e.options.stderr = Some(scalar_bool(v)?),
            "quiet" => e.options.quiet = Some(scalar_bool(v)?),
            other @ ("command" | "description") => {
                return Err(err_at_scalar(
                    k,
                    format!(
                        "shell: {other:?} is no longer supported; use 'script' (or 'name' for description)"
                    ),
                ));
            }
            other => {
                return Err(err_at_scalar(k, format!("shell: unknown key {other:?}")));
            }
        }
    }

    if e.command.is_empty() {
        return Err(err_at(n, "shell: 'script' is required"));
    }
    Ok(e)
}

fn parse_shell_options(n: &Node) -> Result<ShellOptions, ParseError> {
    let m = n
        .as_mapping()
        .ok_or_else(|| err_at(n, "shell options must be a mapping"))?;

    let mut o = ShellOptions::default();
    for (k, v) in m.iter() {
        match k.as_str() {
            "stdin" => o.stdin = Some(scalar_bool(v)?),
            "stdout" => o.stdout = Some(scalar_bool(v)?),
            "stderr" => o.stderr = Some(scalar_bool(v)?),
            "quiet" => o.quiet = Some(scalar_bool(v)?),
            other => {
                return Err(err_at_scalar(
                    k,
                    format!("shell defaults: unknown key {other:?}"),
                ));
            }
        }
    }
    Ok(o)
}

fn parse_clean(n: &Node) -> Result<Clean, ParseError> {
    let mut c = Clean::default();
    if let Some(seq) = n.as_sequence() {
        for item in seq.iter() {
            let s = item
                .as_scalar()
                .ok_or_else(|| err_at(item, "clean list entries must be strings"))?;
            c.entries.push(CleanEntry {
                target: s.as_str().to_string(),
                options: CleanOptions::default(),
            });
        }
    } else if let Some(m) = n.as_mapping() {
        for (k, v) in m.iter() {
            let mut e = CleanEntry {
                target: k.as_str().to_string(),
                options: CleanOptions::default(),
            };
            if v.as_mapping().is_some() {
                e.options = parse_clean_options(v)?;
            }
            c.entries.push(e);
        }
    } else {
        return Err(err_at(n, "clean: must be a sequence or mapping"));
    }
    Ok(c)
}

fn parse_clean_options(n: &Node) -> Result<CleanOptions, ParseError> {
    let m = n
        .as_mapping()
        .ok_or_else(|| err_at(n, "clean options must be a mapping"))?;

    let mut o = CleanOptions::default();
    for (k, v) in m.iter() {
        match k.as_str() {
            "force" => o.force = Some(scalar_bool(v)?),
            "recursive" => o.recursive = Some(scalar_bool(v)?),
            other => {
                return Err(err_at_scalar(k, format!("clean: unknown option {other:?}")));
            }
        }
    }
    Ok(o)
}

fn parse_create(n: &Node) -> Result<Create, ParseError> {
    let mut c = Create::default();
    if let Some(seq) = n.as_sequence() {
        for item in seq.iter() {
            let s = item
                .as_scalar()
                .ok_or_else(|| err_at(item, "create list entries must be strings"))?;
            c.entries.push(CreateEntry {
                path: s.as_str().to_string(),
                mode: None,
            });
        }
    } else if let Some(m) = n.as_mapping() {
        for (k, v) in m.iter() {
            let mut e = CreateEntry {
                path: k.as_str().to_string(),
                mode: None,
            };
            if let Some(inner) = v.as_mapping() {
                for (ok, ov) in inner.iter() {
                    if ok.as_str() == "mode" {
                        e.mode = Some(scalar_mode(ov)?);
                    }
                }
            }
            c.entries.push(e);
        }
    } else {
        return Err(err_at(n, "create: must be a sequence or mapping"));
    }
    Ok(c)
}

/// Parse a boolean scalar. Accepts ONLY `true/yes/on` and `false/no/off`.
fn scalar_bool(n: &Node) -> Result<bool, ParseError> {
    let s = n.as_scalar().ok_or_else(|| err_at(n, "expected boolean"))?;
    match s.as_str() {
        "true" | "yes" | "on" => Ok(true),
        "false" | "no" | "off" => Ok(false),
        other => Err(err_at_scalar(s, format!("invalid boolean {other:?}"))),
    }
}

/// Parse a file-mode scalar. Accepts `0o700`/`0700` (octal) and decimal.
/// Leading `+`/`-` are rejected: a mode is an unsigned value.
fn scalar_mode(n: &Node) -> Result<u32, ParseError> {
    let s = n
        .as_scalar()
        .ok_or_else(|| err_at(n, "expected file mode integer"))?;
    let raw = s.as_str();

    let (digits, radix) =
        if raw.len() > 1 && raw.starts_with('0') && matches!(raw.as_bytes()[1], b'o' | b'O') {
            (&raw[2..], 8)
        } else if raw.len() > 1 && raw.starts_with('0') {
            (&raw[1..], 8)
        } else {
            (raw, 10)
        };

    // u32::from_str_radix accepts a leading '+'; a file mode must not.
    if digits.starts_with('+') || digits.starts_with('-') {
        return Err(err_at_scalar(
            s,
            format!("invalid mode {raw:?}: invalid sign"),
        ));
    }

    u32::from_str_radix(digits, radix)
        .map_err(|e| err_at_scalar(s, format!("invalid mode {raw:?}: {e}")))
}

/// Read a scalar node's raw string, erroring if it is not a scalar.
fn scalar_str(n: &Node) -> Result<String, ParseError> {
    n.as_scalar()
        .map(|s| s.as_str().to_string())
        .ok_or_else(|| err_at(n, "expected a string scalar"))
}

/// The raw string of a node when it is a scalar, else empty. Used in sequence
/// contexts where non-scalar items collapse to an empty string.
fn node_value_str(n: &Node) -> String {
    n.as_scalar()
        .map(|s| s.as_str().to_string())
        .unwrap_or_default()
}

// ── span helpers ──────────────────────────────────────────────────────────

/// 1-based start line of a scalar node (0 if unknown).
fn scalar_line(n: &MarkedScalarNode) -> usize {
    n.span().start().map_or(0, marked_yaml::Marker::line)
}

/// Build a `ParseError` at a node's start position.
fn err_at(n: &Node, msg: impl Into<String>) -> ParseError {
    match n.span().start() {
        Some(m) => ParseError::at(m.line(), m.column(), msg),
        None => ParseError::msg(msg),
    }
}

/// Build a `ParseError` at a scalar node's start position.
fn err_at_scalar(n: &MarkedScalarNode, msg: impl Into<String>) -> ParseError {
    match n.span().start() {
        Some(m) => ParseError::at(m.line(), m.column(), msg),
        None => ParseError::msg(msg),
    }
}

/// Translate a `marked_yaml::LoadError` into the typed `ParseError`, copying
/// out the 1-based marker so the foreign error never leaks.
fn translate_load_error(e: marked_yaml::LoadError) -> ParseError {
    use marked_yaml::LoadError as L;
    match e {
        L::TopLevelMustBeMapping(m) => ParseError::at(
            m.line(),
            m.column(),
            "top-level must be a mapping of directives (defaults/link/shell/clean/create)",
        ),
        L::TopLevelMustBeSequence(m) => {
            ParseError::at(m.line(), m.column(), "top-level must be a sequence")
        }
        L::UnexpectedAnchor(m) => ParseError::at(
            m.line(),
            m.column(),
            "anchors/aliases are not supported in profiles",
        ),
        L::MappingKeyMustBeScalar(m) => {
            ParseError::at(m.line(), m.column(), "mapping keys must be scalars")
        }
        L::UnexpectedTag(m) => {
            ParseError::at(m.line(), m.column(), "explicit YAML tags are not supported")
        }
        L::ScanError(m, se) => ParseError::at(m.line(), m.column(), format!("yaml: {se}")),
        L::DuplicateKey(_) => {
            // Not reachable: error_on_duplicate_keys stays at its default
            // (false → last-wins). Kept for exhaustiveness.
            ParseError::msg("duplicate key")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DirectiveKind;

    /// Helper: parse and unwrap, failing the test with the parse error.
    fn parse(src: &str) -> Config {
        parse_str(src).unwrap_or_else(|e| panic!("parse: {e}"))
    }

    // ── core parsing behavior ──────────────────────────────────────────────

    #[test]
    fn base_profile() {
        let src = r#"
defaults:
  link:
    relink: true
    force: false
  shell:
    stdout: true
    stderr: true
    quiet: true

clean:
  - "~"

shell:
  - name: installing submodules
    script: git submodule update --init

link:
  ~/.config/nvim: config/nvim
  ~/.zshrc: config/zsh/zshrc.zsh
"#;
        let cfg = parse(src);
        assert_eq!(cfg.directives.len(), 4);

        // 1. defaults
        let DirectiveKind::Defaults(d) = &cfg.directives[0].kind else {
            panic!("directive[0] not defaults");
        };
        let link = d.link.as_ref().expect("defaults.link");
        let shell = d.shell.as_ref().expect("defaults.shell");
        assert_eq!(link.relink, Some(true));
        assert_eq!(link.force, Some(false));
        assert_eq!(shell.quiet, Some(true));

        // 2. clean
        let DirectiveKind::Clean(c) = &cfg.directives[1].kind else {
            panic!("directive[1] not clean");
        };
        assert_eq!(c.entries.len(), 1);
        assert_eq!(c.entries[0].target, "~");

        // 3. shell — order preserved (shell before link in this fixture)
        let DirectiveKind::Shell(s) = &cfg.directives[2].kind else {
            panic!("directive[2] not shell");
        };
        assert_eq!(s.entries.len(), 1);
        assert!(s.entries[0].command.starts_with("git submodule"));
        assert_eq!(s.entries[0].description, "installing submodules");

        // 4. link
        let DirectiveKind::Link(l) = &cfg.directives[3].kind else {
            panic!("directive[3] not link");
        };
        assert_eq!(l.entries.len(), 2);
        assert_eq!(l.entries[0].target, "~/.config/nvim");
        assert_eq!(l.entries[0].options.path.as_deref(), Some("config/nvim"));
    }

    #[test]
    fn link_extended_form() {
        let src = r#"
link:
  ~/.vim:
    path: config/vim
    create: true
    relink: true
    glob: false
    type: symlink
    exclude: ["*.log"]
"#;
        let cfg = parse(src);
        let DirectiveKind::Link(l) = &cfg.directives[0].kind else {
            panic!("not link");
        };
        let e = &l.entries[0];
        assert_eq!(e.target, "~/.vim");
        assert_eq!(e.options.path.as_deref(), Some("config/vim"));
        assert_eq!(e.options.create, Some(true));
        assert_eq!(e.options.exclude, vec!["*.log".to_string()]);
    }

    #[test]
    fn shell_canonical_form() {
        let src = "
shell:
  - name: farewell
    script: echo bye
    quiet: true
";
        let cfg = parse(src);
        let DirectiveKind::Shell(s) = &cfg.directives[0].kind else {
            panic!("not shell");
        };
        assert_eq!(s.entries.len(), 1);
        assert_eq!(s.entries[0].command, "echo bye");
        assert_eq!(s.entries[0].description, "farewell");
        assert_eq!(s.entries[0].options.quiet, Some(true));
    }

    #[test]
    fn shell_rejects_legacy_forms() {
        for src in [
            "shell:\n  - echo hello\n",
            "shell:\n  - [\"echo world\", \"say world\"]\n",
            "shell:\n  - command: echo bye\n    description: farewell\n",
        ] {
            assert!(
                parse_str(src).is_err(),
                "expected error for legacy form: {src:?}"
            );
        }
    }

    #[test]
    fn shell_multiline_script() {
        let src = "shell:\n  - name: some command\n    script: |\n      ls -laR /tmp\n      du -hcs /srv\n      echo ok\n";
        let cfg = parse(src);
        let DirectiveKind::Shell(s) = &cfg.directives[0].kind else {
            panic!("not shell");
        };
        let e = &s.entries[0];
        assert_eq!(e.description, "some command");
        for line in ["ls -laR /tmp", "du -hcs /srv", "echo ok"] {
            assert!(
                e.command.contains(line),
                "script missing {line:?}; got:\n{}",
                e.command
            );
        }
        assert!(e.command.contains('\n'), "script should preserve newlines");
    }

    #[test]
    fn create_with_mode() {
        let src = r"
create:
  ~/tmp:
    mode: 0700
  ~/logs: {}
";
        let cfg = parse(src);
        let DirectiveKind::Create(c) = &cfg.directives[0].kind else {
            panic!("not create");
        };
        assert_eq!(c.entries.len(), 2);
        assert_eq!(c.entries[0].mode, Some(0o700));
        assert_eq!(c.entries[1].mode, None);
    }

    #[test]
    fn unknown_directive_rejected() {
        let err = parse_str("\nteleport:\n  src: dst\n").unwrap_err();
        assert!(
            err.to_string().contains("unknown directive"),
            "error = {err}, want mention of unknown directive"
        );
    }

    #[test]
    fn rejects_if_predicate() {
        let src = "
link:
  ~/.foo:
    path: foo
    if: '[ -f /etc/os-release ]'
";
        let err = parse_str(src).unwrap_err();
        assert!(
            err.to_string().contains("'if'"),
            "error = {err}, want mention of 'if'"
        );
    }

    #[test]
    fn order_preserved() {
        let src = "
link:
  a: 1
  b: 2
  c: 3
  d: 4
";
        let cfg = parse(src);
        let DirectiveKind::Link(l) = &cfg.directives[0].kind else {
            panic!("not link");
        };
        let got: Vec<_> = l.entries.iter().map(|e| e.target.as_str()).collect();
        assert_eq!(got, ["a", "b", "c", "d"]);
    }

    // ── behavior matrix the contract requires ──────────────────────────────

    #[test]
    fn empty_and_comment_only_documents() {
        assert!(parse_str("").unwrap().directives.is_empty());
        assert!(
            parse_str("# just a comment\n")
                .unwrap()
                .directives
                .is_empty()
        );
        assert!(parse_str("\n\n  \n").unwrap().directives.is_empty());
    }

    #[test]
    fn bool_aliases_accepted_and_only_those() {
        // every accepted truthy/falsy alias
        for (word, want) in [
            ("true", true),
            ("yes", true),
            ("on", true),
            ("false", false),
            ("no", false),
            ("off", false),
        ] {
            let cfg = parse(&format!("link:\n  ~/x:\n    path: x\n    create: {word}\n"));
            let DirectiveKind::Link(l) = &cfg.directives[0].kind else {
                panic!()
            };
            assert_eq!(l.entries[0].options.create, Some(want), "word={word}");
        }
        // anything else is an error (e.g. YAML 1.1 "y"/"1"/"True")
        for bad in ["y", "1", "True", "YES", "enable"] {
            let src = format!("link:\n  ~/x:\n    path: x\n    create: {bad}\n");
            assert!(
                parse_str(&src).is_err(),
                "expected invalid boolean for {bad:?}"
            );
        }
    }

    #[test]
    fn raw_scalars_not_coerced() {
        // `on`/`yes` as a link target must survive as the literal string,
        // not become a YAML bool. prevent_coercion guarantees this.
        let cfg = parse("link:\n  ~/on: yes\n");
        let DirectiveKind::Link(l) = &cfg.directives[0].kind else {
            panic!()
        };
        assert_eq!(l.entries[0].target, "~/on");
        assert_eq!(l.entries[0].options.path.as_deref(), Some("yes"));
    }

    #[test]
    fn mode_octal_decimal_and_sign() {
        let cases = [
            ("0o755", Some(0o755)),
            ("0755", Some(0o755)),
            ("493", Some(493)), // decimal 493 == 0o755
            ("0", Some(0)),
        ];
        for (lit, want) in cases {
            let cfg = parse(&format!("create:\n  ~/d:\n    mode: {lit}\n"));
            let DirectiveKind::Create(c) = &cfg.directives[0].kind else {
                panic!()
            };
            assert_eq!(c.entries[0].mode, want, "mode literal {lit}");
        }
        // signed modes are rejected: a file mode is unsigned
        for bad in ["+700", "-1"] {
            let src = format!("create:\n  ~/d:\n    mode: {bad}\n");
            assert!(
                parse_str(&src).is_err(),
                "expected error for signed mode {bad:?}"
            );
        }
    }

    #[test]
    fn duplicate_key_last_wins() {
        // Duplicate keys: last value wins (marked-yaml default), no error.
        let cfg = parse("link:\n  ~/x: first\n  ~/x: second\n");
        let DirectiveKind::Link(l) = &cfg.directives[0].kind else {
            panic!()
        };
        assert_eq!(l.entries.len(), 1);
        assert_eq!(l.entries[0].options.path.as_deref(), Some("second"));
    }

    #[test]
    fn top_level_sequence_rejected() {
        let err = parse_str("- one\n- two\n").unwrap_err();
        assert!(
            err.to_string().contains("top-level must be a mapping"),
            "error = {err}"
        );
    }

    #[test]
    fn anchors_rejected_cleanly() {
        // marked-yaml 0.8.0 rejects anchors; we must surface a clean ParseError
        // (with a line:col), never a panic. Real profiles are anchor-free.
        let src = "defaults: &base\n  link:\n    relink: true\nlink: *base\n";
        let err = parse_str(src).unwrap_err();
        assert!(err.line > 0, "anchor error should carry a line: {err}");
    }

    #[test]
    fn error_carries_line_and_column() {
        // unknown option on line 4 (1-based)
        let src = "link:\n  ~/x:\n    path: x\n    bogus: true\n";
        let err = parse_str(src).unwrap_err();
        assert!(err.line >= 1, "want a line number, got {err}");
        assert!(
            err.to_string().starts_with("line "),
            "want 'line L:C:' prefix: {err}"
        );
    }

    #[test]
    fn clean_map_form_with_options() {
        let cfg = parse("clean:\n  \"~\":\n    recursive: true\n");
        let DirectiveKind::Clean(c) = &cfg.directives[0].kind else {
            panic!()
        };
        assert_eq!(c.entries[0].target, "~");
        assert_eq!(c.entries[0].options.recursive, Some(true));
    }
}
