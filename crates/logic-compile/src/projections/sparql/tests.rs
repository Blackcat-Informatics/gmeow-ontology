// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tiny GMEOW mapping controls. No repository sources or producer are consulted.

use super::*;
use crate::projections::correspondence_frontend::transpile_correspondences_indexed;
use crate::projections::get_leg::projections;
use purrdf::sparql::{NativeSparqlEngine, QueryOptions};
use purrdf::{RdfDataset, SparqlResult, parse_dataset};
use std::sync::Arc;

const PREFIXES: &str = "@prefix gm: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .";

fn dataset(source: &str) -> Arc<RdfDataset> {
    parse_dataset(
        format!("{PREFIXES}\n{source}").as_bytes(),
        "text/turtle",
        None,
    )
    .unwrap()
}

fn compile(source: &str, ontology: &str) -> SparqlLowering {
    let dsl = dataset(source);
    let onto = dataset(ontology);
    let dsl = DslView::new(&dsl);
    let onto = DslView::new(&onto);
    let (_, lookup) = transpile_correspondences_indexed(&dsl).unwrap();
    lower_cells(
        lookup.projection_cells(),
        &suppression_vocab(&onto),
        &lookup,
        &["test"],
    )
    .unwrap()
}

fn source_legs<'a>(lowered: &'a SparqlLowering, source: &str, cell: &str) -> &'a MappingLegs {
    let source = dataset(source);
    let cells = projections(&DslView::new(&source)).unwrap();
    let cell = cells.iter().find(|value| value.iri == cell).unwrap();
    &lowered.legs[&binding_key(cell, &cell.bindings[0])]
}

fn run(query: &Query, text: &str, input: &Arc<RdfDataset>) -> Arc<RdfDataset> {
    let engine = NativeSparqlEngine::new();
    let typed = engine
        .prepare_algebra(query.clone(), QueryOptions::EMPTY)
        .unwrap();
    let emitted = engine.prepare_query(text, None).unwrap();
    let evaluate = |query| {
        let SparqlResult::Graph(result) = engine
            .query_prepared(input, query, &[], QueryOptions::EMPTY)
            .unwrap()
        else {
            panic!("mapping requires a carrier")
        };
        result
    };
    let result = evaluate(&typed);
    let rendered = evaluate(&emitted);
    assert!(
        purrdf::datasets_isomorphic(&result, &rendered),
        "emitted mapping changed native execution"
    );
    result
}

const SIMPLE: &str = r#"
<urn:cell> a gm:ProjectionMapping;
  gm:hasMappingPattern [gm:anchor "s"; gm:value "v";
    gm:atom ([gm:subjectVar "s"; gm:predicate <urn:source>; gm:objectVar "v"])];
  gm:hasBinding [gm:profile "test"; gm:toPredicate <urn:view>; gm:relation "="; gm:mnemomorphic true].
"#;

#[test]
fn typed_and_emitted_binding_preserve_get_suppression_and_unguarded_put() {
    let lowered = compile(SIMPLE, "");
    let legs = source_legs(&lowered, SIMPLE, "urn:cell");
    let input = dataset(
        "<urn:visible> <urn:source> <urn:value> . <urn:hidden> <urn:source> <urn:secret>; gm:displayable false .",
    );
    let get = run(&legs.get, &lowered.queries["test.rq"], &input);
    assert!(purrdf::datasets_isomorphic(
        &get,
        &dataset("<urn:visible> <urn:view> <urn:value> .")
    ));
    let put = run(
        legs.put.as_ref().unwrap(),
        &lowered.put_queries["test.put.rq"],
        &dataset("<urn:hidden> <urn:view> <urn:edit> ."),
    );
    assert!(purrdf::datasets_isomorphic(
        &put,
        &dataset("<urn:hidden> <urn:source> <urn:edit> .")
    ));
    assert_eq!(
        lowered.ledger.len(),
        1,
        "isolated leg preparation must not duplicate losses"
    );
}

