// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
// Reification consumer-side vocab used only by the assertions below (the executor's
// production path renders these through the shared reified-claim builder).
use crate::up_projection_corpus::{GM_Q_OBJECT, GM_Q_PREDICATE};

fn run_one(
    engine: &NativeSparqlEngine,
    dataset: &std::sync::Arc<purrdf::RdfDataset>,
    q: Query,
) -> Vec<RdfQuad> {
    let prepared = engine
        .prepare_algebra(q, QueryOptions::EMPTY)
        .expect("admit construct");
    run_construct(engine, dataset, &prepared).expect("construct ok")
}

#[test]
fn canonical_leg_path_lowering_is_exercised() {
    // Single forward + inverse steps: the surface the put legs route through.
    assert_eq!(
        lower_leg_path(&LegPath::Step("http://ex/p".into())).to_string(),
        "<http://ex/p>"
    );
    assert_eq!(
        lower_leg_path(&LegPath::Inverse(Box::new(LegPath::Step(
            "http://ex/p".into()
        ))))
        .to_string(),
        "^<http://ex/p>"
    );
    // Seq / Alt confirm the paths lib handles composite bodies.
    let seq = LegPath::Seq(vec![
        LegPath::Step("http://ex/a".into()),
        LegPath::Step("http://ex/b".into()),
    ]);
    assert_eq!(
        lower_leg_path(&seq).to_string(),
        "<http://ex/a>/<http://ex/b>"
    );
    let alt = LegPath::Alt(vec![
        LegPath::Step("http://ex/a".into()),
        LegPath::Step("http://ex/b".into()),
    ]);
    assert_eq!(
        lower_leg_path(&alt).to_string(),
        "<http://ex/a>|<http://ex/b>"
    );
}

#[test]
fn fact_construct_renames_a_predicate() {
    // A GMEOW rename executes through the same F2 algebra as production.
    let source_nt = "<http://a/1> <http://ex/knows> <http://a/2> .\n";
    let source = Graph::parse(source_nt.as_bytes(), "application/n-triples").expect("parse");
    let query = construct(
        vec![
            triple(
                var("s"),
                "https://blackcatinformatics.ca/gmeow/knows",
                var("o"),
            )
            .unwrap(),
        ],
        filtered(step_pattern("http://ex/knows").unwrap(), resource_object()),
    );
    let engine = NativeSparqlEngine::new();
    let out = run_one(&engine, &source.dataset, query);
    assert_eq!(out.len(), 1, "one renamed triple");
    assert_eq!(
        out[0].predicate,
        "https://blackcatinformatics.ca/gmeow/knows"
    );
    assert!(matches!(&out[0].subject, RdfTerm::Iri(n) if n == "http://a/1"));
    assert!(matches!(&out[0].object, RdfTerm::Iri(n) if n == "http://a/2"));
}

#[test]
fn end_to_end_rename_lifts_via_sssom() {
    // A clean (exactMatch) SSSOM row aligning gmeow:knows and ex:knows, with a
    // custom curie map so `ex:` resolves. sssom_clean_pairs keys on the projection
    // namespace; foaf is a projection prefix, so align via foaf:knows.
    let sssom = concat!(
        "#curie_map:\n",
        "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
        "#  foaf: http://xmlns.com/foaf/0.1/\n",
        "#  skos: http://www.w3.org/2004/02/skos/core#\n",
        "subject_id\tpredicate_id\tobject_id\n",
        "gmeow:knows\tskos:exactMatch\tfoaf:knows\n",
    );
    let source_nt = "<http://a/1> <http://xmlns.com/foaf/0.1/knows> <http://a/2> .\n";
    let report = execute_put_legs(source_nt, &[sssom.to_owned()], &[], "", &BTreeSet::new())
        .expect("execute put legs");
    assert!(
        report
            .graph_nt
            .contains("<https://blackcatinformatics.ca/gmeow/knows>"),
        "renamed predicate present: {}",
        report.graph_nt
    );
    assert!(report.lifted >= 1, "at least one fact lifted");
}

