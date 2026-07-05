// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The [`NifBridge`]: lower a `lang:SurfaceAnchor` (with its declared `lang:offsetSpace` and the
//! `lang:unicodeNormalization` of the surface it anchors) to NIF 2.0 stand-off strings and a
//! W3C Web-Annotation `TextPositionSelector`, for interoperability with annotation tooling.
//!
//! The projection's declared loss is **offset fragility**: an anchor's offsets bind to exactly
//! ONE surface form under exactly ONE offset space and normalization — re-encoding or
//! re-normalizing that text (NFC↔NFD, codepoint↔byte offsets, a different encoding) shifts the
//! span the offsets address, so the anchor no longer locates the same characters. NIF itself
//! leaves this invariant IMPLICIT; this projection makes it EXPLICIT by recording, on every
//! emitted string, which surface and which normalization/offset-space the offsets assume, and
//! by DISCLOSING the fragility in the loss-ledger residue rather than presenting the offsets as
//! encoding-independent. Judgment: SoundUnder — faithful for the surface it names, lossy above.
//!
//! An anchor missing its source, span, or offset space is a HARD FAIL naming the construct (a
//! selector cannot be minted from it), never a silent skip.

use gmeow_logic_compile::ir::PreservationKind;
use gmeow_logic_compile::projections::ProjectionResult;
use purrdf::{RdfDataset, TermId};

use crate::bridge::IngestDiagnostic;
use crate::emit::{digest16, ntriples_sorted};
use crate::rdf_scan::{
    iri_of, local_name, lossy_lens_correspondence, object_iri, object_literal, parse_lang_turtle,
    subjects_with_object, term_label, unrepresentable, EXAMPLE_BASE, LANG_NS,
};
use crate::registry::{
    EmittedArtifact, LangEmission, LangProjectionInput, LangProjectionTarget, NamedSource,
};

/// The NIF 2.0 core namespace.
const NIF_NS: &str = "http://persistence.uni-leipzig.org/nlp2rdf/ontologies/nif-core#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_NNI: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";

/// The `logic:getLeg` program IRI: read a surface anchor into a NIF/OA stand-off selector.
const NIF_GET_LEG: &str = "https://blackcatinformatics.ca/lang/nifAnchorLeg";
/// The `logic:putLeg` program IRI: resolve a NIF selector back to the surface span (carried for
/// provenance only — it is fragile under re-encoding, so the round-trip is never claimed exact).
const NIF_PUT_LEG: &str = "https://blackcatinformatics.ca/lang/nifResolveLeg";
/// The content-address base of the carried NIF correspondence.
const NIF_CORR_BASE: &str = "http://example.org/lang/nif-correspondence/";

/// The strata NIF/OA offsets cannot carry — enumerated per emission.
const NIF_UNSUPPORTED: &[&str] = &[
    "the lang:Form / lang:Sense / lang:Denotation strata above the surface are not carried into \
     NIF (NIF addresses spans of a surface, not analyzed forms)",
];

/// A parsed anchor ready to project: its span, offset space, source, and the surface it locates.
struct Anchor {
    /// The anchor's own IRI (or a content-address for a blank node).
    iri: String,
    /// The source document/blob the offsets index into (`lang:anchorSource`).
    source: String,
    start: i64,
    end: i64,
    /// The `lang:offsetSpace` local name (`byteOffset`, `codepointOffset`, `graphemeClusterOffset`).
    offset_space: String,
    /// The owning surface form's IRI, where one anchors it through `lang:surfaceAnchor`.
    surface: Option<String>,
    /// The owning surface's text, where present.
    surface_text: Option<String>,
    /// The owning surface's `lang:unicodeNormalization`, where present.
    normalization: Option<String>,
}

/// The NIF / Web-Annotation stand-off anchoring target.
pub struct NifBridge;

impl LangProjectionTarget for NifBridge {
    fn name(&self) -> &'static str {
        "nif"
    }

    fn emit(&self, input: &LangProjectionInput) -> Result<Vec<LangEmission>, IngestDiagnostic> {
        let mut emissions = Vec::new();
        for source in &input.lang_models {
            emissions.extend(emit_source(source)?);
        }
        Ok(emissions)
    }
}

fn emit_source(source: &NamedSource) -> Result<Vec<LangEmission>, IngestDiagnostic> {
    let ds = parse_lang_turtle(&source.bytes, &source.name)?;
    let mut emissions = Vec::new();
    for anchor in crate::rdf_scan::subjects_of_type(&ds, &format!("{LANG_NS}SurfaceAnchor")) {
        let parsed = parse_anchor(&ds, anchor)?;
        emissions.push(emit_anchor(source, &parsed));
    }
    Ok(emissions)
}