#[test]
fn typed_cell_domains_stay_isolated_from_profile_union_and_other_contexts() {
    let source = format!(
        "{SIMPLE}\n{}",
        SIMPLE
            .replace("urn:cell", "urn:other-cell")
            .replace("urn:source", "urn:other-source")
            .replace("urn:view", "urn:other-view")
    );
    let lowered = compile(&source, "");
    assert_eq!(lowered.legs.len(), 2);
    let legs = source_legs(&lowered, &source, "urn:cell");
    let isolated_text = native::emit(legs.get.clone()).unwrap();
    let input =
        dataset("<urn:item> <urn:source> <urn:value>; <urn:other-source> <urn:other-value> .");
    let isolated = run(&legs.get, &isolated_text, &input);
    assert!(purrdf::datasets_isomorphic(
        &isolated,
        &dataset("<urn:item> <urn:view> <urn:value> .")
    ));
    let mut contextual = purrdf::RdfDatasetBuilder::new();
    let s = contextual.intern_iri("urn:item");
    let p = contextual.intern_iri("urn:source");
    let o = contextual.intern_iri("urn:context-only");
    let world = contextual.intern_iri("urn:standpoint");
    contextual.push_quad(s, p, o, Some(world));
    let output = run(&legs.get, &isolated_text, &contextual.freeze().unwrap());
    assert_eq!(
        purrdf::native_quads::flat_rdf_quads(&output).count(),
        0,
        "named standpoint data is not a default-world assertion"
    );
}

#[test]
fn typed_mapping_executes_nested_paths_optionals_values_and_legalized_expressions() {
    let source = r#"
<urn:cell> a gm:ProjectionMapping;
 gm:hasMappingPattern [gm:anchor "s"; gm:value "kind";
  gm:atom ([gm:subjectVar "s"; gm:path [a gm:SeqPath; gm:pathSteps (<urn:edge> [a gm:AltPath; gm:pathAlts (<urn:left> <urn:right>)])]; gm:objectVar "kind"]
    [gm:optionalGroup ([gm:subjectVar "s"; gm:predicate <urn:label>; gm:objectVar "label"]
      [gm:optionalGroup ([gm:subjectVar "s"; gm:predicate <urn:extra>; gm:objectVar "extra"])])]);
  gm:bind [gm:bindVar "title"; gm:bindExpr [gm:exprOp gm:opConcat; gm:exprArgs ([gm:exprVar "label"] "!")]];
  gm:mint [gm:bindVar "copy"; gm:bindExpr [gm:exprOp gm:opIri; gm:exprArgs ("urn:copy")]];
  gm:filter [gm:exprOp gm:opEq; gm:exprArgs ([gm:exprVar "title"] "hello!")];
  gm:projectWhen [gm:subjectVar "s"; gm:predicate <urn:approved>; gm:objectLiteral true];
  gm:excludeWhen [gm:subjectVar "s"; gm:predicate <urn:excluded>; gm:objectLiteral true]];
 gm:hasBinding [gm:profile "test"; gm:relation "<=";
  gm:valueClassMap ([gm:whenValue <urn:kind>; gm:toClass <urn:Target>])].
"#;
    let lowered = compile(source, "");
    let legs = source_legs(&lowered, &source, "urn:cell");
    assert!(legs.put.is_none());
    let input = dataset(
        "<urn:s> <urn:edge> <urn:middle>; <urn:label> \"hello\"; <urn:approved> true . <urn:middle> <urn:right> <urn:kind> . <urn:unapproved> <urn:edge> <urn:middle>; <urn:label> \"hello\" .",
    );
    let output = run(&legs.get, &lowered.queries["test.rq"], &input);
    assert!(purrdf::datasets_isomorphic(
        &output,
        &dataset("<urn:s> a <urn:Target> .")
    ));
}

