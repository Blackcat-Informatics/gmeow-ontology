// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust twin of the retired `tests/test_diagnostics_config.py`.
//!
//! Covers the same 15 policy cases for the resolved diagnostics output config.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use gmeow_cli_core::error::{UnknownArtifactKind, UnknownConsoleMode};
use gmeow_cli_core::{ConsoleMode, DiagnosticsConfig};

fn dist_dir() -> PathBuf {
    PathBuf::from("dist")
}

fn env_empty() -> HashMap<String, String> {
    HashMap::new()
}

#[test]
fn defaults_resolve_with_no_flags_or_env() {
    let config = DiagnosticsConfig::resolve(
        None,
        None,
        None,
        None,
        None,
        &env_empty(),
        true,
        &dist_dir(),
    )
    .unwrap();
    assert_eq!(config.console, ConsoleMode::Pretty);
    assert_eq!(config.artifacts, btreeset(&["json", "sarif", "html"]));
    assert_eq!(config.directory, dist_dir());
    assert_eq!(config.stem, "gmeow-feedback");
    assert_eq!(config.category, "gmeow");
}

#[test]
fn auto_console_resolves_by_tty() {
    let on_tty = DiagnosticsConfig::resolve(
        Some("auto"),
        None,
        None,
        None,
        None,
        &env_empty(),
        true,
        &dist_dir(),
    )
    .unwrap();
    assert_eq!(on_tty.console, ConsoleMode::Pretty);

    let off_tty = DiagnosticsConfig::resolve(
        Some("auto"),
        None,
        None,
        None,
        None,
        &env_empty(),
        false,
        &dist_dir(),
    )
    .unwrap();
    assert_eq!(off_tty.console, ConsoleMode::Text);
}

#[test]
fn flag_beats_env_for_console() {
    let mut env = env_empty();
    env.insert("GMEOW_DIAGNOSTICS_CONSOLE".to_owned(), "silent".to_owned());
    let config = DiagnosticsConfig::resolve(
        Some("pretty"),
        None,
        None,
        None,
        None,
        &env,
        false,
        &dist_dir(),
    )
    .unwrap();
    assert_eq!(config.console, ConsoleMode::Pretty);
}

#[test]
fn env_honored_when_no_flag() {
    let mut env = env_empty();
    env.insert("GMEOW_DIAGNOSTICS_CONSOLE".to_owned(), "jsonl".to_owned());
    let config =
        DiagnosticsConfig::resolve(None, None, None, None, None, &env, true, &dist_dir()).unwrap();
    assert_eq!(config.console, ConsoleMode::Jsonl);
}

#[test]
fn stem_precedence() {
    let config = DiagnosticsConfig::resolve(
        None,
        None,
        None,
        Some("foo"),
        None,
        &env_empty(),
        true,
        &dist_dir(),
    )
    .unwrap();
    assert_eq!(config.stem, "foo");

    let mut env = env_empty();
    env.insert("GMEOW_DIAGNOSTICS_STEM".to_owned(), "bar".to_owned());
    let config =
        DiagnosticsConfig::resolve(None, None, None, None, None, &env, true, &dist_dir()).unwrap();
    assert_eq!(config.stem, "bar");

    let config =
        DiagnosticsConfig::resolve(None, None, None, Some("foo"), None, &env, true, &dist_dir())
            .unwrap();
    assert_eq!(config.stem, "foo");

    let config = DiagnosticsConfig::resolve(
        None,
        None,
        None,
        None,
        None,
        &env_empty(),
        true,
        &dist_dir(),
    )
    .unwrap();
    assert_eq!(config.stem, "gmeow-feedback");
}

#[test]
fn category_precedence() {
    let config = DiagnosticsConfig::resolve(
        None,
        None,
        None,
        None,
        Some("lint"),
        &env_empty(),
        true,
        &dist_dir(),
    )
    .unwrap();
    assert_eq!(config.category, "lint");

    let mut env = env_empty();
    env.insert("GMEOW_DIAGNOSTICS_CATEGORY".to_owned(), "rust".to_owned());
    let config =
        DiagnosticsConfig::resolve(None, None, None, None, None, &env, true, &dist_dir()).unwrap();
    assert_eq!(config.category, "rust");

    let config = DiagnosticsConfig::resolve(
        None,
        None,
        None,
        None,
        Some("lint"),
        &env,
        true,
        &dist_dir(),
    )
    .unwrap();
    assert_eq!(config.category, "lint");

    let config = DiagnosticsConfig::resolve(
        None,
        None,
        None,
        None,
        None,
        &env_empty(),
        true,
        &dist_dir(),
    )
    .unwrap();
    assert_eq!(config.category, "gmeow");
}

