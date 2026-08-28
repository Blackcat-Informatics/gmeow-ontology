// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Engine-level tests for the repo-free `gmeow validate <data>` path
//! ([`gmeow_validate::data_validate`]). These exercise the Rust orchestration
//! directly against the committed `gmeow.gts` bundle, independent of the Python
//! CLI surface — the CLI test asserts the wheel-resolution and rendering on top.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use gmeow_validate::data_validate;
use purrdf::RdfDataset;

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

struct BundleFixture {
    bytes: Vec<u8>,
    shapes: data_validate::Tier1Shapes,
    dataset: Arc<RdfDataset>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One process-wide, read-only view of the exact corpus selected by the test runner.
///
/// The explicit pre-test producer publishes the imported dataset and extracted shape
/// union. A missing, stale, corrupt, or differently identified product is terminal;
/// this fixture has no route that can decode, derive, or publish a replacement.
fn bundle_fixture() -> &'static BundleFixture {
    static FIXTURE: OnceLock<BundleFixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let root = repo_root();
        let bytes = gmeow_bundle_import::load_authenticated_source_bytes(&root)
            .expect("load the producer-selected bundle bytes");
        let cache_root = std::env::var_os("GMEOW_BUNDLE_IMPORT_CACHE")
            .map(PathBuf::from)
            .expect("GMEOW_BUNDLE_IMPORT_CACHE selects the produced immutable import");
        let imported = gmeow_bundle_import::load_graph_preserving_cached(&cache_root, &bytes)
            .expect("restore the producer-created bundle import without rebuilding it");
        let shapes_ttl = String::from_utf8(
            gmeow_bundle_import::load_authenticated_corpus_artifact(
                &root,
                "validate-production-shapes.ttl",
            )
            .expect("load the producer-extracted validation shapes without rebuilding them"),
        )
        .expect("authenticated validation shape union is UTF-8");
        let dataset = imported.dataset;
        let shapes =
            data_validate::Tier1Shapes::from_shapes_and_ontology(&shapes_ttl, Arc::clone(&dataset))
                .expect("parse the producer-extracted validation shapes once");
        BundleFixture {
            bytes,
            shapes,
            dataset,
        }
    })
}

fn run(
    data_bytes: &[u8],
    data_format: &str,
    origin: &str,
    deep: bool,
) -> gmeow_errors::Result<gmeow_errors::Report> {
    let fixture = bundle_fixture();
    data_validate::run_with(
        data_validate::BundleParts {
            gts_bytes: &fixture.bytes,
            shapes: &fixture.shapes,
            dataset: fixture.dataset.as_ref(),
        },
        data_bytes,
        data_format,
        NS,
        origin,
        deep,
    )
}

/// A data graph that trips the Tier-1 shape surface. Its problems are a
/// disjoint identity-axis overtyping (error) and a frame-less Event carrying an
/// `eventType` so only the frame-relativity warning fires (warning). Under the
/// closed-world validation-shape derivation it additionally trips the sh:not
/// disjointness pair (Honorific-shape / PronounSet-shape), for a pinned
/// 3-errors-1-warning shape. rdfs:domain/range are open-world INFERENCE axioms
/// (open-world by default, no ClosedWorldClosure opt-in on these properties), so
/// they derive NO domain/range validation shape and the fixture's committedAgent
/// / intentionGoal / eventType edges no longer trip a range/domain error. The
/// Commitment is fully WELL-FORMED (beneficiary included): its exactly-one /
/// at-least-one obligations now ride the projected declarative surface, and a
/// missing-beneficiary counter-witness would double-fire while the bundled
/// shapes-archive still carries the retired hand-authored twin — that
/// obligation's fail witness lives in `conformance_teleology` on the live
/// production shape union instead.
const FAIL_TTL: &str = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <https://example.org/> .

ex:p a gmeow:PronounSet, gmeow:Honorific .

ex:c a gmeow:Commitment ;
    gmeow:committedAgent ex:agent ;
    gmeow:commitmentBeneficiary ex:beneficiary ;
    gmeow:intentionGoal ex:goal .

ex:e1 a gmeow:Event ;
    gmeow:eventType ex:meeting .
"#;

