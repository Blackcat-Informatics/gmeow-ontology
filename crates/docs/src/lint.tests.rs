// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::model::{
    DocCompetency, DocExample, DocFixture, DocFixtureKind, DocLinkage, DocLossTarget, DocTerm,
    DocTermCategory,
};
use crate::render::render_site;
use std::collections::BTreeMap;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn model_with_terms(terms: Vec<DocTerm>) -> DocsModel {
    DocsModel {
        title: "T".to_string(),
        version: "2".to_string(),
        slices: Vec::new(),
        terms,
        dependency_edges: Vec::new(),
        mapping_sets: Vec::new(),
        linkages: Vec::new(),
        examples: Vec::new(),
        fixtures: Vec::new(),
        shapes: Vec::new(),
        competencies: Vec::new(),
        grammars: Vec::new(),
        loss_targets: Vec::new(),
        worked_instances: Vec::new(),
        concerns: Vec::new(),
        external_terms: Vec::new(),
        seams: Vec::new(),
        recipes: Vec::new(),
        learning_paths: Vec::new(),
        constraint_rules: Vec::new(),
        advice_entries: Vec::new(),
        four_boxes: None,
        concept_doi: None,
        pipeline: None,

        available_languages: vec!["english".to_string()],

        translations: crate::i18n::Translations::default(),

        ui_catalog: crate::i18n::UiCatalog::default(),
        reasoning: None,
        diagnostics: None,
        term_loss: None,
        schema_fragments: None,
        lang: String::new(),
    }
}

fn cat(
    local: &str,
    category: DocTermCategory,
    definition: Option<&str>,
    label: Option<&str>,
) -> DocTerm {
    DocTerm {
        iri: format!("{GMEOW}{local}"),
        curie: format!("gmeow:{local}"),
        label: label.map(str::to_string),
        definition: definition.map(str::to_string),
        category,
        owner_slice: format!("{GMEOW}slice/zoo"),
        parents: Vec::new(),
        domain: Vec::new(),
        range: Vec::new(),
        scope_notes: Vec::new(),
        examples: Vec::new(),
        use_when: Vec::new(),
        avoid_when: Vec::new(),
        how_to_use: Vec::new(),
        use_for_consumer: Vec::new(),
        avoid_for_consumer: Vec::new(),
        ..Default::default()
    }
}

fn term(local: &str, definition: Option<&str>, label: Option<&str>) -> DocTerm {
    cat(local, DocTermCategory::Class, definition, label)
}

/// A term with NO coverage gaps: definition, label, scope note, example, the
/// full usage-advice triad, and (via `populated`) a matching alignment.
fn rich(local: &str, category: DocTermCategory, label: &str) -> DocTerm {
    DocTerm {
        scope_notes: vec![format!("Scope of {local}.")],
        examples: vec![format!("Worked use of {local}.")],
        use_when: vec![format!("Use {local} when …")],
        avoid_when: vec![format!("Avoid {local} when …")],
        how_to_use: vec![format!("Idiomatic {local} use.")],
        ..cat(local, category, Some("Fully covered."), Some(label))
    }
}

/// An external term equivalence whose subject is the GMEOW term `local`, so it
/// satisfies the `docs/missing-alignment` check.
fn linkage(local: &str) -> DocLinkage {
    DocLinkage {
        mapping_set: None,
        subject: format!("{GMEOW}{local}"),
        subject_curie: format!("gmeow:{local}"),
        predicate: "skos:closeMatch".to_string(),
        object: format!("http://example.org/{local}"),
        justification: None,
        confidence: None,
        owner_slice: format!("{GMEOW}slice/zoo"),
    }
}

/// The static nav + getting-started page always link to both the classes and
/// properties category indexes, which only render when their category is
/// non-empty; a fully-populated ontology has both, so test models do too. Both
/// seed terms are fully covered (and aligned) so only `extra` terms can warn.
fn populated(extra: Vec<DocTerm>) -> DocsModel {
    let mut terms = vec![
        rich("Animal", DocTermCategory::Class, "Animal"),
        rich("hasOwner", DocTermCategory::Property, "has owner"),
    ];
    terms.extend(extra);
    let mut model = model_with_terms(terms);
    model.linkages = vec![linkage("Animal"), linkage("hasOwner")];
    model
}

