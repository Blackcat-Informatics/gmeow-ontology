// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independently authored gate evidence; no catalog, recorder finish or corpus access.
use super::*;
use purrdf_lex::json::record::{ToJson, from_slice, to_vec};

fn authored_observations() -> Observations {
    let positive = parse_dataset(
        br#"@prefix g: <https://blackcatinformatics.ca/gmeow/> .
            <urn:test:envelope:match> a g:GmnEnvelope; g:gmnCodebookDigest "expected" ."#,
        "text/turtle",
        None,
    )
    .expect("independently authored RDF must parse");
    let negative = parse_dataset(
        br#"@prefix g: <https://blackcatinformatics.ca/gmeow/> .
            <urn:test:envelope:z> a g:GmnEnvelope; g:gmnCodebookDigest "stale-z" .
            <urn:test:envelope:a> a g:GmnEnvelope; g:gmnCodebookDigest "stale-a" ."#,
        "text/turtle",
        None,
    )
    .expect("independently authored RDF must parse");
    let pack = parse_dataset(
        br#"<urn:test:pack> <https://blackcatinformatics.ca/gmeow/gmnPackRoot> "declared-root" ."#,
        "text/turtle",
        None,
    )
    .expect("independently authored RDF must parse");
    let correspondence: Correspondence = from_slice(
        br#"{
        "iri":"urn:test:correspondence", "relation":"RelatedMatch",
        "morphism_class":"BridgeView", "morphism_kind":"CommitmentShiftingBridge",
        "mnemomorphic":false, "determinacy":null, "get_leg":null, "put_leg":"raw",
        "law_claims":[], "caveats":[], "loss_evidence":[],
        "confidence":null, "evidence_strength":null,
        "axis_evidence":{"sources":["z","a","z"],"scale":null,"probability_model":null},
        "weight":null, "probability":null, "according_to":null, "preservation":null,
        "source_endpoint":null, "target_endpoint":null, "grounding":false,
        "recovery_cases":[]
    }"#,
    )
    .expect("authored correspondence must hydrate without source compilation");
    let mut shipped = BTreeMap::new();
    shipped.insert(
        "z/opaque.gmn".to_owned(),
        ShippedWitness {
            artifact_digest: "raw\0 digest 中😀".to_owned(),
            source_iri: "".to_owned(),
            correspondence: correspondence.clone(),
            leg_pair: None,
            round_trip_holds: false,
        },
    );
    shipped.insert(
        "a/opaque.gmn".to_owned(),
        ShippedWitness {
            artifact_digest: "not-a-normalized-digest".to_owned(),
            source_iri: "urn:test:source".to_owned(),
            correspondence,
            leg_pair: Some((
                LegPath::Seq(vec![
                    LegPath::Step("raw\0 path 中😀".to_owned()),
                    LegPath::Step("raw\0 path 中😀".to_owned()),
                ]),
                LegPath::Inverse(Box::new(LegPath::Alt(vec![]))),
            )),
            round_trip_holds: false,
        },
    );
    Observations {
        codebook: check_gmn1_codebook_dataset("expected", "positive", &positive),
        codebook_sources: vec!["z".to_owned(), "a".to_owned(), "z".to_owned()],
        negative: check_gmn1_codebook_dataset("expected", "raw\0 source 中😀", &negative),
        negative_source_digest: "uninterpreted-source-digest".to_owned(),
        pack: compare_gmn1_pack_root("recomputed-root".to_owned(), "raw-major", Some(&pack)),
        pack_artifact_digest: "uninterpreted-pack-digest".to_owned(),
        shipped,
    }
}

#[test]
fn native_gmn_gate_wire_preserves_complete_original_channel() {
    let observations = authored_observations();
    assert_eq!(observations.codebook.checked, 1);
    assert!(observations.codebook.is_clean());
    assert_eq!(observations.negative.checked, 2);
    assert_eq!(observations.negative.mismatches.len(), 2);
    assert!(!observations.negative.is_clean());
    assert!(!observations.pack.is_clean());
    let bytes = observations.to_bytes();
    let original = include_bytes!("fixtures/channel-v1.json")
        .strip_suffix(b"\n")
        .expect("reference stores one terminal LF outside the original JSON");
    assert_eq!(bytes, original, "complete original gate-channel encoding");
    assert!(!bytes.ends_with(b"\n"));
    let decoded: Observations =
        from_slice(&bytes).expect("complete native gate channel must hydrate");
    assert_eq!(decoded.to_bytes(), bytes);
    assert_eq!(decoded.codebook_sources, ["z", "a", "z"]);
    assert!(!decoded.shipped["z/opaque.gmn"].round_trip_holds);
    assert!(decoded.shipped["z/opaque.gmn"].leg_pair.is_none());
    assert!(decoded.shipped["a/opaque.gmn"].leg_pair.is_some());
    assert_eq!(decoded.negative.mismatches[0].source, "raw\0 source 中😀");
}

