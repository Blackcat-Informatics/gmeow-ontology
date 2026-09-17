// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::is_exact_correspondence;

const FORMULA_DEN: &str = r#"
@prefix lang:  <https://blackcatinformatics.ca/lang/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:    <http://example.org/lang/> .

ex:sent a lang:ComposedForm ; rdfs:label "cats chase mice" .
ex:act a lang:CommunicativeAct ; lang:performedOn ex:sent ; lang:communicativeForce lang:assertForce .
ex:den a lang:Denotation ;
    lang:denotedForm ex:sent ;
    lang:denotationKind lang:denotesLogicFormula ;
    lang:denotationTarget logic:catsChaseMiceFormula ;
    lang:isIndexical false .
"#;

const QUERY_DEN: &str = r#"
@prefix lang:  <https://blackcatinformatics.ca/lang/> .
@prefix ex:    <http://example.org/lang/> .
ex:q a lang:Denotation ;
    lang:denotationKind lang:denotesQuery ;
    lang:denotationTarget ex:someQuery .
"#;

fn src(name: &str, ttl: &str) -> NamedSource {
    NamedSource {
        name: name.to_owned(),
        bytes: ttl.as_bytes().to_vec(),
    }
}

#[test]
fn logic_formula_denotation_lowers_to_amr_soundunder() {
    let input = LangProjectionInput {
        lang_models: vec![src("f", FORMULA_DEN)],
        ..Default::default()
    };
    let emissions = SemafBridge.emit(&input).expect("emit");
    assert_eq!(emissions.len(), 1);
    let e = &emissions[0];
    assert_eq!(e.artifacts.len(), 1, "one AMR graph");
    let amr = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();
    assert!(amr.contains("::snt cats chase mice"), "{amr}");
    // The communicative force lowered to a SemAF dialogue act.
    assert!(amr.contains("::semaf-dialogue-act Inform"), "{amr}");
    assert!(amr.contains("catsChaseMiceFormula"), "{amr}");

    // Program-dependent judgment: SoundUnder, never exact.
    assert!(!is_exact_correspondence(&e.correspondence));
    assert_eq!(e.lossy_kind, PreservationKind::SoundUnder);
    // The AMR gaps are enumerated (no scope, no modality, no vantage).
    let joined = e.unsupported.join("\n");
    assert!(joined.contains("no quantifier scope"), "{joined}");
    assert!(joined.contains("no modality depth"), "{joined}");
    assert!(joined.contains("no vantage"), "{joined}");
}

/// A properly-modelled denotation whose target is an EXAMPLE-namespace individual TYPED
/// `logic:Formula` (the dogfooded case the DenotationKindMatchShape requires) must lower —
/// the target's type, not its IRI namespace, decides lowerability.
#[test]
fn example_namespace_typed_formula_lowers_to_amr() {
    const TYPED: &str = r#"
@prefix lang:  <https://blackcatinformatics.ca/lang/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/lang/> .
ex:sent a lang:ComposedForm ; rdfs:label "cats chase mice" .
ex:formula a logic:Formula .
ex:den a lang:Denotation ;
    lang:denotedForm ex:sent ;
    lang:denotationKind lang:denotesLogicFormula ;
    lang:denotationTarget ex:formula ;
    lang:isIndexical false .
"#;
    let input = LangProjectionInput {
        lang_models: vec![src("typed", TYPED)],
        ..Default::default()
    };
    let e = &SemafBridge.emit(&input).expect("emit")[0];
    assert_eq!(
        e.artifacts.len(),
        1,
        "a typed logic:Formula target lowers to one AMR graph"
    );
    assert_eq!(e.lossy_kind, PreservationKind::SoundUnder);
    assert!(e.artifacts[0].path_suffix.ends_with(".amr"));
}

#[test]
fn unmapped_communicative_force_is_not_defaulted_to_inform() {
    // A declared force with no DiAML mapping must NOT fabricate an `Inform` dialogue act:
    // the AMR header omits the dialogue-act line and the unmapped force is residue.
    const UNMAPPED: &str = r#"
@prefix lang:  <https://blackcatinformatics.ca/lang/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/lang/> .
ex:sent a lang:ComposedForm ; rdfs:label "brrr" .
ex:formula a logic:Formula .
ex:act a lang:CommunicativeAct ; lang:performedOn ex:sent ; lang:communicativeForce lang:exclaimForce .
ex:den a lang:Denotation ;
    lang:denotedForm ex:sent ;
    lang:denotationKind lang:denotesLogicFormula ;
    lang:denotationTarget ex:formula ;
    lang:isIndexical false .
"#;
    let input = LangProjectionInput {
        lang_models: vec![src("unmapped", UNMAPPED)],
        ..Default::default()
    };
    let e = &SemafBridge.emit(&input).expect("emit")[0];
    let amr = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();
    assert!(
        !amr.contains("::semaf-dialogue-act"),
        "an unmapped force must omit the dialogue-act line, not default to Inform: {amr}"
    );
    assert!(
        e.unsupported.iter().any(|r| r.contains("exclaimForce")),
        "the unmapped force must be enumerated as residue: {:?}",
        e.unsupported
    );
}

#[test]
fn non_formula_denotation_is_unsupported_with_reason() {
    let input = LangProjectionInput {
        lang_models: vec![src("q", QUERY_DEN)],
        ..Default::default()
    };
    let e = &SemafBridge.emit(&input).expect("emit")[0];
    assert!(
        e.artifacts.is_empty(),
        "no fabricated AMR for a non-formula denotation"
    );
    assert_eq!(e.lossy_kind, PreservationKind::Unsupported);
    assert_eq!(e.ledger[0].preservation, PreservationKind::Unsupported);
    let joined = e.unsupported.join("\n");
    assert!(joined.contains("denotesQuery"), "{joined}");
    assert!(
        joined.contains("only lang:denotesLogicFormula lowers"),
        "{joined}"
    );
}

#[test]
fn emitter_is_byte_reproducible() {
    let input = LangProjectionInput {
        lang_models: vec![src("f", FORMULA_DEN)],
        ..Default::default()
    };
    let a = SemafBridge.emit(&input).expect("a");
    let b = SemafBridge.emit(&input).expect("b");
    assert_eq!(a[0].artifacts[0].bytes, b[0].artifacts[0].bytes);
}