#[test]
fn shared_put_program_keeps_each_sources_answers_separate() {
    let sssom = concat!(
        "#curie_map:\n",
        "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
        "#  foaf: http://xmlns.com/foaf/0.1/\n",
        "#  skos: http://www.w3.org/2004/02/skos/core#\n",
        "subject_id\tpredicate_id\tobject_id\n",
        "gmeow:knows\tskos:exactMatch\tfoaf:knows\n",
    );
    let program = PutLegProgram::derive(&[sssom.into()], &[], "", &BTreeSet::new())
        .expect("prepare GMEOW lift");
    std::thread::scope(|scope| {
        let workers: Vec<_> = ["alice", "bob"].into_iter().map(|name| {
                let program = &program;
                scope.spawn(move || {
                    let source = format!(
                        "<http://example.org/{name}> <http://xmlns.com/foaf/0.1/knows> <http://example.org/friend> .\n"
                    );
                    let graph = Graph::parse(source.as_bytes(), "application/n-triples")
                        .expect("source");
                    let native = execute_put_legs_default_graph(&graph.dataset, program)
                        .expect("native lift").into_text().expect("text boundary");
                    let textual = execute_put_legs_with(&source, program).expect("text lift");
                    assert_eq!(native, textual);
                    assert_eq!(native.lifted, 1);
                    assert!(native.graph_nt.contains(&format!("<http://example.org/{name}>")));
                })
            }).collect();
        for worker in workers {
            worker.join().expect("isolated lift worker");
        }
    });
}

#[test]
fn close_match_yields_a_reified_claim() {
    // A closeMatch row (with confidence) becomes a lossy claim, not a fact.
    let sssom = concat!(
        "#curie_map:\n",
        "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
        "#  foaf: http://xmlns.com/foaf/0.1/\n",
        "#  skos: http://www.w3.org/2004/02/skos/core#\n",
        "subject_id\tpredicate_id\tobject_id\tconfidence\n",
        "gmeow:acquaintanceOf\tskos:closeMatch\tfoaf:knows\t0.8\n",
    );
    let source_nt = "<http://a/1> <http://xmlns.com/foaf/0.1/knows> <http://a/2> .\n";
    let report = execute_put_legs(source_nt, &[sssom.to_owned()], &[], "", &BTreeSet::new())
        .expect("execute put legs");
    assert!(report.claimed >= 1, "at least one claim cell: {report:?}");
    assert!(
        report
            .graph_nt
            .contains("<https://blackcatinformatics.ca/gmeow/StatementMetadata>"),
        "statement-metadata cell present: {}",
        report.graph_nt
    );
    assert!(
        report.graph_nt.contains(
            "<https://blackcatinformatics.ca/gmeow/qPredicate> \
                     <https://blackcatinformatics.ca/gmeow/acquaintanceOf>"
        ) || report
            .graph_nt
            .contains("<https://blackcatinformatics.ca/gmeow/acquaintanceOf>"),
        "qPredicate points at the gmeow term: {}",
        report.graph_nt
    );
    assert!(
        report
            .graph_nt
            .contains("<https://blackcatinformatics.ca/gmeow/mappedFrom>"),
        "mappedFrom annotation present: {}",
        report.graph_nt
    );
}