#[test]
fn typed_retag_preserves_script_selection_and_hidden_bearer_guard() {
    let source = r#"
<urn:cell> a gm:ProjectionMapping;
 gm:hasMappingPattern [gm:anchor "name"; gm:value "text"; gm:edoalSource gm:fullName;
   gm:atom ([gm:subjectVar "name"; gm:predicate gm:fullName; gm:objectVar "text"])];
 gm:hasBinding [gm:profile "test"; gm:toPredicate <urn:label>; gm:relation "<="].
"#;
    let ontology = "gm:hasName rdfs:range gm:Appellation . gm:fullName rdfs:domain gm:Appellation; gm:coarsenGuarded true .";
    let lowered = compile(source, ontology);
    let legs = source_legs(&lowered, &source, "urn:cell");
    let input = dataset(
        r#"
<urn:visible> gm:fullName "hello"@x-gmeow-test; gm:nameLanguage <urn:language>; gm:nameScript "Latn" .
<urn:variety> lang:varietyOf <urn:language>; lang:carrierTag "x-gmeow-test"; gm:bcp47Tag "en" .
<urn:bearer> gm:hasName <urn:hidden>; gm:displayable false .
<urn:hidden> gm:fullName "hidden"@x-gmeow-test; gm:nameLanguage <urn:language> .
<urn:coarsened> gm:fullName "coarsened"@x-gmeow-test; gm:coarsenTo <urn:replacement> .
"#,
    );
    let output = run(&legs.get, &lowered.queries["test.rq"], &input);
    assert!(purrdf::datasets_isomorphic(
        &output,
        &dataset("<urn:visible> <urn:label> \"hello\"@en-Latn .")
    ));
}

#[test]
fn unsupported_expression_has_loss_evidence_without_a_placeholder_binding() {
    let changed = SIMPLE.replace("gm:value \"v\";", "gm:value \"v\"; gm:bind [gm:bindVar \"ignored\"; gm:bindExpr [gm:exprOp gm:unsupportedOperator; gm:exprArgs ()]];");
    let lowered = compile(&changed, "");
    let legs = source_legs(&lowered, &changed, "urn:cell");
    let output = run(
        &legs.get,
        &lowered.queries["test.rq"],
        &dataset("<urn:s> <urn:source> <urn:o> ."),
    );
    assert!(purrdf::datasets_isomorphic(
        &output,
        &dataset("<urn:s> <urn:view> <urn:o> .")
    ));
    let drops = lowered.loss.projection_drops_for(&lowered.ledger[0].target);
    assert!(
        drops
            .iter()
            .any(|drop| drop.contains("bind ?ignored dropped")
                && drop.contains("unsupportedOperator"))
    );
    assert!(!lowered.queries["test.rq"].contains("unsupportedOperator"));
}

#[test]
fn native_and_emitted_candidate_put_keep_literal_evidence_and_import_ownership() {
    let source = r#"
<urn:cell> a gm:ProjectionMapping;
 gm:hasMappingPattern [gm:anchor "s";
  gm:atom ([gm:subjectVar "s"; gm:predicate <urn:measurement>; gm:objectLiteral "0.90"^^xsd:decimal])];
 gm:hasBinding [gm:profile "test"; gm:toClass <urn:External>; gm:relation "<=";
  gm:ingestClaim [gm:ingestLaw <https://blackcatinformatics.ca/logic/PutGet>;
   gm:ingestVerdict <https://blackcatinformatics.ca/logic/ObligationUnknown>;
   gm:ingestResidue "external input cannot attest the original measurement"]].
"#;
    let lowered = compile(source, "");
    let legs = source_legs(&lowered, &source, "urn:cell");
    let result = run(
        legs.put.as_ref().unwrap(),
        &lowered.put_queries["test.put.rq"],
        &dataset("<urn:item> a <urn:External> ."),
    );
    let rows = purrdf::native_quads::flat_rdf_quads(&result).collect::<Vec<_>>();
    assert!(!rows.iter().any(|row| row.predicate == "urn:measurement"));
    assert!(rows.iter().any(|row| row.predicate == "https://blackcatinformatics.ca/gmeow/qObjectLiteral" && matches!(&row.object,
        purrdf::RdfTerm::Literal(literal) if literal.lexical_form == "0.90" && literal.datatype.as_deref() == Some("http://www.w3.org/2001/XMLSchema#decimal"))));
    assert!(rows.iter().any(|row| row.predicate
        == "https://blackcatinformatics.ca/gmeow/wasGeneratedBy"
        && row.object == purrdf::RdfTerm::iri("https://blackcatinformatics.ca/gmeow/import/test")));
}