#[test]
fn artifacts_parsing() {
    let cases: &[(&str, &[&str])] = &[
        ("all", &["json", "sarif", "html"]),
        ("none", &[]),
        ("json,sarif", &["json", "sarif"]),
        ("sarif,json", &["json", "sarif"]),
        ("HTML", &["html"]),
    ];
    for (raw, expected) in cases {
        let config = DiagnosticsConfig::resolve(
            None,
            Some(raw),
            None,
            None,
            None,
            &env_empty(),
            true,
            &dist_dir(),
        )
        .unwrap();
        assert_eq!(config.artifacts, btreeset(expected), "artifacts={raw}");
    }
}

#[test]
fn unknown_artifact_token_hard_fails() {
    for raw in ["json,xml", "pdf", "json,,sarif,bogus"] {
        let err = DiagnosticsConfig::resolve(
            None,
            Some(raw),
            None,
            None,
            None,
            &env_empty(),
            true,
            &dist_dir(),
        )
        .expect_err(&format!("raw={raw} must hard-fail"));
        // The hard failure is the substrate's own downcastable kind, carrying the
        // stable registered code — not a bespoke error enum.
        assert!(
            err.is::<UnknownArtifactKind>(),
            "raw={raw}: expected UnknownArtifactKind, got {err}"
        );
        assert_eq!(err.code(), UnknownArtifactKind::register(), "raw={raw}");
    }
}

#[test]
fn invalid_console_token_hard_fails() {
    let err = DiagnosticsConfig::resolve(
        Some("loud"),
        None,
        None,
        None,
        None,
        &env_empty(),
        true,
        &dist_dir(),
    )
    .expect_err("an unknown console token must hard-fail");
    assert!(
        err.is::<UnknownConsoleMode>(),
        "expected UnknownConsoleMode, got {err}"
    );
    assert_eq!(err.code(), UnknownConsoleMode::register());
}

#[test]
fn directory_default_is_flat_dist_without_a_category() {
    for is_tty in [true, false] {
        let config = DiagnosticsConfig::resolve(
            None,
            None,
            None,
            None,
            None,
            &env_empty(),
            is_tty,
            &dist_dir(),
        )
        .unwrap();
        assert_eq!(config.directory, dist_dir());
    }
}

#[test]
fn directory_is_category_scoped_when_category_explicit() {
    for is_tty in [true, false] {
        let config = DiagnosticsConfig::resolve(
            None,
            None,
            None,
            None,
            Some("lint"),
            &env_empty(),
            is_tty,
            &dist_dir(),
        )
        .unwrap();
        assert_eq!(
            config.directory,
            dist_dir().join("diagnostics").join("lint")
        );
    }
}

#[test]
fn directory_is_category_scoped_when_category_from_env() {
    let mut env = env_empty();
    env.insert("GMEOW_DIAGNOSTICS_CATEGORY".to_owned(), "rust".to_owned());
    let config =
        DiagnosticsConfig::resolve(None, None, None, None, None, &env, true, &dist_dir()).unwrap();
    assert_eq!(
        config.directory,
        dist_dir().join("diagnostics").join("rust")
    );
}

#[test]
fn explicit_directory_flag_wins_in_both_modes() {
    let explicit = PathBuf::from("/tmp/explicit-dir");
    for is_tty in [true, false] {
        let config = DiagnosticsConfig::resolve(
            None,
            None,
            Some(&explicit),
            None,
            Some("lint"),
            &env_empty(),
            is_tty,
            &dist_dir(),
        )
        .unwrap();
        assert_eq!(config.directory, explicit);
    }
}

#[test]
fn env_directory_wins_over_default_off_tty() {
    let env_dir = PathBuf::from("/tmp/env-dir");
    let mut env = env_empty();
    env.insert(
        "GMEOW_DIAGNOSTICS_DIR".to_owned(),
        env_dir.to_str().unwrap().to_owned(),
    );
    let config =
        DiagnosticsConfig::resolve(None, None, None, None, None, &env, false, &dist_dir()).unwrap();
    assert_eq!(config.directory, env_dir);
}

#[test]
fn is_tty_falls_back_to_stderr_terminal() {
    let is_tty = std::io::IsTerminal::is_terminal(&std::io::stderr());
    let config = DiagnosticsConfig::resolve(
        Some("auto"),
        None,
        None,
        None,
        None,
        &env_empty(),
        is_tty,
        &dist_dir(),
    )
    .unwrap();
    let expected = if is_tty {
        ConsoleMode::Pretty
    } else {
        ConsoleMode::Text
    };
    assert_eq!(config.console, expected);
}

fn btreeset(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}
