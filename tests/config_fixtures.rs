//! Golden-parse the real dotfiles profiles. Skips when the dotfiles repo is
//! unavailable so the suite still passes in CI / on other machines.
//!
//! The real repo lives at `/Volumes/workplace/dotfiles/profiles`.

use std::path::Path;

use dfm::config;

const PROFILE_DIRS: &[&str] = &[
    "/Volumes/workplace/dotfiles/profiles",
    // fallbacks for other machines / the bundled fixtures path
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/profiles"),
];

#[test]
fn real_profiles_parse_cleanly() {
    let Some(dir) = PROFILE_DIRS.iter().map(Path::new).find(|p| p.is_dir()) else {
        eprintln!("skip: no profiles dir found in {PROFILE_DIRS:?}");
        return;
    };

    let mut checked = 0;
    for entry in std::fs::read_dir(dir).expect("read profiles dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        if !path.to_string_lossy().ends_with(".conf.yaml") {
            continue;
        }

        let cfg = config::load(&path).unwrap_or_else(|e| panic!("load {}: {e}", path.display()));
        assert!(
            !cfg.directives.is_empty(),
            "{} parsed to zero directives",
            path.display()
        );
        checked += 1;
    }

    if checked == 0 {
        eprintln!("skip: no *.conf.yaml in {}", dir.display());
    }
}
