// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::loss::canonical_codec_name;

// ── Fixtures ──────────────────────────────────────────────────────────────

/// N-Triples fixture with an RDF-1.2 quoted triple (reifier binding + annotation).
///
/// N-Triples is used rather than Turtle because the RDF-1.2 `<<( )>>` annotation
/// subject syntax has wider oxigraph parser support in N-Triples than in Turtle.
const STAR_FIXTURE_TTL: &str = concat!(
    "<http://example.org/s> <http://example.org/p> <http://example.org/o> .\n",
    "<http://example.org/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ",
    "<<( <http://example.org/s> <http://example.org/p> <http://example.org/o> )>> .\n",
    "<http://example.org/r> <http://example.org/certainty> \"0.9\"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n",
);
const STAR_FIXTURE_FORMAT: Codec = Codec::NTriples;

/// TriG fixture with a named graph.
const NAMED_GRAPH_FIXTURE_TRIG: &str = r#"
@prefix ex: <http://example.org/> .
ex:g1 { ex:s ex:p ex:o . }
ex:s ex:p ex:o .
"#;

/// Turtle fixture with OWL axioms (valid input for logic projections).
const LOGIC_FIXTURE_TTL: &str = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex: <http://example.org/> .
ex:MyClass a owl:Class .
ex:MyProp a owl:ObjectProperty ; rdfs:domain ex:MyClass .
"#;

// ── Round-trip tests ──────────────────────────────────────────────────────

#[test]
fn roundtrip_star_through_turtle() {
    let out = transcode(
        STAR_FIXTURE_TTL.as_bytes(),
        STAR_FIXTURE_FORMAT,
        Codec::Turtle,
        None,
    )
    .expect("ntriples → turtle transcode");
    assert!(!out.bytes.is_empty());
    // ntriples→turtle: both are star-capable, no graph loss — should be lossless.
    assert!(
        out.realized.is_empty(),
        "ntriples→turtle should be lossless"
    );
}

#[test]
fn roundtrip_star_through_ntriples() {
    let out = transcode(
        STAR_FIXTURE_TTL.as_bytes(),
        STAR_FIXTURE_FORMAT,
        Codec::NTriples,
        None,
    )
    .expect("ntriples transcode");
    assert!(!out.bytes.is_empty());
    // N-Triples is star-capable, so no star loss.
    assert!(
        !out.realized
            .iter()
            .any(|r| r.code == "rdf12-star-unrepresentable"),
        "N-Triples is star-capable"
    );
}

#[test]
fn roundtrip_star_through_gts() {
    let out = transcode(
        STAR_FIXTURE_TTL.as_bytes(),
        STAR_FIXTURE_FORMAT,
        Codec::Gts,
        None,
    )
    .expect("gts transcode");
    assert!(!out.bytes.is_empty());
    assert!(out.realized.is_empty(), "gts is lossless for turtle input");
}

/// Transformed consumer output is authored GMEOW GTS: the bytes `gmeow convert
/// --to gts` writes carry the one mandated transform on every payload frame.
/// This is the exact function the shipped CLI's `convert` verb calls, so the
/// audit here binds the shipped surface.
#[test]
fn gts_target_output_uses_the_mandated_frame_profile() {
    let out = transcode(
        STAR_FIXTURE_TTL.as_bytes(),
        STAR_FIXTURE_FORMAT,
        Codec::Gts,
        None,
    )
    .expect("gts transcode");
    gmeow_gts_profile::validate_mandated_frames(&out.bytes)
        .expect("`convert --to gts` output uses the mandated zstd-rsyncable-L12 profile");
}

/// The same audit over a NAMED-GRAPH source: the graph slot rides the snapshot
/// frame, so the transform rule must hold for a multi-graph carrier too.
#[test]
fn named_graph_gts_output_uses_the_mandated_frame_profile() {
    let out = transcode(
        NAMED_GRAPH_FIXTURE_TRIG.as_bytes(),
        Codec::TriG,
        Codec::Gts,
        None,
    )
    .expect("trig → gts transcode");
    gmeow_gts_profile::validate_mandated_frames(&out.bytes)
        .expect("named-graph `--to gts` output uses the mandated frame profile");
}

// ── Loss recording tests ──────────────────────────────────────────────────

#[test]
fn star_to_jsonld_has_star_drop_code() {
    let out = transcode(
        STAR_FIXTURE_TTL.as_bytes(),
        STAR_FIXTURE_FORMAT,
        Codec::JsonLd,
        None,
    )
    .expect("ntriples → jsonld");
    // The fixture has reifier bindings + annotations, so star rows are dropped.
    let star_loss = out
        .realized
        .iter()
        .find(|r| r.code == "rdf12-star-jsonld-rejected");
    assert!(
        star_loss.is_some(),
        "expected rdf12-star-jsonld-rejected in realized losses"
    );
    let loss = star_loss.unwrap();
    assert!(
        loss.count > 0,
        "star drop count must be > 0 when reifiers are present"
    );
}

#[test]
fn star_to_rdfxml_has_star_drop_code() {
    let out = transcode(
        STAR_FIXTURE_TTL.as_bytes(),
        STAR_FIXTURE_FORMAT,
        Codec::RdfXml,
        None,
    )
    .expect("ntriples → rdfxml");
    let star_loss = out
        .realized
        .iter()
        .find(|r| r.code == "rdf12-star-unrepresentable");
    assert!(
        star_loss.is_some(),
        "expected rdf12-star-unrepresentable in realized losses"
    );
    let loss = star_loss.unwrap();
    assert!(
        loss.count > 0,
        "star drop count must be > 0 when reifiers are present"
    );
}

