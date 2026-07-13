// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Matrix executable-path gate.
//!
//! Verifies that every `(from, to)` row in `generated/transcode-matrix.json`
//! has a working code path in [`transcode()`].  The matrix is generated from
//! capability predicates; this test catches decoupling gaps where a pair is
//! advertised but throws a runtime error.
//!
//! # GTS fixture bootstrapping
//!
//! The GTS fixture cannot be a static byte literal because GTS bytes are
//! content-addressed by the encoder.  It is produced at test start by
//! transcoding the Turtle fixture through `Codec::Gts`.

use gmeow_pipeline::transcode::{Codec, transcode};

// ── Matrix row schema ─────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Debug)]
struct MatrixRow {
    from: String,
    to: String,
    #[allow(dead_code)]
    lossy: bool,
    #[allow(dead_code)]
    loss_codes: Vec<String>,
}

// ── Source fixtures ───────────────────────────────────────────────────────────

/// Minimal Turtle with one triple.
const TURTLE_FIXTURE: &str = "@prefix ex: <http://example.org/> .\nex:s ex:p ex:o .\n";

/// Minimal N-Triples with one triple.
const NTRIPLES_FIXTURE: &str =
    "<http://example.org/s> <http://example.org/p> <http://example.org/o> .\n";

/// Minimal N-Quads with a named graph and a default-graph triple.
const NQUADS_FIXTURE: &str = concat!(
    "<http://example.org/s> <http://example.org/p> <http://example.org/o> ",
    "<http://example.org/g> .\n",
    "<http://example.org/s> <http://example.org/p> <http://example.org/o> .\n",
);

/// Minimal TriG with a named graph.
const TRIG_FIXTURE: &str = concat!(
    "@prefix ex: <http://example.org/> .\n",
    "ex:g { ex:s ex:p ex:o . }\n",
    "ex:s ex:p ex:o .\n",
);

/// Minimal JSON-LD (no named graph, no star triples).
const JSONLD_FIXTURE: &str = concat!(
    r#"{"@context":{"ex":"http://example.org/"},"@id":"ex:s","ex:p":{"@id":"ex:o"}}"#,
    "\n",
);

/// Minimal RDF/XML.
const RDFXML_FIXTURE: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    "\n",
    r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#""#,
    r#"         xmlns:ex="http://example.org/">"#,
    "\n",
    r#"  <rdf:Description rdf:about="http://example.org/s">"#,
    "\n",
    r#"    <ex:p rdf:resource="http://example.org/o"/>"#,
    "\n",
    r#"  </rdf:Description>"#,
    "\n",
    r#"</rdf:RDF>"#,
    "\n",
);

/// OWL-RDF12 is parsed as Turtle (same oxigraph path); reuse the Turtle fixture.
const OWL_RDF12_FIXTURE: &str = TURTLE_FIXTURE;

// ── GTS fixture (bootstrapped at runtime) ─────────────────────────────────────

fn make_gts_fixture() -> Vec<u8> {
    transcode(TURTLE_FIXTURE.as_bytes(), Codec::Turtle, Codec::Gts, None)
        .expect("bootstrap: turtle → gts must succeed")
        .bytes
}

// ── Fixture dispatch ──────────────────────────────────────────────────────────

/// Return the representative fixture bytes for a given `from` codec name.
///
/// The GTS fixture is passed in pre-computed (`gts_bytes`) to avoid
/// regenerating it for every row.
fn fixture_for<'a>(from: &str, gts_bytes: &'a [u8]) -> &'a [u8] {
    match from {
        "turtle" => TURTLE_FIXTURE.as_bytes(),
        "ntriples" => NTRIPLES_FIXTURE.as_bytes(),
        "nquads" => NQUADS_FIXTURE.as_bytes(),
        "trig" => TRIG_FIXTURE.as_bytes(),
        "jsonld" => JSONLD_FIXTURE.as_bytes(),
        "rdfxml" => RDFXML_FIXTURE.as_bytes(),
        "owl-rdf12" => OWL_RDF12_FIXTURE.as_bytes(),
        "gts" => gts_bytes,
        other => panic!("no fixture defined for from-codec `{other}`"),
    }
}

// ── Main test ─────────────────────────────────────────────────────────────────

/// Expected row count (byte-stable against the committed artifact).
const EXPECTED_ROW_COUNT: usize = 120;

#[test]
fn every_matrix_row_has_an_executable_path() {
    // Load matrix.
    let matrix_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../generated/transcode-matrix.json"
    );
    let matrix_text = std::fs::read_to_string(matrix_path)
        .unwrap_or_else(|e| panic!("cannot read transcode-matrix.json: {e}"));
    let rows: Vec<MatrixRow> = serde_json::from_str(&matrix_text)
        .unwrap_or_else(|e| panic!("cannot parse transcode-matrix.json: {e}"));

    // Row-count guard — fail loudly if the matrix changes without updating this test.
    assert_eq!(
        rows.len(),
        EXPECTED_ROW_COUNT,
        "transcode-matrix.json has {} rows but test expects {}; \
         update EXPECTED_ROW_COUNT in transcode_matrix_executable.rs",
        rows.len(),
        EXPECTED_ROW_COUNT,
    );

    // Bootstrap the GTS fixture once.
    let gts_bytes = make_gts_fixture();

    let mut failures: Vec<String> = Vec::new();

    for row in &rows {
        // Resolve codecs via the public parser (same path the CLI uses).
        let from_codec = match Codec::from_cli_str(&row.from) {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!("[{}→{}] unknown from-codec: {e}", row.from, row.to));
                continue;
            }
        };
        let to_codec = match Codec::from_cli_str(&row.to) {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!("[{}→{}] unknown to-codec: {e}", row.from, row.to));
                continue;
            }
        };

        let input = fixture_for(&row.from, &gts_bytes);

        if let Err(e) = transcode(input, from_codec, to_codec, None) {
            failures.push(format!("[{}→{}] transcode error: {e}", row.from, row.to));
        }
    }

    if !failures.is_empty() {
        let msg = failures.join("\n");
        panic!(
            "{} matrix row(s) have no executable path out of {}:\n{msg}",
            failures.len(),
            rows.len(),
        );
    }
}
