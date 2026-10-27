// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_lang_bridge::registry::{LangProjectionInput, registry};

#[test]
fn native_reparse_is_bound_to_the_actual_emitted_bytes() {
    let (input, mut emissions) = authored_emissions();
    let mut observations = Observations::new(&input.grammars);
    observations.record("ebnf", &emissions[0]).unwrap();
    let observed = &observations.grammars["tiny"];
    assert_eq!(
        observed.emissions["ebnf"][0].ebnf_reparsed[0],
        Ok(observed.canonical.clone())
    );
    let bytes = observations.to_bytes();
    let hydrated: Observations = purrdf_lex::json::record::from_slice(&bytes).unwrap();
    assert_eq!(hydrated.to_bytes(), bytes);
    assert_eq!(hydrated.grammars["tiny"].canonical, observed.canonical);
    emissions[0].artifacts[0].bytes.push(b' ');
    assert!(
        observations.record("ebnf", &emissions[0]).is_err(),
        "equivalent text still has a different byte identity"
    );
    emissions[0].grammar_reparse = None;
    assert!(
        observations.record("ebnf", &emissions[0]).is_err(),
        "a selected grammar cannot omit its native result"
    );
}

fn authored_emissions() -> (LangProjectionInput, Vec<LangEmission>) {
    let source = GrammarSource::parse("tiny", b"r ::= 'a' | 'b'\n").unwrap();
    let input = LangProjectionInput {
        grammars: vec![source],
        ..Default::default()
    };
    let target = registry()
        .into_iter()
        .find(|target| target.name() == "ebnf")
        .unwrap();
    let emissions = target.emit(&input).unwrap();
    (input, emissions)
}