/// Parse one `lang:SurfaceAnchor` into an [`Anchor`], or HARD FAIL naming the missing component.
fn parse_anchor(ds: &RdfDataset, anchor: TermId) -> Result<Anchor, IngestDiagnostic> {
    let iri = iri_of(ds, anchor).unwrap_or_else(|| {
        format!(
            "{EXAMPLE_BASE}nif/blank/{}",
            digest16("lang-nif-blank", &term_label(ds, anchor))
        )
    });
    let source = object_iri(ds, anchor, &format!("{LANG_NS}anchorSource")).ok_or_else(|| {
        unrepresentable(format!(
            "lang:SurfaceAnchor {} has no lang:anchorSource; a NIF referenceContext cannot be minted",
            term_label(ds, anchor)
        ))
    })?;
    let start = int_component(ds, anchor, "anchorStart")?;
    let end = int_component(ds, anchor, "anchorEnd")?;
    let offset_space = object_iri(ds, anchor, &format!("{LANG_NS}offsetSpace"))
        .map(|iri| local_name(&iri).to_owned())
        .ok_or_else(|| {
            unrepresentable(format!(
                "lang:SurfaceAnchor {} has no lang:offsetSpace; the offsets are dimensionless \
                 without a declared unit",
                term_label(ds, anchor)
            ))
        })?;

    // The owning surface, via the inverse of lang:surfaceAnchor — the surface whose text and
    // normalization the offsets assume.
    let owner = subjects_with_object(ds, &format!("{LANG_NS}surfaceAnchor"), anchor)
        .into_iter()
        .next();
    let (surface, surface_text, normalization) = match owner {
        Some(s) => (
            iri_of(ds, s),
            object_literal(ds, s, &format!("{LANG_NS}surfaceText")),
            object_literal(ds, s, &format!("{LANG_NS}unicodeNormalization")),
        ),
        None => (None, None, None),
    };

    Ok(Anchor {
        iri,
        source,
        start,
        end,
        offset_space,
        surface,
        surface_text,
        normalization,
    })
}

/// Read a required integer span component (`anchorStart` / `anchorEnd`) or HARD FAIL.
fn int_component(ds: &RdfDataset, anchor: TermId, local: &str) -> Result<i64, IngestDiagnostic> {
    let lex = object_literal(ds, anchor, &format!("{LANG_NS}{local}")).ok_or_else(|| {
        unrepresentable(format!(
            "lang:SurfaceAnchor {} has no lang:{local}; a span cannot be located without it",
            term_label(ds, anchor)
        ))
    })?;
    lex.trim().parse().map_err(|_| {
        unrepresentable(format!(
            "lang:{local} '{lex}' on {} is not an integer",
            term_label(ds, anchor)
        ))
    })
}

