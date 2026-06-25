// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Universal RDF-1.2 transcode hub (#671 Task 3).
//!
//! Hub-and-spoke through the frozen [`RdfDataset`] IR: parse any supported
//! source codec into the IR, serialize the IR to any supported target codec,
//! and record loss on every lossy edge.
//!
//! # Codec taxonomy
//!
//! Codecs split into two families:
//! - **Syntax codecs** — lossless (or near-lossless) RDF serialization formats
//!   that can be decoded (parsed) into the IR: Turtle, N-Triples, N-Quads, TriG,
//!   JSON-LD, JSON-LD-star\*, YAML-LD-star\*, RDF/XML, GTS, OWL-RDF-1.2.
//! - **Projection codecs** — lossy, semantic-subset targets that cannot be
//!   decoded (no inverse parse exists): OWL-DL, OWL-EL, Datalog, N3, Nemo,
//!   gUFO, canonical-RDF12.
//!
//! (\* JSON-LD-star and YAML-LD-star are syntax codecs but are NOT decodable via
//! the current oxigraph path — use N-Quads-star or GTS instead.)
//!
//! # Loss recording
//!
//! Every transcode pair is checked against [`gmeow_rdf::loss::pair_loss_ledger`]
//! for its static loss contract. Each contract entry is materialized into a
//! [`RealizedLoss`] with a runtime count attached:
//! - `named-graph-dropped`: number of distinct non-default graph-name terms
//!   in the parsed dataset.
//! - `rdf12-star-unrepresentable` / `rdf12-star-jsonld-rejected`: the
//!   `statement_rows_dropped` field of the oxigraph serializer outcome.
//! - projection codes (`owl-dl-projection` etc.): `actual_drops.len()`.
//! - all other contract entries: 0.

use std::collections::HashSet;
use std::sync::Arc;

use gmeow_logic::compile::frontend::parse_logic_str;
use gmeow_logic::compile::projections::{
    rdf::{project_canonical_rdf12, project_gufo, project_owl_dl, project_owl_el},
    text::{project_datalog, project_n3, project_nemo},
};
use gmeow_rdf::loss::pair_loss_ledger;
use gmeow_rdf::{
    dataset_from_bytes, import_gts_events, serialize_dataset_to_format, RdfDataset, RdfLookaside,
    TermId,
};
use oxigraph::io::RdfFormat;

/// A supported transcode codec (source or target).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Codec {
    /// Turtle (RDF 1.2 star-capable).
    Turtle,
    /// N-Triples (RDF 1.2 star-capable).
    NTriples,
    /// N-Quads (RDF 1.2 star-capable).
    NQuads,
    /// TriG (RDF 1.2 star-capable).
    TriG,
    /// JSON-LD 1.1 (no quoted-triple support; star rows are dropped).
    JsonLd,
    /// JSON-LD 1.1 with RDF-1.2 star syntax (currently not decodable via this
    /// hub — use [`Codec::NQuads`] or [`Codec::Gts`] as the input instead).
    JsonLdStar,
    /// YAML-LD with RDF-1.2 star syntax (currently not decodable via this
    /// hub — use [`Codec::NQuads`] or [`Codec::Gts`] as the input instead).
    YamlLdStar,
    /// RDF/XML (no quoted-triple support; star rows are dropped).
    RdfXml,
    /// GMEOW Transport Serialization (lossless, star-capable, named-graph-capable).
    Gts,
    /// OWL-RDF 1.2 (canonical RDF-star Turtle; star-capable, decodable).
    OwlRdf12,
    /// OWL 2 DL projection (lossy; NOT decodable).
    OwlDl,
    /// OWL 2 EL projection (lossy; NOT decodable).
    OwlEl,
    /// Datalog projection (lossy; NOT decodable).
    Datalog,
    /// Notation-3 projection (lossy; NOT decodable).
    N3,
    /// Nemo existential-rules projection (lossy; NOT decodable).
    Nemo,
    /// gUFO foundational-classes projection (lossy; NOT decodable).
    Gufo,
    /// Canonical RDF-1.2 logic form projection (exact; NOT decodable as a
    /// projection target).
    CanonicalRdf12,
}

