// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::{BlankScope, RdfDatasetBuilder};

const EX: &str = "http://example.org/";

/// Build a small default-graph dataset from `(s, p, o)` triples where each term is
/// `i:<iri>`, `l:<lexical>` (a plain literal), or `b:<label>`.
fn dataset(triples: &[(&str, &str, &str)]) -> std::sync::Arc<RdfDataset> {
    let mut b = RdfDatasetBuilder::new();
    let intern = |b: &mut RdfDatasetBuilder, t: &str| -> TermId {
        if let Some(rest) = t.strip_prefix("i:") {
            b.intern_iri(rest)
        } else if let Some(rest) = t.strip_prefix("b:") {
            b.intern_blank(rest, BlankScope::DEFAULT)
        } else if let Some(rest) = t.strip_prefix("l:") {
            b.intern_literal(purrdf::RdfLiteral::simple(rest.to_owned()))
        } else {
            panic!("bad test term {t}")
        }
    };
    for (s, p, o) in triples {
        let s = intern(&mut b, s);
        let pid = match p.strip_prefix("i:") {
            Some(rest) => b.intern_iri(rest),
            None => panic!("predicate must be i:<iri>"),
        };
        let o = intern(&mut b, o);
        b.push_quad(s, pid, o, None);
    }
    b.freeze().expect("freeze")
}

#[test]
fn subjects_of_type_sorted_and_deduped() {
    let t = format!("i:{RDF_TYPE}");
    let cls = format!("i:{EX}Cls");
    let ds = dataset(&[
        (&format!("i:{EX}b"), &t, &cls),
        (&format!("i:{EX}a"), &t, &cls),
        (&format!("i:{EX}a"), &t, &cls),
    ]);
    let v = DslView::new(&ds);
    assert_eq!(
        v.subjects_of_type(&format!("{EX}Cls")),
        vec![format!("{EX}a"), format!("{EX}b")]
    );
}

#[test]
fn object_accessors() {
    let ds = dataset(&[
        (
            &format!("i:{EX}s"),
            &format!("i:{EX}p"),
            &format!("i:{EX}o"),
        ),
        (&format!("i:{EX}s"), &format!("i:{EX}lit"), "l:hello"),
    ]);
    let v = DslView::new(&ds);
    assert_eq!(
        v.object_iri(&format!("{EX}s"), &format!("{EX}p")),
        Some(format!("{EX}o"))
    );
    assert_eq!(
        v.object_literal(&format!("{EX}s"), &format!("{EX}lit")),
        Some("hello".to_owned())
    );
    assert_eq!(
        v.object_iri(&format!("{EX}s"), &format!("{EX}missing")),
        None
    );
}

#[test]
fn rdf_list_in_order_through_blanks() {
    let first = format!("i:{RDF_FIRST}");
    let rest = format!("i:{RDF_REST}");
    let nil = format!("i:{RDF_NIL}");
    // ( ex:x ex:y ) as _:l1 -> _:l2 -> nil
    let ds = dataset(&[
        ("b:l1", &first, &format!("i:{EX}x")),
        ("b:l1", &rest, "b:l2"),
        ("b:l2", &first, &format!("i:{EX}y")),
        ("b:l2", &rest, &nil),
    ]);
    let v = DslView::new(&ds);
    let head = DslTerm::Blank {
        label: "l1".to_owned(),
        scope: BlankScope::DEFAULT,
    };
    let items: Vec<String> = v
        .rdf_list(Some(&head))
        .into_iter()
        .filter_map(|t| t.as_iri().map(str::to_owned))
        .collect();
    assert_eq!(items, vec![format!("{EX}x"), format!("{EX}y")]);
}

#[test]
fn reified_statement_and_annotation_accessors_round_trip() {
    use purrdf::{RdfAnnotation, RdfLiteral, RdfReifier, RdfTerm, RdfTriple};

    let subj = format!("{EX}VirtualLocation");
    let pred = "http://www.w3.org/2004/02/skos/core#closeMatch";
    let obj = format!("{EX}Target");
    let reifier = RdfTerm::iri(format!("{EX}cell1"));
    let base = RdfTriple::new(RdfTerm::iri(&subj), pred, RdfTerm::iri(&obj));

    let mut b = RdfDatasetBuilder::new();
    b.push_owned_reifier(&RdfReifier::new(reifier.clone(), base));
    // An IRI annotation, two literal annotations under one predicate (multi), a typed
    // annotation (the grounding flag), and a numeric literal.
    b.push_owned_annotation(&RdfAnnotation::new(
        reifier.clone(),
        format!("{EX}justification"),
        RdfTerm::iri("https://w3id.org/semapv/vocab/ManualMappingCuration"),
    ));
    b.push_owned_annotation(&RdfAnnotation::new(
        reifier.clone(),
        format!("{EX}lossyDrop"),
        RdfTerm::literal(RdfLiteral::simple("beta")),
    ));
    b.push_owned_annotation(&RdfAnnotation::new(
        reifier.clone(),
        format!("{EX}lossyDrop"),
        RdfTerm::literal(RdfLiteral::simple("alpha")),
    ));
    b.push_owned_annotation(&RdfAnnotation::new(
        reifier.clone(),
        format!("{EX}confidence"),
        RdfTerm::literal(RdfLiteral::simple("0.9")),
    ));
    b.push_owned_annotation(&RdfAnnotation::new(
        reifier.clone(),
        RDF_TYPE,
        RdfTerm::iri(format!("{EX}GroundingCorrespondence")),
    ));
    let ds = b.freeze().expect("freeze rdf-1.2 dataset");
    let v = DslView::new(&ds);

    let stmts: Vec<_> = v.reified_statements().collect();
    assert_eq!(stmts.len(), 1);
    let stmt = &stmts[0];
    assert!(matches!(stmt.reifier(), TermRef::Iri(iri) if iri == format!("{EX}cell1")));
    let (s, p, o) = stmt.triple().unwrap();
    assert!(matches!(s, TermRef::Iri(iri) if iri == subj));
    assert!(matches!(p, TermRef::Iri(iri) if iri == pred));
    assert!(matches!(o, TermRef::Iri(iri) if iri == obj));

    assert_eq!(
        stmt.annotation_iri(&format!("{EX}justification")).unwrap(),
        Some("https://w3id.org/semapv/vocab/ManualMappingCuration")
    );
    assert_eq!(
        stmt.annotation_literal(&format!("{EX}confidence")).unwrap(),
        Some("0.9")
    );
    assert_eq!(
        stmt.annotation_literals(&format!("{EX}lossyDrop")).unwrap(),
        vec!["alpha".to_owned(), "beta".to_owned()]
    );
    assert!(stmt.annotation_has_type(&format!("{EX}GroundingCorrespondence")));
    assert!(!stmt.annotation_has_type(&format!("{EX}Nope")));
    // A predicate/reifier miss yields nothing.
    assert_eq!(stmt.annotation_iri(&format!("{EX}missing")).unwrap(), None);
}
