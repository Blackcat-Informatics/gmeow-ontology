// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Task 7 (Part A): executable production-surface acceptance tests over the
//! PUBLIC `gmeow_validate::data_validate::run` entry — the exact fn the CLI
//! renders (`commands.rs:572`) — proving the proof-carrying enrichment
//! (remediation / derivation / per-term guidance) reaches the rendered
//! text/JSON/RDF surfaces on a real `--deep` run, not just the internal
//! `enrich`/`guidance`/`remediation` unit tests.
//!
//! The fixture is a self-built, in-memory `gmeow.gts` bundle (never the
//! committed `generated/dist/gmeow.gts`, which is regenerate-only and would
//! make this test depend on an unrelated `make regen` pass): it carries
//! its OWN `shapes-archive` blob (so `data_validate::run` can validate
//! repo-free) plus a small TBox that reasons to a genuine entailed
//! inconsistency under `--deep` (mirroring the `INCONSISTENT_TTL` pattern
//! `crates/validate/src/validate_all.rs`'s Task-5 tests already use) and a
//! test-authored guidance-carrying term (mirroring the shape of Task 4's
//! seeded `gmeow:requiresFrame` guidance, but authored fresh in THIS bundle
//! since the real seeded terms live in `slices/*.ttl` sources that only reach
//! `generated/dist/gmeow.gts` via `make regen`, which this test must not
//! depend on).