fn profile_output(lowered: &SparqlLowering, put: bool, input: &Arc<RdfDataset>) -> Arc<RdfDataset> {
    let text = if put {
        &lowered.put_queries["test.put.rq"]
    } else {
        &lowered.queries["test.rq"]
    };
    let query = purrdf::sparql::SparqlParser::new()
        .parse_query(text)
        .unwrap();
    run(&query, text, input)
}

fn independent_output(
    lowered: &SparqlLowering,
    put: bool,
    input: &Arc<RdfDataset>,
) -> Arc<RdfDataset> {
    let mut union = purrdf::RdfDatasetBuilder::new();
    for legs in lowered.legs.values() {
        let query = if put {
            let Some(put) = &legs.put else { continue };
            put
        } else {
            &legs.get
        };
        let result = run(query, &native::emit(query.clone()).unwrap(), input);
        union.push_dataset(&result);
    }
    union.freeze().unwrap()
}

#[test]
fn full_profiles_equal_independent_binding_execution_despite_shared_variable_names() {
    let other = SIMPLE
        .replace("urn:cell", "urn:other-cell")
        .replace("urn:source", "urn:other-source")
        .replace("urn:view", "urn:other-view");
    let source = format!("{SIMPLE}\n{other}");
    let lowered = compile(&source, "");
    for (put, input, expected) in [
        (
            false,
            "<urn:a> <urn:source> <urn:v> .",
            "<urn:a> <urn:view> <urn:v> .",
        ),
        (
            true,
            "<urn:a> <urn:view> <urn:v> .",
            "<urn:a> <urn:source> <urn:v> .",
        ),
        (
            false,
            "<urn:a> <urn:source> <urn:v> . <urn:b> <urn:other-source> <urn:w> .",
            "<urn:a> <urn:view> <urn:v> . <urn:b> <urn:other-view> <urn:w> .",
        ),
        (
            true,
            "<urn:a> <urn:view> <urn:v> . <urn:b> <urn:other-view> <urn:w> .",
            "<urn:a> <urn:source> <urn:v> . <urn:b> <urn:other-source> <urn:w> .",
        ),
    ] {
        let input = dataset(input);
        let actual = profile_output(&lowered, put, &input);
        assert!(
            purrdf::datasets_isomorphic(&actual, &dataset(expected)),
            "another binding's template crossed its branch"
        );
        assert!(purrdf::datasets_isomorphic(
            &actual,
            &independent_output(&lowered, put, &input)
        ));
    }
}

#[test]
fn candidate_ground_rows_and_template_blanks_belong_only_to_their_successful_branch() {
    let candidate = r#"
<urn:candidate> a gm:ProjectionMapping;
 gm:hasMappingPattern [gm:anchor "s";
  gm:atom ([gm:subjectVar "s"; gm:predicate <urn:measurement>; gm:objectLiteral "0.90"^^xsd:decimal])];
 gm:hasBinding [gm:profile "test"; gm:toClass <urn:External>; gm:relation "<=";
  gm:ingestClaim [gm:ingestLaw <https://blackcatinformatics.ca/logic/PutGet>;
   gm:ingestVerdict <https://blackcatinformatics.ca/logic/ObligationUnknown>;
   gm:ingestResidue "external type cannot attest the measurement"]].
"#;
    let other = candidate
        .replace("urn:candidate", "urn:other-candidate")
        .replace("urn:measurement", "urn:other-measurement")
        .replace("urn:External", "urn:OtherExternal");
    let lowered = compile(&format!("{SIMPLE}\n{candidate}\n{other}"), "");
    let recovered = profile_output(
        &lowered,
        true,
        &dataset("<urn:item> <urn:view> <urn:value> ."),
    );
    assert!(
        purrdf::datasets_isomorphic(
            &recovered,
            &dataset("<urn:item> <urn:source> <urn:value> .")
        ),
        "inactive claim branches must not emit ground import metadata or partial blank claims"
    );
    for input in [
        "<urn:item> a <urn:External> .",
        "<urn:item> a <urn:External>, <urn:OtherExternal> .",
    ] {
        let input = dataset(input);
        let actual = profile_output(&lowered, true, &input);
        assert!(
            purrdf::datasets_isomorphic(&actual, &independent_output(&lowered, true, &input)),
            "candidate templates must allocate independent blanks and retain branch-local claims"
        );
    }
}