#[test]
fn fail_fixture_yields_three_errors_one_warning_with_locations() {
    let report =
        run(FAIL_TTL.as_bytes(), "turtle", "fixture-fail.ttl", false).expect("validate_data run");

    let errors: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.severity == gmeow_errors::Severity::Error)
        .collect();
    let warnings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.severity == gmeow_errors::Severity::Warning)
        .collect();

    assert_eq!(
        errors.len(),
        3,
        "expected exactly three errors, got: {errors:#?}"
    );
    assert_eq!(
        warnings.len(),
        1,
        "expected exactly one warning, got: {warnings:#?}"
    );

    // Assert stable rule identity via (code, source-shape IRI) — not prose.
    // Shapes discovered by running the test with --nocapture:
    //   disjointness: shacl.SPARQLConstraintComponent + IdentityAxisDisjointnessConstraintShape
    //     (the P17 projection of gmeow:identityAxisDisjointness in constraint-shapes.ttl,
    //      the former hand-authored IdentityAxisOrthogonalityShape was migrated to logic:)
    //   frame:        shacl.MinCountConstraintComponent + EventFrameRequirementShape (Warning)
    // The former Commitment-mediation leg (MinCountConstraintComponent +
    // the retired hand-authored CommitmentShape) migrated to the projected
    // Commitment-shape; its fail witness rides
    // `conformance_teleology::commitment_without_beneficiary_fails_on_union`,
    // and the fixture's Commitment is now fully well-formed.
    const IDENTITY_SHAPE: &str = "IdentityAxisDisjointnessConstraintShape";
    const FRAME_SHAPE: &str = "EventFrameRequirementShape";

    assert!(
        errors.iter().any(|f| {
            f.code == "shacl.SPARQLConstraintComponent"
                && f.detail
                    .as_deref()
                    .is_some_and(|d| d.contains(IDENTITY_SHAPE))
        }),
        "missing P9 disjointness error (IdentityAxisDisjointnessConstraintShape / SPARQLConstraintComponent)"
    );
    assert!(
        warnings[0].code == "shacl.MinCountConstraintComponent"
            && warnings[0]
                .detail
                .as_deref()
                .is_some_and(|d| d.contains(FRAME_SHAPE))
            && warnings[0].severity == gmeow_errors::Severity::Warning,
        "warning is not the frame-relativity one (EventFrameRequirementShape)"
    );

    // Every finding carries a location: the data file as the physical artifact
    // (for SARIF) and the focus-node IRI as the logical anchor.
    for finding in &report.findings {
        let loc = finding
            .locations
            .first()
            .unwrap_or_else(|| panic!("finding has no location: {finding:?}"));
        assert_eq!(loc.path.as_deref(), Some("fixture-fail.ttl"));
        assert!(loc.logical.is_some(), "finding lacks a logical anchor");
    }
}

#[test]
fn clean_fixture_passes() {
    let clean = "@prefix ex: <https://example.org/> .\nex:a ex:b ex:c .\n";
    let report = run(clean.as_bytes(), "turtle", "clean.ttl", false).expect("run clean");
    assert_eq!(report.error_count(), 0, "clean graph reported errors");
    assert_eq!(report.warning_count(), 0, "clean graph reported warnings");
}

#[test]
fn nquads_named_graph_is_flattened_and_validated() {
    // The two type quads sit in a named graph; flattening must surface them so
    // the disjointness checks fire (the P9 IdentityAxisDisjointnessConstraintShape
    // plus the closed-world sh:not pair Honorific-shape / PronounSet-shape).
    let nq = concat!(
        "<https://example.org/p> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ",
        "<https://blackcatinformatics.ca/gmeow/PronounSet> <https://example.org/g> .\n",
        "<https://example.org/p> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ",
        "<https://blackcatinformatics.ca/gmeow/Honorific> <https://example.org/g> .\n",
    );
    let report = run(nq.as_bytes(), "nquads", "g.nq", false).expect("run nq");
    assert_eq!(
        report.error_count(),
        3,
        "named-graph quads were not flattened"
    );
}

#[test]
fn json_ld_is_parsed_as_rdf() {
    let jsonld = r#"{"@context":{"gmeow":"https://blackcatinformatics.ca/gmeow/"},
        "@id":"https://example.org/p",
        "@type":["gmeow:PronounSet","gmeow:Honorific"]}"#;
    let report = run(jsonld.as_bytes(), "json-ld", "p.jsonld", false).expect("run jsonld");
    assert_eq!(report.error_count(), 3, "JSON-LD was not validated as RDF");
}

#[test]
fn unknown_format_hard_fails() {
    let err = run(b"{}", "application/json", "x.json", false)
        .expect_err("JSON instance is not an RDF format");
    assert!(err.message().contains("parse error") || err.message().contains("unsupported"));
}