use gmeow_errors::Severity;
use gmeow_errors::render::{to_gmeow_rdf, to_json, to_text};
use gmeow_validate::data_validate;
use purrdf::gts_compose::{BlobRow, DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// The rule code the shapes archive's `sh:minCount` violation below always
/// produces (a purrdf-shapes engine convention: `shacl.<ConstraintComponent>`),
/// which the dynamic `shacl.` family in `gmeow_validate::rule_catalog` resolves
/// remediation/help-URI guidance for regardless of which shape fired it.
const SHACL_MINCOUNT_CODE: &str = "shacl.MinCountConstraintComponent";
/// The code the deep-reason pass emits for a forced `owl:Nothing` entailment
/// (`gmeow_validate::codes::VALIDATE_DEEP_INCONSISTENT`).
const DEEP_INCONSISTENT_CODE: &str = "validate.deep.inconsistent";

/// The bundle's default-graph Turtle: a TBox that reasons to a genuine
/// entailed inconsistency (`ex:A rdfs:subClassOf ex:B, ex:C`; `ex:B
/// owl:disjointWith ex:C` forces any `ex:A` instance into `owl:Nothing` —
/// the SAME shape `validate_all.rs`'s `INCONSISTENT_TTL` fixture uses), plus a
/// test-authored guidance-carrying term and the constraint-catalog
/// `gmeow:ValidationRule` node whose `gmeow:ruleCode` matches the SHACL
/// finding the shapes archive below produces, so `enrich_findings`'
/// governing-term join (Part 3, the `RuleGoverningTerm` key) resolves it.
const BUNDLE_GRAPH_TTL: &str = r#"
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex: <https://example.org/> .

ex:A rdfs:subClassOf ex:B .
ex:A rdfs:subClassOf ex:C .
ex:B owl:disjointWith ex:C .

ex:GovernedProperty
    gmeow:howToUse "Author ex:GovernedProperty on every ex:Widget instance." ;
    gmeow:useWhen "Use when a Widget must carry its mandatory identifying property." ;
    gmeow:avoidWhen "Avoid leaving an ex:Widget without ex:GovernedProperty." .

ex:rule/test-governed-property
    a gmeow:ValidationRule ;
    gmeow:ruleCode "shacl.MinCountConstraintComponent" ;
    logic:formalizes ex:GovernedProperty .
"#;

/// The bundle's `shapes-archive` member: a `sh:minCount` shape on
/// `ex:GovernedProperty` — its `sh:path` becomes the SHACL finding's
/// `documented_terms` entry (the `DocumentedTerm` guidance key), the SAME IRI
/// the `RuleGoverningTerm` key above resolves, so a real finding exercises
/// BOTH keys and the render-once dedup.
const SHAPES_TTL: &str = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <https://example.org/> .

ex:WidgetShape
    a sh:NodeShape ;
    sh:targetClass ex:Widget ;
    sh:property [
        sh:path ex:GovernedProperty ;
        sh:minCount 1 ;
        sh:severity sh:Violation ;
        sh:message "ex:Widget must carry ex:GovernedProperty" ;
    ] .
"#;

/// The user data graph: an `ex:Widget` missing `ex:GovernedProperty` (trips
/// the Tier-1 SHACL shape above) plus an `ex:A` instance (trips the Tier-2
/// `--deep` entailed inconsistency against the bundle's TBox).
const DATA_TTL: &str = r#"
@prefix ex: <https://example.org/> .

ex:widget1 a ex:Widget .
ex:x a ex:A .
"#;

/// Build a self-contained `gmeow.gts` bundle carrying [`BUNDLE_GRAPH_TTL`] as
/// its default graph and [`SHAPES_TTL`] as its `shapes-archive` blob — mirrors
/// the real `stage-carrier`/`stage-snapshot` compose (`purrdf::gts_compose`),
/// never the committed `generated/dist/gmeow.gts` (regenerate-only, and this
/// test must stay independent of `make regen`).
fn build_test_bundle() -> Vec<u8> {
    let dataset = purrdf::parse_dataset(BUNDLE_GRAPH_TTL.as_bytes(), "text/turtle", None)
        .expect("bundle graph turtle parses");
    let mut builder = SnapshotBuilder::new();
    builder.add_dataset(&dataset).expect("add_dataset");

    let shapes_archive = purrdf::ustar::write_archive(&[(
        "governed-property-shapes.ttl".to_owned(),
        SHAPES_TTL.as_bytes().to_vec(),
    )])
    .expect("write shapes archive");
    let shapes_blob = BlobRow {
        data: shapes_archive,
        media_type: "application/x-tar".to_owned(),
        rep: "shapes-archive".to_owned(),
    };

    emit_gts(
        &builder,
        "dist",
        None,
        vec![shapes_blob],
        Vec::new(),
        None,
        None,
        None,
        DEFAULT_RSYNCABLE_THRESHOLD,
        &purrdf::gts_compose::MediumPlan::undicted(Some(12)),
    )
    .expect("emit test gts bundle")
}

fn run_deep() -> gmeow_errors::Report {
    let gts = build_test_bundle();
    data_validate::run(DATA_TTL.as_bytes(), "turtle", &gts, NS, "widget.ttl", true)
        .expect("data_validate::run over the test bundle")
}

fn find_code<'a>(report: &'a gmeow_errors::Report, code: &str) -> &'a gmeow_errors::model::Finding {
    report
        .findings
        .iter()
        .find(|f| f.code == code)
        .unwrap_or_else(|| {
            panic!(
                "no finding with code {code}: {:?}",
                report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
            )
        })
}

/// The fixture actually produces both findings this test suite depends on —
/// pinned once so a fixture regression fails loudly here rather than as a
/// confusing downstream assertion failure.
#[test]
fn fixture_produces_the_shacl_and_deep_findings() {
    let report = run_deep();
    let shacl = find_code(&report, SHACL_MINCOUNT_CODE);
    assert_eq!(shacl.severity, Severity::Error);
    assert_eq!(
        shacl.documented_terms,
        vec!["https://example.org/GovernedProperty".to_owned()],
        "the SHACL finding must document ex:GovernedProperty (its sh:path)"
    );
    let deep = find_code(&report, DEEP_INCONSISTENT_CODE);
    assert_eq!(deep.severity, Severity::Error);
}