impl Codec {
    /// The canonical kebab-case name for this codec.
    ///
    /// Each name is the exact string accepted by [`Codec::from_cli_str`] and
    /// recognized by [`canonical_codec_name`] in the loss ledger.
    pub fn name(self) -> &'static str {
        match self {
            Self::Turtle => "turtle",
            Self::NTriples => "ntriples",
            Self::NQuads => "nquads",
            Self::TriG => "trig",
            Self::JsonLd => "jsonld",
            Self::JsonLdStar => "jsonld-star",
            Self::YamlLdStar => "yaml-ld-star",
            Self::RdfXml => "rdfxml",
            Self::Gts => "gts",
            Self::OwlRdf12 => "owl-rdf12",
            Self::OwlDl => "owl-dl",
            Self::OwlEl => "owl-el",
            Self::Datalog => "datalog",
            Self::N3 => "n3",
            Self::Nemo => "nemo",
            Self::Gufo => "gufo",
            Self::CanonicalRdf12 => "canonical-rdf12",
        }
    }

    /// All supported codecs in a stable, canonical order.
    pub fn all() -> &'static [Codec] {
        &[
            Self::Turtle,
            Self::NTriples,
            Self::NQuads,
            Self::TriG,
            Self::JsonLd,
            Self::JsonLdStar,
            Self::YamlLdStar,
            Self::RdfXml,
            Self::Gts,
            Self::OwlRdf12,
            Self::OwlDl,
            Self::OwlEl,
            Self::Datalog,
            Self::N3,
            Self::Nemo,
            Self::Gufo,
            Self::CanonicalRdf12,
        ]
    }

    /// Parse a CLI string into a [`Codec`].
    ///
    /// Accepts the same names as [`Codec::name`]. Returns
    /// [`TranscodeError::UnknownCodec`] on an unrecognized string.
    pub fn from_cli_str(s: &str) -> Result<Codec, TranscodeError> {
        match s {
            "turtle" | "ttl" => Ok(Self::Turtle),
            "ntriples" | "nt" => Ok(Self::NTriples),
            "nquads" | "nq" => Ok(Self::NQuads),
            "trig" => Ok(Self::TriG),
            "jsonld" | "json-ld" => Ok(Self::JsonLd),
            "jsonld-star" | "json-ld-star" => Ok(Self::JsonLdStar),
            "yaml-ld-star" | "yamlld-star" => Ok(Self::YamlLdStar),
            "rdfxml" | "rdf-xml" | "xml" => Ok(Self::RdfXml),
            "gts" => Ok(Self::Gts),
            "owl-rdf12" => Ok(Self::OwlRdf12),
            "owl-dl" => Ok(Self::OwlDl),
            "owl-el" => Ok(Self::OwlEl),
            "datalog" | "dl" => Ok(Self::Datalog),
            "n3" => Ok(Self::N3),
            "nemo" => Ok(Self::Nemo),
            "gufo" => Ok(Self::Gufo),
            "canonical-rdf12" => Ok(Self::CanonicalRdf12),
            other => Err(TranscodeError::UnknownCodec(other.to_owned())),
        }
    }

    /// `true` when this codec is a projection (lossy, semantic-subset) target.
    ///
    /// Projection codecs cannot be used as the `from` argument to
    /// [`read_to_dataset`].
    pub fn is_projection(self) -> bool {
        matches!(
            self,
            Self::OwlDl
                | Self::OwlEl
                | Self::Datalog
                | Self::N3
                | Self::Nemo
                | Self::Gufo
                | Self::CanonicalRdf12
        )
    }

    /// `true` when this codec can be parsed (decoded) into the IR.
    ///
    /// JSON-LD-star and YAML-LD-star are classified as syntax codecs but are
    /// NOT currently decodable via the oxigraph path. All projection codecs
    /// also return `false`.
    pub fn can_decode(self) -> bool {
        !self.is_projection() && !matches!(self, Self::JsonLdStar | Self::YamlLdStar)
    }
}