#[test]
fn clean_site_has_zero_errors() {
    let model = populated(vec![term("Cat", Some("A cat."), Some("Cat"))]);
    let site = render_site(&model);
    let report = lint(&model, &site);
    assert_eq!(
        report.error_count(),
        0,
        "a rendered site must have no dangling links: {:?}",
        report.legacy_errors()
    );
}

#[test]
fn dangling_link_is_an_error() {
    // A site with a single page that links to a non-existent page.
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    files.insert(
        "index.html".to_string(),
        br#"<a href="missing/index.html">x</a>"#.to_vec(),
    );
    let site = Site { files };
    let model = model_with_terms(Vec::new());
    let report = lint(&model, &site);
    assert_eq!(report.error_count(), 1);
    assert!(report.legacy_errors()[0].contains("missing/index.html"));
}

#[test]
fn broken_anchor_is_an_error() {
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    files.insert(
        "index.html".to_string(),
        br##"<a href="#nope">x</a><h2 id="real">y</h2>"##.to_vec(),
    );
    let site = Site { files };
    let model = model_with_terms(Vec::new());
    let report = lint(&model, &site);
    assert_eq!(report.error_count(), 1);
    assert!(report.legacy_errors()[0].contains("#nope"));
}

#[test]
fn coverage_gaps_are_warnings() {
    // A bare, superset-NATIVE term (no external-correspondence intent, not a
    // lossy-projection source) trips a `docs/missing-*` WARNING for every
    // UNCONDITIONAL per-term dimension it lacks, but NONE of the four
    // applicability-conditioned dimensions (alignment, linkage coverage, loss
    // ledger row, loss judgment sound) — those apply only where a term declares
    // an external correspondence or is a lossy projection. No errors.
    let model = populated(vec![term("Bare", None, None)]);
    let site = render_site(&model);
    let report = lint(&model, &site);
    assert_eq!(report.error_count(), 0);
    let codes: BTreeSet<&str> = report.findings.iter().map(|f| f.code.as_str()).collect();
    for code in [
        "docs/missing-definition",
        "docs/missing-label",
        "docs/missing-usage-advice",
        "docs/missing-example",
        "docs/missing-scope-note",
        "docs/missing-fixture-pair",
        "docs/missing-competency-rationale",
        "docs/missing-worked-instance",
        "docs/missing-annotation-coat",
        "docs/missing-test-reach",
        "docs/missing-prose-quality",
    ] {
        assert!(codes.contains(code), "expected `{code}`; got {codes:?}");
    }
    // `dimProseQuality` is a FOUR-way conjunction; the warning must say which
    // conjuncts are unmet, not merely that the dimension is missing — the whole
    // point of computing it per conjunct instead of collapsing it to a bool.
    let prose = report
        .findings
        .iter()
        .find(|f| f.code == "docs/missing-prose-quality" && f.message.contains("Bare"))
        .expect("the bare term misses dimProseQuality");
    for conjunct in [
        "definition states no boundary (what it is NOT)",
        "no example is a worked triple",
        "usage coat is blank or restates the definition",
        "no competency rationale distinct from the label",
    ] {
        assert!(
            prose.message.contains(conjunct),
            "the prose-quality warning must name the unmet conjunct `{conjunct}`; got `{}`",
            prose.message
        );
    }

    // The novel `Bare` term is NOT applicable for the external-correspondence /
    // lossy dimensions, so it contributes no such miss. `missing-alignment`,
    // `missing-loss-ledger-row`, and `missing-loss-judgment-sound` are absent from
    // the whole report (the aligned seed terms cover alignment; nothing is a lossy
    // source). The vacuously-covered dimensions (no non-English langs, no
    // rationale) are also absent.
    assert!(!codes.contains("docs/missing-alignment"));
    assert!(!codes.contains("docs/missing-loss-ledger-row"));
    assert!(!codes.contains("docs/missing-loss-judgment-sound"));
    assert!(!codes.contains("docs/missing-translation-coverage"));
    assert!(!codes.contains("docs/missing-provenance-honesty"));
    // `missing-linkage-coverage` DOES fire — but only for the seed terms, which
    // DECLARE an external correspondence (they are alignment subjects) yet carry
    // no mapping-set-backed linkage: applicable ∧ ¬present, a real defect the
    // applicability layer still catches.
    assert!(codes.contains("docs/missing-linkage-coverage"));
}