/// P1: a finding carries a non-empty `remediation`, the text render shows the
/// "how to fix" line + a helpUri, and JSON carries the same payload. Exercises
/// `crate::remediation::attach_remediations` through the PUBLIC
/// `data_validate::run` entry — never the internal unit test alone.
#[test]
fn p1_remediation_reaches_text_and_json() {
    let report = run_deep();
    let shacl = find_code(&report, SHACL_MINCOUNT_CODE);
    assert!(
        !shacl.remediation.is_empty(),
        "the SHACL finding must carry a registry-authored remediation: {shacl:?}"
    );
    let remediation = &shacl.remediation[0];
    assert!(!remediation.text.is_empty());
    assert!(
        remediation.help_uri.is_some(),
        "the remediation must link the catalog: {remediation:?}"
    );

    let text = to_text(&report);
    assert!(
        text.contains(&format!("how to fix: {}", remediation.text)),
        "expected a 'how to fix' line in the text render: {text}"
    );
    assert!(
        text.contains(&format!(
            "see: {}",
            remediation.help_uri.as_deref().unwrap()
        )),
        "expected the remediation's helpUri to render: {text}"
    );

    let json = to_json(&report).expect("json render");
    let value: serde_json::Value = serde_json::from_str(&json).expect("json parses");
    let findings = value["findings"].as_array().expect("findings array");
    let shacl_json = findings
        .iter()
        .find(|f| f["code"] == SHACL_MINCOUNT_CODE)
        .expect("shacl finding in JSON");
    let remediation_json = shacl_json["remediation"]
        .as_array()
        .expect("remediation array in JSON");
    assert!(!remediation_json.is_empty());
    assert_eq!(
        remediation_json[0]["text"].as_str(),
        Some(remediation.text.as_str())
    );
    assert!(remediation_json[0]["help_uri"].as_str().is_some());
}

/// P2: the deep-reason inconsistency finding carries a non-empty
/// `derived_from_quads`, the text render's derivation-citation section lists
/// at least one cited reifier, `antecedents` stays empty and `root_cause`
/// stays `None` (the namespace guard — quad-reifier derivation is a SEPARATE
/// edge from the finding-fingerprint witness DAG), and the `gmeow:` RDF render
/// emits `gmeow:findingDerivedFromQuad`.
#[test]
fn p2_derivation_reaches_text_and_rdf_with_namespace_guard() {
    let report = run_deep();
    let deep = find_code(&report, DEEP_INCONSISTENT_CODE);

    assert!(
        !deep.derived_from_quads.is_empty(),
        "the deep-reason verdict must carry its explain-skeleton derivation: {deep:?}"
    );
    assert!(
        deep.antecedents.is_empty(),
        "antecedents (finding-fingerprint IRIs) must stay empty on a reasoned finding"
    );
    assert!(
        deep.root_cause.is_none(),
        "root_cause (a finding-fingerprint IRI) must stay unset on a reasoned finding"
    );

    let text = to_text(&report);
    let cited = &deep.derived_from_quads[0];
    assert!(
        text.contains(&format!("derived from: {cited}")),
        "expected a derivation-citation line for {cited}: {text}"
    );

    let rdf = to_gmeow_rdf(&report);
    assert!(
        rdf.contains(&format!("<{NS}findingDerivedFromQuad>")),
        "expected gmeow:findingDerivedFromQuad in the RDF render: {rdf}"
    );
    for cited in &deep.derived_from_quads {
        assert!(
            rdf.contains(&format!("<{cited}>")),
            "expected the cited reifier {cited} to appear as an RDF object: {rdf}"
        );
    }
}