/// A loss event that actually occurred during transcoding, with a runtime count
/// attached to the static contract entry.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RealizedLoss {
    /// The stable loss code (e.g. `"named-graph-dropped"`).
    pub code: String,
    /// Source codec name (e.g. `"trig"`).
    pub from: String,
    /// Target codec name (e.g. `"turtle"`).
    pub to: String,
    /// Human-readable explanation from the static ledger.
    pub note: String,
    /// Runtime count of the dropped items (0 when the ledger declares the loss
    /// but no items were actually present in this particular dataset).
    pub count: u64,
}

/// The bytes produced by a transcode operation together with any losses
/// realized against the static loss contract.
pub struct TranscodeOutput {
    /// The serialized output bytes.
    pub bytes: Vec<u8>,
    /// The losses realized for this transcode pair on this particular dataset.
    /// Empty for lossless pairs.
    pub realized: Vec<RealizedLoss>,
}

/// Errors that can occur during a transcode operation.
#[derive(Debug)]
pub enum TranscodeError {
    /// The codec string was not recognized.
    UnknownCodec(String),
    /// The source codec is a projection and cannot be decoded.
    NonInvertibleSource(String),
    /// The source codec is syntactically valid but currently has no decode path
    /// in this hub (JSON-LD-star, YAML-LD-star). Use N-Quads-star or GTS instead.
    UndecodableInput(String),
    /// A codec-level error during parsing or serialization.
    Codec(String),
}

impl std::fmt::Display for TranscodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCodec(s) => write!(f, "unknown codec: {s}"),
            Self::NonInvertibleSource(s) => write!(f, "non-invertible source codec: {s}"),
            Self::UndecodableInput(s) => write!(f, "currently undecodable input codec: {s}"),
            Self::Codec(s) => write!(f, "codec error: {s}"),
        }
    }
}

impl std::error::Error for TranscodeError {}

/// Parse `input` bytes in the `from` codec into a frozen [`RdfDataset`] IR.
///
/// # Hard failures
///
/// - [`TranscodeError::NonInvertibleSource`] when `from.is_projection()`.
/// - [`TranscodeError::UndecodableInput`] when `from` is `JsonLdStar` or
///   `YamlLdStar` — these codecs have no decode path in the current hub;
///   provide N-Quads-star or GTS instead.
pub fn read_to_dataset(input: &[u8], from: Codec) -> Result<Arc<RdfDataset>, TranscodeError> {
    if from.is_projection() {
        return Err(TranscodeError::NonInvertibleSource(format!(
            "codec `{}` is a projection (semantic-subset) target and cannot be used as \
             a decode source; provide a syntax codec instead",
            from.name()
        )));
    }
    if !from.can_decode() {
        return Err(TranscodeError::UndecodableInput(format!(
            "codec `{}` is not currently decodable via the oxigraph path; \
             provide nquads (N-Quads-star) or gts instead",
            from.name()
        )));
    }

    match from {
        Codec::Gts => {
            let bundle =
                import_gts_events(input).map_err(|e| TranscodeError::Codec(e.to_string()))?;
            Ok(bundle.dataset)
        }
        _ => {
            let fmt = codec_to_rdf_format(from).ok_or_else(|| {
                TranscodeError::Codec(format!("no RdfFormat mapping for `{}`", from.name()))
            })?;
            dataset_from_bytes(input, fmt).map_err(TranscodeError::Codec)
        }
    }
}