#[test]
fn native_put_preserves_directional_claim_values_and_discloses_the_mapping() {
    use gmeow_logic_compile::projections::reified_claim::{
        GM_ANN_PROPERTY, GM_ANN_VALUE, GM_Q_OBJECT_LITERAL,
    };

    let sssom = concat!(
        "#curie_map:\n",
        "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
        "#  foaf: http://xmlns.com/foaf/0.1/\n",
        "#  skos: http://www.w3.org/2004/02/skos/core#\n",
        "subject_id\tpredicate_id\tobject_id\tconfidence\n",
        "gmeow:acquaintanceOf\tskos:closeMatch\tfoaf:knows\t0.8\n",
    );
    let source = Graph::parse(
        br#"<http://a/1> <http://xmlns.com/foaf/0.1/knows> "Ada"@ar--rtl ."#,
        "application/n-triples",
    )
    .unwrap();
    let expected = source.quads[0].object.clone();
    let program = PutLegProgram::derive(&[sssom.to_owned()], &[], "", &BTreeSet::new()).unwrap();
    let result = execute_put_legs_default_graph(&source.dataset, &program).unwrap();
    assert_eq!(
        result.lifted, 0,
        "a lossy claim must not become an asserted relation"
    );
    assert_eq!(result.claimed, 1);
    let rows = purrdf::native_quads::flat_rdf_quads_from_dataset(&result.dataset);
    assert!(
        rows.iter()
            .any(|row| row.predicate == GM_Q_OBJECT_LITERAL && row.object == expected)
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.predicate == "https://blackcatinformatics.ca/gmeow/acquaintanceOf")
    );

    // Resolve annotation cells by their property instead of relying on template labels.
    let value_for = |property: &str| {
        let annotation = rows
            .iter()
            .find(|row| row.predicate == GM_ANN_PROPERTY && row.object == RdfTerm::iri(property))
            .expect("required claim annotation");
        &rows
            .iter()
            .find(|row| row.subject == annotation.subject && row.predicate == GM_ANN_VALUE)
            .expect("required annotation value")
            .object
    };
    assert_eq!(
        value_for(GM_MAPPED_FROM),
        &RdfTerm::iri("http://xmlns.com/foaf/0.1/knows")
    );
    assert_eq!(
        value_for(GM_CONFIDENCE),
        &RdfTerm::Literal(purrdf::RdfLiteral::typed("0.8", XSD_DECIMAL))
    );
}

#[test]
fn native_put_refuses_invalid_mapping_iris_before_query_execution() {
    for invalid in ["relative-predicate", "https://example.org/not an iri"] {
        assert!(step_pattern(invalid).is_err());
        assert!(
            claim_query(
                invalid,
                "https://example.org/target",
                "",
                ClaimSlot::PredicateIri
            )
            .is_err()
        );
        assert!(
            claim_query(
                "https://example.org/source",
                invalid,
                "",
                ClaimSlot::PredicateLiteral
            )
            .is_err()
        );
    }
}

#[test]
fn two_distinct_claim_rules_stay_on_separate_cells() {
    // Two distinct closeMatch claim rules, each matched by one source triple. Before
    // the per-query blank rescoping, the two claim CONSTRUCTs minted the same `_:cell`
    // label in independent datasets and merged into ONE corrupt node carrying both
    // qPredicates. After the fix they stay on two separate cells.
    let sssom = concat!(
        "#curie_map:\n",
        "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
        "#  foaf: http://xmlns.com/foaf/0.1/\n",
        "#  skos: http://www.w3.org/2004/02/skos/core#\n",
        "subject_id\tpredicate_id\tobject_id\tconfidence\n",
        "gmeow:a\tskos:closeMatch\tfoaf:knows\t0.8\n",
        "gmeow:b\tskos:closeMatch\tfoaf:member\t0.7\n",
    );
    let source_nt = concat!(
        "<http://a/1> <http://xmlns.com/foaf/0.1/knows> <http://a/2> .\n",
        "<http://a/1> <http://xmlns.com/foaf/0.1/member> <http://a/3> .\n",
    );
    let report = execute_put_legs(source_nt, &[sssom.to_owned()], &[], "", &BTreeSet::new())
        .expect("execute put legs");
    assert_eq!(report.claimed, 2, "two distinct claim cells: {report:?}");

    // Parse the lifted graph and map each cell blank -> the set of qPredicate IRIs on it.
    let graph = Graph::parse(report.graph_nt.as_bytes(), "application/n-triples")
        .expect("parse lifted graph");
    let mut cell_preds: std::collections::BTreeMap<String, BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for q in &graph.quads {
        if q.predicate == GM_Q_PREDICATE
            && let (RdfTerm::BlankNode(cell), RdfTerm::Iri(p)) = (&q.subject, &q.object)
        {
            cell_preds
                .entry(cell.clone())
                .or_default()
                .insert(p.clone());
        }
    }
    let a = "https://blackcatinformatics.ca/gmeow/a";
    let b = "https://blackcatinformatics.ca/gmeow/b";
    // No single cell carries BOTH qPredicates.
    for (cell, preds) in &cell_preds {
        assert!(
            !(preds.contains(a) && preds.contains(b)),
            "cell {cell} corruptly carries both qPredicates: {preds:?}"
        );
    }
    // Each qPredicate lands on exactly one distinct cell, and they differ.
    let cell_of = |pred: &str| -> Option<String> {
        cell_preds
            .iter()
            .find(|(_, preds)| preds.contains(pred))
            .map(|(cell, _)| cell.clone())
    };
    let cell_a = cell_of(a).expect("cell for gmeow:a");
    let cell_b = cell_of(b).expect("cell for gmeow:b");
    assert_ne!(cell_a, cell_b, "the two claims must be on separate cells");
}

