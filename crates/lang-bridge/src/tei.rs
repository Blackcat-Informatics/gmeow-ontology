// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The [`TeiBridge`]: lower a document-scale `lang:ComposedForm` (and, where present, the
//! `lang:Rendering` that realizes it) to a TEI P5 XML document for scholarly interchange.
//!
//! TEI carries constituent STRUCTURE and some analysis — a sentence's word tokens in order,
//! its analyzed head — but has no element for the strata above the form layer: the
//! `lang:Denotation` meaning records, the co-resident `lang:Reading` alternatives of an
//! ambiguous form, the `logic:preservationKind` judgments, and the perspectival `lang:vantage`
//! support all drop. So the projection is a LOSSY LENS (`logic:LossyLens`, never `Exact`): it
//! carries a `logic:Correspondence` whose `GetPut` law is `ObligationUnknown`, folds one honest
//! `ProjectionResult` per document with the dropped strata as residue, and ENUMERATES every
//! stratum it cannot carry as `unsupported` — a faithful FRAGMENT, total honesty.
//!
//! Like every projection FROM the `lang:` model, the lift reads the model through the shared
//! [`crate::rdf_scan`] surface and never re-implements the scan.

use gmeow_logic_compile::ir::PreservationKind;
use gmeow_logic_compile::projections::ProjectionResult;
use purrdf::{RdfDataset, TermId};

use crate::bridge::IngestDiagnostic;
use crate::emit::digest16;
use crate::rdf_scan::{
    label_of, local_name, lossy_lens_correspondence, object_iri, object_literal, objects,
    parse_lang_turtle, term_label, EXAMPLE_BASE, LANG_NS,
};
use crate::registry::{
    EmittedArtifact, LangEmission, LangProjectionInput, LangProjectionTarget, NamedSource,
};

/// The `logic:getLeg` program IRI: read a composed form's constituent structure into TEI.
const TEI_GET_LEG: &str = "https://blackcatinformatics.ca/lang/teiProjectLeg";
/// The content-address base of the carried TEI correspondence.
const TEI_CORR_BASE: &str = "http://example.org/lang/tei-correspondence/";

/// Every stratum above the form layer that TEI has no element for — enumerated per emission so
/// the loss is carried and flagged, never hidden in a footnote.
const TEI_DROPPED_STRATA: &[&str] = &[
    "lang:Denotation meaning records (form→referent assignments) have no TEI element",
    "co-resident lang:Reading alternatives of an ambiguous form collapse to a single TEI \
     constituent structure",
    "logic:preservationKind / preservation judgments have no TEI target",
    "lang:vantage / perspectival standpoint support is not carried into TEI",
    "lang:Sense inventory (the middle Frege corner) has no TEI element",
];

/// The TEI document-surface projection target: lowers document-scale `lang:ComposedForm`
/// individuals from the composed model to TEI P5 XML. Lossy (SoundUnder).
pub struct TeiBridge;

impl LangProjectionTarget for TeiBridge {
    fn name(&self) -> &'static str {
        "tei"
    }

    fn emit(&self, input: &LangProjectionInput) -> Result<Vec<LangEmission>, IngestDiagnostic> {
        let mut emissions = Vec::new();
        for source in &input.lang_models {
            emissions.extend(emit_source(source)?);
        }
        Ok(emissions)
    }
}

/// One emission per `lang:ComposedForm` in a single `lang:` RDF surface.
fn emit_source(source: &NamedSource) -> Result<Vec<LangEmission>, IngestDiagnostic> {
    let ds = parse_lang_turtle(&source.bytes, &source.name)?;
    let mut emissions = Vec::new();
    for composed in crate::rdf_scan::subjects_of_type(&ds, &format!("{LANG_NS}ComposedForm")) {
        emissions.push(emit_composed_form(&ds, source, composed)?);
    }
    Ok(emissions)
}