#[test]
fn optional_binding_remains_unbound_in_its_own_profile_branch() {
    let source = r#"
<urn:optional> a gm:ProjectionMapping;
 gm:hasMappingPattern [gm:anchor "s"; gm:value "v";
  gm:atom ([gm:subjectVar "s"; gm:predicate <urn:source>; gm:objectVar "present"]
   [gm:subjectVar "s"; gm:predicate <urn:optional>; gm:objectVar "v"; gm:optional true])];
 gm:hasBinding [gm:profile "test"; gm:toPredicate <urn:optional-view>; gm:relation "<="].
"#;
    let lowered = compile(&format!("{SIMPLE}\n{source}"), "");
    let input = dataset("<urn:item> <urn:source> <urn:value> .");
    let output = profile_output(&lowered, false, &input);
    assert!(purrdf::datasets_isomorphic(
        &output,
        &dataset("<urn:item> <urn:view> <urn:value> .")
    ));
    assert!(purrdf::datasets_isomorphic(
        &output,
        &independent_output(&lowered, false, &input)
    ));
}

#[test]
fn complete_source_and_binding_identity_prevents_report_relation_and_law_aliases() {
    let a = SIMPLE.replace("urn:cell", "https://a.example/name");
    let b = SIMPLE
        .replace("urn:cell", "https://b.example/name")
        .replace("urn:source", "urn:other-source")
        .replace("urn:view", "urn:other-view");
    let source = format!(
        "{a}\n{b}\n<https://a.example/name> gm:hasBinding [gm:profile \"test\"; gm:toPredicate <urn:lossy-view>; gm:relation \"<=\"; gm:lossyDrop \"the additional profile omits a source distinction\"]."
    );
    let source = dataset(&source);
    let view = DslView::new(&source);
    let cells = projections(&view).unwrap();
    let (correspondences, lookup) = transpile_correspondences_indexed(&view).unwrap();
    let lowered = lower_cells(&cells, &SuppressionVocab::empty(), &lookup, &["test"]).unwrap();
    assert_eq!(correspondences.correspondences.len(), 3);
    assert_eq!(lookup.binding_keys().len(), 3);
    assert_eq!(lowered.legs.len(), 3);
    assert_eq!(
        lowered
            .ledger
            .iter()
            .map(|row| &row.target)
            .collect::<BTreeSet<_>>()
            .len(),
        3
    );
    for cell in &cells {
        for binding in &cell.bindings {
            let key = binding_key(cell, binding);
            let typed = lookup.binding(cell, binding).unwrap();
            assert_eq!(typed.morphism_class, binding.lattice().1);
            let legs = &lowered.legs[&key];
            assert_eq!(legs.put.is_some(), binding.mnemomorphic);
            let correspondence_iri = lookup
                .binding_keys()
                .iter()
                .find(|(_, value)| *value == &key)
                .unwrap()
                .0;
            let correspondence = correspondences
                .correspondences
                .iter()
                .find(|value| &value.iri == correspondence_iri)
                .unwrap();
            assert_eq!(correspondence.mnemomorphic, binding.mnemomorphic);
            let mut changed = binding.clone();
            changed.to_predicate = Some("urn:unpublished-binding".into());
            assert!(
                lookup.binding(cell, &changed).is_err(),
                "same profile must never substitute another binding's authority"
            );
            assert!(!lowered.legs.contains_key(&binding_key(cell, &changed)));
            let row = lowered
                .ledger
                .iter()
                .find(|row| row.target.starts_with("sparql:") && row.target.contains(&key))
                .unwrap();
            assert_eq!(
                row.target.rsplit(':').next().unwrap().len(),
                64,
                "retain the full report digest"
            );
            let actual = lowered
                .loss
                .projection_drops_for(&row.target)
                .into_iter()
                .filter(|note| note.starts_with("actual: "))
                .collect::<Vec<_>>();
            assert_eq!(actual.len(), binding.lossy_drops.len());
            assert!(actual.iter().all(|note| !note.contains("correspondence: ")));
            assert!(lookup.binding_keys().values().any(|value| value == &key));
        }
    }
}