/// Emit one anchor as a NIF string (RDF) plus a Web-Annotation JSON-LD companion, DISCLOSING the
/// offset fragility in the ledger residue.
fn emit_anchor(source: &NamedSource, a: &Anchor) -> LangEmission {
    let local = local_name(&a.iri).to_owned();
    let nif_iri = format!("{EXAMPLE_BASE}nif/{}", digest16("lang-nif-string", &a.iri));

    // The NIF stand-off string: begin/end index, reference context, and — the invariant NIF
    // itself leaves implicit — the offset space and the normalization the offsets assume.
    let mut lines = vec![
        format!("<{nif_iri}> <{RDF_TYPE}> <{NIF_NS}String> ."),
        format!("<{nif_iri}> <{RDF_TYPE}> <{NIF_NS}OffsetBasedString> ."),
        format!(
            "<{nif_iri}> <{NIF_NS}beginIndex> \"{}\"^^<{XSD_NNI}> .",
            a.start
        ),
        format!(
            "<{nif_iri}> <{NIF_NS}endIndex> \"{}\"^^<{XSD_NNI}> .",
            a.end
        ),
        format!("<{nif_iri}> <{NIF_NS}referenceContext> <{}> .", a.source),
        // The declared binding: which offset space the indices are measured in.
        format!(
            "<{nif_iri}> <{LANG_NS}offsetSpace> <{LANG_NS}{}> .",
            a.offset_space
        ),
    ];
    if let Some(text) = &a.surface_text {
        lines.push(format!(
            "<{nif_iri}> <{NIF_NS}anchorOf> {} .",
            nt_literal(text)
        ));
    }
    if let Some(surface) = &a.surface {
        // Record WHICH surface these offsets address (NIF leaves this implicit).
        lines.push(format!("<{nif_iri}> <{LANG_NS}realizes> <{surface}> ."));
    }
    if let Some(norm) = &a.normalization {
        lines.push(format!(
            "<{nif_iri}> <{LANG_NS}unicodeNormalization> {} .",
            nt_literal(norm)
        ));
    }
    let nif_bytes = ntriples_sorted(lines);

    let anno_iri = format!(
        "{EXAMPLE_BASE}nif/anno/{}",
        digest16("lang-nif-anno", &a.iri)
    );
    let anno_bytes = render_web_annotation(&anno_iri, a).into_bytes();

    // The DISCLOSED offset-fragility residue: exactly which surface / offset-space / normalization
    // these offsets assume, and that re-encoding invalidates them.
    let norm = a.normalization.as_deref().unwrap_or("unstated");
    let surface = a.surface.as_deref().unwrap_or("(no owning surface form)");
    let fragility = format!(
        "offset fragility (declared): NIF/OA offsets [{start},{end}) for anchor <{iri}> bind to \
         surface <{surface}> under offsetSpace=lang:{space}, unicodeNormalization={norm}; \
         re-encoding or re-normalizing that surface (NFC↔NFD, codepoint↔byte, a different \
         encoding) shifts the addressed span so the offsets no longer address the same characters \
         — this invalidation is by design, not carried into NIF",
        start = a.start,
        end = a.end,
        iri = a.iri,
        space = a.offset_space,
    );
    let mut residue: Vec<String> = NIF_UNSUPPORTED.iter().map(|s| (*s).to_owned()).collect();
    residue.push(fragility);
    residue.sort();
    residue.dedup();

    let corr = lossy_lens_correspondence(
        NIF_CORR_BASE,
        &format!("{}\u{1f}{}\u{1f}{}", a.iri, a.start, a.end),
        NIF_GET_LEG,
        Some(NIF_PUT_LEG),
    );

    LangEmission {
        artifacts: vec![
            EmittedArtifact {
                path_suffix: format!("nif/{}.{}.nif.nt", source.name, local),
                bytes: nif_bytes,
                is_rdf: true,
            },
            EmittedArtifact {
                path_suffix: format!("nif/{}.{}.anno.jsonld", source.name, local),
                bytes: anno_bytes,
                is_rdf: false,
            },
        ],
        correspondence: corr,
        ledger: vec![ProjectionResult {
            target: format!("nif:{}#{}", source.name, local),
            content: String::new(),
            is_rdf: true,
            preservation: PreservationKind::SoundUnder,
            complexity: "n/a".to_owned(),
            lossy_drops: Vec::new(),
            actual_drops: residue.clone(),
        }],
        leg_pair: None,
        emitted_reading_count: None,
        source_iri: a.iri.clone(),
        unsupported: residue,
        round_trip_holds: false,
        lossy_kind: PreservationKind::SoundUnder,
        source_rdf: Vec::new(),
    }
}

/// Render a deterministic W3C Web-Annotation JSON-LD document with a `TextPositionSelector`
/// (and a `TextQuoteSelector` where the surface text is known). Hand-built with a fixed key
/// order so the bytes are reproducible; the offset space is carried in an `x-offset-space`
/// field so the position selector is not silently assumed to be code points.
fn render_web_annotation(anno_iri: &str, a: &Anchor) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"@context\": \"http://www.w3.org/ns/anno.jsonld\",\n");
    out.push_str(&format!("  \"id\": {},\n", json_str(anno_iri)));
    out.push_str("  \"type\": \"Annotation\",\n");
    if let Some(text) = &a.surface_text {
        out.push_str("  \"body\": {\n");
        out.push_str("    \"type\": \"TextualBody\",\n");
        out.push_str("    \"purpose\": \"identifying\",\n");
        out.push_str(&format!("    \"value\": {}\n", json_str(text)));
        out.push_str("  },\n");
    }
    out.push_str("  \"target\": {\n");
    out.push_str(&format!("    \"source\": {},\n", json_str(&a.source)));
    out.push_str("    \"selector\": {\n");
    out.push_str("      \"type\": \"TextPositionSelector\",\n");
    out.push_str(&format!("      \"start\": {},\n", a.start));
    out.push_str(&format!("      \"end\": {},\n", a.end));
    out.push_str(&format!(
        "      \"x-offset-space\": {}\n",
        json_str(&format!("lang:{}", a.offset_space))
    ));
    out.push_str("    }");
    if let Some(text) = &a.surface_text {
        out.push_str(",\n    \"refinedBy\": {\n");
        out.push_str("      \"type\": \"TextQuoteSelector\",\n");
        out.push_str(&format!("      \"exact\": {}\n", json_str(text)));
        out.push_str("    }\n");
    } else {
        out.push('\n');
    }
    out.push_str("  }\n}\n");
    out
}

/// Escape a string as a JSON string literal (control chars per the JSON grammar).
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Escape a string as an N-Triples quoted literal.
fn nt_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::is_exact_correspondence;

    /// A surface anchor over "café" — é is one NFC codepoint but two UTF-8 bytes, so the same
    /// [0,4) span addresses different characters under codepoint vs byte offsets.
    const CAFE: &str = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix ex:   <http://example.org/lang/> .