/// A term wired to cover ALL sixteen per-term dimensions: full annotation coat,
/// a fixture pair, a competency question with a clean rationale, a worked
/// example, a projection-loss target, and a mapping-set-backed alignment.
fn fully_covered_term(local: &str, category: DocTermCategory, label: &str) -> DocTerm {
    DocTerm {
        scope_notes: vec![format!("Scope of {local}.")],
        // A boundary definition (states what it is NOT) — for dimProseQuality.
        examples: vec![format!("gmeow:{local} a owl:Class .")],
        use_when: vec![format!("Use {local} when modelling.")],
        avoid_when: vec![format!("Avoid {local} for raw strings.")],
        how_to_use: vec![format!("Attach {local} idiomatically.")],
        box_role: Some("gmeow:boxTBox".to_string()),
        ..cat(
            local,
            category,
            Some("A living thing, not a mineral."),
            Some(label),
        )
    }
}

#[test]
fn fully_covered_terms_emit_no_coverage_warnings() {
    // Two fully-wired terms (a class + a property so both category indexes
    // render without dangling links) covering every per-term dimension → zero
    // coverage warnings and zero errors. No slices ⇒ no slice-scoped warnings.
    let mut model = model_with_terms(vec![
        fully_covered_term("Animal", DocTermCategory::Class, "Animal"),
        fully_covered_term("hasOwner", DocTermCategory::Property, "has owner"),
    ]);
    for local in ["Animal", "hasOwner"] {
        let iri = format!("{GMEOW}{local}");
        let curie = format!("gmeow:{local}");
        model.fixtures.push(DocFixture {
            slice: format!("{GMEOW}slice/zoo"),
            logical_path: format!("tests/conformance-fixtures/{local}-ok.ttl"),
            title: "ok".to_string(),
            text: String::new(),
            kind: DocFixtureKind::Wellformed,
            terms_referenced: vec![curie.clone()],
            expected_outcome: None,
            violation_code: None,
            rationale: None,
            catalog_slug: None,
        });
        model.fixtures.push(DocFixture {
            slice: format!("{GMEOW}slice/zoo"),
            logical_path: format!("tests/counter-examples/{local}-bad.ttl"),
            title: "bad".to_string(),
            text: String::new(),
            kind: DocFixtureKind::CounterExample,
            terms_referenced: vec![curie.clone()],
            expected_outcome: None,
            violation_code: None,
            rationale: None,
            catalog_slug: None,
        });
        model.examples.push(DocExample {
            slice: format!("{GMEOW}slice/zoo"),
            logical_path: format!("examples/{local}.ttl"),
            title: local.to_string(),
            text: String::new(),
            terms_referenced: vec![curie.clone()],
        });
        model.competencies.push(DocCompetency {
            iri: format!("{GMEOW}cq/{local}"),
            rationale: Some("Every animal is a living thing.".to_string()),
            exercises: vec![iri.clone()],
            owner_slice: format!("{GMEOW}slice/zoo"),
            ..Default::default()
        });
        model.loss_targets.push(DocLossTarget {
            target: local.to_string(),
            label: None,
            preservation_kind: "SoundUnderApproximation".to_string(),
            complexity_class: "PTIME".to_string(),
            slice: format!("{GMEOW}slice/zoo"),
        });
        model.linkages.push(DocLinkage {
            mapping_set: Some(format!("{GMEOW}mappingSet/1")),
            subject: iri.clone(),
            subject_curie: curie.clone(),
            predicate: "skos:closeMatch".to_string(),
            object: format!("http://example.org/{local}"),
            justification: None,
            confidence: None,
            owner_slice: format!("{GMEOW}slice/zoo"),
        });
    }
    let site = render_site(&model);
    let report = lint(&model, &site);
    assert_eq!(report.error_count(), 0, "{:?}", report.legacy_errors());
    let coverage: Vec<&str> = report
        .findings
        .iter()
        .map(|f| f.code.as_str())
        .filter(|c| c.starts_with("docs/missing-"))
        .collect();
    assert!(
        coverage.is_empty(),
        "fully-covered terms must emit no coverage warnings; got {coverage:?}"
    );
}
