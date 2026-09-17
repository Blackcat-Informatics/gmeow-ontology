// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn auto_resolves_to_pretty_on_a_tty() {
    assert_eq!(ConsoleMode::resolve(None, None, true), ConsoleMode::Pretty);
}

#[test]
fn auto_resolves_to_jsonl_off_a_tty() {
    // The DX rule: a non-TTY agent/pipe gets machine-readable output.
    assert_eq!(ConsoleMode::resolve(None, None, false), ConsoleMode::Jsonl);
}

#[test]
fn env_overrides_the_default() {
    assert_eq!(
        ConsoleMode::resolve(None, Some("text"), true),
        ConsoleMode::Text
    );
    // Case-insensitive, whitespace-trimmed.
    assert_eq!(
        ConsoleMode::resolve(None, Some(" Silent "), false),
        ConsoleMode::Silent
    );
}

#[test]
fn flag_wins_over_env() {
    assert_eq!(
        ConsoleMode::resolve(Some(ConsoleMode::Pretty), Some("jsonl"), false),
        ConsoleMode::Pretty
    );
}

#[test]
fn unknown_env_falls_through_to_default() {
    // An unrecognized env value is ignored, not a hard-fail: Auto default
    // then resolves by TTY.
    assert_eq!(
        ConsoleMode::resolve(None, Some("garbage"), true),
        ConsoleMode::Pretty
    );
    assert_eq!(
        ConsoleMode::resolve(None, Some("garbage"), false),
        ConsoleMode::Jsonl
    );
}

#[test]
fn exit_code_maps_ok_and_failure() {
    let clean = Report::new("t");
    assert_eq!(exit_code(&clean), 0);
    let mut failed = Report::new("t");
    failed.add_finding(Finding::new(gmeow_errors::Severity::Error, "x", "boom"));
    assert_eq!(exit_code(&failed), 1);
}

#[test]
fn write_docs_projection_writes_a_clean_tree() {
    let (_tmp, tmp) = tempdir();
    let mut tree = BTreeMap::new();
    tree.insert("a/b.md".to_owned(), b"hello".to_vec());
    let code = write_docs_projection(&tmp, Ok(tree));
    assert_eq!(code, 0);
    assert_eq!(
        std::fs::read(tmp.join("a/b.md")).unwrap(),
        b"hello".to_vec()
    );
}

#[test]
fn write_docs_projection_removes_stale_members() {
    let (_tmp, tmp) = tempdir();
    std::fs::create_dir_all(tmp.join("stale/nested")).unwrap();
    std::fs::write(tmp.join("stale/nested/old.md"), b"old").unwrap();
    let tree = BTreeMap::from([("live.md".to_owned(), b"live".to_vec())]);
    assert_eq!(write_docs_projection(&tmp, Ok(tree)), 0);
    assert_eq!(std::fs::read(tmp.join("live.md")).unwrap(), b"live");
    assert!(!tmp.join("stale/nested/old.md").exists());
    assert!(!tmp.join("stale").exists());
}

#[test]
fn docs_projection_report_accounts_for_write_skip_and_removal() {
    let (_tmp, tmp) = tempdir();
    std::fs::write(tmp.join("same.md"), b"same").unwrap();
    std::fs::write(tmp.join("changed.md"), b"old").unwrap();
    std::fs::write(tmp.join("stale.md"), b"stale").unwrap();
    let tree = BTreeMap::from([
        ("changed.md".to_owned(), b"new".to_vec()),
        ("same.md".to_owned(), b"same".to_vec()),
    ]);

    let report = reconcile_docs_projection_tree(&tmp, &tree).unwrap();

    assert_eq!(
        report,
        DocsProjectionReport {
            produced: 2,
            written: 1,
            unchanged: 1,
            removed: 1,
        }
    );
    assert_eq!(std::fs::read(tmp.join("changed.md")).unwrap(), b"new");
    assert!(!tmp.join("stale.md").exists());
}

#[test]
fn write_docs_projection_does_not_touch_equal_files() {
    let (_tmp, tmp) = tempdir();
    let tree = BTreeMap::from([("same.md".to_owned(), b"same".to_vec())]);
    assert_eq!(write_docs_projection(&tmp, Ok(tree.clone())), 0);
    let before = std::fs::metadata(tmp.join("same.md"))
        .unwrap()
        .modified()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert_eq!(write_docs_projection(&tmp, Ok(tree)), 0);
    let after = std::fs::metadata(tmp.join("same.md"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(before, after, "equal docs output had its mtime touched");
}

#[test]
fn write_docs_projection_rejects_absolute_member_paths() {
    let (_tmp, tmp) = tempdir();
    let mut tree = BTreeMap::new();
    // An absolute member would replace `dir` outright under `Path::join`,
    // escaping the export directory entirely.
    tree.insert("/etc/passwd".to_owned(), b"pwned".to_vec());
    let code = write_docs_projection(&tmp, Ok(tree));
    assert_eq!(code, 1);
    assert!(!Path::new("/etc/passwd_gmeow_test_marker").exists());
}

#[test]
fn write_docs_projection_rejects_parent_dir_traversal() {
    let (_tmp, tmp) = tempdir();
    let mut tree = BTreeMap::new();
    tree.insert("../escape.md".to_owned(), b"pwned".to_vec());
    let code = write_docs_projection(&tmp, Ok(tree));
    assert_eq!(code, 1);
    assert!(!tmp.parent().unwrap().join("escape.md").exists());
}

/// A fresh temp directory for a single test, owned by a [`tempfile::TempDir`]
/// so it is removed when the guard drops — on success, on panic, and on early
/// return. The caller must bind the guard (`let (_tmp, dir) = tempdir();`);
/// binding it to a bare `_` would drop it immediately and delete the directory
/// out from under the test. The working root is a child of the guard's
/// directory, so even a path-traversal escape one level up stays inside the
/// cleaned-up tree.
fn tempdir() -> (tempfile::TempDir, PathBuf) {
    let guard = tempfile::tempdir().expect("create temp dir");
    let dir = guard.path().join("gmeow-cli-core-test");
    std::fs::create_dir_all(&dir).unwrap();
    (guard, dir)
}

#[test]
fn report_diag_of_an_error_grade_gates() {
    // An Error-grade pre-carrier Diag lowers to a report that is NOT ok and
    // exits 1 — the same gate a carrier-borne Error finding hits.
    use gmeow_errors::grade::{FindingCategory, Grade, Severity, Standpoint};
    let code = gmeow_errors::code::register_code("test.cli-core.pre-carrier.error");
    let diag = Diag::new(
        code,
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        ),
        "config could not be resolved",
    );
    let report = report_diag(diag, "gmeow");
    assert!(!report.ok());
    assert_eq!(report.error_count(), 1);
    assert_eq!(exit_code(&report), 1);
}

#[test]
fn report_diag_of_transient_chatter_is_clean() {
    // A Note/Info Transient chatter Diag lowers to an ok report and exits 0 —
    // chatter never gates.
    let code = gmeow_errors::code::register_code("test.cli-core.pre-carrier.note");
    let report = report_diag(Diag::note(code, "just narrating progress"), "gmeow");
    assert!(report.ok());
    assert_eq!(exit_code(&report), 0);

    let info_code = gmeow_errors::code::register_code("test.cli-core.pre-carrier.info");
    let info_report = report_diag(Diag::info(info_code, "low-severity witness"), "gmeow");
    assert!(info_report.ok());
    assert_eq!(exit_code(&info_report), 0);
}