/// Transcode `input` bytes from `from` to `to`.
///
/// - Identity (`from == to`) is allowed and lossless.
/// - Projection targets serialize the IR to Turtle, parse it with the logic
///   front-end, run the matching projection back-end, and return the projection
///   output as bytes.
/// - The returned [`TranscodeOutput`] carries every [`RealizedLoss`] for the
///   pair (with runtime counts attached).
pub fn transcode(
    input: &[u8],
    from: Codec,
    to: Codec,
    base_iri: Option<&str>,
) -> Result<TranscodeOutput, TranscodeError> {
    let dataset = read_to_dataset(input, from)?;

    // Compute the static loss contract for this (from, to) pair.
    let ledger = pair_loss_ledger(from.name(), to.name());

    // Count distinct non-default named graphs in the dataset.
    let named_graph_count: u64 = {
        let mut seen: HashSet<TermId> = HashSet::new();
        for quad in dataset.quads() {
            if let Some(g) = quad.g {
                seen.insert(g);
            }
        }
        seen.len() as u64
    };

    if to == Codec::Gts {
        let bytes =
            gmeow_rdf::gts_write::to_gts(&dataset, &RdfLookaside::default(), "gmeow-transcode")
                .map_err(|e| TranscodeError::Codec(e.to_string()))?;
        let realized = realize_losses(ledger.entries(), named_graph_count, 0, 0);
        return Ok(TranscodeOutput { bytes, realized });
    }

    if to.is_projection() {
        return transcode_to_projection(&dataset, from, to, named_graph_count);
    }

    // Syntax-to-syntax path via oxigraph serializer.
    let fmt = codec_to_rdf_format(to).ok_or_else(|| {
        TranscodeError::Codec(format!("no RdfFormat mapping for `{}`", to.name()))
    })?;
    let outcome = serialize_dataset_to_format(&dataset, fmt, base_iri)
        .map_err(|e| TranscodeError::Codec(e.to_string()))?;

    let star_dropped = outcome.statement_rows_dropped as u64;
    let realized = realize_losses(ledger.entries(), named_graph_count, star_dropped, 0);

    Ok(TranscodeOutput {
        bytes: outcome.bytes,
        realized,
    })
}

/// Serialize a dataset to a projection target by:
/// 1. Serializing the IR to Turtle.
/// 2. Parsing the Turtle with the logic front-end.
/// 3. Running the matching projection back-end.
fn transcode_to_projection(
    dataset: &RdfDataset,
    from: Codec,
    to: Codec,
    named_graph_count: u64,
) -> Result<TranscodeOutput, TranscodeError> {
    // Serialize the IR to Turtle via the oxigraph serializer.
    let turtle_outcome = serialize_dataset_to_format(dataset, RdfFormat::Turtle, None)
        .map_err(|e| TranscodeError::Codec(format!("projection pre-step turtle serialize: {e}")))?;
    let turtle_str = String::from_utf8(turtle_outcome.bytes)
        .map_err(|e| TranscodeError::Codec(format!("projection pre-step turtle utf8: {e}")))?;

    // Parse with the logic front-end.
    let (program, _diagnostics) = parse_logic_str(&turtle_str, None)
        .map_err(|e| TranscodeError::Codec(format!("logic front-end parse: {e}")))?;

    // Run the appropriate projection back-end.
    let (content, actual_drop_count) = match to {
        Codec::OwlDl => {
            let result = project_owl_dl(&program)
                .map_err(|e| TranscodeError::Codec(format!("owl-dl projection: {e}")))?;
            let drops = result.actual_drops.len() as u64;
            (result.content, drops)
        }
        Codec::OwlEl => {
            let result = project_owl_el(&program)
                .map_err(|e| TranscodeError::Codec(format!("owl-el projection: {e}")))?;
            let drops = result.actual_drops.len() as u64;
            (result.content, drops)
        }
        Codec::Datalog => {
            let result = project_datalog(&program);
            let drops = result.actual_drops.len() as u64;
            (result.content, drops)
        }
        Codec::N3 => {
            let result = project_n3(&program);
            let drops = result.actual_drops.len() as u64;
            (result.content, drops)
        }
        Codec::Nemo => {
            let result = project_nemo(&program)
                .map_err(|e| TranscodeError::Codec(format!("nemo projection: {e}")))?;
            let drops = result.actual_drops.len() as u64;
            (result.content, drops)
        }
        Codec::Gufo => {
            let result = project_gufo(&program)
                .map_err(|e| TranscodeError::Codec(format!("gufo projection: {e}")))?;
            let drops = result.actual_drops.len() as u64;
            (result.content, drops)
        }
        Codec::CanonicalRdf12 => {
            let result = project_canonical_rdf12(&program)
                .map_err(|e| TranscodeError::Codec(format!("canonical-rdf12 projection: {e}")))?;
            let drops = result.actual_drops.len() as u64;
            (result.content, drops)
        }
        _ => unreachable!("transcode_to_projection called with non-projection target"),
    };

    let ledger = pair_loss_ledger(from.name(), to.name());
    let realized = realize_losses(ledger.entries(), named_graph_count, 0, actual_drop_count);

    Ok(TranscodeOutput {
        bytes: content.into_bytes(),
        realized,
    })
}

