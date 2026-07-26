// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Corpus regression gate for the native correspondence-soundness pass.
//!
//! The seven correspondence-stack semantic checks (the five alignment checks + the two
//! FnO back-end checks — including the SOLE native enforcer of Constitution Principle 5,
//! the equivalence-collapse gate) live in the wasm-clean
//! `gmeow_logic_compile::projections::correspondence_soundness` pass, driven by the
//! oxigraph-free pipeline edge `stages::correspondence_soundness::lint_correspondence_soundness`.
//!
//! The original parity harness proved this pass byte-identical to the (now deleted)
//! oxigraph-coupled lints over the REAL committed repo corpus. The retired lints are gone,
//! so this harness pins the pass to the committed corpus' full finding CONTENT — every
//! field of every finding, captured as a sorted snapshot. A count-only floor would miss a
//! net-zero swap (one family loses a finding while another gains one); the content snapshot
//! catches any drift in severity, check family, code, instance, the SSSOM-row CURIEs, or the
//! message — not just the total.

use gmeow_logic_compile::projections::correspondence_soundness::ProjectionDiagnostic;
use gmeow_pipeline::stages::correspondence_soundness::lint_correspondence_soundness;

/// The committed corpus' correspondence-soundness finding count. The content snapshot below
/// is the authority; this is a fast floor that fails with a readable count delta before the
/// (larger) snapshot diff. A drift here is a coverage regression: investigate the snapshot,
/// it is NOT a number to blindly re-bless.
/// Lowered 449 -> 443 by the enactment-kernel supersession, and the six lost findings were
/// identified individually rather than re-blessed on sight (the assertion below says not to).
///
/// They are exactly the six alignment cells that were keyed on the retired process-model
/// properties: `gmeow:stepInput` -> `prov:used` / `schema:supply` / `schema:tool`,
/// `gmeow:stepOutput` -> `prov:wasGeneratedBy` / `schema:result`, and
/// `gmeow:hasProcedureStep` -> `schema:step`.
///
/// The MAPPINGS survive: all six targets are re-authored in
/// `slices/core/work-orchestration/mappings/equivalences.ttl` onto the kernel spine
/// (`logic:precondition`, `logic:planBody`), with their confidence and justification intact,
/// so the alignment-target set is still a superset of the pre-branch one. What changed is the
/// SUBJECT namespace, and `referenced_prefixes` scopes the domain-range check to
/// `gmeow:`-subject mappings (correspondence_soundness.rs:289) — so the six cells are no
/// longer domain-range checked.
///
/// That scope reduction is recorded in `.deficiencies` rather than treated as a non-event.
const EXPECTED_FINDING_COUNT: usize = 443;

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

/// Render one finding as a stable, fully-ordered line capturing EVERY field, so a change in
/// any field surfaces in the snapshot diff. `None` options render as `-` (no real CURIE is
/// `-`). Pipe-delimited; the message is last (it is the only field that may contain spaces).
fn canonical_line(d: &ProjectionDiagnostic) -> String {
    let opt = |o: &Option<String>| o.as_deref().unwrap_or("-").to_string();
    format!(
        "{sev} | {check} | {code} | inst={inst} | s={s} | p={p} | o={o} | {msg}",
        sev = d.severity,
        check = d.check,
        code = d.code,
        inst = opt(&d.instance),
        s = opt(&d.subject_id),
        p = opt(&d.predicate_id),
        o = opt(&d.object_id),
        msg = d.message,
    )
}

#[test]
fn native_soundness_pass_matches_committed_corpus() {
    let root = repo_root();

    // The oxigraph-free soundness pass over the committed corpus, allow_network=false
    // (the on-gate path).
    let findings = lint_correspondence_soundness(&root, false)
        .expect("native correspondence-soundness pass should not error");

    // 1. The corpus exercises real findings (the alignment checks emit INFO findings for
    //    unavailable targets at minimum), so an empty result would be a silent no-op rather
    //    than a real regression proof.
    assert!(
        !findings.is_empty(),
        "expected the committed corpus to exercise at least one finding (sanity floor)"
    );

    // 2. Every finding carries a recognized check token — a typo'd or newly-introduced
    //    unknown family must fail loudly here rather than silently riding in the snapshot.
    for finding in &findings {
        assert!(
            is_known_check(&finding.check),
            "unrecognized soundness check token {:?} in finding: {}",
            finding.check,
            canonical_line(finding)
        );
    }

    // 3. The committed-corpus finding count is pinned (a fast, readable floor before the
    //    content snapshot below).
    assert_eq!(
        findings.len(),
        EXPECTED_FINDING_COUNT,
        "correspondence-soundness finding COUNT drifted from the committed-corpus floor \
         ({EXPECTED_FINDING_COUNT}) to {} — investigate a coverage regression, do not blindly \
         re-bless",
        findings.len()
    );

    // 4. The full finding CONTENT is pinned. Sort for order-independence (the pass does not
    //    promise a stable emission order), then snapshot every field of every finding. This
    //    is what catches a net-zero family swap that the count floor cannot.
    let mut lines: Vec<String> = findings.iter().map(canonical_line).collect();
    lines.sort();
    insta::assert_snapshot!("committed_corpus_findings", lines.join("\n"));
}

/// Whether `check` is one of the nine canonical correspondence-soundness check tokens.
fn is_known_check(check: &str) -> bool {
    matches!(
        check,
        "fno-type"
            | "fno-ref"
            | "inverse-direction"
            | "domain-range"
            | "property-character"
            | "equivalence-collapse"
            | "dc-refinement"
            | "dc-hand-authored"
            | "edoal-entity-kind"
    )
}
