// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored observation values only; no projection, measurements or corpus are produced.
use super::controls::{Controls, Pair, VerbalizerControl};
use super::*;
use purrdf_lex::json::record::{ToJson, from_slice, to_vec};

fn authored_metrics() -> Metrics {
    Metrics {
        bytes_on_disk: u64::MAX,
        tokens_in_context: 2,
        ast_validity_rate: -0.0,
        roundtrip_loss: 1.0,
        compression_ratio: 1.5e-7,
        glyph_density: 1e20,
        dictionary_hit_rate: -2.5,
        gmn_worst_case_tokens: 3,
        gmn_realistic_tokens: 4,
        turtle_best_case_tokens: 5,
        gmn_ascii_bytes: 6,
        gmn_nonascii_bytes: 7,
        turtle_bytes_on_disk: 8,
        jsonld_bytes_on_disk: 9,
        total_sources: 10,
        measured_sources: 11,
        compression_gate_holds: false,
    }
}

fn authored_witness(path: &str, with_legs: bool) -> EmissionWitness {
    let correspondence: Correspondence = from_slice(
        br#"{
        "iri":"urn:test:pack-correspondence", "relation":"Overlaps",
        "morphism_class":"LossyLens", "morphism_kind":"CommitmentShiftingBridge",
        "mnemomorphic":false, "determinacy":null, "get_leg":"raw", "put_leg":null,
        "law_claims":[], "caveats":[], "loss_evidence":[],
        "confidence":null, "evidence_strength":null,
        "axis_evidence":{"sources":["z","a","z"],"scale":null,"probability_model":null},
        "weight":null, "probability":null, "according_to":null, "preservation":null,
        "source_endpoint":null, "target_endpoint":null, "grounding":false,
        "recovery_cases":[]
    }"#,
    )
    .expect("authored correspondence must hydrate without source compilation");
    EmissionWitness {
        path: path.to_owned(),
        is_rdf: false,
        correspondence,
        round_trip: false,
        leg_pair: if with_legs {
            Some((
                LegPath::Seq(vec![
                    LegPath::Step("z".to_owned()),
                    LegPath::Step("a".to_owned()),
                    LegPath::Step("z".to_owned()),
                ]),
                LegPath::Inverse(Box::new(LegPath::Alt(vec![]))),
            ))
        } else {
            None
        },
        source_matches_artifact: true,
    }
}

fn authored_verbalizer(path: &str, with_legs: bool) -> VerbalizerControl {
    VerbalizerControl {
        emission: authored_witness(path, with_legs),
        text: "raw\0 verbalizer 中😀".to_owned(),
        pairs: vec![
            Pair {
                label: "z".to_owned(),
                nl: "raw\0 NL 中😀".to_owned(),
                surface: "opaque".to_owned(),
                inverse_recovers_form: false,
            },
            Pair {
                label: "a".to_owned(),
                nl: "duplicate".to_owned(),
                surface: "duplicate".to_owned(),
                inverse_recovers_form: true,
            },
            Pair {
                label: "z".to_owned(),
                nl: "raw\0 NL 中😀".to_owned(),
                surface: "opaque".to_owned(),
                inverse_recovers_form: false,
            },
        ],
        round_trip: true,
    }
}

fn authored_observations() -> Observations {
    let mut grounding_metrics = authored_metrics();
    grounding_metrics.measured_sources = u64::MAX;
    grounding_metrics.ast_validity_rate = 0.0;
    grounding_metrics.compression_gate_holds = true;
    Observations {
        major: "raw-major".to_owned(),
        paths: vec!["z".to_owned(), "a".to_owned(), "z".to_owned()],
        emissions: vec![
            authored_witness("z", true),
            authored_witness("a", false),
            authored_witness("z", true),
        ],
        metrics: authored_metrics(),
        grounding_metrics,
        grounding_sources: vec!["z".to_owned(), "a".to_owned(), "z".to_owned()],
        expected_codebook_digest: "raw\0 codebook 中😀".to_owned(),
        expected_pack_root: "not-a-normalized-pack-root".to_owned(),
        expected_grammar_leaf: "not-a-normalized-grammar-leaf".to_owned(),
        ecosystem_leaves: vec![
            (
                "z".to_owned(),
                "predicate-z".to_owned(),
                "raw\0 leaf 中😀".to_owned(),
            ),
            (
                "a".to_owned(),
                "predicate-a".to_owned(),
                "opaque".to_owned(),
            ),
            (
                "z".to_owned(),
                "predicate-z".to_owned(),
                "raw\0 leaf 中😀".to_owned(),
            ),
        ],
        controls: Controls {
            bumped_major: "not-a-version".to_owned(),
            bumped_paths: vec!["z".to_owned(), "a".to_owned(), "z".to_owned()],
            verbalizer: authored_verbalizer("raw/verbalizer", true),
            perturbed_text: "raw\0 perturbed 中😀".to_owned(),
            scoped: authored_verbalizer("raw/scoped", false),
            erased_scope_error: "raw\0 scope refusal 中😀".to_owned(),
        },
    }
}