/// Attach runtime counts to the static ledger entries, producing [`RealizedLoss`] values.
///
/// The count policy:
/// - `named-graph-dropped` → `named_graph_count`
/// - `rdf12-star-unrepresentable` | `rdf12-star-jsonld-rejected` → `star_dropped`
/// - any projection code (ends in `-projection`) → `projection_drops`
/// - anything else → 0
fn realize_losses(
    entries: &[gmeow_rdf::loss::LossEntry],
    named_graph_count: u64,
    star_dropped: u64,
    projection_drops: u64,
) -> Vec<RealizedLoss> {
    entries
        .iter()
        .map(|e| {
            let count = match e.code {
                "named-graph-dropped" => named_graph_count,
                "rdf12-star-unrepresentable" | "rdf12-star-jsonld-rejected" => star_dropped,
                c if c.ends_with("-projection") => projection_drops,
                _ => 0,
            };
            RealizedLoss {
                code: e.code.to_owned(),
                from: e.from.to_owned(),
                to: e.to.to_owned(),
                note: e.note.to_owned(),
                count,
            }
        })
        .collect()
}

/// Render a slice of [`RealizedLoss`] values as deterministic, pretty-printed
/// JSON sorted by `(from, to, code)`.
pub fn realized_loss_json(losses: &[RealizedLoss]) -> String {
    let mut sorted: Vec<&RealizedLoss> = losses.iter().collect();
    sorted.sort_by(|a, b| {
        a.from
            .cmp(&b.from)
            .then(a.to.cmp(&b.to))
            .then(a.code.cmp(&b.code))
    });
    serde_json::to_string_pretty(&sorted).unwrap_or_else(|_| "[]".to_owned())
}

/// One row of the supported-transcode matrix: a `from → to` pair the engine
/// accepts, with the static loss codes the conversion declares.
#[derive(Debug, serde::Serialize)]
struct MatrixEntry {
    from: &'static str,
    to: &'static str,
    /// `true` when the static contract declares any loss for this pair.
    lossy: bool,
    /// The declared loss codes (sorted), empty for a lossless pair.
    loss_codes: Vec<&'static str>,
}

/// Render the supported-transcode matrix as deterministic, pretty JSON.
///
/// Enumerates every `from → to` pair the engine accepts — `from` is any
/// decodable codec, `to` is any codec except the identity — annotated with the
/// static [`pair_loss_ledger`] loss codes. Sorted by `(from, to)` so the
/// committed `generated/transcode-matrix.json` artifact is byte-stable; a drift
/// test re-derives and compares it.
pub fn transcode_matrix_json() -> String {
    let mut entries: Vec<MatrixEntry> = Vec::new();
    for &from in Codec::all() {
        if !from.can_decode() {
            continue;
        }
        for &to in Codec::all() {
            if from == to {
                continue;
            }
            let ledger = pair_loss_ledger(from.name(), to.name());
            let mut loss_codes: Vec<&'static str> =
                ledger.entries().iter().map(|e| e.code).collect();
            loss_codes.sort_unstable();
            entries.push(MatrixEntry {
                from: from.name(),
                to: to.name(),
                lossy: !loss_codes.is_empty(),
                loss_codes,
            });
        }
    }
    entries.sort_by(|a, b| a.from.cmp(b.from).then(a.to.cmp(b.to)));
    let mut json = serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_owned());
    json.push('\n');
    json
}