#[test]
fn native_gmn_gate_admission_preserves_raw_evidence_and_refuses_invalid_members() {
    let mut observations = authored_observations();
    observations.negative.mismatches.reverse();
    observations
        .negative
        .mismatches
        .push(observations.negative.mismatches[0].clone());
    observations.negative.checked = 0;
    observations.pack.pack_present = false;
    // These deliberately contradictory observations are data, not a request to rerun the gate.
    let bytes = observations.to_bytes();
    let decoded: Observations = from_slice(&bytes).expect("raw gate evidence must not be repaired");
    assert_eq!(decoded.to_bytes(), bytes);
    assert_eq!(decoded.negative.checked, 0);
    assert_eq!(
        decoded
            .negative
            .mismatches
            .iter()
            .map(|row| row.envelope.as_str())
            .collect::<Vec<_>>(),
        [
            "urn:test:envelope:z",
            "urn:test:envelope:a",
            "urn:test:envelope:z"
        ]
    );
    assert!(!decoded.pack.pack_present);
    assert_eq!(decoded.pack.declared_root.as_deref(), Some("declared-root"));
    let wire = String::from_utf8(bytes).expect("JSON publication is UTF-8");
    let future = wire
        .replacen("{", r#"{"future":[],"#, 1)
        .replace(r#""codebook":{"#, r#""codebook":{"future":null,"#)
        .replace(r#"{"source":"#, r#"{"future":false,"source":"#)
        .replace(r#""pack":{"#, r#""pack":{"future":{},"#)
        .replace(
            r#"{"artifact_digest":"#,
            r#"{"future":0,"artifact_digest":"#,
        );
    assert!(from_slice::<Observations>(future.as_bytes()).is_ok());
    for count in ["-0", "-1", "1.0", "1e0", "18446744073709551616", "null"] {
        let bad = wire.replacen(r#""checked":1"#, &format!(r#""checked":{count}"#), 1);
        assert_ne!(
            bad, wire,
            "count mutation must reach the actual report field"
        );
        assert!(
            from_slice::<Observations>(bad.as_bytes()).is_err(),
            "{count}"
        );
    }
    for bad in [
        wire.replace(r#""codebook_sources":["z","a","z"],"#, ""),
        wire.replace(
            r#""codebook_sources":["z","a","z"]"#,
            r#""codebook_sources":null"#,
        ),
        wire.replacen(
            r#""codebook":{"mismatches":[],"checked":1}"#,
            r#""codebook":null"#,
            1,
        ),
        wire.replace(r#""pack_present":false"#, r#""pack_present":null"#),
        wire.replace(r#""round_trip_holds":false"#, r#""round_trip_holds":null"#),
        wire.replacen(r#""mismatches":[]"#, r#""mismatches":null"#, 1),
        "[]".to_owned(),
    ] {
        assert_ne!(
            bad, wire,
            "record mutation must reach its intended boundary"
        );
        assert!(from_slice::<Observations>(bad.as_bytes()).is_err(), "{bad}");
    }
    // Both duplicate occurrences are individually valid, so refusal proves named-field uniqueness.
    for bad in [
        wire.replacen(
            r#""codebook_sources":"#,
            r#""codebook_sources":[],"codebook_sources":"#,
            1,
        ),
        wire.replacen(r#""checked":1"#, r#""checked":0,"checked":1"#, 1),
        wire.replacen(r#""envelope":"#, r#""source":"duplicate","envelope":"#, 1),
        wire.replacen(
            r#""pack_present":false"#,
            r#""pack_present":true,"pack_present":false"#,
            1,
        ),
        wire.replacen(
            r#""round_trip_holds":false"#,
            r#""round_trip_holds":true,"round_trip_holds":false"#,
            1,
        ),
    ] {
        let error = from_slice::<Observations>(bad.as_bytes())
            .err()
            .expect("duplicate known field must fail");
        assert!(error.to_string().contains("duplicate field"), "{error}");
    }
    for optional in [
        wire.replace(r#""declared_root":"declared-root","#, ""),
        wire.replace(
            r#""declared_root":"declared-root""#,
            r#""declared_root":null"#,
        ),
    ] {
        let decoded: Observations =
            from_slice(optional.as_bytes()).expect("missing/null root is absent");
        assert!(decoded.pack.declared_root.is_none());
    }
    let witness = &observations.shipped["a/opaque.gmn"];
    let witness_wire = String::from_utf8(to_vec(witness)).expect("witness JSON is UTF-8");
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
            from_slice::<ShippedWitness>(optional.as_bytes())
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
            from_slice::<ShippedWitness>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    let bad = witness_wire.replace(&pair, r#""leg_pair":[{"Step":false},{"Step":"q"}],"#);
    let error = from_slice::<ShippedWitness>(bad.as_bytes())
        .err()
        .expect("malformed first leg must fail");
    assert_eq!(error.pointer(), "/leg_pair/0/Step");
    let second_witness = witness_wire.replace("not-a-normalized-digest", "second");
    let last_wins = format!(
        r#"{{"codebook":{{"mismatches":[],"checked":0}},"codebook_sources":[],"negative":{{"mismatches":[],"checked":0}},"negative_source_digest":"raw","pack":{{"recomputed_root":"raw","pack_present":false,"pack_rel":"raw"}},"pack_artifact_digest":"raw","shipped":{{"x":{witness_wire},"x":{second_witness}}}}}"#
    );
    let decoded: Observations =
        from_slice(last_wins.as_bytes()).expect("valid duplicate map values retain the last");
    assert_eq!(decoded.shipped.len(), 1);
    assert_eq!(decoded.shipped["x"].artifact_digest, "second");
    let shadowed = last_wins.replacen(&format!(r#""x":{witness_wire}"#), r#""x":null"#, 1);
    assert!(from_slice::<Observations>(shadowed.as_bytes()).is_err());
}