#[test]
fn named_graph_trig_to_turtle_has_named_graph_drop() {
    let out = transcode(
        NAMED_GRAPH_FIXTURE_TRIG.as_bytes(),
        Codec::TriG,
        Codec::Turtle,
        None,
    )
    .expect("trig → turtle");
    let ng_loss = out
        .realized
        .iter()
        .find(|r| r.code == "named-graph-dropped");
    assert!(
        ng_loss.is_some(),
        "expected named-graph-dropped in realized losses"
    );
    let loss = ng_loss.unwrap();
    assert_eq!(loss.count, 1, "fixture has exactly one named graph (ex:g1)");
}

// ── Projection target tests ───────────────────────────────────────────────

#[test]
fn logic_to_datalog_succeeds_nonempty() {
    let out = transcode(
        LOGIC_FIXTURE_TTL.as_bytes(),
        Codec::Turtle,
        Codec::Datalog,
        None,
    )
    .expect("turtle → datalog");
    assert!(!out.bytes.is_empty(), "datalog output must be non-empty");
}

#[test]
fn logic_to_owl_dl_succeeds_nonempty() {
    let out = transcode(
        LOGIC_FIXTURE_TTL.as_bytes(),
        Codec::Turtle,
        Codec::OwlDl,
        None,
    )
    .expect("turtle → owl-dl");
    assert!(!out.bytes.is_empty(), "owl-dl output must be non-empty");
}

#[test]
fn logic_to_n3_succeeds_nonempty() {
    let out = transcode(LOGIC_FIXTURE_TTL.as_bytes(), Codec::Turtle, Codec::N3, None)
        .expect("turtle → n3");
    assert!(!out.bytes.is_empty(), "n3 output must be non-empty");
}

#[test]
fn logic_to_canonical_rdf12_succeeds_nonempty() {
    let out = transcode(
        LOGIC_FIXTURE_TTL.as_bytes(),
        Codec::Turtle,
        Codec::CanonicalRdf12,
        None,
    )
    .expect("turtle → canonical-rdf12");
    assert!(
        !out.bytes.is_empty(),
        "canonical-rdf12 output must be non-empty"
    );
}

// ── Error path tests ──────────────────────────────────────────────────────

#[test]
fn read_to_dataset_on_owl_dl_is_non_invertible_source_error() {
    let err = read_to_dataset(b"anything", Codec::OwlDl).expect_err("expected NonInvertibleSource");
    assert!(err.is::<crate::NonInvertibleSource>(), "got {err:?}");
}

#[test]
fn read_to_dataset_on_jsonld_star_is_undecodable_error() {
    let err =
        read_to_dataset(b"anything", Codec::JsonLdStar).expect_err("expected UndecodableInput");
    assert!(err.is::<crate::UndecodableInput>(), "got {err:?}");
}

#[test]
fn read_to_dataset_on_yaml_ld_star_is_undecodable_error() {
    let err =
        read_to_dataset(b"anything", Codec::YamlLdStar).expect_err("expected UndecodableInput");
    assert!(err.is::<crate::UndecodableInput>(), "got {err:?}");
}

// ── Codec::from_cli_str round-trip ────────────────────────────────────────

#[test]
fn from_cli_str_round_trips_all_names() {
    for codec in Codec::all() {
        let name = codec.name();
        let parsed = Codec::from_cli_str(name)
            .unwrap_or_else(|_| panic!("from_cli_str failed for `{name}`"));
        assert_eq!(
            parsed.name(),
            name,
            "round-trip failed: from_cli_str({name:?}).name() != {name:?}"
        );
    }
}

#[test]
fn from_cli_str_fails_on_bogus() {
    let err = Codec::from_cli_str("bogus").expect_err("expected UnknownCodec");
    assert!(err.is::<crate::UnknownCodec>());
}

// ── canonical_codec_name compatibility ────────────────────────────────────

#[test]
fn every_codec_name_accepted_by_canonical_codec_name() {
    for codec in Codec::all() {
        let name = codec.name();
        // canonical_codec_name panics on unknown names; this test passes if
        // no panic occurs for any codec in the enum.
        let canonical = canonical_codec_name(name);
        assert_eq!(canonical, name, "canonical name mismatch for `{name}`");
    }
}

// ── transcode matrix drift gate ───────────────────────────────────────────

#[test]
fn transcode_matrix_is_deterministic() {
    assert_eq!(transcode_matrix_json(), transcode_matrix_json());
}

#[test]
fn transcode_loss_matrix_contains_only_supported_codecs() {
    let supported: HashSet<&str> = Codec::all().iter().map(|codec| codec.name()).collect();
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&transcode_loss_matrix_json()).expect("valid matrix JSON");
    assert!(rows.iter().all(|row| {
        row.get("from")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|name| supported.contains(name))
            && row
                .get("to")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|name| supported.contains(name))
    }));
}

/// Drift gate: the committed artifact must byte-equal the freshly rendered
/// matrix. Regenerate `generated/transcode-matrix.json` from
/// `transcode_matrix_json()` when the codec set or loss contract changes.
#[test]
fn transcode_matrix_has_not_drifted() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("generated")
        .join("transcode-matrix.json");
    let committed =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert_eq!(
        committed,
        transcode_matrix_json(),
        "generated/transcode-matrix.json is stale; regenerate from transcode_matrix_json()"
    );
}
