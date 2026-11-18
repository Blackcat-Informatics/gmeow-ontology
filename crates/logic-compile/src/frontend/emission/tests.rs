// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW lowering ownership and source attribution, using synthetic source only.

use std::collections::BTreeSet;

use purrdf::{RdfDatasetBuilder, RdfLiteral};

use super::*;
use crate::frontend::{Severity, SourceUnitKind};
use crate::ir::{Formula, Term};

pub(super) fn prepared(text: &str) -> PreparedLogicSource {
    let dataset = purrdf::parse_dataset(
        format!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix ex: <https://example.org/> .
            @prefix math: <https://blackcatinformatics.ca/math/> . {text}"
        )
        .as_bytes(),
        "application/trig",
        None,
    )
    .unwrap();
    PreparedLogicSource::new(&dataset).unwrap()
}

pub(super) fn node(source: &PreparedLogicSource, name: &str) -> SourceNode {
    SourceNode {
        term: source
            .dataset()
            .term_id_by_iri(&format!("https://example.org/{name}"))
            .unwrap(),
        graph: None,
    }
}

fn disposition<'a>(compiled: &'a SourceCompilation<'_>, name: &str) -> &'a FormulaDisposition {
    &compiled
        .formula_lowerings()
        .iter()
        .find(|lowering| lowering.source == node(compiled.source(), name))
        .unwrap()
        .disposition
}

#[test]
fn horn_dedup_keeps_every_formula_and_statement_origin_after_sorting() {
    let source = prepared(
        r#"
        ex:Zulu logic:subClassOf ex:Animal .
        ex:Alfa logic:subClassOf ex:Animal .
        ex:lawA a logic:Formula ; logic:relation logic:subClassOf ; logic:argument ex:s, ex:o .
        ex:lawB a logic:Formula ; logic:relation logic:subClassOf ; logic:argument ex:s, ex:o .
        ex:s logic:termIndex 0 ; logic:termIri ex:Zulu .
        ex:o logic:termIndex 1 ; logic:termIri ex:Animal .
    "#,
    );
    let compiled = source.compile_with_sources(None).unwrap();
    assert!(
        compiled.diagnostics().is_empty(),
        "{:?}",
        compiled.diagnostics()
    );
    let FormulaDisposition::Axiom(index) = *disposition(&compiled, "lawA") else {
        panic!("Horn axiom expected")
    };
    assert_eq!(
        disposition(&compiled, "lawB"),
        &FormulaDisposition::Axiom(index)
    );
    assert_eq!(
        compiled.program().axioms[index].subject,
        "https://example.org/Zulu"
    );
    assert!(index > 0, "canonical order moved the Zulu axiom past Alfa");
    let origins = &compiled.axiom_sources()[index];
    assert_eq!(origins.len(), 3);
    assert!(origins.contains(&AxiomSource::Formula(node(&source, "lawA"))));
    assert!(origins.contains(&AxiomSource::Formula(node(&source, "lawB"))));
    assert!(origins.iter().any(|origin| matches!(origin, AxiomSource::Statement { subject, .. } if *subject == node(&source, "Zulu"))));
    assert!(compiled.program().formulas.is_empty());
}

#[test]
fn shared_malformed_subtree_accounts_for_each_affected_formula() {
    let source = prepared(
        "ex:first a logic:Formula ; logic:not ex:broken .
        ex:second a logic:Formula ; logic:not ex:broken . ex:broken a logic:Formula .",
    );
    let compiled = source.compile_with_sources(None).unwrap();
    assert_eq!(compiled.formula_lowerings().len(), 3);
    for name in ["first", "second", "broken"] {
        let FormulaDisposition::Malformed { diagnostic } = *disposition(&compiled, name) else {
            panic!("malformed root must be accounted for")
        };
        let error = &compiled.diagnostics()[diagnostic];
        assert_eq!(error.code, "MALFORMED_FORMULA");
        assert_eq!(error.severity, Severity::Error);
    }
    assert!(compiled.program().formulas.is_empty());
}

