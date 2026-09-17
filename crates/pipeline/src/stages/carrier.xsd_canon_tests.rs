// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm, RdfTriple};

const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";

/// A native import uses the same required lexical policy as the former byte
/// transport, including rejection of malformed recognized XSD values.
#[test]
fn native_import_transport_preserves_lexical_normalization_and_refusal() {
    let fixture = tempfile::tempdir().expect("fixture directory");
    let root = fixture.path();
    std::fs::create_dir(root.join("imports")).expect("imports directory");
    let path = root.join("imports/example.ttl");
    for lexical in ["0.90", "invalid-decimal"] {
        let literal = format!("\"{lexical}\"^^<{XSD_DECIMAL}>");
        for text in [
            format!("<https://example.org/s> <https://example.org/p> {literal} ."),
            format!(
                "<https://example.org/s> <https://example.org/p> <<( <https://example.org/s> <https://example.org/p> {literal} )>> ."
            ),
            format!(
                "<https://example.org/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> <<( <https://example.org/s> <https://example.org/p> {literal} )>> ."
            ),
        ] {
            std::fs::write(&path, text).expect("small source fixture");
            let sources =
                crate::stages::source_load::ParsedAuthoredSources::load(root).expect("RDF parse");
            let imported = load_imports(root, &sources);
            if lexical == "invalid-decimal" {
                assert!(
                    imported.is_err(),
                    "native transport must refuse malformed XSD"
                );
            } else {
                let imported = imported.expect("normalized native import");
                let literals: Vec<&str> = (0..imported.term_count())
                    .filter_map(|index| {
                        match imported.resolve(purrdf::TermId::from_index(
                            u32::try_from(index).expect("fixture term"),
                        )) {
                            purrdf::TermRef::Literal { lexical, .. } => Some(lexical),
                            _ => None,
                        }
                    })
                    .collect();
                assert_eq!(
                    literals,
                    ["0.9"],
                    "ordinary and nested literals normalize before publication"
                );
            }
        }
    }
}

/// Serialize a single-quad dataset through `dataset_to_nquads` and return the
/// canonical N-Quads as a string.
fn nquads_of(quad: RdfQuad) -> String {
    let mut b = RdfDatasetBuilder::new();
    b.push_owned_quad(&quad);
    let ds = b.freeze().expect("freeze");
    String::from_utf8(dataset_to_nquads(ds.as_ref()).expect("nquads")).expect("utf8")
}

fn typed_quad(lexical: &str, datatype: &str) -> RdfQuad {
    RdfQuad::new(
        RdfTerm::iri("https://example.org/s"),
        "https://example.org/p",
        RdfTerm::literal(RdfLiteral::typed(lexical, datatype)),
    )
}

/// A recognized XSD datatype is rewritten to its W3C-canonical lexical form —
/// `415.0`→`415.0`, `0.90`→`0.9`, `+00:00`→`Z` (correct native output; oxigraph
/// byte-parity is NOT a goal).
#[test]
fn recognized_xsd_literal_is_canonicalized() {
    for (lex, datatype, expected) in [
        ("0.90", XSD_DECIMAL, "0.9"),
        // XSD 1.1 canonical decimal drops the trailing `.0` for whole values.
        ("415.0", XSD_DECIMAL, "415"),
        ("-200.0", XSD_DECIMAL, "-200"),
        (
            "2024-06-01T10:00:00+00:00",
            XSD_DATETIME,
            "2024-06-01T10:00:00Z",
        ),
    ] {
        let nq = nquads_of(typed_quad(lex, datatype));
        assert!(
            nq.contains(&format!("\"{expected}\"^^<{datatype}>")),
            "{lex}^^<{datatype}> must canonicalize to {expected}; got:\n{nq}"
        );
    }
}

/// A language-tagged literal passes through VERBATIM (rdf:langString has no
/// numeric value space).
#[test]
fn language_tagged_literal_is_verbatim() {
    let nq = nquads_of(RdfQuad::new(
        RdfTerm::iri("https://example.org/s"),
        "https://example.org/p",
        RdfTerm::literal(RdfLiteral::language_tagged("hallo", "de")),
    ));
    assert!(
        nq.contains("\"hallo\"@de"),
        "lang literal verbatim; got:\n{nq}"
    );
}

/// An unrecognized-datatype literal passes through VERBATIM (parse_by_iri →
/// Ok(None)): `0.90` keeps its trailing zero under a custom datatype.
#[test]
fn unknown_datatype_literal_is_verbatim() {
    let custom = "https://example.org/myType";
    let nq = nquads_of(typed_quad("0.90", custom));
    assert!(
        nq.contains(&format!("\"0.90\"^^<{custom}>")),
        "unknown-datatype literal keeps its raw lexical form; got:\n{nq}"
    );
}

/// A plain `xsd:string`-no-datatype literal passes through VERBATIM.
#[test]
fn plain_string_literal_is_verbatim() {
    let nq = nquads_of(RdfQuad::new(
        RdfTerm::iri("https://example.org/s"),
        "https://example.org/p",
        RdfTerm::literal(RdfLiteral::simple("0.90")),
    ));
    assert!(nq.contains("\"0.90\""), "plain string verbatim; got:\n{nq}");
}

/// A malformed lexical for a RECOGNIZED XSD datatype HARD-fails (no-optionality):
/// an authored ontology should never carry one, so surface it.
#[test]
fn malformed_recognized_literal_hard_fails() {
    let mut b = RdfDatasetBuilder::new();
    b.push_owned_quad(&typed_quad("not-a-decimal", XSD_DECIMAL));
    let ds = b.freeze().expect("freeze");
    let err = dataset_to_nquads(ds.as_ref())
        .expect_err("a malformed xsd:decimal must hard-fail, not pass through");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("malformed typed literal"),
        "error must name the malformed typed literal; got: {msg}"
    );
}

/// A literal nested inside a quoted-triple (RDF 1.2 `<< s p o >>`) object is
/// canonicalized too (the recursion contract): `xsd:decimal` `0.90`→`0.9`.
#[test]
fn quoted_triple_object_literal_is_canonicalized() {
    let inner = RdfTriple::new(
        RdfTerm::iri("https://example.org/qs"),
        "https://example.org/qp",
        RdfTerm::literal(RdfLiteral::typed("0.90", XSD_DECIMAL)),
    );
    let nq = nquads_of(RdfQuad::new(
        RdfTerm::iri("https://example.org/s"),
        "https://example.org/p",
        RdfTerm::triple(inner),
    ));
    assert!(
        nq.contains(&format!("\"0.9\"^^<{XSD_DECIMAL}>")),
        "the literal inside a quoted triple must canonicalize 0.90→0.9; got:\n{nq}"
    );
    assert!(
        !nq.contains("\"0.90\""),
        "the raw 0.90 form must not survive inside the quoted triple; got:\n{nq}"
    );
}