#[test]
fn semantic_binding_key_retains_full_native_literal_evidence_and_leg_polarity() {
    let source = dataset(SIMPLE);
    let cell = projections(&DslView::new(&source)).unwrap().remove(0);
    let baseline = binding_key(&cell, &cell.bindings[0]);
    let mut binding = cell.bindings[0].clone();
    binding.mnemomorphic = false;
    assert_ne!(baseline, binding_key(&cell, &binding));
    binding = cell.bindings[0].clone();
    binding.lossy_drops.push("precise source evidence".into());
    assert_ne!(baseline, binding_key(&cell, &binding));
    let mut keys = BTreeSet::new();
    for (datatype, language, direction) in [
        ("urn:decimal", None, None),
        ("urn:string", None, None),
        (
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString",
            Some("en"),
            None,
        ),
        (
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString",
            Some("fr"),
            None,
        ),
        (
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString",
            Some("en"),
            Some(purrdf::RdfTextDirection::Ltr),
        ),
        (
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString",
            Some("en"),
            Some(purrdf::RdfTextDirection::Rtl),
        ),
    ] {
        let mut changed = cell.clone();
        let Item::Atom(atom) = &mut changed.pattern.atoms[0] else {
            panic!("fixture atom")
        };
        atom.object_var = None;
        atom.object_literal = Some(DslTerm::Literal {
            lexical_form: "0.90".into(),
            datatype: datatype.into(),
            language: language.map(str::to_owned),
            direction,
        });
        assert!(keys.insert(binding_key(&changed, &changed.bindings[0])));
    }
}

#[test]
fn expression_operator_names_do_not_grant_foreign_vocabulary_authority() {
    use crate::projections::get_leg::Expr;
    for operator in ["https://foreign.example/opEq", "urn:foreign#opConcat"] {
        let expression = Expr::Op {
            op: operator.into(),
            args: vec![Expr::Var("v".into()), Expr::Var("w".into())],
        };
        let refusal = native::expression(&expression).unwrap_err().to_string();
        assert!(refusal.contains(operator));
        assert!(refusal.contains("canonical GMEOW namespace"));
    }
    let admitted = Expr::Op {
        op: "https://blackcatinformatics.ca/gmeow/opEq".into(),
        args: vec![Expr::Var("v".into()), Expr::Var("w".into())],
    };
    assert!(native::expression(&admitted).is_ok());
}

#[test]
fn retag_helpers_never_capture_an_authored_value_with_the_same_name() {
    for value in ["_extTag", "_intTag", "_variety", "_sc", "_lang"] {
        let source = format!(
            r#"
<urn:cell> a gm:ProjectionMapping;
 gm:hasMappingPattern [gm:anchor "s"; gm:value "{value}";
  gm:atom ([gm:subjectVar "s"; gm:predicate rdfs:label; gm:objectVar "{value}"])];
 gm:hasBinding [gm:profile "test"; gm:toPredicate <urn:label>; gm:relation "<="].
"#
        );
        let lowered = compile(&source, "");
        let input = dataset(
            r#"
<urn:item> rdfs:label "hello"@x-gmeow-test .
<urn:variety> lang:varietyOf <urn:language>; lang:carrierTag "x-gmeow-test"; gm:bcp47Tag "en" .
"#,
        );
        let output = profile_output(&lowered, false, &input);
        assert!(
            purrdf::datasets_isomorphic(&output, &dataset("<urn:item> <urn:label> \"hello\"@en .")),
            "authored value ?{value} must remain distinct from generated retag roles"
        );
    }
}

