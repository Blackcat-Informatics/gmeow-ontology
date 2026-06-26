// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

#![cfg(feature = "gts")]

//! Property-based round-trip tests (#787, T6 of #781): `parse ∘ serialize = id`,
//! modulo canonical form, for the RDF serialization codecs the kernel exposes.
//!
//! # Equivalence is canonical, never byte-exact
//!
//! A faithful round-trip is allowed to rename blank nodes and to collapse the
//! `"x"` ≡ `"x"^^xsd:string` distinction. Byte equality would therefore produce
//! spurious failures (cf. the GTS codec-skew doctrine, PR #595: the drift gate is
//! semantic). Every property here compares **RDFC-1.0 canonical quad sets**
//! (oxigraph `Dataset::canonicalize`), which is the same comparator the
//! python-gated `py_store::canonicalize_quads` core wraps — re-used here against
//! oxigraph's public API because that core is compiled only under the `python`
//! feature and is unreachable from a plain `cargo nextest` run.
//!
//! # Single generator, three codecs
//!
//! One generator authors a frozen [`RdfDataset`] fixture; the reachable production
//! seam [`store_from_dataset`](gmeow_rdf::oxigraph::store_from_dataset) converts it
//! once to the oxigraph "before" set, which drives N-Quads, TriG, and GTS
//! fold/unfold alike.
//!
//! # Generators dodge codec-lossy inputs deliberately
//!
//! GTS drops language *direction* and the oxigraph `Store` canonicalizes
//! non-canonical typed-literal lexical forms (`0.70` → `0.7`). The generators
//! therefore emit no direction and only already-canonical literals (`i32`
//! integers, `true`/`false`, plain/typed strings, standard language tags), so the
//! preserve-path (GTS) and the normalize-path (Store) cannot disagree.
//!
//! # Coverage and deferrals
//!
//! * **JSON-LD** round-trips basic terms (IRIs, blank nodes, literals, language
//!   tags, named graphs). **JSON-LD-star** is deferred: oxigraph's JSON-LD
//!   serializer rejects RDF-1.2 quoted triples (no standard star JSON-LD
//!   encoding), so the JSON-LD property uses the star-free generator while
//!   N-Quads/TriG carry the quoted-triple coverage.
//! * **CLIF / CGIF / XCL** round-trips: depend on the open Common Logic epic
//!   (#718/#719) and do not exist yet.
//!
//! # INTENTIONAL oxigraph cross-check (#909) — NOT a production native-codec path
//!
//! The `oxigraph::io` parse/serialize used here is deliberate: this gate cross-checks
//! the python-gated `py_store::{canonicalize_quads, parse_quads}` core (which wraps
//! oxigraph and is unreachable from a plain `cargo nextest` run) against oxigraph's
//! own public RDFC-1.0 comparator and codecs. Re-using the *independent* oxigraph
//! implementation as the reference is the whole point. The native text codec's own
//! round-trip fidelity is covered separately by the isomorphism round-trips in
//! `crates/rdf/src/native_codecs/mod.rs`. So the `oxigraph::io` use here is an
//! explicit, documented carve-out from the #909 grep gate, not a production codec.

use gmeow_rdf::oxigraph::{store_from_dataset, GraphPolicy};
use gmeow_rdf::{
    RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfLookaside, RdfQuad, RdfTerm, RdfTriple,
};
use oxigraph::io::{RdfFormat, RdfParser, RdfSerializer};
use oxigraph::model::Quad;
use proptest::prelude::*;

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

// ── Canonical comparator (native RDFC-1.0, #910) ─────────────────────────────────

/// The default JSON-LD format (no profile flags).
fn jsonld() -> RdfFormat {
    RdfFormat::JsonLd {
        profile: Default::default(),
    }
}

/// Canonicalize a quad set's blank-node labels under the native full RDFC-1.0 and
/// return the quads in a stable order — this IS `gmeow_rdf::canonicalize_quads`
/// (which replaced oxrdf `Dataset::canonicalize`).
fn canonical(quads: Vec<Quad>) -> Vec<Quad> {
    gmeow_rdf::canonicalize_quads(quads).expect("RDFC-1.0 canonicalization")
}