/// Map a syntax [`Codec`] to its oxigraph [`RdfFormat`] equivalent.
///
/// Returns `None` for codecs that have no direct oxigraph mapping (GTS, any
/// projection codec). The caller is responsible for routing those separately.
fn codec_to_rdf_format(codec: Codec) -> Option<RdfFormat> {
    match codec {
        Codec::Turtle | Codec::OwlRdf12 => Some(RdfFormat::Turtle),
        Codec::NTriples => Some(RdfFormat::NTriples),
        Codec::NQuads => Some(RdfFormat::NQuads),
        Codec::TriG => Some(RdfFormat::TriG),
        Codec::JsonLd => Some(RdfFormat::JsonLd {
            profile: Default::default(),
        }),
        Codec::RdfXml => Some(RdfFormat::RdfXml),
        // GTS and projection codecs do not map to RdfFormat.
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf::loss::canonical_codec_name;

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
    fn roundtrip_star_through_nquads() {
        let out = transcode(
            STAR_FIXTURE_TTL.as_bytes(),
            Codec::Turtle,
            Codec::NQuads,
            None,
        )
        .expect("nquads transcode");
        assert!(!out.bytes.is_empty());
    }

    #[test]
    fn roundtrip_star_through_trig() {
        let out = transcode(
            STAR_FIXTURE_TTL.as_bytes(),
            Codec::Turtle,
            Codec::TriG,
            None,
        )
        .expect("trig transcode");
        assert!(!out.bytes.is_empty());
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

    /// GTS round-trip: parse via GTS path, verify the round-tripped dataset has
    /// the same quad/reifier count as the direct path.
    #[test]
    fn gts_round_trip_via_canonical_nquads() {
        use gmeow_rdf::dataset_from_bytes;
        use oxigraph::io::RdfFormat;

        // Turtle → GTS → NTriples
        let gts_out = transcode(
            STAR_FIXTURE_TTL.as_bytes(),
            STAR_FIXTURE_FORMAT,
            Codec::Gts,
            None,
        )
        .expect("turtle → gts");
        let nt_out =
            transcode(&gts_out.bytes, Codec::Gts, Codec::NTriples, None).expect("gts → ntriples");
        assert!(!nt_out.bytes.is_empty());

        // Direct NTriples → NTriples (identity, to compare content)
        let direct_nq = transcode(
            STAR_FIXTURE_TTL.as_bytes(),
            STAR_FIXTURE_FORMAT,
            Codec::NTriples,
            None,
        )
        .expect("direct ntriples → ntriples");

        // Compare via parsed dataset quad/reifier counts (canonical content comparison).
        let ds_a = dataset_from_bytes(&nt_out.bytes, RdfFormat::NTriples).expect("parse a");
        let ds_b = dataset_from_bytes(&direct_nq.bytes, RdfFormat::NTriples).expect("parse b");
        assert_eq!(
            ds_a.quad_count(),
            ds_b.quad_count(),
            "GTS round-trip must preserve quad count"
        );
        assert_eq!(
            ds_a.reifiers().count(),
            ds_b.reifiers().count(),
            "GTS round-trip must preserve reifier count"
        );
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
    fn logic_to_nemo_succeeds_nonempty() {
        let out = transcode(
            LOGIC_FIXTURE_TTL.as_bytes(),
            Codec::Turtle,
            Codec::Nemo,
            None,
        )
        .expect("turtle → nemo");
        assert!(!out.bytes.is_empty(), "nemo output must be non-empty");
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
        let err =
            read_to_dataset(b"anything", Codec::OwlDl).expect_err("expected NonInvertibleSource");
        assert!(
            matches!(err, TranscodeError::NonInvertibleSource(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn read_to_dataset_on_jsonld_star_is_undecodable_error() {
        let err =
            read_to_dataset(b"anything", Codec::JsonLdStar).expect_err("expected UndecodableInput");
        assert!(
            matches!(err, TranscodeError::UndecodableInput(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn read_to_dataset_on_yaml_ld_star_is_undecodable_error() {
        let err =
            read_to_dataset(b"anything", Codec::YamlLdStar).expect_err("expected UndecodableInput");
        assert!(
            matches!(err, TranscodeError::UndecodableInput(_)),
            "got {err:?}"
        );
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
        assert!(matches!(err, TranscodeError::UnknownCodec(_)));
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
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        assert_eq!(
            committed,
            transcode_matrix_json(),
            "generated/transcode-matrix.json is stale; regenerate from transcode_matrix_json()"
        );
    }
}