ex:surf a lang:SurfaceForm ;
    lang:surfaceText "café bar" ;
    lang:unicodeNormalization "NFC" ;
    lang:surfaceAnchor ex:anc .
ex:anc a lang:SurfaceAnchor ;
    lang:anchorSource ex:doc ;
    lang:anchorStart 0 ;
    lang:anchorEnd 4 ;
    lang:offsetSpace lang:codepointOffset .
"#;

    fn source() -> NamedSource {
        NamedSource {
            name: "cafe".to_owned(),
            bytes: CAFE.as_bytes().to_vec(),
        }
    }

    #[test]
    fn anchor_emits_nif_and_web_annotation() {
        let input = LangProjectionInput {
            lang_models: vec![source()],
            ..Default::default()
        };
        let emissions = NifBridge.emit(&input).expect("emit");
        assert_eq!(emissions.len(), 1);
        let e = &emissions[0];
        assert_eq!(
            e.artifacts.len(),
            2,
            "NIF string + Web-Annotation companion"
        );

        let nif = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();
        assert!(e.artifacts[0].is_rdf, "NIF is RDF");
        assert!(nif.contains("beginIndex> \"0\""), "{nif}");
        assert!(nif.contains("endIndex> \"4\""), "{nif}");
        assert!(
            nif.contains("referenceContext> <http://example.org/lang/doc>"),
            "{nif}"
        );
        // The DECLARED binding: which offset space + normalization the offsets assume.
        assert!(
            nif.contains("offsetSpace> <https://blackcatinformatics.ca/lang/codepointOffset>"),
            "{nif}"
        );
        assert!(nif.contains("unicodeNormalization> \"NFC\""), "{nif}");

        let anno = String::from_utf8(e.artifacts[1].bytes.clone()).unwrap();
        assert!(!e.artifacts[1].is_rdf);
        assert!(anno.contains("\"TextPositionSelector\""), "{anno}");
        assert!(anno.contains("\"start\": 0"), "{anno}");
        assert!(anno.contains("\"end\": 4"), "{anno}");

        // Honest preservation: never exact; SoundUnder.
        assert!(!is_exact_correspondence(&e.correspondence));
        assert_eq!(e.lossy_kind, PreservationKind::SoundUnder);
    }

    #[test]
    fn emitter_is_byte_reproducible() {
        let input = LangProjectionInput {
            lang_models: vec![source()],
            ..Default::default()
        };
        let a = NifBridge.emit(&input).expect("a");
        let b = NifBridge.emit(&input).expect("b");
        for i in 0..a[0].artifacts.len() {
            assert_eq!(a[0].artifacts[i].bytes, b[0].artifacts[i].bytes);
        }
    }

    /// The offset-fragility DISCLOSING test: emit the anchor, then re-encode the surface (switch
    /// the offset space from codepoints to bytes) and show the SAME [0,4) span no longer
    /// addresses "café" — and ASSERT the ledger row discloses exactly this re-encoding
    /// invalidation.
    #[test]
    fn offsets_are_fragile_under_reencoding_and_the_ledger_discloses_it() {
        let text = "café bar";
        // Under the anchor's declared codepoint offset space, [0,4) addresses "café".
        let by_codepoint: String = text.chars().take(4).collect();
        assert_eq!(by_codepoint, "café");
        // Re-encode to a BYTE offset space (a different encoding of the same text): [0,4) now
        // addresses only the first 4 bytes — the ASCII `c`, `a`, `f` plus the first byte of é's
        // 2-byte sequence, which is not even a whole character. The offsets no longer address the
        // same span.
        let by_byte = String::from_utf8_lossy(&text.as_bytes()[0..4]);
        assert_ne!(
            by_byte, by_codepoint,
            "re-encoding codepoint→byte must shift the span the offsets address"
        );

        let input = LangProjectionInput {
            lang_models: vec![source()],
            ..Default::default()
        };
        let e = &NifBridge.emit(&input).expect("emit")[0];

        // The ledger row DISCLOSES the fragility (residue mentions re-encoding invalidation).
        let residue = e.ledger[0].actual_drops.join("\n");
        assert!(residue.contains("offset fragility"), "{residue}");
        assert!(
            residue.contains("re-encoding") || residue.contains("re-normalizing"),
            "the ledger must disclose that re-encoding invalidates the offsets: {residue}"
        );
        assert!(residue.contains("codepoint↔byte"), "{residue}");
        // And it names WHICH surface + normalization the offsets assume.
        assert!(residue.contains("unicodeNormalization=NFC"), "{residue}");
    }
}