/// An individual typed into two classes the bundled TBox declares
/// `owl:disjointWith` (`gmeow:PhysicalObject` ⊥ `gmeow:Agent`). This is a DL
/// entailment, NOT a SHACL shape, so Tier-1 cannot see it — only the merged
/// `--deep` chase forces the individual into `owl:Nothing`.
const DEEP_INCONSISTENT_TTL: &str = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <https://example.org/> .

ex:rover a gmeow:PhysicalObject, gmeow:Agent .
"#;

fn has_code(report: &gmeow_errors::Report, code: &str) -> bool {
    report.findings.iter().any(|f| f.code == code)
}

fn any_deep_code(report: &gmeow_errors::Report) -> bool {
    report
        .findings
        .iter()
        .any(|f| f.code.starts_with("validate.deep."))
}

/// AC1, off-gated (`heavy_offgate`): the consumer `--deep` pass reasons over the
/// user data merged with the WHOLE bundled TBox via the native chase — an
/// engine-heavy full-fold run, like `ontology_entailments`. It remains an exhaustive
/// `maint-heavy` proof; focused default-lane coverage of the same
/// merge→inconsistency path is the gmeow-logic unit
/// test `reason_all_with_data_merges_user_abox_into_bundle_tbox` (tiny TBox) plus the
/// on-gate `deep_pass_failure_*` / `deep_false_*` tests.
#[test]
fn deep_surfaces_entailed_inconsistency_tier1_misses_heavy_offgate() {
    // Tier-1 (deep=false) sees nothing: PhysicalObject ⊥ Agent is not a structural
    // shape, so the data passes the default reasoner-free pass with no deep findings.
    let tier1 = run(
        DEEP_INCONSISTENT_TTL.as_bytes(),
        "turtle",
        "deep.ttl",
        false,
    )
    .expect("tier-1 run");
    assert!(
        !any_deep_code(&tier1),
        "the default pass must emit no validate.deep.* findings"
    );
    assert!(
        !has_code(&tier1, "validate.deep.inconsistent"),
        "Tier-1 alone must NOT surface the entailed inconsistency"
    );

    // Tier-2 (deep=true) merges the data with the bundle TBox and the native DL
    // reasoner forces the individual into owl:Nothing — an error Tier-1 missed.
    let deep = run(DEEP_INCONSISTENT_TTL.as_bytes(), "turtle", "deep.ttl", true).expect("deep run");
    assert!(
        has_code(&deep, "validate.deep.inconsistent"),
        "--deep must surface the entailed inconsistency: {:?}",
        deep.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );
    assert!(
        deep.error_count() > tier1.error_count(),
        "the entailed inconsistency must raise the error count over Tier-1 alone"
    );
}

/// An under-specified A-Box that drives the D5 abductive "what to ADD" wing on the
/// PRODUCTION consumer surface (`data_validate::run`, the `gmeow validate <rdf>` entry).
///
/// `ex:c` is a `gmeow:Commitment` relator carrying only `committedAgent` — its
/// beneficiary/intentionGoal relata are MISSING (relatum-completion candidates); `ex:i`
/// is a bare `gmeow:Item` (sortal-specialization candidate). Each is marked
/// `gmeow:graphBoxRole gmeow:boxABox` so the producer's A-Box scoping admits it. Tier-1 is
/// reasoner-free, so this is the honest ASSERTED-ONLY surface.
const UNDERSPEC_ABDUCTIVE_TTL: &str = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <https://example.org/> .

ex:c a gmeow:Commitment ;
    gmeow:graphBoxRole gmeow:boxABox ;
    gmeow:committedAgent ex:agent .

ex:i a gmeow:Item ;
    gmeow:graphBoxRole gmeow:boxABox .
"#;