#[test]
fn authored_language_final_value_and_hidden_bearer_roles_keep_their_own_bindings() {
    let source = r#"
<urn:cell> a gm:ProjectionMapping;
 gm:hasMappingPattern [gm:anchor "name"; gm:value "text"; gm:edoalSource gm:fullName;
  gm:atom ([gm:subjectVar "name"; gm:predicate gm:fullName; gm:objectVar "text"]
   [gm:subjectVar "name"; gm:predicate gm:nameLanguage; gm:objectVar "_lang"]
   [gm:subjectVar "name"; gm:predicate <urn:unrelated>; gm:objectVar "_supBearer"]
   [gm:subjectVar "name"; gm:predicate <urn:sentinel>; gm:objectVar "_final_text"]
   [gm:subjectVar "name"; gm:predicate <urn:script-sentinel>; gm:objectVar "_sc"]);
  gm:filter [gm:exprOp gm:opEq; gm:exprArgs ([gm:exprVar "_final_text"] "sentinel")]];
 gm:hasBinding [gm:profile "test"; gm:toPredicate <urn:label>; gm:relation "<="].
"#;
    let ontology =
        "gm:hasName rdfs:range gm:Appellation . gm:fullName rdfs:domain gm:Appellation .";
    let lowered = compile(source, ontology);
    let input = dataset(
        r#"
<urn:visible> gm:fullName "hello"@x-gmeow-test; gm:nameLanguage <urn:english>; <urn:unrelated> <urn:visible-bearer>; <urn:sentinel> "sentinel"; <urn:script-sentinel> "unrelated"; gm:nameScript "Latn" .
<urn:hidden> gm:fullName "secret"@x-gmeow-test; gm:nameLanguage <urn:english>; <urn:unrelated> <urn:visible-bearer>; <urn:sentinel> "sentinel"; <urn:script-sentinel> "unrelated"; gm:nameScript "Latn" .
<urn:hidden-bearer> gm:hasName <urn:hidden>; gm:displayable false .
<urn:english-variety> lang:varietyOf <urn:english>; lang:carrierTag "x-gmeow-test"; gm:bcp47Tag "en" .
<urn:french-variety> lang:varietyOf <urn:french>; lang:carrierTag "x-gmeow-test"; gm:bcp47Tag "fr" .
"#,
    );
    let output = profile_output(&lowered, false, &input);
    assert!(
        purrdf::datasets_isomorphic(
            &output,
            &dataset("<urn:visible> <urn:label> \"hello\"@en-Latn .")
        ),
        "authored language correlation, final-value filter and hidden-bearer exclusion must all survive fresh helper allocation"
    );
}

#[test]
fn value_class_lookup_does_not_capture_an_authored_class_suffixed_variable() {
    let source = r#"
<urn:cell> a gm:ProjectionMapping;
 gm:hasMappingPattern [gm:anchor "s"; gm:value "kind";
  gm:atom ([gm:subjectVar "s"; gm:predicate <urn:kind>; gm:objectVar "kind"]
   [gm:subjectVar "s"; gm:predicate <urn:evidence>; gm:objectVar "kindClass"]);
  gm:filter [gm:exprOp gm:opEq; gm:exprArgs ([gm:exprVar "kindClass"] <urn:SourceEvidence>)]];
 gm:hasBinding [gm:profile "test"; gm:relation "<=";
  gm:valueClassMap ([gm:whenValue <urn:kind-value>; gm:toClass <urn:Target>])].
"#;
    let lowered = compile(source, "");
    let input =
        dataset("<urn:item> <urn:kind> <urn:kind-value>; <urn:evidence> <urn:SourceEvidence> .");
    let output = profile_output(&lowered, false, &input);
    assert!(purrdf::datasets_isomorphic(
        &output,
        &dataset("<urn:item> a <urn:Target> .")
    ));
}