/// Lower one composed form to a TEI document, or HARD FAIL naming the construct.
fn emit_composed_form(
    ds: &RdfDataset,
    source: &NamedSource,
    composed: TermId,
) -> Result<LangEmission, IngestDiagnostic> {
    let source_iri = crate::rdf_scan::iri_of(ds, composed).unwrap_or_else(|| {
        format!(
            "{EXAMPLE_BASE}tei/blank/{}",
            digest16("lang-tei-blank", &term_label(ds, composed))
        )
    });
    let sentence = label_of(ds, composed).unwrap_or_default();
    let head = object_iri(ds, composed, &format!("{LANG_NS}formHead"));

    // The constituent tokens, ordered by lang:slotIndex (constituent order is identity-bearing,
    // so a non-integer or duplicate index is a hard fail, never a silent reorder).
    let mut tokens: Vec<(i64, String, bool)> = Vec::new();
    for slot in objects(ds, composed, &format!("{LANG_NS}formSlot")) {
        let idx_lex =
            object_literal(ds, slot, &format!("{LANG_NS}slotIndex")).ok_or_else(|| {
                crate::rdf_scan::unrepresentable(format!(
                "lang:FormSlot {} on composed form {} has no lang:slotIndex; TEI word order is \
                 the slot-index order and cannot be inferred",
                term_label(ds, slot),
                term_label(ds, composed)
            ))
            })?;
        let idx: i64 = idx_lex.trim().parse().map_err(|_| {
            crate::rdf_scan::unrepresentable(format!(
                "lang:slotIndex '{idx_lex}' on {} is not an integer",
                term_label(ds, slot)
            ))
        })?;
        let form = objects(ds, slot, &format!("{LANG_NS}slotForm"))
            .into_iter()
            .next();
        let (text, is_head) = match form {
            Some(f) => (
                label_of(ds, f).unwrap_or_default(),
                head.as_deref() == crate::rdf_scan::iri_of(ds, f).as_deref(),
            ),
            None => (String::new(), false),
        };
        tokens.push((idx, text, is_head));
    }
    tokens.sort_by_key(|(idx, _, _)| *idx);
    // Constituent order is identity-bearing: a duplicate lang:slotIndex is ambiguous word order
    // and a HARD FAIL (as the doc promises), never a silently-serialized token order.
    if let Some(dup) = tokens.windows(2).find(|w| w[0].0 == w[1].0) {
        return Err(crate::rdf_scan::unrepresentable(format!(
            "duplicate lang:slotIndex {} among the slots of composed form {} — TEI word order is \
             the slot-index order and a repeated index is ambiguous",
            dup[0].0,
            term_label(ds, composed)
        )));
    }

    let local = local_name(&source_iri).to_owned();
    let xml = render_tei(&sentence, &tokens, &source_iri);

    let mut residue: Vec<String> = TEI_DROPPED_STRATA.iter().map(|s| (*s).to_owned()).collect();
    // Concrete per-document residue: the analyzed head TEI carries only as a @function hint,
    // and any dependency edges among slots are not carried into the flat <s> token run.
    if head.is_some() {
        residue.push(format!(
            "dependency relations among the constituents of composed form <{source_iri}> are not \
             carried into the flat TEI <s> token run (only the analyzed head is hinted)"
        ));
    }
    residue.sort();
    residue.dedup();

    let corr = lossy_lens_correspondence(
        TEI_CORR_BASE,
        &format!("{source_iri}\u{1f}{xml}"),
        TEI_GET_LEG,
        None,
    );

    Ok(LangEmission {
        artifacts: vec![EmittedArtifact {
            path_suffix: format!("tei/{}.{}.tei.xml", source.name, local),
            bytes: xml.into_bytes(),
            is_rdf: false,
        }],
        correspondence: corr,
        ledger: vec![ProjectionResult {
            target: format!("tei:{}#{}", source.name, local),
            content: String::new(),
            is_rdf: false,
            preservation: PreservationKind::SoundUnder,
            complexity: "n/a".to_owned(),
            lossy_drops: Vec::new(),
            actual_drops: residue.clone(),
        }],
        leg_pair: None,
        emitted_reading_count: None,
        source_iri,
        unsupported: residue,
        round_trip_holds: false,
        lossy_kind: PreservationKind::SoundUnder,
        source_rdf: Vec::new(),
    })
}

/// Render a composed form to a deterministic TEI P5 document: a `<teiHeader>` naming the
/// projected source, and a `<text><body><s>` carrying one `<w>` per constituent in slot order
/// (the analyzed head marked `function="head"`). No clock, no randomness — a pure function of
/// the form.
fn render_tei(sentence: &str, tokens: &[(i64, String, bool)], source_iri: &str) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<TEI xmlns=\"http://www.tei-c.org/ns/1.0\">\n");
    out.push_str("  <teiHeader>\n    <fileDesc>\n      <titleStmt>\n        <title>");
    out.push_str(&xml_escape(sentence));
    out.push_str("</title>\n      </titleStmt>\n      <sourceDesc>\n        <p>Projected from lang:ComposedForm ");
    out.push_str(&xml_escape(source_iri));
    out.push_str("</p>\n      </sourceDesc>\n    </fileDesc>\n  </teiHeader>\n");
    out.push_str("  <text>\n    <body>\n      <s>\n");
    if tokens.is_empty() {
        // No analyzed constituents: carry the sentence surface as the segment text.
        out.push_str("        ");
        out.push_str(&xml_escape(sentence));
        out.push('\n');
    } else {
        for (idx, text, is_head) in tokens {
            if *is_head {
                out.push_str(&format!(
                    "        <w n=\"{idx}\" function=\"head\">{}</w>\n",
                    xml_escape(text)
                ));
            } else {
                out.push_str(&format!(
                    "        <w n=\"{idx}\">{}</w>\n",
                    xml_escape(text)
                ));
            }
        }
    }
    out.push_str("      </s>\n    </body>\n  </text>\n</TEI>\n");
    out
}