#[test]
fn class_level_close_match_yields_a_type_object_claim() {
    // A source `<x> rdf:type <ext_class>` where ext_class is a closeMatch claim rule.
    // lift_edge treats this as a claim: qPredicate = rdf:type, qObject = the gmeow class.
    let sssom = concat!(
        "#curie_map:\n",
        "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
        "#  foaf: http://xmlns.com/foaf/0.1/\n",
        "#  skos: http://www.w3.org/2004/02/skos/core#\n",
        "subject_id\tpredicate_id\tobject_id\tconfidence\n",
        "gmeow:Acquaintance\tskos:closeMatch\tfoaf:Agent\t0.75\n",
    );
    let source_nt = concat!(
        "<http://a/1> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ",
        "<http://xmlns.com/foaf/0.1/Agent> .\n",
    );
    let report = execute_put_legs(source_nt, &[sssom.to_owned()], &[], "", &BTreeSet::new())
        .expect("execute put legs");
    assert!(
        report.claimed >= 1,
        "at least one type-object claim: {report:?}"
    );

    let graph = Graph::parse(report.graph_nt.as_bytes(), "application/n-triples")
        .expect("parse lifted graph");
    // A cell whose qPredicate is rdf:type AND qObject is the gmeow class.
    let type_object_cell = graph.quads.iter().any(|q| {
        q.predicate == GM_Q_PREDICATE
            && matches!(&q.object, RdfTerm::Iri(p) if p == RDF_TYPE)
            && matches!(&q.subject, RdfTerm::BlankNode(cell) if graph.quads.iter().any(|r| {
                matches!(&r.subject, RdfTerm::BlankNode(c) if c == cell)
                    && r.predicate == GM_Q_OBJECT
                    && matches!(&r.object, RdfTerm::Iri(o)
                        if o == "https://blackcatinformatics.ca/gmeow/Acquaintance")
            }))
    });
    assert!(
        type_object_cell,
        "a claim cell with qPredicate rdf:type and qObject gmeow:Acquaintance: {}",
        report.graph_nt
    );
}

#[test]
fn empty_source_is_an_error() {
    let err =
        execute_put_legs("", &[], &[], "", &BTreeSet::new()).expect_err("empty source rejected");
    assert!(err.to_string().contains("source graph is empty"), "{err}");
}

/// One `gmeow:ProjectionMapping` EDOAL-path cell: a single-atom, no-mint pattern whose atom
/// carries `predicate <apred>` and (`subjectVar` == anchor ⇒ direct / `objectVar` == anchor
/// ⇒ inverse), with a binding `toPredicate <target>`. Two such cells for the same target —
/// a direct one and an inverse one — give `edoalpath_pairs` a `direct`/`inverse` pair, and
/// the single-atom no-mint binding also registers the target as a `simple-1to1` structural
/// class (bucket `clean`), so the term reaches `VerifiableRoundTrip` in the shared classifier.
fn edoal_cell(cell: &str, target: &str, apred: &str, inverse: bool) -> String {
    let (subj, obj) = if inverse { ("o", "s") } else { ("s", "o") };
    format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix foaf: <http://xmlns.com/foaf/0.1/> .\n\
             gmeow:{cell} a gmeow:ProjectionMapping ;\n\
               gmeow:hasMappingPattern [\n\
                 gmeow:anchor \"s\" ; gmeow:value \"o\" ;\n\
                 gmeow:atom ( [ gmeow:subjectVar \"{subj}\" ; gmeow:predicate <{apred}> ; \
                                gmeow:objectVar \"{obj}\" ] ) ;\n\
                 gmeow:edoalPath true ] ;\n\
               gmeow:hasBinding [ gmeow:profile \"foaf\" ; gmeow:toPredicate <{target}> ; \
                                  gmeow:relation \"=\" ] .\n"
    )
}