#[test]
fn formula_owner_and_graph_exclusion_are_distinct_from_global_emission() {
    let source = prepared(
        r#"
        ex:global a logic:Formula ; logic:not ex:atom .
        ex:owned a logic:Formula ; logic:not ex:atom .
        ex:atom a logic:Formula ; logic:relation ex:p ; logic:argument
            [ logic:termIndex 0 ; logic:termIri ex:individual ],
            [ logic:termIndex 1 ; logic:termIri ex:value ] .
        ex:set math:memberCondition ex:owned .
        ex:named { ex:global a logic:Formula . ex:broken a logic:Formula . }
    "#,
    );
    let compiled = source.compile_with_sources(None).unwrap();
    assert_eq!(
        disposition(&compiled, "global"),
        &FormulaDisposition::Formula(0)
    );
    assert_eq!(
        disposition(&compiled, "owned"),
        &FormulaDisposition::ReadForOwner
    );
    assert_eq!(
        disposition(&compiled, "atom"),
        &FormulaDisposition::ReadForOwner
    );
    assert_eq!(compiled.program().formulas.len(), 1);
    let excluded: Vec<_> = compiled
        .formula_lowerings()
        .iter()
        .filter(|lowering| lowering.disposition == FormulaDisposition::OutsideDefaultGraph)
        .collect();
    assert_eq!(excluded.len(), 2);
    assert!(
        excluded
            .iter()
            .all(|lowering| lowering.source.graph.is_some())
    );
    let declared = source
        .source_graph()
        .units()
        .filter(|unit| unit.declared_kinds.contains(&SourceUnitKind::Formula))
        .count();
    assert_eq!(compiled.formula_lowerings().len(), declared);
}

