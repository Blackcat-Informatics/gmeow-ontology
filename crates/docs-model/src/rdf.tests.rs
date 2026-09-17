// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::model::{DocChangelogEntry, DocChangelogSource, DocTerm, DocTermCategory};

fn tiny_model() -> DocsModel {
    DocsModel {
        title: "T".to_string(),
        version: "2".to_string(),
        slices: Vec::new(),
        terms: vec![
            DocTerm {
                iri: format!("{GMEOW}Cat"),
                curie: "gmeow:Cat".to_string(),
                label: Some("Cat".to_string()),
                definition: Some("A cat.".to_string()),
                category: DocTermCategory::Class,
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
            },
            DocTerm {
                iri: format!("{GMEOW}hasOwner"),
                curie: "gmeow:hasOwner".to_string(),
                label: None,
                definition: None,
                category: DocTermCategory::Property,
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
            },
        ],
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

#[test]
fn projection_is_well_formed_and_deterministic() {
    let model = tiny_model();
    let a = to_gmeow_rdf(&model, &BTreeMap::new());
    let b = to_gmeow_rdf(&model, &BTreeMap::new());
    assert_eq!(a, b, "projection must be deterministic");

    // Every line is a 4-term N-Quad in the documentation graph.
    for line in a.lines() {
        assert!(
            line.ends_with(&format!("<{DOCUMENTATION_GRAPH}> .")),
            "line not in documentation graph: {line}"
        );
    }
    assert!(a.contains("DocumentedTerm"));
    assert!(a.contains("docCategory"));
    // The definition-less property records false; the cat records true.
    assert!(a.contains(&format!("\"true\"^^<{XSD_BOOLEAN}>")));
    assert!(a.contains(&format!("\"false\"^^<{XSD_BOOLEAN}>")));
    assert!(a.ends_with('\n'));
}

#[test]
fn authored_and_computed_same_release_entries_have_distinct_rdf_identities() {
    let mut model = tiny_model();
    model.terms[0].changelog = vec![
        DocChangelogEntry {
            version: "1.0.0".to_string(),
            note: Some("Authored release note.".to_string()),
            source: DocChangelogSource::Authored,
        },
        DocChangelogEntry {
            version: "1.0.0".to_string(),
            note: Some("Definition changed".to_string()),
            source: DocChangelogSource::Computed,
        },
    ];

    let nq = to_gmeow_rdf(&model, &BTreeMap::new());
    let links: Vec<&str> = nq
        .lines()
        .filter(|line| {
            line.starts_with(&format!("<{GMEOW}Cat>")) && line.contains(GMEOW_HAS_CHANGELOG_ENTRY)
        })
        .collect();

    assert_eq!(links.len(), 2, "both independent records must be linked");
    assert!(links.iter().any(|line| line.contains("/1-0-0/authored/")));
    assert!(links.iter().any(|line| line.contains("/1-0-0/computed/")));
    assert_ne!(links[0], links[1], "record identities must not collide");
    assert_eq!(nq, to_gmeow_rdf(&model, &BTreeMap::new()));
}

#[test]
fn empty_model_yields_empty_string() {
    let model = DocsModel {
        title: "T".to_string(),
        version: "2".to_string(),
        slices: Vec::new(),
        terms: Vec::new(),
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
    };
    assert_eq!(to_gmeow_rdf(&model, &BTreeMap::new()), "");
}

#[test]
fn entailment_dag_round_trips_rule_conclusion_and_all_premises() {
    let model = tiny_model();
    let mut entailments: BTreeMap<String, Vec<crate::exec::Entailment>> = BTreeMap::new();
    entailments.insert(
        format!("{GMEOW}Cat"),
        vec![crate::exec::Entailment {
            rule: "owl:subClassOf-transitive".to_string(),
            conclusion: "gmeow:Cat rdfs:subClassOf gmeow:Animal".to_string(),
            premises: vec![
                "gmeow:Cat rdfs:subClassOf gmeow:Feline".to_string(),
                "gmeow:Feline rdfs:subClassOf gmeow:Animal".to_string(),
            ],
        }],
    );
    let nq = to_gmeow_rdf(&model, &entailments);

    // Determinism holds with a non-empty map too.
    assert_eq!(nq, to_gmeow_rdf(&model, &entailments));

    // The entailment node is minted, typed, term-keyed, and grounded.
    assert!(nq.contains(&format!("<{GMEOW}Entailment>")));
    assert!(nq.contains(&format!("{GMEOW}entailmentRule> ")));
    assert!(nq.contains("owl:subClassOf-transitive"));
    assert!(nq.contains("gmeow:Cat rdfs:subClassOf gmeow:Animal"));
    // BOTH premises round-trip (the derivation DAG, not a flattened hop).
    assert!(nq.contains("gmeow:Cat rdfs:subClassOf gmeow:Feline"));
    assert!(nq.contains("gmeow:Feline rdfs:subClassOf gmeow:Animal"));
    let premise_lines = nq
        .lines()
        .filter(|l| l.contains(&format!("{GMEOW}entailmentPremise>")))
        .count();
    assert_eq!(premise_lines, 2, "both premises must be emitted");

    // The node grounds by the documented term IRI.
    assert!(nq.contains(&format!("{GMEOW}docGroundedBy> <{GMEOW}Cat>")));

    // Empty map ⇒ no entailment nodes (honest absence).
    let bare = to_gmeow_rdf(&model, &BTreeMap::new());
    assert!(!bare.contains(&format!("<{GMEOW}Entailment>")));
}

#[test]
fn search_facets_carry_real_content_and_honest_absence() {
    use crate::model::DocLinkage;
    let mut model = tiny_model();
    // Cat gains advisory prose + a crosswalk linkage, so its documentation-entry
    // record projects the full search-facet set.
    model.terms[0].scope_notes = vec!["Prefer for a domestic cat.".to_string()];
    model.linkages = vec![DocLinkage {
        mapping_set: None,
        subject: format!("{GMEOW}Cat"),
        subject_curie: "gmeow:Cat".to_string(),
        predicate: "http://www.w3.org/2004/02/skos/core#exactMatch".to_string(),
        object: "http://www.wikidata.org/entity/Q146".to_string(),
        justification: None,
        confidence: None,
        owner_slice: format!("{GMEOW}slice/zoo"),
    }];
    let nq = to_gmeow_rdf(&model, &BTreeMap::new());

    // Cat: the REAL label / definition, the advice string, and the alignment token
    // — NOT the meta annotate() label.
    assert!(nq.contains(&format!("{GMEOW}docSearchLabel> \"Cat\"")));
    assert!(nq.contains(&format!("{GMEOW}docSearchDefinition> \"A cat.\"")));
    assert!(nq.contains(&format!(
        "{GMEOW}docSearchAdvice> \"Prefer for a domestic cat.\""
    )));
    assert!(nq.contains(&format!("{GMEOW}docSearchAlignment> \"exactMatch:Q146\"")));

    // hasOwner has no label/definition/advice/linkage: docSearchLabel falls back to
    // the CURIE, and NO definition/advice/alignment facet is fabricated.
    assert!(nq.contains(&format!("{GMEOW}docSearchLabel> \"gmeow:hasOwner\"")));
    // The definition-less property emits no docSearchDefinition line for itself.
    let has_owner_subject = format!("<{GMEOW}documentation/term/hasowner>");
    for line in nq.lines().filter(|l| l.starts_with(&has_owner_subject)) {
        assert!(
            !line.contains("docSearchDefinition"),
            "definition-less property must not emit docSearchDefinition: {line}"
        );
        assert!(
            !line.contains("docSearchAdvice"),
            "advice-less property must not emit docSearchAdvice: {line}"
        );
        assert!(
            !line.contains("docSearchAlignment"),
            "linkage-less property must not emit docSearchAlignment: {line}"
        );
    }
}
