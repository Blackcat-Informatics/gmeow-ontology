// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `convert` tool's contract: ONE transcode implementation, and RDF-1.2 all the way
//! through it.
//!
//! `convert` is the fourth verb of parse / reason / validate / **serialize**, and it is the
//! one verb where a second implementation would be invisible: two serializers can agree on
//! the easy cases for a long time and disagree only on the quoted triple that mattered.
//! This file makes both halves of that falsifiable.
//!
//! # Group A — one implementation
//!
//! Structural assertions over the committed sources (the
//! `crates/gmeow-dev-cli/tests/docs_distribution_contract.rs` idiom: read the real file
//! under `repo_root()` and assert what it says), plus a BEHAVIOURAL assertion over a real
//! input: the bytes and the loss ledger the MCP tool returns must equal the bytes and the
//! ledger `gmeow-transcode` produces for the same call — which is the very function
//! `gmeow convert` reaches through `gmeow_pipeline::transcode`'s re-export. Structure says
//! they call the same code; behaviour says the code they call gives the same answer.
//!
//! # Group B — RDF-1.2 is load-bearing
//!
//! Given a Turtle document carrying a quoted triple, every syntax target must fall into
//! exactly one of two cases, and there is no third:
//!
//! * A **star-capable** target (`turtle`, `ntriples`, `nquads`, `trig`, `owl-rdf12`, and
//!   the binary `gts`) must round-trip to an ISOMORPHIC dataset with the quoted triple
//!   still a genuine RDF-1.2 triple term, and report NO loss.
//! * An **RDF-1.1-shaped** target — `rdfxml` and `jsonld`, whose syntaxes have no
//!   triple-term construct at all — must REALIZE the drop in the loss ledger, under the
//!   code the static contract declares, with a run-time count.
//!
//! Either way, an RDF-1.1-shaped downgrade that happens *silently* is a HARD FAILURE here,
//! not a warning. That is the failure mode the tests exist to catch: not that a syntax
//! without triple terms cannot carry one, but that the hub might drop one without saying
//! so.
//!
//! Each target's output is re-parsed by calling purrdf's decoder DIRECTLY, never by asking
//! the hub to decode its own output, so the test cannot pass by the hub being
//! self-consistently wrong.

use std::path::{Path, PathBuf};

use gmeow_mcp::McpServer;
use gmeow_transcode::{Codec, realized_loss_json, transcode};
use purrdf::{NativeRdfFormat, RdfQuad, RdfTerm};
use serde_json::{Value, json};

// ── repo-anchored file readers ────────────────────────────────────────────────────

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/mcp has a workspace root two levels up")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

fn snapshot() -> Vec<u8> {
    gmeow_bundle_import::load_authenticated_source_bytes(&repo_root())
        .expect("authenticated snapshot; tests never produce it")
}

fn server() -> McpServer {
    McpServer::from_snapshot(&snapshot()).expect("consumer server constructs")
}

