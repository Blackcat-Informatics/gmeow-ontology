// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production-surface proof that the SHIPPED `gmeow` binary can RE-EMIT a stored GMN-1
//! document at a target dialect major across an authored inter-version correspondence —
//! the `Commands::Gmn { Migrate }` clap dispatch in `src/lib.rs`, driven through
//! `assert_cmd`. It drives the version-migration executor
//! (`gmeow_lang_bridge::GmnMigration::migrate`) end-to-end over the built binary
//! and asserts:
//!
//! * a stored v1 document migrates to v2 — a ¬→! rename, a ⊑ native survivor, and a
//!   bridged ⊻→^ drop — with the header re-stamped and the preservation JUDGMENT reported;
//! * an operator the target major drops with NO covering rewrite HARD-FAILS with the named
//!   `lang:GmnUnbridgedGlyphDrop` class and a non-zero exit — never a silent repair.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

/// The built `gmeow` binary.
fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

/// The repo root (this crate lives at `crates/gmeow-cli`). Absolute so the test is
/// insensitive to the process CWD `cargo`/`nextest` chooses.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root canonicalizes")
}

/// The lang codebook/dictionary source (the dictionary + executable glyph registry).
fn lang_module() -> PathBuf {
    repo_root().join("slices/grounding/lang/module.ttl")
}

/// The authored synthetic v1 → v2 migration demonstrator (correspondence + rewrites +
/// the target major's `gmeow:gmnVersionDefinesOperator` native inventory).
fn migrations() -> PathBuf {
    repo_root().join("slices/grounding/lang/examples/gmn-migration.ttl")
}

/// The demonstrator correspondence IRI.
const CORRESPONDENCE: &str =
    "https://blackcatinformatics.ca/gmeow/examples/lang/gmnMigrationVSrcToVTgt";

/// Write `contents` to a `.gmn` file in a fresh temp dir and return both (the dir keeps the
/// file alive for the command run).
fn stored_doc(contents: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = dir.path().join("stored.gmn");
    fs::write(&path, contents).expect("write stored .gmn");
    (dir, path)
}

// ── success: a stored v1 document re-emitted at v2 ───────────────────────────────

/// `gmeow gmn migrate` re-emits a stored v1 document at v2: the ¬→! rename and the bridged
/// ⊻→^ drop are applied, the ⊑ native survivor is unchanged, the `@gmn{v: …}` header is
/// re-stamped 1 → 2, and the preservation JUDGMENT (never a boolean) is reported on stderr.
#[test]
fn gmn_migrate_reemits_stored_document_at_target_major() {
    // A stored source-major document using logic:not (¬), logic:subClassOf (⊑), and the
    // retired xor operator (⊻, resolved via the migration leg's own rewrite).
    let (_dir, doc) = stored_doc(
        "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n\
         @ℒ{s:ex__a,p:ex__rel,o:¬}\n\
         @ℒ{s:ex__b,p:ex__rel,o:⊑}\n\
         @ℒ{s:ex__c,p:ex__rel,o:⊻}\n",
    );
    gmeow()
        .args([
            "gmn",
            "migrate",
            doc.to_str().unwrap(),
            "--correspondence",
            CORRESPONDENCE,
            "--migrations",
            migrations().to_str().unwrap(),
            "--lang-module",
            lang_module().to_str().unwrap(),
        ])
        .assert()
        .success()
        // the header is re-stamped to the target major …
        .stdout(predicate::str::contains("@gmn{v: 2,"))
        // … the ¬→! rename is applied …
        .stdout(predicate::str::contains("o:!}"))
        // … the ⊑ native survivor is unchanged …
        .stdout(predicate::str::contains("o:⊑}"))
        // … the bridged ⊻→^ drop is applied …
        .stdout(predicate::str::contains("o:^}"))
        // … and neither source operator glyph survives verbatim.
        .stdout(predicate::str::contains("o:¬}").not())
        .stdout(predicate::str::contains("o:⊻}").not())
        // the crossing's preservation JUDGMENT is surfaced, not a boolean.
        .stderr(predicate::str::contains(
            "preservation logic:ExactPreservation",
        ))
        .stderr(predicate::str::contains("3 operator(s) migrated"));
}

// ── hard fail: an unbridged glyph drop ───────────────────────────────────────────

/// A stored document using an operator the TARGET major does not define natively AND that
/// the migration authors NO covering rewrite for HARD-FAILS with the named
/// `lang:GmnUnbridgedGlyphDrop` class and a non-zero exit — no silent repair or drop.
#[test]
fn gmn_migrate_unbridged_drop_hard_fails_with_named_class() {
    // math:pi (π) is an adopted registry glyph the synthetic target major does NOT list in
    // its gmeow:gmnVersionDefinesOperator inventory and no rewrite bridges.
    let (_dir, doc) = stored_doc(
        "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n\
         @μ{s:ex__d,p:ex__rel,o:π}\n",
    );
    gmeow()
        .args([
            "gmn",
            "migrate",
            doc.to_str().unwrap(),
            "--correspondence",
            CORRESPONDENCE,
            "--migrations",
            migrations().to_str().unwrap(),
            "--lang-module",
            lang_module().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("lang:GmnUnbridgedGlyphDrop"))
        .stderr(predicate::str::contains(
            "https://blackcatinformatics.ca/math/pi",
        ));
}

// ── hard fail: a document out of the crossing's source window ─────────────────────

/// A stored document whose `@gmn{v: …}` header pins a major OTHER than the migration's
/// source major HARD-FAILS (exit 1) — the document is outside the crossing's source window,
/// never migrated on a guess.
#[test]
fn gmn_migrate_out_of_window_source_major_hard_fails() {
    let (_dir, doc) = stored_doc(
        "@gmn{v: 2, aliases: dict-v3, glyphs: 2}\n\
         @ℒ{s:ex__a,p:ex__rel,o:¬}\n",
    );
    gmeow()
        .args([
            "gmn",
            "migrate",
            doc.to_str().unwrap(),
            "--correspondence",
            CORRESPONDENCE,
            "--migrations",
            migrations().to_str().unwrap(),
            "--lang-module",
            lang_module().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "outside the crossing's source window",
        ));
}