#[test]
fn native_gmn_pack_wire_preserves_complete_original_channel() {
    let observations = authored_observations();
    let bytes = observations.to_bytes();
    let original = include_bytes!("fixtures/channel-v1.json")
        .strip_suffix(b"\n")
        .expect("reference stores one terminal LF outside the original JSON");
    assert_eq!(bytes, original, "complete original pack-channel encoding");
    assert!(!bytes.ends_with(b"\n"));
    let decoded: Observations =
        from_slice(&bytes).expect("complete native pack channel must hydrate");
    assert_eq!(decoded.to_bytes(), bytes);
    assert_eq!(decoded.metrics.bytes_on_disk, u64::MAX);
    assert_eq!(decoded.grounding_metrics.measured_sources, u64::MAX);
    assert_eq!(
        decoded.metrics.ast_validity_rate.to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        decoded.grounding_metrics.ast_validity_rate.to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(decoded.metrics.dictionary_hit_rate, -2.5);
    assert!(!decoded.metrics.compression_gate_holds);
    assert!(decoded.grounding_metrics.compression_gate_holds);
    assert_eq!(decoded.paths, ["z", "a", "z"]);
    assert_eq!(decoded.ecosystem_leaves[0], decoded.ecosystem_leaves[2]);
    assert!(decoded.emissions[0].leg_pair.is_some());
    assert!(decoded.emissions[1].leg_pair.is_none());
    assert!(decoded.controls.verbalizer.round_trip);
    assert!(!decoded.controls.verbalizer.pairs[0].inverse_recovers_form);
    assert_eq!(decoded.controls.verbalizer.pairs[0].nl, "raw\0 NL 中😀");
}

#[test]
fn native_gmn_pack_admission_preserves_raw_evidence_and_refuses_invalid_members() {
    let mut observations = authored_observations();
    let wire = String::from_utf8(observations.to_bytes()).expect("pack JSON is UTF-8");
    let future = wire
        .replacen("{", r#"{"future":[],"#, 1)
        .replace(r#"{"path":"#, r#"{"future":null,"path":"#)
        .replace(r#""metrics":{"#, r#""metrics":{"future":false,"#)
        .replace(r#""controls":{"#, r#""controls":{"future":{},"#)
        .replace(r#""verbalizer":{"#, r#""verbalizer":{"future":0,"#)
        .replace(r#"{"label":"#, r#"{"future":"raw","label":"#);
    assert!(from_slice::<Observations>(future.as_bytes()).is_ok());
    for count in ["-0", "-1", "1.0", "1e0", "null", "18446744073709551616"] {
        let bad = wire.replacen(
            r#""tokens_in_context":2"#,
            &format!(r#""tokens_in_context":{count}"#),
            1,
        );
        assert_ne!(bad, wire, "count mutation must reach its intended field");
        assert!(
            from_slice::<Observations>(bad.as_bytes()).is_err(),
            "{count}"
        );
    }
    for float in ["null", "1e400", "-1e400", r#""NaN""#] {
        let bad = wire.replacen(
            r#""ast_validity_rate":-0.0"#,
            &format!(r#""ast_validity_rate":{float}"#),
            1,
        );
        assert_ne!(bad, wire, "float mutation must reach its intended field");
        assert!(
            from_slice::<Observations>(bad.as_bytes()).is_err(),
            "{float}"
        );
    }
    for nonfinite in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        observations.metrics.ast_validity_rate = nonfinite;
        let bytes = observations.to_bytes();
        let encoded = String::from_utf8(bytes.clone()).expect("nonfinite JSON is UTF-8");
        assert!(encoded.contains(r#""ast_validity_rate":null"#));
        assert!(
            from_slice::<Observations>(&bytes).is_err(),
            "null cannot hydrate a required float"
        );
    }
    for bad in [
        wire.replacen(r#""major":"raw-major","#, "", 1),
        wire.replacen(r#""paths":["z","a","z"]"#, r#""paths":null"#, 1),
        wire.replacen(r#""metrics":{"#, r#""metrics":null,"unused":{"#, 1),
        wire.replacen(r#""is_rdf":false,"#, "", 1),
        wire.replacen(r#""round_trip":false"#, r#""round_trip":null"#, 1),
        wire.replacen(r#""bumped_major":"not-a-version","#, "", 1),
        wire.replacen(r#""text":"#, r#""text":null,"unused":"#, 1),
        wire.replacen(
            r#""inverse_recovers_form":false"#,
            r#""inverse_recovers_form":null"#,
            1,
        ),
        "[]".to_owned(),
    ] {
        assert_ne!(
            bad, wire,
            "record mutation must reach its intended boundary"
        );
        assert!(from_slice::<Observations>(bad.as_bytes()).is_err(), "{bad}");
    }
    // Both duplicate values are individually valid; each record must still refuse ambiguity.
    for bad in [
        wire.replacen(r#""major":"#, r#""major":"duplicate","major":"#, 1),
        wire.replacen(r#""is_rdf":false"#, r#""is_rdf":true,"is_rdf":false"#, 1),
        wire.replacen(
            r#""tokens_in_context":2"#,
            r#""tokens_in_context":0,"tokens_in_context":2"#,
            1,
        ),
        wire.replacen(
            r#""bumped_major":"#,
            r#""bumped_major":"duplicate","bumped_major":"#,
            1,
        ),
        wire.replacen(r#""text":"#, r#""text":"duplicate","text":"#, 1),
        wire.replacen(r#""label":"#, r#""label":"duplicate","label":"#, 1),
    ] {
        let error = from_slice::<Observations>(bad.as_bytes())
            .err()
            .expect("duplicate known field must fail");
        assert!(error.to_string().contains("duplicate field"), "{error}");
    }
    let leaf = to_vec(&observations.ecosystem_leaves);
    let leaf = String::from_utf8(leaf).expect("ecosystem row JSON is UTF-8");
    for rows in [
        "null",
        "[null]",
        "[[]]",
        r#"[["subject","predicate"]]"#,
        r#"[["subject","predicate","digest","extra"]]"#,
    ] {
        let bad = wire.replace(
            &format!(r#""ecosystem_leaves":{leaf}"#),
            &format!(r#""ecosystem_leaves":{rows}"#),
        );
        assert_ne!(bad, wire, "ecosystem mutation must reach the actual field");
        assert!(from_slice::<Observations>(bad.as_bytes()).is_err(), "{bad}");
    }
    let bad = wire.replace(
        &format!(r#""ecosystem_leaves":{leaf}"#),
        r#""ecosystem_leaves":[["subject",false,"digest"]]"#,
    );
    let error = from_slice::<Observations>(bad.as_bytes())
        .err()
        .expect("nonstring row cell must fail");
    assert_eq!(error.pointer(), "/ecosystem_leaves/0/1");
    let witness = &observations.emissions[0];
    let witness_wire = String::from_utf8(to_vec(witness)).expect("emission JSON is UTF-8");
    let pair = format!(
        r#""leg_pair":{},"#,
        purrdf_lex::json::write_compact(&witness.leg_pair.to_json())
    );
    assert!(witness_wire.contains(&pair));
    for optional in [
        witness_wire.replace(&pair, ""),
        witness_wire.replace(&pair, r#""leg_pair":null,"#),
    ] {
        assert!(
            from_slice::<EmissionWitness>(optional.as_bytes())
                .expect("missing/null legs are absent")
                .leg_pair
                .is_none()
        );
    }
    for invalid in [
        "[]",
        r#"[{"Step":"p"}]"#,
        r#"[{"Step":"p"},{"Step":"q"},{"Step":"r"}]"#,
    ] {
        let bad = witness_wire.replace(&pair, &format!(r#""leg_pair":{invalid},"#));
        assert!(
            from_slice::<EmissionWitness>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    let bad = witness_wire.replace(&pair, r#""leg_pair":[{"Step":"p"},{"Step":false}],"#);
    let error = from_slice::<EmissionWitness>(bad.as_bytes())
        .err()
        .expect("malformed second leg must fail");
    assert_eq!(error.pointer(), "/leg_pair/1/Step");
}