#[test]
fn gate_red_excluded_term_is_never_lifted_by_the_executor() {
    // The parity guard. `foaf:bad` has a direct EDOAL path on
    // <gmeow:forward> and an inverse EDOAL path on a DIFFERENT predicate <gmeow:notInverse>:
    // the reverse path does NOT invert the forward path, so the correspondence round-trip
    // gate RED-excludes it (audit tier = red_excluded). `foaf:good` has a matching
    // direct+inverse pair on <gmeow:good>, so it is proved-lawful and MUST be lifted.
    //
    // The executor's lifted external-term set must therefore be exactly the audit's
    // non-red-excluded (proved+claimed) set: `foaf:good` lifts, `foaf:bad` never does. If a
    // future change reintroduces an ungated derivation, `foaf:bad` would leak back in and
    // this test FAILS.
    let good = "http://xmlns.com/foaf/0.1/good";
    let bad = "http://xmlns.com/foaf/0.1/bad";
    let projection_ttls = vec![
        edoal_cell(
            "mapGoodFwd",
            good,
            "https://blackcatinformatics.ca/gmeow/good",
            false,
        ),
        edoal_cell(
            "mapGoodInv",
            good,
            "https://blackcatinformatics.ca/gmeow/good",
            true,
        ),
        edoal_cell(
            "mapBadFwd",
            bad,
            "https://blackcatinformatics.ca/gmeow/forward",
            false,
        ),
        edoal_cell(
            "mapBadInv",
            bad,
            "https://blackcatinformatics.ca/gmeow/notInverse",
            true,
        ),
    ];

    // Cross-check the shared producer directly: bad is gate-excluded, good survives.
    let program = gate_verified_lift_program(&[], &projection_ttls, &BTreeSet::new())
        .expect("lift program builds");
    assert!(
        program.rules.contains_key(good),
        "the proved (matching-inverse) term must survive the gate: {program:?}"
    );
    assert!(
        !program.rules.contains_key(bad),
        "the red-excluded (non-inverting) term must NOT survive the gate: {program:?}"
    );
    assert!(
        program.gate_excluded >= 1,
        "the non-inverting term is surfaced as gate-excluded residue: {program:?}"
    );

    // And end-to-end through the executor: a source using BOTH terms lifts good, never bad.
    let source_nt = concat!(
        "<http://a/1> <http://xmlns.com/foaf/0.1/good> <http://a/2> .\n",
        "<http://a/1> <http://xmlns.com/foaf/0.1/bad> <http://a/3> .\n",
        "<http://a/2> <http://xmlns.com/foaf/0.1/bad> <http://a/4> .\n",
    );
    let report = execute_put_legs(source_nt, &[], &projection_ttls, "", &BTreeSet::new())
        .expect("execute put legs");
    assert!(
        report
            .graph_nt
            .contains("<https://blackcatinformatics.ca/gmeow/good>"),
        "the proved term is lifted: {}",
        report.graph_nt
    );
    // No lifted triple may mention EITHER the gate-excluded external term or its gmeow
    // targets — the executor must not have run a rule for `foaf:bad` at all.
    for leaked in [
        bad,
        "https://blackcatinformatics.ca/gmeow/forward",
        "https://blackcatinformatics.ca/gmeow/notInverse",
    ] {
        assert!(
            !report.graph_nt.contains(leaked),
            "the gate-excluded term leaked a lifted triple mentioning <{leaked}>: {}",
            report.graph_nt
        );
    }
    // `foaf:bad` has no gate-surviving rule, so it is an honest gap term, not a silent drop.
    // It occurs TWICE in the source; the reported count must be the real occurrence count
    // (2), never a fabricated constant (1).
    assert_eq!(
        report.gap_terms.get("foaf:bad").copied(),
        Some(2),
        "the gate-excluded term's gap count must be the TRUE occurrence count, not a \
             fabricated constant: {:?}",
        report.gap_terms
    );
}
