// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// A synthetic per-slice fact: covers the given dimension local names, with the
/// given bounded coverage fraction.
fn fact(fraction: f64, covers: &[&str]) -> DocSliceFacts {
    DocSliceFacts {
        subject: "https://blackcatinformatics.ca/gmeow/documentation/slice/zoo".to_owned(),
        documents: "https://blackcatinformatics.ca/gmeow/slices/zoo".to_owned(),
        covers: covers.iter().map(|s| (*s).to_owned()).collect(),
        coverage_fraction: fraction,
        earned: None,
        asserted: None,
    }
}

#[test]
fn score_is_the_consumed_coverage_fraction() {
    // The axis score is the slice's bounded coverage fraction verbatim — the
    // producer consumes the docs computation, never recomputes a dimension.
    let f = fact(0.5833, &["dimDefinition", "dimLabel"]);
    let scored = score_and_advice(&f);
    assert!(
        (scored.score - 0.5833).abs() < 1e-12,
        "score is the fraction"
    );
}

#[test]
fn advisories_name_the_uncovered_full_dimensions() {
    // A slice covering only the two Minimal dimensions is short every other
    // FULL-anchor dimension; each missing one is a ranked uplift advisory, and a
    // covered one is not advised.
    let f = fact(0.1667, &["dimDefinition", "dimLabel"]);
    let scored = score_and_advice(&f);
    let full_extra = MaturityAnchor::Full.intent().len() - 2; // minus Definition+Label
    assert_eq!(
        scored.findings.len(),
        full_extra,
        "one advisory per uncovered FULL dimension"
    );
    // The covered dimensions are never advised.
    assert!(
        !scored
            .findings
            .iter()
            .any(|f| f.message.contains("dimDefinition") || f.message.contains("dimLabel")),
        "a covered dimension is not an uplift target"
    );
    // A genuinely-missing FULL dimension is advised, naming the slice.
    assert!(
        scored
            .findings
            .iter()
            .any(|f| f.message.contains("dimExample") && f.message.contains("slices/zoo")),
        "an uncovered FULL dimension is named for the slice"
    );
}

/// Scaffold a temp repo root carrying exactly one real slice (copied from the
/// committed `gmeow-docs-model` single-slice fixture). Returns the owning
/// [`tempfile::TempDir`], the root, and the slice directory. No `generated/`
/// tree is created.
///
/// Two scaffolded roots never collide because each owns a distinct
/// `TempDir`, and each tree is removed when its guard drops — on success, on
/// panic, and on early return. The caller must bind the guard
/// (`let (_tmp, root, slice) = scaffold_single_slice_root();`); a bare `_`
/// binding would drop it at once and delete the tree out from under the test.
fn scaffold_single_slice_root() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let guard = tempfile::tempdir().expect("create temp dir");
    let root = guard.path().join("gmeow-docmaturity-det");
    let slice_dir = root.join("slices").join("fixture").join("single");
    std::fs::create_dir_all(&slice_dir).expect("mkdir slice");
    // The committed fixture lives in the sibling gmeow-docs-model crate — it moved
    // there with the model when `gmeow-docs` was split, and this reader was left
    // pointing at the old `crates/docs/tests/fixtures/` path, so the copy failed and
    // the determinism check could not run at all.
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("docs-model")
        .join("tests")
        .join("fixtures")
        .join("single-slice");
    for file in ["manifest.ttl", "module.ttl"] {
        std::fs::copy(fixture.join(file), slice_dir.join(file))
            .unwrap_or_else(|e| panic!("copy fixture {file}: {e}"));
    }
    (guard, root, slice_dir)
}

/// Minimal but valid constraint-catalog N-Quads: one `gmeow:ValidationRule` with a
/// `gmeow:ruleCode`.
fn sample_catalog_bytes() -> Vec<u8> {
    let graph = "https://blackcatinformatics.ca/gmeow/graph/fanout/catalog/constraint-catalog.nq";
    let rule = "https://blackcatinformatics.ca/gmeow/rule/box-roles-invalid";
    format!(
            "<{rule}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
             <https://blackcatinformatics.ca/gmeow/ValidationRule> <{graph}> .\n\
             <{rule}> <https://blackcatinformatics.ca/gmeow/ruleCode> \"box-roles.invalid\" <{graph}> .\n"
        )
        .into_bytes()
}

/// The DocMaturity axis score is BYTE-IDENTICAL whether the constraint catalog is
/// sourced from THIS run's live bytes (a cold tree with no `generated/`) or read
/// from disk (a warm tree carrying the SAME bytes). This is the determinism the
/// two-generation sync gate needs: without the fix, the cold run's absent catalog
/// fails the model build and collapses the axis to the model-unavailable floor of
/// `0.0`, while the warm run scores the real fraction — a divergent
/// `graph/quality-assessment`.
#[test]
fn doc_maturity_score_identical_live_bytes_vs_disk_catalog() {
    let catalog = sample_catalog_bytes();
    let empty = purrdf::RdfDatasetBuilder::new()
        .freeze()
        .expect("empty dataset");
    let slice_iri = "https://blackcatinformatics.ca/gmeow/slices/fixture-single".to_owned();

    // The repo arm reads only the documentation model and the checkout anchor, so
    // the slice's own file map is irrelevant here — an empty map is the honest input.
    let files = std::collections::BTreeMap::new();

    // COLD tree: no generated/ on disk, catalog primed from LIVE bytes.
    let (_live_tmp, live_root, live_slice) = scaffold_single_slice_root();
    prime_repo_facts(&live_root, Some(&catalog));
    let live_ctx = ScoreContext::new(
        slice_iri.clone(),
        &files,
        &empty,
        ScoringEnv::Repo {
            slice_dir: live_slice,
        },
    );
    let live = DocMaturity::axis(&live_ctx);

    // WARM tree: the SAME catalog bytes on disk, sourced by the disk path (None).
    let (_warm_tmp, warm_root, warm_slice) = scaffold_single_slice_root();
    std::fs::create_dir_all(warm_root.join("generated").join("catalog")).expect("mkdir generated");
    std::fs::write(
        warm_root
            .join("generated")
            .join("catalog")
            .join("constraint-catalog.nq"),
        &catalog,
    )
    .expect("write catalog");
    prime_repo_facts(&warm_root, None);
    let warm_ctx = ScoreContext::new(
        slice_iri,
        &files,
        &empty,
        ScoringEnv::Repo {
            slice_dir: warm_slice,
        },
    );
    let warm = DocMaturity::axis(&warm_ctx);

    assert_eq!(
        live.score.to_bits(),
        warm.score.to_bits(),
        "DocMaturity score must be byte-identical cold(live) vs warm(disk); live={} warm={}",
        live.score,
        warm.score
    );
    // The live path genuinely BUILT the model — it did not fall back to the vacuous
    // model-unavailable 1.0 the cold disk read would have forced.
    assert!(
        !live
            .findings
            .iter()
            .any(|f| f.message.contains("documentation model could not be built")),
        "the live-bytes path must build the model, never the model-unavailable fallback"
    );
}

#[test]
fn a_fully_covered_full_intent_has_no_advice() {
    // Covering exactly the FULL intent → fraction 1.0, no uplift advice at this
    // axis's own (FULL) measure.
    let covers: Vec<String> = MaturityAnchor::Full
        .intent()
        .iter()
        .map(|d| d.local_name().to_owned())
        .collect();
    let refs: Vec<&str> = covers.iter().map(String::as_str).collect();
    let scored = score_and_advice(&fact(1.0, &refs));
    assert!(
        scored.findings.is_empty(),
        "a FULL-covered slice has no missing-dimension advice"
    );
}