/// PROTECTS: canonical structural payloads retain every actual contributor;
/// collection selection follows exact constructors without inventing class heads.
#[test]
fn canonical_structural_expansions_retain_every_native_contributor() {
    let source = prepared(
        "ex:Thing logic:subClassOf _:a, _:b .
        _:a a logic:Restriction ; logic:onProperty ex:part ; logic:someValuesFrom ex:Part .
        _:b a logic:Restriction ; logic:onProperty ex:part ; logic:someValuesFrom ex:Part .",
    );
    let compiled = source.compile_with_sources(None).unwrap();
    assert!(
        compiled.diagnostics().is_empty(),
        "{:?}",
        compiled.diagnostics()
    );
    assert_eq!(compiled.program().axioms.len(), 4);
    let mut roots = None;
    for origins in compiled.axiom_sources() {
        assert_eq!(origins.len(), 2);
        assert!(
            origins
                .iter()
                .all(|origin| matches!(origin, AxiomSource::ClassExpression(_)))
        );
        if let Some(expected) = roots {
            assert_eq!(origins, expected);
        } else {
            roots = Some(origins);
        }
    }

    let base = prepared(
        r#"
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        ex:M a logic:Module . ex:N a logic:Module .
        ex:chain logic:propertyChainAxiom ex:head ; logic:inModule ex:N .
        ex:key logic:hasKey ex:head ; logic:inModule ex:M .
        ex:Enum logic:oneOf ex:head ; logic:inModule ex:M .
        ex:untyped logic:members ex:head ; logic:inModule ex:N .
        ex:head rdf:first ex:value ; rdf:rest rdf:nil .
        ex:other {
            ex:chain logic:propertyChainAxiom ex:shadow .
            ex:shadow rdf:first ex:foreign ; rdf:rest rdf:nil .
        }
    "#,
    );
    let mut builder = RdfDatasetBuilder::new();
    for quad in base.dataset().owned_quads() {
        builder
            .push_owned_quad(&quad)
            .expect("the authored source enters one native builder");
    }
    let chain = builder
        .intern_iri("https://example.org/chain")
        .expect("chain IRI");
    let predicate = builder
        .intern_iri("https://blackcatinformatics.ca/logic/propertyChainAxiom")
        .expect("constructor IRI");
    let head = builder
        .intern_iri("https://example.org/head")
        .expect("head IRI");
    builder
        .push_annotation(chain, predicate, head)
        .expect("the duplicate annotation is a separate physical carrier");
    let dataset = builder.freeze().expect("the collection fixture freezes");
    let theory = PreparedLogicSource::new(&dataset)
        .expect("the collection source prepares")
        .into_compiled(None)
        .expect("the collection source compiles");
    assert!(
        theory.diagnostics().is_empty(),
        "{:?}",
        theory.diagnostics()
    );
    let first = theory
        .program()
        .axioms
        .iter()
        .position(|axiom| {
            axiom.subject == "https://example.org/head"
                && axiom.predicate == crate::lists::RDF_FIRST
        })
        .expect("the shared first cell has one canonical payload");
    let origins = &theory.axiom_sources()[first];
    assert_eq!(
        origins.len(),
        5,
        "every constructor and physical carrier contributes"
    );
    assert!(
        origins.iter().all(|source| matches!(source,
            AxiomSource::Collection { source, field: super::CollectionField::Cell }
                if source.constructor.canonical.g.is_none()
        )),
        "{origins:?}"
    );
    let untyped_roles: BTreeSet<_> = origins
        .iter()
        .filter_map(|origin| match origin {
            AxiomSource::Collection { source, .. }
                if source.constructor.canonical.s == node(theory.source(), "untyped").term =>
            {
                Some(source.role)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        untyped_roles,
        BTreeSet::from([super::CollectionRole {
            owner: super::SourceOwnerRole::SchemaLaw,
            members: super::CollectionMembers::Uninterpreted,
        }]),
        "a members field without n-ary typing grants no interpreted member role"
    );
    let inventory = crate::source_inventory::SourceInventory::new(
        &theory,
        super::super::SourceSelection::all_classical_roots(&theory),
    );
    let classes: Vec<_> = inventory
        .obligations()
        .filter_map(|unit| match unit.origin() {
            crate::source_inventory::SourceObligationOrigin::ClassDefinition(node) => Some(node),
            _ => None,
        })
        .collect();
    assert_eq!(
        classes,
        vec![node(theory.source(), "Enum")],
        "only the genuine enumeration defines a class"
    );
    assert_eq!(
        inventory
            .obligations()
            .filter(|unit| matches!(
                unit.origin(),
                crate::source_inventory::SourceObligationOrigin::Axiom(_)
            ))
            .count(),
        2,
        "constructor rows are already inventoried; only cell expansions need separate axiom obligations"
    );
    let request =
        crate::ir::TheorySelection::composed_modules(["https://example.org/M".to_owned()])
            .expect("the requested module is an absolute IRI");
    let resolved = inventory
        .resolve_theory_selection(&request)
        .expect("the supplied module resolves");
    let view = resolved
        .program_view(&inventory)
        .expect("the same compilation supplies the selected view");
    let admitted: Vec<_> = view
        .axiom_sources_at(first)
        .expect("the cell has original contributors")
        .collect();
    assert_eq!(
        admitted.len(),
        2,
        "the included key and enumeration share the cell"
    );
    assert!(
        admitted.iter().all(|source| match source {
            AxiomSource::Collection { source, .. } =>
                source.constructor.canonical.s != node(theory.source(), "chain").term,
            _ => false,
        }),
        "an excluded chain cannot borrow the key's property-list membership"
    );
    assert!(
        admitted.iter().any(|source| matches!(source,
            AxiomSource::Collection { source, .. }
                if source.role.members == super::CollectionMembers::Values
                    && source.role.owner == super::SourceOwnerRole::ClassDefinition
        )),
        "oneOf retains value members under its genuine class owner"
    );
    let admission = super::super::SourceAdmission::classical_fof(
        &theory,
        super::super::SourceSelection::all_classical_roots(&theory),
    );
    assert!(
        admission.units.iter().any(|unit| {
            unit.anchor.contains("https://example.org/untyped")
                && matches!(&unit.disposition,
                    super::super::SemanticUnitDisposition::Blocked { code, .. }
                        if code == "UNSUPPORTED_COLLECTION_LAW")
        }),
        "recording an uninterpreted collection does not admit its law"
    );
}

#[test]
fn native_annotation_and_physical_duplicate_keep_one_exact_statement_origin() {
    let mut builder = RdfDatasetBuilder::new();
    let s = builder.intern_iri("https://example.org/Thing").unwrap();
    let p = builder
        .intern_iri("https://blackcatinformatics.ca/logic/subClassOf")
        .unwrap();
    let o = builder.intern_iri("https://example.org/Supertype").unwrap();
    builder.push_annotation(s, p, o).unwrap();
    builder.push_quad(s, p, o, None).unwrap();
    let source = PreparedLogicSource::new(&builder.freeze().unwrap()).unwrap();
    let compiled = source.compile_with_sources(None).unwrap();
    assert_eq!(compiled.program().axioms.len(), 1);
    assert_eq!(
        compiled.axiom_sources(),
        &[vec![AxiomSource::Statement {
            subject: node(&source, "Thing"),
            predicate: source
                .dataset()
                .term_id_by_iri("https://blackcatinformatics.ca/logic/subClassOf")
                .unwrap(),
            object: node(&source, "Supertype").term,
        }]]
    );
}

#[test]
fn reifier_origins_follow_scope_sensitive_deduplication() {
    let mut builder = RdfDatasetBuilder::new();
    let s = builder.intern_iri("https://example.org/Thing").unwrap();
    let p = builder
        .intern_iri("https://blackcatinformatics.ca/logic/subClassOf")
        .unwrap();
    let o = builder.intern_iri("https://example.org/Supertype").unwrap();
    let triple = builder.intern_triple(s, p, o).unwrap();
    let confidence = builder
        .intern_iri("https://blackcatinformatics.ca/logic/confidence")
        .unwrap();
    for (name, value) in [("first", "0.75"), ("second", "0.75"), ("third", "0.25")] {
        let reifier = builder
            .intern_iri(&format!("https://example.org/{name}"))
            .unwrap();
        let value = builder
            .intern_literal(RdfLiteral::simple(value.to_owned()))
            .unwrap();
        builder.push_reifier(reifier, triple).unwrap();
        builder.push_annotation(reifier, confidence, value).unwrap();
    }
    let source = PreparedLogicSource::new(&builder.freeze().unwrap()).unwrap();
    let compiled = source.compile_with_sources(None).unwrap();
    let claims: Vec<_> = compiled
        .program()
        .axioms
        .iter()
        .zip(compiled.axiom_sources())
        .filter(|(axiom, _)| axiom.subject == "https://example.org/Thing")
        .collect();
    assert_eq!(claims.len(), 2);
    for (axiom, origins) in claims {
        let expected: &[_] = if axiom.scope.confidence == Some(0.75) {
            &["first", "second"]
        } else {
            &["third"]
        };
        assert_eq!(origins.len(), expected.len());
        for name in expected {
            assert!(origins.contains(&AxiomSource::Reification(node(&source, name))));
        }
    }
}

#[test]
fn literal_subject_keeps_its_formula_carrier_and_source_anchor() {
    let source = prepared(
        r#"ex:bad a logic:Formula ; logic:relation ex:p ; logic:argument
        [ logic:termIndex 0 ; logic:termLiteral "invalid subject" ],
        [ logic:termIndex 1 ; logic:termIri ex:value ] ."#,
    );
    let compiled = source.compile_with_sources(None).unwrap();
    let FormulaDisposition::Formula(index) = *disposition(&compiled, "bad") else {
        panic!("a literal argument is legal in FOL and requires the full formula carrier")
    };
    assert!(compiled.diagnostics().is_empty());
    assert_eq!(compiled.program().formulas.len(), 1);
    assert!(matches!(
        &compiled.program().formulas[index],
        Formula::Atom { args, .. } if matches!(&args[0], Term::Literal(literal) if literal.lexical_form == "invalid subject")
    ));
    assert!(
        compiled
            .axiom_sources()
            .iter()
            .flatten()
            .all(|origin| !matches!(origin, AxiomSource::Formula(_)))
    );
}