/// Drive the real `tools/call convert` envelope and return the parsed result object,
/// failing loudly (with the tool's own message) when the call errors.
fn call_convert(server: &McpServer, args: &Value) -> Value {
    let envelope = server.call_tool_result("convert", args);
    let text = envelope["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("convert returned no text content: {envelope}"));
    let parsed: Value =
        serde_json::from_str(text).unwrap_or_else(|e| panic!("convert result is not JSON: {e}"));
    assert_eq!(
        envelope.get("isError"),
        Some(&json!(false)),
        "convert hard-failed: {parsed}"
    );
    parsed
}

// ── the fixture ───────────────────────────────────────────────────────────────────

/// A Turtle document carrying an RDF-1.2 **quoted triple**: a base assertion, a reifier
/// bound to that assertion with `rdf:reifies`, and an annotation hung off the reifier.
/// Blank-node-free by construction, so dataset isomorphism is decidable by set equality of
/// the flattened quad streams — no blank-node labelling to canonicalize away.
const STAR_TURTLE: &str = concat!(
    "@prefix ex: <http://example.org/> .\n",
    "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n",
    "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n",
    "ex:s ex:p ex:o .\n",
    "ex:r rdf:reifies <<( ex:s ex:p ex:o )>> .\n",
    "ex:r ex:certainty \"0.9\"^^xsd:decimal .\n",
);

/// Every text target that DECLARES RDF-1.2 triple-term capability
/// (`purrdf::loss::supports_stars`) and can also be decoded back, so a round-trip is
/// checkable. `owl-rdf12` is included deliberately: it is an intent label over the same
/// RDF-1.2 Turtle codec, and an intent label that quietly stopped carrying triple terms
/// would be the worst kind of downgrade. (`gts`, the binary star-capable target, is
/// covered by [`a_binary_target_is_carried_as_declared_base64`]; `jsonld-star` and
/// `yaml-ld-star` are write-only through this hub and so cannot be round-tripped here.)
const STAR_CAPABLE_TARGETS: &[Codec] = &[
    Codec::Turtle,
    Codec::NTriples,
    Codec::NQuads,
    Codec::TriG,
    Codec::OwlRdf12,
];

/// The two targets whose SYNTAX has no triple-term construct at all, paired with the loss
/// code the static contract declares for dropping one. These are the RDF-1.1-shaped
/// targets: the requirement on them is not that the quoted triple survives (it cannot),
/// but that its loss is REALIZED and named — see
/// [`an_rdf11_shaped_target_declares_the_star_loss_it_takes`].
const RDF11_SHAPED_TARGETS: &[(Codec, &str)] = &[
    (Codec::RdfXml, "rdf12-star-unrepresentable"),
    (Codec::JsonLd, "rdf12-star-jsonld-rejected"),
];

/// Re-parse `bytes` written in `codec`, by calling purrdf's decoder DIRECTLY — never by
/// routing back through the transcode hub, which would let a self-consistent decoder/
/// encoder pair agree with each other while both being wrong.
fn reparse(codec: Codec, bytes: &[u8]) -> Vec<RdfQuad> {
    let dataset = match codec {
        Codec::JsonLd => purrdf::native_codecs::jsonld::parse_jsonld(bytes)
            .unwrap_or_else(|e| panic!("re-parse jsonld output: {e}")),
        // `owl-rdf12` is an intent label over the SAME RDF-1.2 Turtle codec, so it reads
        // back through the same decoder — which is precisely the claim under test.
        Codec::Turtle | Codec::OwlRdf12 => decode_native(bytes, NativeRdfFormat::Turtle),
        Codec::NTriples => decode_native(bytes, NativeRdfFormat::NTriples),
        Codec::NQuads => decode_native(bytes, NativeRdfFormat::NQuads),
        Codec::TriG => decode_native(bytes, NativeRdfFormat::TriG),
        Codec::RdfXml => decode_native(bytes, NativeRdfFormat::RdfXml),
        other => panic!("no direct decoder wired for codec {}", other.name()),
    };
    let mut quads = purrdf::flat_rdf_quads_from_dataset(&dataset);
    quads.sort_by_key(|q| format!("{q:?}"));
    quads
}

fn decode_native(bytes: &[u8], format: NativeRdfFormat) -> std::sync::Arc<purrdf::RdfDataset> {
    purrdf::dataset_from_bytes(bytes, format)
        .unwrap_or_else(|e| panic!("re-parse {format:?} output: {e}"))
}

/// Whether `quads` still carries the fixture's quoted triple as a genuine RDF-1.2 triple
/// TERM (`RdfTerm::Triple`), not as some RDF-1.1-shaped stand-in.
fn carries_the_quoted_triple(quads: &[RdfQuad]) -> bool {
    quads.iter().any(|quad| {
        let RdfTerm::Triple(inner) = &quad.object else {
            return false;
        };
        inner.subject == RdfTerm::Iri("http://example.org/s".to_owned())
            && inner.predicate == "http://example.org/p"
            && inner.object == RdfTerm::Iri("http://example.org/o".to_owned())
    })
}

// ── Group A: one transcode implementation ─────────────────────────────────────────

/// The consumer CLI's `gmeow convert` reaches the transcode hub through
/// `gmeow_pipeline::transcode`, which is a bare re-export of the `gmeow-transcode` crate —
/// so "the CLI's transcode" and "`gmeow-transcode`" name one thing, not two.
#[test]
fn the_cli_convert_verb_routes_through_the_gmeow_transcode_crate() {
    let commands = read("crates/gmeow-cli/src/commands.rs");
    assert!(
        commands.contains("use gmeow_pipeline::transcode::{Codec, realized_loss_json, transcode"),
        "gmeow-cli's `convert` must import the transcode triple from the hub, not \
         re-implement one"
    );

    let pipeline_lib = read("crates/pipeline/src/lib.rs");
    assert!(
        pipeline_lib.contains("pub use gmeow_transcode as transcode;"),
        "`gmeow_pipeline::transcode` must be a bare re-export of the gmeow-transcode \
         crate — if it ever becomes a module of its own, the CLI and the MCP tool stop \
         sharing an implementation"
    );
}

/// The MCP `convert` tool imports the SAME three items from the SAME leaf crate, and never
/// from `gmeow-pipeline` (which it may not depend on at all).
#[test]
fn the_mcp_convert_tool_routes_through_the_same_gmeow_transcode_crate() {
    let mcp = read("crates/mcp/src/lib.rs");
    assert!(
        mcp.contains(
            "use gmeow_transcode::{Codec, realized_loss_json, transcode as run_transcode};"
        ),
        "`tool_convert` must import the transcode triple from gmeow-transcode directly"
    );
    assert!(
        !mcp.contains("gmeow_pipeline"),
        "gmeow-mcp is a LEAF with respect to the build executor: it must never name \
         gmeow_pipeline, not even to reach the transcode hub"
    );
    let manifest = read("crates/mcp/Cargo.toml");
    // `optional = true` is the SEGMENT selector, not a weakening of the edge: `convert` is
    // a core-segment tool and `gmeow-transcode` is one of the two dependencies nothing
    // else in this crate reaches, so the demand-loaded reasoning image links no transcode
    // hub at all. What this contract owns is unchanged — when the tool is compiled, its
    // hub is a DIRECT dependency of `gmeow-mcp` and never reached through
    // `gmeow-pipeline`.
    assert!(
        manifest.contains("gmeow-transcode = { path = \"../transcode\", optional = true }"),
        "the transcode edge must be a declared direct dependency of gmeow-mcp"
    );
    assert!(
        manifest.contains("core = [")
            && manifest
                .lines()
                .find(|l| l.starts_with("core = ["))
                .is_some_and(|l| l.contains("\"dep:gmeow-transcode\"")),
        "the optional transcode edge must be selected by the `core` segment feature, so a \
         default build still links it and `convert` is never silently unavailable"
    );
}

/// Structure proves they call the same code; THIS proves the code they call answers the
/// same. The MCP tool's output bytes and realized-loss ledger must equal what
/// `gmeow-transcode` produces for the identical call — the exact functions
/// `gmeow_pipeline::transcode` re-exports to `gmeow convert`.
#[test]
fn the_mcp_tool_and_the_cli_transcode_path_agree_byte_for_byte() {
    let server = server();
    // A lossy edge on purpose: trig → turtle drops the named graph, so the ledger is
    // non-empty and the two paths have something to disagree about.
    let source =
        "@prefix ex: <http://example.org/> .\nex:g { ex:s ex:p ex:o . }\nex:s ex:p ex:o .\n";

    let direct = transcode(source.as_bytes(), Codec::TriG, Codec::Turtle, None)
        .expect("direct transcode through the hub");
    let direct_text = String::from_utf8(direct.bytes).expect("turtle output is UTF-8");
    let direct_loss: Value =
        serde_json::from_str(&realized_loss_json(&direct.realized)).expect("ledger is JSON");
    assert!(
        !direct.realized.is_empty(),
        "trig → turtle must realize named-graph loss, or this test proves nothing"
    );

    let tool = call_convert(
        &server,
        &json!({"data": source, "from": "trig", "to": "turtle"}),
    );
    assert_eq!(
        tool["output"].as_str().expect("output text"),
        direct_text,
        "the MCP tool's bytes must be the hub's bytes"
    );
    assert_eq!(
        tool["loss"], direct_loss,
        "the MCP tool's realized-loss ledger must be the hub's ledger"
    );
    assert_eq!(tool["encoding"], json!("utf8"));
    assert_eq!(tool["bytes"], json!(direct_text.len()));
}

/// An unknown codec name is a HARD error naming the codec, on either side of the pair —
/// never a silent fallback to a default serializer.
#[test]
fn an_unknown_codec_is_a_hard_error() {
    let server = server();
    for (from, to) in [("turtle", "no-such-codec"), ("no-such-codec", "turtle")] {
        let envelope =
            server.call_tool_result("convert", &json!({"data": "", "from": from, "to": to}));
        assert_eq!(
            envelope["isError"],
            json!(true),
            "convert {from} → {to} must refuse: {envelope}"
        );
        assert!(
            envelope["content"][0]["text"]
                .as_str()
                .expect("text")
                .contains("no-such-codec"),
            "the refusal must name the codec: {envelope}"
        );
    }
}

/// A binary target (`gts`) is carried as base64, and the declared `encoding` says so — the
/// bytes are never lossily forced through UTF-8 and never silently omitted.
#[test]
fn a_binary_target_is_carried_as_declared_base64() {
    let server = server();
    let tool = call_convert(
        &server,
        &json!({"data": STAR_TURTLE, "from": "turtle", "to": "gts"}),
    );
    assert_eq!(
        tool["encoding"],
        json!("base64"),
        "gts bytes are not UTF-8 and must be declared base64: {tool}"
    );
    let direct = transcode(STAR_TURTLE.as_bytes(), Codec::Turtle, Codec::Gts, None)
        .expect("direct gts transcode");
    assert_eq!(
        tool["bytes"],
        json!(direct.bytes.len()),
        "the declared byte length must be the real one"
    );
    // Round-trip the declared encoding: base64-decoding the tool's `output` must recover
    // exactly the hub's bytes, or the encoding claim is a lie.
    let text = tool["output"].as_str().expect("output text");
    let decoded = call_convert(
        &server,
        &json!({"data": text, "encoding": "base64", "from": "gts", "to": "nquads"}),
    );
    let from_direct = transcode(&direct.bytes, Codec::Gts, Codec::NQuads, None)
        .expect("direct gts → nquads transcode");
    let recovered = decoded["output"].as_str().expect("output text");
    assert_eq!(
        recovered,
        String::from_utf8(from_direct.bytes).expect("nquads is UTF-8"),
        "base64 in must recover the same bytes the hub was handed directly"
    );
    // `gts` is the star-capable BINARY target: the quoted triple must survive the whole
    // base64 round-trip, not merely the byte count.
    assert!(
        carries_the_quoted_triple(&reparse(Codec::NQuads, recovered.as_bytes())),
        "the gts round-trip dropped the RDF-1.2 quoted triple:\n{recovered}"
    );
}

// ── Group B: RDF-1.2 survives every star-capable target ───────────────────────────

/// The load-bearing one. A Turtle document with a quoted triple, converted to each
/// star-capable target, must re-parse to a dataset ISOMORPHIC to the source with the
/// quoted triple still a genuine RDF-1.2 triple term.
///
/// An RDF-1.1-shaped downgrade is a hard failure: this asserts the reifier binding is
/// still there and still a `Triple` term, so a target that "succeeded" by flattening the
/// quoted triple into plain triples fails here rather than passing quietly. The realized
/// ledger must also be EMPTY — a star-capable, single-graph edge that reports loss is
/// wrong in the other direction.
#[test]
fn a_quoted_triple_survives_every_star_capable_target_isomorphically() {
    let server = server();
    let expected = reparse(Codec::Turtle, STAR_TURTLE.as_bytes());
    assert!(
        carries_the_quoted_triple(&expected),
        "the fixture itself must parse to a dataset carrying the quoted triple"
    );

    for &target in STAR_CAPABLE_TARGETS {
        assert!(
            purrdf::loss::supports_stars(target.name()),
            "{} is in the star-capable set but the static contract says it is not",
            target.name()
        );
        let tool = call_convert(
            &server,
            &json!({"data": STAR_TURTLE, "from": "turtle", "to": target.name()}),
        );
        assert_eq!(
            tool["encoding"],
            json!("utf8"),
            "{} is a text codec: {tool}",
            target.name()
        );
        let output = tool["output"].as_str().expect("output text");

        let actual = reparse(target, output.as_bytes());
        assert!(
            carries_the_quoted_triple(&actual),
            "converting to {} dropped the RDF-1.2 quoted triple — an RDF-1.1-shaped \
             downgrade is a HARD FAILURE, not a warning.\n  output:\n{output}",
            target.name()
        );
        assert_eq!(
            actual,
            expected,
            "converting to {} did not round-trip to an isomorphic dataset.\n  output:\n{output}",
            target.name()
        );
        assert_eq!(
            tool["loss"],
            json!([]),
            "{} is star-capable and single-graph here: the realized ledger must be empty",
            target.name()
        );
    }
}

/// RDF/XML and JSON-LD 1.1 have NO triple-term construct — the RDF-1.2 quoted triple
/// genuinely cannot cross those edges. That is a declared capability boundary, not a bug,
/// and the thing this asserts is the part that IS a bug if it regresses: the drop must be
/// REALIZED in the ledger, under the specific code the static contract declares, with a
/// non-zero count. A silent RDF-1.1-shaped downgrade — output that lost the quoted triple
/// while reporting no loss — fails here.
///
/// This is the honest complement of
/// [`a_quoted_triple_survives_every_star_capable_target_isomorphically`]: taken together
/// they say every target either carries the triple term or says, in the ledger, exactly
/// what it dropped. There is no third case.
#[test]
fn an_rdf11_shaped_target_declares_the_star_loss_it_takes() {
    let server = server();
    for &(target, expected_code) in RDF11_SHAPED_TARGETS {
        assert!(
            !purrdf::loss::supports_stars(target.name()),
            "{} is in the RDF-1.1-shaped set but the static contract says it carries stars \
             — if that changed, move it into STAR_CAPABLE_TARGETS and require the \
             round-trip",
            target.name()
        );
        let tool = call_convert(
            &server,
            &json!({"data": STAR_TURTLE, "from": "turtle", "to": target.name()}),
        );
        let losses = tool["loss"]
            .as_array()
            .unwrap_or_else(|| panic!("{}: loss must be an array: {tool}", target.name()));
        let row = losses
            .iter()
            .find(|row| row["code"] == json!(expected_code))
            .unwrap_or_else(|| {
                panic!(
                    "converting to {} dropped the RDF-1.2 quoted triple WITHOUT realizing \
                     `{expected_code}` — a silent RDF-1.1-shaped downgrade is a HARD \
                     FAILURE, not a warning.\n  ledger: {}",
                    target.name(),
                    tool["loss"]
                )
            });
        assert_eq!(row["from"], json!("turtle"), "{row}");
        assert_eq!(row["to"], json!(target.name()), "{row}");
        assert!(
            row["count"].as_u64().unwrap_or(0) > 0,
            "{}: the realized loss must carry the RUN-TIME count of dropped rows, not just \
             the static contract entry: {row}",
            target.name()
        );
        assert!(
            !row["note"].as_str().unwrap_or_default().is_empty(),
            "{}: the realized loss must explain itself: {row}",
            target.name()
        );
    }
}
