// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only assertions over authored grammar preparation and actual projection emissions.

use gmeow_lang_bridge::{Formalism, exact_round_trip_holds, is_exact_correspondence};
use gmeow_logic_compile::ir::PreservationKind;

use super::grammar_observations::GrammarObservation;

fn grammar(name: &str) -> &'static GrammarObservation {
    super::contract_fixtures::grammar(name)
}

fn assert_isomorphism(name: &str) -> &'static GrammarObservation {
    let observed = grammar(name);
    let raw = observed
        .raw_reparsed
        .as_ref()
        .expect("raw-source serialization reparses");
    assert_eq!(raw, &observed.canonical, "{name}: raw-source roundtrip");
    let path = format!("ebnf/{name}.ebnf");
    let emitted = observed
        .emissions
        .get("ebnf")
        .expect("selected EBNF target")
        .iter()
        .find(|emission| emission.paths.contains(&path))
        .expect("named grammar artifact");
    assert_eq!(emitted.ebnf_reparsed.len(), 1, "one EBNF artifact");
    assert_eq!(
        emitted.ebnf_reparsed[0]
            .as_ref()
            .expect("emitted EBNF reparses"),
        &observed.canonical,
        "{name}: canonical-source roundtrip"
    );
    observed
}

pub(crate) fn gate3_turtle_grammar_round_trips_isomorphically() {
    let observed = assert_isomorphism("turtle");
    assert!(
        observed.source_rule_count >= 40,
        "full Turtle production set"
    );
    assert_eq!(observed.canonical.formalism, Formalism::Ebnf);
}

pub(crate) fn gate3_gts_grammar_round_trips_isomorphically() {
    let observed = assert_isomorphism("gts");
    assert!(observed.source_rule_count >= 10, "GTS surface productions");
    assert!(
        observed
            .canonical
            .rules
            .iter()
            .any(|rule| rule.name == "tripleTerm")
    );
}

pub(crate) fn ebnf_target_emits_exact_round_tripping_grammar() {
    let observed = assert_isomorphism("turtle");
    let emissions = &observed.emissions["ebnf"];
    let matching: Vec<_> = emissions
        .iter()
        .filter(|emission| emission.paths.iter().any(|path| path == "ebnf/turtle.ebnf"))
        .collect();
    assert_eq!(matching.len(), 1, "exactly one named Turtle emission");
    let emission = matching[0];
    assert_eq!(emission.paths.len(), 1);
    assert!(is_exact_correspondence(&emission.correspondence));
    assert!(emission.round_trip_holds);
    let (get, put) = emission.leg_pair.as_ref().expect("grammar leg pair");
    assert!(exact_round_trip_holds(get, put));
}

pub(crate) fn abnf_target_is_honest_lossy_for_char_class_grammars() {
    let emissions = grammar("turtle")
        .emissions
        .get("abnf")
        .expect("selected ABNF target");
    assert!(!emissions.is_empty());
    for emission in emissions {
        assert!(!is_exact_correspondence(&emission.correspondence));
        assert_eq!(emission.lossy_kind, PreservationKind::SoundUnder);
        assert!(emission.paths.is_empty(), "no fabricated partial ABNF");
        assert!(
            emission
                .unsupported
                .iter()
                .any(|value| value.contains("character class"))
        );
        assert!(!emission.round_trip_holds);
    }
}
