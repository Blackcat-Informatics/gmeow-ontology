// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

fn authenticated_catalog() -> String {
    let root = repo_root();
    let artifacts = crate::fixture::stage_artifacts(&root, 1, "stage-constraint-catalog")
        .expect("authenticated constraint-catalog fixture");
    String::from_utf8(
        artifacts
            .get(CONSTRAINT_CATALOG_RDF_PATH)
            .expect("constraint-catalog artifact")
            .clone(),
    )
    .expect("constraint-catalog utf8")
}

#[test]
fn catalog_fanout_iri_is_auto_derived() {
    // The declared graph IRI must equal what the superset helper derives from the
    // committed path, so the fold reconstructs the committed 4th column.
    assert_eq!(
        crate::stages::superset::rdf_fanout_graph_iri(CONSTRAINT_CATALOG_RDF_PATH).as_deref(),
        Some(CATALOG_GRAPH_IRI)
    );
}

#[test]
fn every_registry_seed_becomes_a_rule() {
    let text = authenticated_catalog();
    for seed in all_rules() {
        let iri = rule_iri(&seed);
        assert!(
            text.contains(&format!("<{iri}> ")),
            "missing rule IRI for code {}",
            seed.code
        );
    }
    // The catalog is non-empty and every quad carries the fanout 4th column.
    assert!(text.contains(CATALOG_GRAPH_IRI));
    assert!(text.contains(&format!("{GMEOW}ValidationRule")));
}

#[test]
fn frame_completeness_is_enriched_from_the_graph() {
    let text = authenticated_catalog();
    // The frame-completeness rule formalizes gmeow:requiresFrame and applies to
    // at least one frame-carrier class resolved from the authored ontology.
    let rule = format!("{GMEOW}rule/discipline-frame-completeness");
    assert!(text.contains(&format!(
        "<{rule}> <{LOGIC}formalizes> <{GMEOW_REQUIRES_FRAME}>"
    )));
    assert!(text.contains(&format!("<{rule}> <{GMEOW}appliesToTerm>")));
}

/// The authenticated producer output emits a `gmeow:ruleRemediation` triple
/// for EVERY enforced rule whose code is NOT on
/// [`gmeow_validate::rule_catalog::REMEDIATION_ABSENT`], and NO such triple
/// for a code that IS on the allowlist — the honest-absence twin. Falsifiable:
/// if the `if let Some(remediation) = seed.remediation` guard in
/// `build_catalog_nquads` were dropped (or a remediation were fabricated for
/// an allowlisted code), this test fails.
#[test]
fn every_enforced_rule_carries_remediation_except_the_honest_absence_allowlist() {
    use gmeow_validate::rule_catalog::REMEDIATION_ABSENT;

    let nq = authenticated_catalog();

    let mut checked_present = 0usize;
    let mut checked_absent = 0usize;
    for seed in all_rules() {
        let iri = rule_iri(&seed);
        let prefix = format!("<{iri}> <{GMEOW}ruleRemediation> ");
        let has_remediation_triple = nq.lines().any(|line| line.starts_with(&prefix));
        if REMEDIATION_ABSENT.contains(&seed.code) {
            assert!(
                !has_remediation_triple,
                "honest-absence code {} (rule {iri}) must carry NO gmeow:ruleRemediation \
                     triple in the projection",
                seed.code
            );
            checked_absent += 1;
        } else {
            assert!(
                has_remediation_triple,
                "enforced rule {iri} (code {}) must carry a gmeow:ruleRemediation triple \
                     in the projection",
                seed.code
            );
            checked_present += 1;
        }
    }
    assert!(
        checked_absent > 0,
        "the honest-absence allowlist must cover at least one seed in this catalog"
    );
    assert!(
        checked_present > 0,
        "at least one enforced rule must carry a projected remediation"
    );
}