fn serialize_quads(quads: &[Quad], format: RdfFormat) -> Vec<u8> {
    let mut serializer = RdfSerializer::from_format(format).for_writer(Vec::new());
    for quad in quads {
        serializer
            .serialize_quad(quad.as_ref())
            .expect("serialize quad");
    }
    serializer.finish().expect("finish serializer")
}

fn parse_quads(bytes: &[u8], format: RdfFormat) -> Vec<Quad> {
    RdfParser::from_format(format)
        .lenient()
        .for_slice(bytes)
        .map(|quad| quad.expect("parse quad"))
        .collect()
}

/// The oxigraph "before" quad set for `dataset`, via the reachable production seam.
fn before_quads(dataset: &RdfDataset) -> Vec<Quad> {
    let ox = store_from_dataset(dataset, GraphPolicy::PreserveNamedGraphs)
        .expect("convert RdfDataset to oxigraph store");
    ox.iter().map(|quad| quad.expect("store quad")).collect()
}

/// Freeze generated quads into the IR. The bnode-label rewrite from scope
/// qualification is irrelevant here: the comparator canonicalizes blank nodes
/// under RDFC-1.0.
fn dataset_from_quads(quads: Vec<RdfQuad>) -> std::sync::Arc<RdfDataset> {
    let mut b = RdfDatasetBuilder::new();
    for quad in quads {
        b.push_owned_quad(&quad);
    }
    b.freeze()
        .expect("generated quads must freeze into a valid dataset")
}

// ── Generators (valid, codec-safe inputs) ───────────────────────────────────────

fn arb_iri() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9]{0,6}".prop_map(|s| format!("https://example.org/{s}"))
}

fn arb_bnode_label() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9]{0,6}".prop_map(String::from)
}

fn arb_text() -> impl Strategy<Value = String> {
    // Printable ASCII without quote/backslash/control chars so GTS and oxigraph
    // escaping cannot diverge; widening this is a follow-up, not a v1 concern.
    "[A-Za-z0-9._-]{0,12}".prop_map(String::from)
}

fn arb_lang() -> impl Strategy<Value = String> {
    prop::sample::select(vec!["en", "fr", "de", "es"]).prop_map(String::from)
}

fn arb_literal() -> impl Strategy<Value = RdfLiteral> {
    prop_oneof![
        arb_text().prop_map(RdfLiteral::simple),
        arb_text().prop_map(|t| RdfLiteral::typed(t, XSD_STRING)),
        // i32::to_string is already a canonical xsd:integer lexical form (no
        // leading zeros, no "-0"), so the Store does not rewrite it.
        any::<i32>().prop_map(|n| RdfLiteral::typed(n.to_string(), XSD_INTEGER)),
        prop::sample::select(vec!["true", "false"]).prop_map(|b| RdfLiteral::typed(b, XSD_BOOLEAN)),
        (arb_text(), arb_lang()).prop_map(|(t, l)| RdfLiteral::language_tagged(t, l)),
    ]
}

/// Leaf object terms (no quoted triple) — used inside quoted triples to keep the
/// nesting bounded and free of inner blank nodes.
fn arb_simple_object() -> impl Strategy<Value = RdfTerm> {
    prop_oneof![
        arb_iri().prop_map(RdfTerm::iri),
        arb_literal().prop_map(RdfTerm::literal),
    ]
}

/// One level of RDF-1.2 quoted triple: `<< iri iri (iri|literal) >>`.
fn arb_quoted_triple() -> impl Strategy<Value = RdfTriple> {
    (arb_iri(), arb_iri(), arb_simple_object())
        .prop_map(|(s, p, o)| RdfTriple::new(RdfTerm::iri(s), p, o))
}

/// Object terms without a quoted triple — the surface GTS represents faithfully
/// (GTS lowers bare triple-term objects to blank nodes, since its quoted-triple
/// support goes through the reifier idiom, not bare triple terms).
fn arb_object_basic() -> impl Strategy<Value = RdfTerm> {
    prop_oneof![
        arb_iri().prop_map(RdfTerm::iri),
        arb_bnode_label().prop_map(RdfTerm::blank_node),
        arb_literal().prop_map(RdfTerm::literal),
    ]
}