/// P3: the SHACL finding resolves guidance from BOTH honest keys (its rule's
/// governing term AND its own `documented_terms`), which resolve to the SAME
/// `ex:GovernedProperty` term here — so a genuinely dual-keyed claim renders
/// exactly once (dedup), never twice. The three modalities all reach the text
/// render, the JSON payload, and the `gmeow:` RDF render. The deep-reason
/// finding, whose rule authors no guidance and which carries no
/// `documented_terms`, is the honest-absence twin.
#[test]
fn p3_guidance_both_keys_dedup_and_honest_absence() {
    let report = run_deep();
    let shacl = find_code(&report, SHACL_MINCOUNT_CODE);

    assert_eq!(
        shacl.guidance.len(),
        3,
        "exactly one claim per modality after RuleGoverningTerm/DocumentedTerm dedup: {:?}",
        shacl.guidance
    );
    let how_to_use = shacl
        .guidance
        .iter()
        .find(|g| g.modality == gmeow_errors::diag::GuidanceModality::HowToUse)
        .expect("howToUse claim");
    let use_when = shacl
        .guidance
        .iter()
        .find(|g| g.modality == gmeow_errors::diag::GuidanceModality::UseWhen)
        .expect("useWhen claim");
    let avoid_when = shacl
        .guidance
        .iter()
        .find(|g| g.modality == gmeow_errors::diag::GuidanceModality::AvoidWhen)
        .expect("avoidWhen claim");
    assert_eq!(
        how_to_use.text,
        "Author ex:GovernedProperty on every ex:Widget instance."
    );
    assert_eq!(
        use_when.text,
        "Use when a Widget must carry its mandatory identifying property."
    );
    assert_eq!(
        avoid_when.text,
        "Avoid leaving an ex:Widget without ex:GovernedProperty."
    );

    let text = to_text(&report);
    assert!(text.contains(&format!("how to use: {}", how_to_use.text)));
    assert!(text.contains(&format!("use when: {}", use_when.text)));
    assert!(text.contains(&format!("avoid when: {}", avoid_when.text)));

    let json = to_json(&report).expect("json render");
    let value: serde_json::Value = serde_json::from_str(&json).expect("json parses");
    let findings = value["findings"].as_array().expect("findings array");
    let shacl_json = findings
        .iter()
        .find(|f| f["code"] == SHACL_MINCOUNT_CODE)
        .expect("shacl finding in JSON");
    let guidance_json = shacl_json["guidance"].as_array().expect("guidance array");
    assert_eq!(
        guidance_json.len(),
        3,
        "JSON must carry the same deduped three-claim guidance: {guidance_json:?}"
    );

    let rdf = to_gmeow_rdf(&report);
    assert!(rdf.contains(&format!("<{NS}findingHowToUse>")));
    assert!(rdf.contains(&format!("<{NS}findingUseWhen>")));
    assert!(rdf.contains(&format!("<{NS}findingAvoidWhen>")));

    // Honest absence: the deep-reason finding's rule authors no guidance and it
    // carries no documented_terms, so neither key resolves anything.
    let deep = find_code(&report, DEEP_INCONSISTENT_CODE);
    assert!(
        deep.documented_terms.is_empty(),
        "a reasoned finding carries no documented_terms: {deep:?}"
    );
    assert!(
        deep.guidance.is_empty(),
        "a finding whose terms author no guidance must carry none (never fabricated): {:?}",
        deep.guidance
    );
}

/// Cross-surface parity (adversary F1): `data_validate::run`'s report carries
/// `report.rules` populated (`rule_catalog::populate_rules` ran) AND at least
/// one finding with a non-empty remediation for a remediable code — proving
/// the CLI/consumer path is enriched. Falsifiable: removing the
/// `enrich_findings` call from `data_validate::run` (`crates/validate/src/data_validate.rs`)
/// makes both assertions fail. Pairs with the pipeline-side coverage in
/// `crates/pipeline/src/stages/validate.rs` (`stage_validate_run_is_enriched_matching_the_cli_path`),
/// which proves the SAME `enrich_findings` fn is exercised on the pipeline
/// validate-stage surface too, so the two consumer paths cannot silently
/// drift apart.
#[test]
fn cross_surface_parity_cli_path_is_enriched() {
    let report = run_deep();
    assert!(
        !report.rules.is_empty(),
        "data_validate::run must populate report.rules (rule_catalog::populate_rules)"
    );
    assert!(
        report.findings.iter().any(|f| !f.remediation.is_empty()),
        "at least one finding must carry a non-empty remediation on the CLI path"
    );
}
