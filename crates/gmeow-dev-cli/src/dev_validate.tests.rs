// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_validate::validate_all::MergedShacl;

/// Repo root: this crate's manifest is `<repo>/crates/gmeow-dev-cli`.
fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root must resolve from crates/gmeow-dev-cli")
}

/// The regression guard that binds the PRODUCTION call site. The historical
/// original defect was `dev_validate` passing empty DSL dir/shape arguments to
/// `ValidationRun::run`, so the mapping/statement/test DSL SHACL phases never
/// executed on `make validate`. The other guards check the resolver, the engine,
/// and the help-text — but NOT the args `validate()` actually assembles. This one
/// does: it drives the real assembly ([`authored_source_invocation`], the single
/// place those args are built) against the real repository and asserts every DSL
/// surface is wired. Reverting any surface to an empty dir or a `None`/empty
/// shapes text makes this FAIL, on every `make check`, with no `generated/`
/// dependency — so the exact original regression can no longer pass green.
#[test]
fn authored_source_invocation_wires_every_dsl_surface() {
    let root = repo_root();
    // `MergedShacl::Live` is a stand-in so the assembly does not touch
    // `generated/`; the DSL wiring under test is independent of it.
    let inv = authored_source_invocation(&root, false, false, MergedShacl::Live)
        .expect("authored-source invocation must assemble on the real repo");

    assert!(
        !inv.source_paths.is_empty(),
        "the authored source set must be non-empty"
    );
    // Positional args 3 and 4 to ValidationRun::run — the mapping/statement DSL
    // directories. Empty here IS the historical original defect.
    assert!(
        !inv.mapping_dsl_dir.is_empty(),
        "mapping DSL dir (ValidationRun::run arg 3) must be wired, not empty"
    );
    assert!(
        !inv.statement_dsl_dir.is_empty(),
        "statement DSL dir (ValidationRun::run arg 4) must be wired, not empty"
    );
    // The three committed DSL shape texts + the test DSL dir carried on options.
    for (label, value) in [
        ("mapping_shapes_ttl", &inv.options.mapping_shapes_ttl),
        ("statement_shapes_ttl", &inv.options.statement_shapes_ttl),
        ("test_dsl_shapes_ttl", &inv.options.test_dsl_shapes_ttl),
    ] {
        assert!(
            value.as_deref().is_some_and(|s| !s.trim().is_empty()),
            "options.{label} must be wired with non-empty committed shapes text"
        );
    }
    assert!(
        inv.options
            .test_dsl_dir
            .as_deref()
            .is_some_and(|d| !d.is_empty()),
        "options.test_dsl_dir must be wired, not None/empty"
    );
}