/// Escape the five XML predefined entities so the emitted TEI is well-formed for any surface
/// text.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::is_exact_correspondence;

    const DOC: &str = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .

ex:sent a lang:ComposedForm ;
    rdfs:label "cats chase mice" ;
    lang:formHead ex:wChase ;
    lang:formSlot ex:s0 , ex:s1 , ex:s2 .
ex:s0 a lang:FormSlot ; lang:slotIndex 0 ; lang:slotForm ex:wCats .
ex:s1 a lang:FormSlot ; lang:slotIndex 1 ; lang:slotForm ex:wChase .
ex:s2 a lang:FormSlot ; lang:slotIndex 2 ; lang:slotForm ex:wMice .
ex:wCats  rdfs:label "cats" .
ex:wChase rdfs:label "chase" .
ex:wMice  rdfs:label "mice" .
"#;

    fn source() -> NamedSource {
        NamedSource {
            name: "doc".to_owned(),
            bytes: DOC.as_bytes().to_vec(),
        }
    }

    #[test]
    fn composed_form_emits_faithful_tei_fragment() {
        let input = LangProjectionInput {
            lang_models: vec![source()],
            ..Default::default()
        };
        let emissions = TeiBridge.emit(&input).expect("emit");
        assert_eq!(emissions.len(), 1, "one TEI document per composed form");
        let e = &emissions[0];
        let xml = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();

        // The faithful fragment: the tokens in slot order, the analyzed head marked.
        assert!(xml.contains("<w n=\"0\">cats</w>"), "{xml}");
        assert!(
            xml.contains("<w n=\"1\" function=\"head\">chase</w>"),
            "{xml}"
        );
        assert!(xml.contains("<w n=\"2\">mice</w>"), "{xml}");
        assert!(xml.contains("<title>cats chase mice</title>"));
        assert!(e.artifacts[0].path_suffix.ends_with(".tei.xml"));
        assert!(!e.artifacts[0].is_rdf);

        // Honest preservation: the carried correspondence is NEVER exact; SoundUnder.
        assert!(!is_exact_correspondence(&e.correspondence));
        assert_eq!(e.lossy_kind, PreservationKind::SoundUnder);
        assert_eq!(e.ledger[0].preservation, PreservationKind::SoundUnder);

        // Every dropped stratum is enumerated (denotation, readings, preservation, vantage).
        let joined = e.unsupported.join("\n");
        assert!(joined.contains("lang:Denotation"), "{joined}");
        assert!(joined.contains("lang:Reading"), "{joined}");
        assert!(joined.contains("preservation"), "{joined}");
        assert!(joined.contains("vantage"), "{joined}");
    }

    #[test]
    fn emitter_is_byte_reproducible() {
        let input = LangProjectionInput {
            lang_models: vec![source()],
            ..Default::default()
        };
        let a = TeiBridge.emit(&input).expect("a");
        let b = TeiBridge.emit(&input).expect("b");
        assert_eq!(a[0].artifacts[0].bytes, b[0].artifacts[0].bytes);
    }

    #[test]
    fn non_integer_slot_index_hard_fails() {
        let bad = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .
ex:sent a lang:ComposedForm ; rdfs:label "x" ; lang:formSlot ex:s0 .
ex:s0 a lang:FormSlot ; lang:slotIndex "oops" ; lang:slotForm ex:w0 .
ex:w0 rdfs:label "x" .
"#;
        let input = LangProjectionInput {
            lang_models: vec![NamedSource {
                name: "bad".to_owned(),
                bytes: bad.as_bytes().to_vec(),
            }],
            ..Default::default()
        };
        let err = TeiBridge
            .emit(&input)
            .expect_err("non-integer slot index must hard-fail");
        assert!(err.construct.contains("not an integer"), "{err:?}");
    }
}
