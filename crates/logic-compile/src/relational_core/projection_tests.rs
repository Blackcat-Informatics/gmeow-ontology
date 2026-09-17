// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Product contracts for signed relational transport and fail-closed cache binding.

use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfTerm};

use super::*;

fn program() -> RelationalCoreProgram {
    let fact = RcAtom {
        subject: RcTerm::Literal(RdfLiteral::typed(
            "?a",
            format!("{LOGIC_NAMESPACE}Variable"),
        )),
        predicate: "urn:relation".to_owned(),
        object: RcTerm::Blank("shared".to_owned()),
        negated: true,
    };
    let head = RcAtom {
        subject: RcTerm::Blank("shared".to_owned()),
        predicate: "urn:head".to_owned(),
        object: RcTerm::Var("??exact-name".to_owned()),
        negated: false,
    };
    let signed = RcAtom {
        subject: RcTerm::Var("??exact-name".to_owned()),
        predicate: "urn:body".to_owned(),
        object: RcTerm::Literal(RdfLiteral::typed(
            "name",
            format!("{LOGIC_NAMESPACE}Variable"),
        )),
        negated: true,
    };
    RelationalCoreProgram {
        facts: vec![fact],
        rules: vec![RcRule {
            numeric: Vec::new(),
            head: head.clone(),
            head_conjuncts: vec![signed.clone()],
            body: vec![head, signed.clone(), signed],
            distinct_pairs: vec![("??exact-name".to_owned(), "?other".to_owned())],
        }],
        residue: vec![RcResidue {
            reason: "unrepresented source obligation".to_owned(),
        }],
        source_iri: Some("relative/source.ttl".to_owned()),
    }
}

#[test]
fn native_projection_retains_signs_typed_terms_and_occurrences() {
    let source = program();
    let native = project_relational_core_dataset(&source).unwrap();
    let restored = parse_relational_core(&native).unwrap();
    assert_eq!(restored, source);
    let exported = project_relational_core(&source);
    let parsed = purrdf::parse_dataset(exported.as_bytes(), "application/n-triples", None).unwrap();
    assert_eq!(parse_relational_core(&parsed).unwrap(), source);
}

#[test]
fn projection_identity_preserves_execution_order_and_blank_sharing() {
    let source = program();
    let mut reordered = source.clone();
    reordered.rules[0].body.reverse();
    assert_eq!(
        source.content_key().unwrap(),
        reordered.content_key().unwrap()
    );
    assert_ne!(
        source.projection_key().unwrap(),
        reordered.projection_key().unwrap()
    );
    let mut shortened = source.clone();
    shortened.rules[0].body.pop();
    assert_eq!(
        source.content_key().unwrap(),
        shortened.content_key().unwrap()
    );
    assert_ne!(
        source.projection_key().unwrap(),
        shortened.projection_key().unwrap()
    );

    let native = project_relational_core_dataset(&source).unwrap();
    let canonical = purrdf::try_canonicalize(&native).unwrap();
    let reparsed =
        purrdf::parse_dataset(canonical.nquads.as_bytes(), "application/n-quads", None).unwrap();
    let renamed = parse_relational_core(&reparsed).unwrap();
    assert_eq!(
        source.projection_key().unwrap(),
        renamed.projection_key().unwrap()
    );
    let mut disconnected = source.clone();
    disconnected.facts[0].object = RcTerm::Blank("different".to_owned());
    assert_ne!(
        source.projection_key().unwrap(),
        disconnected.projection_key().unwrap()
    );
}

fn change_field(
    dataset: &RdfDataset,
    field: &str,
    replacement: RdfTerm,
    duplicate: bool,
) -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for mut quad in dataset.owned_quads() {
        if quad.predicate == format!("{LOGIC_NAMESPACE}{field}") {
            if duplicate {
                builder.push_owned_quad(&quad);
            }
            quad.object = replacement.clone();
        }
        builder.push_owned_quad(&quad);
    }
    builder.freeze().unwrap()
}