/// The advice-catalog projection emits one `gmeow:AdviceEntry`
/// per governed term with a realized advice carrier (today `gmeow:Entity` and
/// `gmeow:Event`), each hung beneath the `advice.` family rule and carrying the
/// three deontic-modality prose legs. Falsifiable: if `emit_advice_entries` were
/// dropped, or a term lost its realized carrier, this fails.
#[test]
fn advice_entries_are_projected_for_realized_carriers() {
    let nq = authenticated_catalog();
    for term in ["Entity", "Event"] {
        let entry = format!("{GMEOW}advice/{term}");
        assert!(
            nq.contains(&format!("<{entry}> <{RDF_TYPE}> <{GMEOW}AdviceEntry>")),
            "missing gmeow:AdviceEntry projection for {term}"
        );
        assert!(
            nq.contains(&format!(
                "<{entry}> <{GMEOW}documentedByRule> <{GMEOW}rule/family/advice>"
            )),
            "AdviceEntry for {term} must hang beneath the advice family rule"
        );
        assert!(
            nq.contains(&format!("<{entry}> <{LOGIC}formalizes> <{GMEOW}{term}>")),
            "AdviceEntry for {term} must formalize its governed term"
        );
        assert!(
            nq.contains(&format!("<{entry}> <{GMEOW}adviceAvoidWhen>")),
            "AdviceEntry for {term} must carry its avoidWhen prohibition prose"
        );
        assert!(
            nq.contains(&format!("<{entry}> <{GMEOW}adviceUseWhen>")),
            "AdviceEntry for {term} must carry its useWhen permission prose"
        );
        assert!(
            nq.contains(&format!("<{entry}> <{GMEOW}adviceHowToUse>")),
            "AdviceEntry for {term} must carry its howToUse directive prose"
        );
    }
}

/// Prose-binding over a controlled graph: the projected advice fields are copied
/// verbatim from their realized carriers and governed term. This exercises the
/// producer logic without loading or rebuilding the repository corpus.
#[test]
fn advice_prose_is_projected_verbatim_from_a_synthetic_graph() {
    let dataset = Dataset::parse_turtle(
        br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .

gmeow:SyntheticTerm
    rdfs:label "Synthetic term" ;
    skos:definition "Synthetic definition" ;
    gmeow:howToUse "Use exactly this way" .

gmeow:syntheticAvoid a logic:Constraint ;
    logic:severity "Info" ;
    logic:adviceSourceField logic:ProseFieldAvoidWhen ;
    logic:formalizes gmeow:SyntheticTerm ;
    logic:message "Avoid exactly this case" .

gmeow:syntheticUse a logic:AdviceGuidance ;
    logic:adviceSourceField logic:ProseFieldUseWhen ;
    logic:formalizes gmeow:SyntheticTerm ;
    logic:message "Use exactly this case" .
"#,
        None,
        "synthetic advice graph",
    )
    .expect("parse synthetic advice graph");
    let advice = collect_advice(&dataset).expect("collect advice");
    let entity = format!("{GMEOW}SyntheticTerm");
    let prose = advice
        .get(&entity)
        .expect("synthetic term has realized advice");
    assert_eq!(
        prose.avoid_when,
        BTreeSet::from(["Avoid exactly this case".to_string()])
    );
    assert_eq!(
        prose.use_when,
        BTreeSet::from(["Use exactly this case".to_string()])
    );
    assert_eq!(
        prose.how_to_use,
        BTreeSet::from(["Use exactly this way".to_string()])
    );

    let projected = build_catalog_nquads(&dataset).expect("project synthetic advice");
    for literal in [
        "Avoid exactly this case",
        "Use exactly this case",
        "Use exactly this way",
    ] {
        assert!(
            projected.contains(literal),
            "missing projected prose {literal:?}"
        );
    }
}

/// The minted advice-entry slugs are distinct across governed terms (two terms
/// whose local names collided would mint the same subject). `emit_advice_entries`
/// asserts this at build time; this pins it at the collector level too.
#[test]
fn advice_entry_slugs_are_distinct() {
    let catalog = authenticated_catalog();
    let mut slugs = BTreeSet::new();
    let mut count = 0usize;
    for line in catalog
        .lines()
        .filter(|line| line.contains(&format!("<{RDF_TYPE}> <{GMEOW}AdviceEntry>")))
    {
        let subject = line.split_whitespace().next().expect("N-Quads subject");
        assert!(
            slugs.insert(subject.to_string()),
            "duplicate advice-entry subject {subject}"
        );
        count += 1;
    }
    assert!(
        count >= 2,
        "expected at least the Entity + Event authenticated advice entries, got {count}"
    );
}