#[test]
fn helper_inventory_includes_templates_nested_atoms_predicate_variables_and_expressions() {
    let source = dataset(
        r#"
<urn:cell> a gm:ProjectionMapping;
 gm:hasMappingPattern [gm:anchor "s"; gm:value "kind";
  gm:atom ([gm:optionalGroup ([gm:subjectVar "s"; gm:predicate <urn:optional>; gm:objectVar "_extTag"])]
   [gm:subjectVar "s"; gm:predicateVar "_sc"; gm:objectVar "kind"]);
  gm:suppressWhen [gm:subjectVar "s"; gm:predicate <urn:guard>; gm:objectVar "_supBearer"];
  gm:projectWhen [gm:subjectVar "_final_kind"; gm:predicate <urn:guard>; gm:objectVar "kind"];
  gm:excludeWhen [gm:subjectVar "s"; gm:predicate <urn:guard>; gm:objectVar "_extTag_0"];
  gm:filter [gm:exprOp gm:opBound; gm:exprArgs ([gm:exprVar "_lang"])];
  gm:mint [gm:bindVar "minted"; gm:bindExpr [gm:exprVar "_variety"]];
  gm:bind [gm:bindVar "_intTag"; gm:bindExpr "literal"]];
 gm:hasBinding [gm:profile "test"; gm:relation "<=";
  gm:templateAtoms ([gm:tSubj "s"; gm:tPred <urn:view>; gm:tObj "kindClass"])].
"#,
    );
    let cell = projections(&DslView::new(&source)).unwrap().remove(0);
    let names = HelperNames::for_binding(&cell, &cell.bindings[0]);
    let allocated = [
        &names.class,
        &names.bearer,
        &names.language,
        &names.variety,
        &names.internal_tag,
        &names.external_tag,
        &names.script,
        &names.final_value,
    ];
    let authored = BTreeSet::from([
        "s",
        "kind",
        "_extTag",
        "_extTag_0",
        "_sc",
        "_supBearer",
        "_final_kind",
        "_lang",
        "minted",
        "_variety",
        "_intTag",
        "kindClass",
    ]);
    assert_eq!(
        allocated.iter().collect::<BTreeSet<_>>().len(),
        allocated.len()
    );
    assert!(
        allocated
            .iter()
            .all(|name| !authored.contains(name.as_str()))
    );
}

#[test]
fn name_part_retag_reuses_an_authored_parent_language_binding() {
    let source = r#"
<urn:cell> a gm:ProjectionMapping;
 gm:hasMappingPattern [gm:anchor "part"; gm:value "text"; gm:edoalSource gm:partText;
  gm:atom ([gm:subjectVar "part"; gm:predicate gm:partText; gm:objectVar "text"]
   [gm:subjectVar "name"; gm:predicate gm:hasNamePart; gm:objectVar "part"]
   [gm:subjectVar "name"; gm:predicate gm:nameLanguage; gm:objectVar "_lang"]);
  gm:filter [gm:exprOp gm:opEq; gm:exprArgs ([gm:exprVar "_lang"] <urn:english>)]];
 gm:hasBinding [gm:profile "test"; gm:toPredicate <urn:label>; gm:relation "<="].
"#;
    let lowered = compile(source, "");
    let input = dataset(
        r#"
<urn:part> gm:partText "hello"@x-gmeow-test .
<urn:name> gm:hasNamePart <urn:part>; gm:nameLanguage <urn:english>, <urn:french> .
<urn:english-variety> lang:varietyOf <urn:english>; lang:carrierTag "x-gmeow-test"; gm:bcp47Tag "en" .
<urn:french-variety> lang:varietyOf <urn:french>; lang:carrierTag "x-gmeow-test"; gm:bcp47Tag "fr" .
"#,
    );
    let output = profile_output(&lowered, false, &input);
    assert!(
        purrdf::datasets_isomorphic(&output, &dataset("<urn:part> <urn:label> \"hello\"@en .")),
        "allocated helper names must not disconnect a real authored parent-language correlation"
    );
}