/// Basic objects plus RDF-1.2 quoted triples — round-tripped by the lossless
/// N-Quads/TriG codecs (NOT GTS, see [`arb_object_basic`]).
fn arb_object_star() -> impl Strategy<Value = RdfTerm> {
    prop_oneof![
        4 => arb_object_basic(),
        1 => arb_quoted_triple().prop_map(RdfTerm::triple),
    ]
}

fn arb_subject() -> impl Strategy<Value = RdfTerm> {
    prop_oneof![
        arb_iri().prop_map(RdfTerm::iri),
        arb_bnode_label().prop_map(RdfTerm::blank_node),
    ]
}

fn mk_quad(
    (subject, predicate, object, graph): (RdfTerm, String, RdfTerm, Option<String>),
) -> RdfQuad {
    let quad = RdfQuad::new(subject, predicate, object);
    match graph {
        Some(g) => quad.in_graph(RdfTerm::iri(g)),
        None => quad,
    }
}

/// Dataset over the GTS-faithful surface (no bare quoted-triple objects).
fn arb_dataset() -> impl Strategy<Value = std::sync::Arc<RdfDataset>> {
    let quad = (
        arb_subject(),
        arb_iri(),
        arb_object_basic(),
        prop::option::of(arb_iri()),
    )
        .prop_map(mk_quad);
    prop::collection::vec(quad, 0..16).prop_map(dataset_from_quads)
}

/// Dataset including RDF-1.2 quoted triples (for the lossless N-Quads/TriG codecs).
fn arb_dataset_star() -> impl Strategy<Value = std::sync::Arc<RdfDataset>> {
    let quad = (
        arb_subject(),
        arb_iri(),
        arb_object_star(),
        prop::option::of(arb_iri()),
    )
        .prop_map(mk_quad);
    prop::collection::vec(quad, 0..16).prop_map(dataset_from_quads)
}

// ── Config ──────────────────────────────────────────────────────────────────────

fn config() -> ProptestConfig {
    // Bounded case count keeps each property well under the nextest 60s
    // slow-timeout; raise locally with PROPTEST_CASES to deepen the search.
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(64);
    ProptestConfig {
        cases,
        // No on-disk regression files in a clean checkout / CI tree.
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

// ── Properties ──────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(config())]

    /// N-Quads: serialize → parse round-trips to the same canonical quad set,
    /// including RDF-1.2 quoted triples.
    #[test]
    fn nquads_roundtrip(dataset in arb_dataset_star()) {
        let before = before_quads(dataset.as_ref());
        let after = parse_quads(&serialize_quads(&before, RdfFormat::NQuads), RdfFormat::NQuads);
        prop_assert_eq!(canonical(before), canonical(after));
    }

    /// TriG: same property, exercising named graphs and quoted triples.
    #[test]
    fn trig_roundtrip(dataset in arb_dataset_star()) {
        let before = before_quads(dataset.as_ref());
        let after = parse_quads(&serialize_quads(&before, RdfFormat::TriG), RdfFormat::TriG);
        prop_assert_eq!(canonical(before), canonical(after));
    }

    /// JSON-LD: serialize → parse round-trips to the same canonical quad set
    /// (basic terms; quoted triples excluded — see module docs).
    #[test]
    fn jsonld_roundtrip(dataset in arb_dataset()) {
        let before = before_quads(dataset.as_ref());
        let after = parse_quads(&serialize_quads(&before, jsonld()), jsonld());
        prop_assert_eq!(canonical(before), canonical(after));
    }

    /// GTS fold/unfold: RdfDataset → `to_gts` → fold → N-Quads round-trips to the
    /// same canonical quad set.
    #[test]
    fn gts_roundtrip(dataset in arb_dataset()) {
        let before = before_quads(dataset.as_ref());
        let bytes = gmeow_rdf::gts_write::to_gts(dataset.as_ref(), &RdfLookaside::default(), "gmeow-rdf-proptest")
            .expect("to_gts should succeed");
        let graph = gmeow_gts::reader::read(&bytes, false, None);
        prop_assert!(graph.diagnostics.is_empty(), "GTS fold diagnostics: {:?}", graph.diagnostics);
        let after = parse_quads(gmeow_gts::nquads::to_nquads(&graph).as_bytes(), RdfFormat::NQuads);
        prop_assert_eq!(canonical(before), canonical(after));
    }
}