#[test]
fn malformed_relational_fields_refuse_instead_of_selecting_or_skipping() {
    let mut source = program();
    source.facts.push(RcAtom {
        subject: RcTerm::Iri("urn:individual".into()),
        predicate: "urn:property".into(),
        object: RcTerm::Literal(RdfLiteral::simple("value")),
        negated: false,
    });
    let native = project_relational_core_dataset(&source).unwrap();
    let cases = [
        (
            "rcVariable",
            RdfTerm::literal(RdfLiteral::simple("another variable")),
            true,
            "ambiguous term record",
        ),
        (
            "rcIri",
            RdfTerm::literal(RdfLiteral::simple("not an IRI")),
            false,
            "invalid typed term record payload",
        ),
        (
            "rcSubject",
            RdfTerm::iri("urn:untyped"),
            true,
            "ambiguous rcSubject",
        ),
        (
            "hasFact",
            RdfTerm::literal(RdfLiteral::simple("not a link")),
            false,
            "requires an IRI",
        ),
        (
            "hasRule",
            RdfTerm::literal(RdfLiteral::simple("not a link")),
            false,
            "requires an IRI",
        ),
        (
            "sourceIri",
            RdfTerm::literal(RdfLiteral::simple("another source")),
            true,
            "ambiguous sourceIri",
        ),
        (
            "lossyDrop",
            RdfTerm::iri("urn:not-loss-text"),
            false,
            "xsd:string",
        ),
        (
            "rcNegated",
            RdfTerm::literal(RdfLiteral::simple("true")),
            false,
            "XMLSchema#boolean",
        ),
        (
            "rcIndex",
            RdfTerm::literal(RdfLiteral::simple("0")),
            false,
            "XMLSchema#integer",
        ),
        (
            "hasPreservation",
            RdfTerm::iri(PreservationKind::Exact.iri()),
            false,
            "disagrees",
        ),
    ];
    for (field, replacement, duplicate, expected) in cases {
        let corrupt = change_field(&native, field, replacement, duplicate);
        let failure = parse_relational_core(&corrupt).unwrap_err();
        assert!(failure.message().contains(expected), "{field}: {failure}");
    }
}

#[test]
fn duplicate_or_gapped_body_positions_refuse() {
    let mut source = program();
    source.rules[0].head_conjuncts.clear();
    let native = project_relational_core_dataset(&source).unwrap();
    for position in ["0", "5", "-1"] {
        let corrupt = change_field(
            &native,
            "rcIndex",
            RdfTerm::literal(RdfLiteral::typed(
                position,
                "http://www.w3.org/2001/XMLSchema#integer",
            )),
            false,
        );
        assert!(
            parse_relational_core(&corrupt).is_err(),
            "accepted body index {position}"
        );
    }
}

#[test]
fn named_graph_fields_cannot_repair_missing_default_graph_fields() {
    let native = project_relational_core_dataset(&program()).unwrap();
    let mut builder = RdfDatasetBuilder::new();
    for mut quad in native.owned_quads() {
        if quad.predicate == p_rc_subject() {
            quad.graph_name = Some(RdfTerm::iri("urn:unrelated-standpoint"));
        }
        builder.push_owned_quad(&quad);
    }
    let corrupt = builder.freeze().unwrap();
    let failure = parse_relational_core(&corrupt).unwrap_err();
    assert!(failure.message().contains("missing rcSubject"));
}

#[test]
fn negative_rule_head_is_residue_instead_of_a_positive_consequence() {
    let mut source = super::tests::horn_program();
    source.rules[0].head.negated = true;
    let lowered = lower_program(&source);
    assert!(lowered.rules.is_empty());
    assert_eq!(lowered.facts.len(), source.axioms.len());
    assert_eq!(lowered.preservation(), PreservationKind::SoundUnder);
    assert!(lowered.residue[0].reason.contains("negative head"));
}

#[test]
fn contextual_statements_cannot_become_unscoped_relational_facts_or_rules() {
    use crate::ir::{ContextualScope, LogicModality};
    let scopes = [
        ContextualScope {
            standpoint: Some("urn:observer".into()),
            ..Default::default()
        },
        ContextualScope {
            time: Some("2026".into()),
            ..Default::default()
        },
        ContextualScope {
            confidence: Some(0.5),
            ..Default::default()
        },
        ContextualScope {
            modality: LogicModality::Deontic,
            ..Default::default()
        },
        ContextualScope {
            provenance: Some("urn:source".into()),
            ..Default::default()
        },
        ContextualScope {
            module: Some("urn:module".into()),
            ..Default::default()
        },
    ];
    for scope in scopes {
        for position in 0..4 {
            let mut source = super::tests::horn_program();
            match position {
                0 => source.axioms[0].scope = scope.clone(),
                1 => source.rules[0].scope = scope.clone(),
                2 => source.rules[0].head.scope = scope.clone(),
                3 => source.rules[0].body[0].scope = scope.clone(),
                _ => unreachable!(),
            }
            let lowered = lower_program(&source);
            assert_eq!(lowered.residue.len(), 1);
            assert!(lowered.residue[0].reason.contains("contextual scope"));
            assert_eq!(lowered.preservation(), PreservationKind::SoundUnder);
            assert_eq!(
                lowered.facts.len(),
                source.axioms.len() - usize::from(position == 0)
            );
            assert_eq!(lowered.rules.len(), usize::from(position == 0));
        }
    }
}