#[test]
fn native_grammar_wire_retains_both_outcomes_and_raw_emission_evidence() {
    use gmeow_lang_bridge::LangFailure;
    use purrdf_lex::json::record::from_slice;
    let empty_wire = br#"{"grammars":{"raw":{"source_iri":"","source_rule_count":0,"canonical":{"formalism":"Ebnf","rules":[]},"raw_reparsed":{"Err":{"failure_class":"SilentIngestDrop","construct":"raw"}},"emissions":{}}}}"#;
    let raw: Observations = from_slice(empty_wire).unwrap();
    assert_eq!(raw.to_bytes(), empty_wire);
    for formalism in ["Ebnf", "Abnf", "Gbnf", "Lark"] {
        let wire = String::from_utf8(empty_wire.to_vec())
            .unwrap()
            .replace("Ebnf", formalism);
        assert_eq!(
            from_slice::<Observations>(wire.as_bytes())
                .unwrap()
                .to_bytes(),
            wire.as_bytes()
        );
    }
    let raw_tree = String::from_utf8(empty_wire.to_vec()).unwrap().replace(r#""rules":[]"#, r#""rules":[{"name":"z","body":{"Group":{"Ref":"raw"}}},{"name":"a","body":{"Repeat":[3,1,{"Terminal":"x"}]}}]"#);
    assert_eq!(
        from_slice::<Observations>(raw_tree.as_bytes())
            .unwrap()
            .to_bytes(),
        raw_tree.as_bytes()
    );

    let (input, emissions) = authored_emissions();
    let mut observations = Observations::new(&input.grammars);
    observations.record("ebnf", &emissions[0]).unwrap();
    let observed = observations.grammars.get_mut("tiny").unwrap();
    let refusal = IngestDiagnostic {
        failure_class: LangFailure::SilentIngestDrop,
        construct: "raw\0 refusal 中😀".to_owned(),
    };
    observed.raw_reparsed = Err(refusal.clone());
    let emission = &mut observed.emissions.get_mut("ebnf").unwrap()[0];
    emission.round_trip_holds = false;
    emission.unsupported = vec!["z".to_owned(), "a".to_owned(), "z".to_owned()];
    emission.paths = vec!["opaque".to_owned(), "opaque".to_owned()];
    emission.leg_pair = Some((
        LegPath::Seq(vec![]),
        LegPath::Inverse(Box::new(LegPath::Alt(vec![]))),
    ));
    emission.ebnf_reparsed.push(Err(refusal));
    let bytes = observations.to_bytes();
    let expected = include_bytes!("grammar_observations/fixtures/channel-v1.json")
        .strip_suffix(b"\n")
        .expect("reference text fixture has one storage line feed");
    assert_eq!(
        bytes.as_slice(),
        expected,
        "complete original grammar-channel encoding"
    );
    assert!(!bytes.ends_with(b"\n"));
    let hydrated: Observations = from_slice(&bytes).unwrap();
    assert_eq!(hydrated.to_bytes(), bytes);
    let observed = &hydrated.grammars["tiny"];
    assert_eq!(
        observed.raw_reparsed,
        Err(IngestDiagnostic {
            failure_class: LangFailure::SilentIngestDrop,
            construct: "raw\0 refusal 中😀".to_owned()
        })
    );
    let emission = &observed.emissions["ebnf"][0];
    assert!(!emission.round_trip_holds);
    assert_eq!(emission.unsupported, ["z", "a", "z"]);
    assert_eq!(emission.paths, ["opaque", "opaque"]);
    assert!(emission.ebnf_reparsed[0].is_ok());
    assert!(emission.ebnf_reparsed[1].is_err());
}

#[test]
fn native_grammar_admission_checks_every_map_value_and_named_boundary() {
    use purrdf_lex::json::record::{from_slice, to_vec};
    let (input, emissions) = authored_emissions();
    let mut observations = Observations::new(&input.grammars);
    observations.record("ebnf", &emissions[0]).unwrap();
    let bytes = observations.to_bytes();
    let wire = String::from_utf8(bytes).unwrap();
    let future = wire
        .replacen("{", r#"{"future":[],"#, 1)
        .replacen(r#""source_iri":"#, r#""future":false,"source_iri":"#, 1)
        .replacen(
            r#""correspondence":{"#,
            r#""correspondence":{"future":null,"#,
            1,
        );
    assert!(from_slice::<Observations>(future.as_bytes()).is_ok());
    let observation = to_vec(&observations.grammars["tiny"]);
    let observation = String::from_utf8(observation).unwrap();
    let first = observation.replace(r#""source_rule_count":1"#, r#""source_rule_count":0"#);
    assert_ne!(first, observation);
    let last_wins = format!(r#"{{"grammars":{{"x":{first},"x":{observation}}}}}"#);
    let decoded: Observations = from_slice(last_wins.as_bytes()).unwrap();
    assert_eq!(decoded.grammars.len(), 1);
    assert_eq!(decoded.grammars["x"].source_rule_count, 1);
    let invalid_shadowed = format!(r#"{{"grammars":{{"x":null,"x":{observation}}}}}"#);
    assert!(from_slice::<Observations>(invalid_shadowed.as_bytes()).is_err());
    for bad in [
        wire.replacen(r#""source_rule_count":1"#, r#""source_rule_count":-0"#, 1),
        wire.replacen(
            r#""source_rule_count":1"#,
            r#""source_rule_count":18446744073709551616"#,
            1,
        ),
        wire.replacen(r#""source_rule_count":1"#, r#""source_rule_count":1.0"#, 1),
        wire.replacen(r#""source_rule_count":1"#, r#""source_rule_count":1e0"#, 1),
        wire.replacen(r#""source_iri":"#, r#""source_iri":null,"source_iri":"#, 1),
        wire.replace(r#""round_trip_holds":true"#, r#""round_trip_holds":null"#),
        wire.replace(r#""round_trip_holds":true,"#, ""),
        wire.replacen(
            r#""raw_reparsed":{"Ok":"#,
            r#""raw_reparsed":{"Err":null,"Ok":"#,
            1,
        ),
        "[]".to_owned(),
        r#"{"grammars":null}"#.to_owned(),
    ] {
        assert_ne!(
            bad, wire,
            "fixture mutation must reach its intended boundary"
        );
        assert!(from_slice::<Observations>(bad.as_bytes()).is_err(), "{bad}");
    }
    let emission = &observations.grammars["tiny"].emissions["ebnf"][0];
    assert!(emission.leg_pair.is_some());
    let emission_wire = String::from_utf8(to_vec(emission)).unwrap();
    // Missing and explicit-null optional pairs are both absent; every present row is exact width.
    let pair = format!(
        r#""leg_pair":{},"#,
        purrdf_lex::json::write_compact(&purrdf_lex::json::record::ToJson::to_json(
            &emission.leg_pair
        ))
    );
    for absent in [
        emission_wire.replace(&pair, ""),
        emission_wire.replace(&pair, r#""leg_pair":null,"#),
    ] {
        assert!(
            from_slice::<Emission>(absent.as_bytes())
                .unwrap()
                .leg_pair
                .is_none()
        );
    }
    for bad_pair in [
        "[]",
        r#"[{"Step":"p"}]"#,
        r#"[{"Step":"p"},{"Step":"q"},{"Step":"r"}]"#,
        r#"[{"Step":false},{"Step":"q"}]"#,
    ] {
        let bad = emission_wire.replace(&pair, &format!(r#""leg_pair":{bad_pair},"#));
        assert_ne!(bad, emission_wire);
        assert!(from_slice::<Emission>(bad.as_bytes()).is_err(), "{bad}");
    }
}