/// C1 on the production surface: the real `data_validate::run` (the `gmeow validate <rdf>`
/// CLI entry) over an under-specified A-Box must now emit `advice.abductive.<case>`
/// advisories naming the minimal element to ADD — the wing was previously wired only into
/// the pipeline / dev-gate paths (which only ever see VALID input → 0 output), so a real
/// user's under-specified model got ZERO abductive advice. This asserts, in order: that
/// ≥1 `advice.abductive.*` advisory fires (the relator + the sortal case); that EVERY
/// abductive-tier finding (advice + warrant) is a `Severity::Note` — the tier NEVER
/// fabricates a hard Error/Warning and NEVER gates the pass/fail verdict; that each
/// abductive advisory carries its warrant twin (a paired `advice.abductive.warrant.*`
/// Note), i.e. the dual projection rode the ledger with a real warrant edge; and that the
/// output is DETERMINISTIC — the same input twice yields identical abductive codes (a
/// proxy for "the producer only READ the graph; the base graph is unmutated").
#[test]
fn abductive_wing_fires_on_the_real_validate_cli_path() {
    let report = run(
        UNDERSPEC_ABDUCTIVE_TTL.as_bytes(),
        "turtle",
        "underspec.ttl",
        false,
    )
    .expect("validate run");

    let abductive: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code.starts_with("advice.abductive."))
        .collect();
    assert!(
        !abductive.is_empty(),
        "the abductive wing must fire on the real consumer path: {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );

    // The tier is ADVISORY: every abductive finding (advice + warrant) is a Note. No hard
    // Error/Warning is fabricated by the abductive tier, so it can never gate the verdict.
    for f in &abductive {
        assert_eq!(
            f.severity,
            gmeow_errors::Severity::Note,
            "abductive finding {} must be a Note, never a gating severity",
            f.code
        );
    }

    // At least one candidate names the missing Commitment relatum, and each advice carries
    // its warrant twin (the dual projection rode the ledger).
    let advice_codes: Vec<&str> = abductive
        .iter()
        .map(|f| f.code.as_str())
        .filter(|c| !c.starts_with("advice.abductive.warrant."))
        .collect();
    assert!(
        advice_codes.iter().any(|c| c.contains(".Commitment.")),
        "an abductive advisory must name the under-specified Commitment relator: {advice_codes:?}"
    );
    for code in &advice_codes {
        // The warrant twin shares the advice code's `<discipline>.<digest>` tail under the
        // `advice.abductive.warrant.` prefix (see abductive.rs::build_suggestion).
        let tail = code
            .strip_prefix("advice.abductive.")
            .expect("advice.abductive. prefix");
        let want_warrant = format!("advice.abductive.warrant.{tail}");
        assert!(
            abductive.iter().any(|f| f.code == want_warrant),
            "advice {code} must carry its warrant twin {want_warrant}: {:?}",
            abductive.iter().map(|f| &f.code).collect::<Vec<_>>()
        );
    }

    // Determinism / read-only: the same input twice yields the identical abductive code set.
    let report2 = run(
        UNDERSPEC_ABDUCTIVE_TTL.as_bytes(),
        "turtle",
        "underspec.ttl",
        false,
    )
    .expect("second validate run");
    let codes = |r: &gmeow_errors::Report| -> Vec<String> {
        let mut v: Vec<String> = r
            .findings
            .iter()
            .filter(|f| f.code.starts_with("advice.abductive."))
            .map(|f| f.code.clone())
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        codes(&report),
        codes(&report2),
        "the abductive wing must be deterministic (same input ⇒ same advice codes)"
    );
}

/// A conforming input must still PASS: the abductive Notes ride alongside without changing
/// the pass/fail verdict. A clean graph guard-matches no A-Box gap subject, so no abductive
/// advice fires and no bundle self-advice leaks in — 0 errors, 0 warnings.
#[test]
fn abductive_wing_does_not_gate_a_conforming_input() {
    let clean = "@prefix ex: <https://example.org/> .\nex:a ex:b ex:c .\n";
    let report = run(clean.as_bytes(), "turtle", "clean.ttl", false).expect("run clean");
    assert_eq!(report.error_count(), 0, "clean graph must report no errors");
    assert_eq!(
        report.warning_count(),
        0,
        "clean graph must report no warnings"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.code.starts_with("advice.abductive.")),
        "a conforming graph must not surface abductive advice (no bundle self-advice): {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );
}

#[test]
fn deep_false_is_the_reasoner_free_default() {
    // AC3: the pinned Tier-1 fixture under the default (deep=false) keeps its exact
    // 3-errors-1-warning shape AND carries no validate.deep.* findings — the deep
    // reasoner does not run without the flag.
    let report = run(FAIL_TTL.as_bytes(), "turtle", "fail.ttl", false).expect("tier-1 run");
    assert_eq!(
        report.error_count(),
        3,
        "Tier-1 default error shape changed"
    );
    assert_eq!(
        report.warning_count(),
        1,
        "Tier-1 default warning shape changed"
    );
    assert!(
        !any_deep_code(&report),
        "no validate.deep.* findings may appear without --deep"
    );
}
