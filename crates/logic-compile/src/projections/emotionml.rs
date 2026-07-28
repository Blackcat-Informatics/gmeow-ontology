// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The EmotionML correspondence lowering: the affect category + dimension vocabularies
//! → a W3C EmotionML 1.0 XML document (`gmeow-affect.emotionml.xml`).
//!
//! EmotionML is an XML annotation vocabulary, not an RDF surface, so this is a
//! downward *emitter* of GMEOW's own affect categories (`gmeow:EmotionType`) and axes
//! (`gmeow:AppraisalDimension` / `gmeow:CoreAffectDimension`) into
//! `<vocabulary type="category">` / `<vocabulary type="dimension">` blocks, plus a
//! worked `<emotion>` envelope template. It reads nothing external — the enumeration is
//! over the merged-ontology [`DslView`] the correspondence lane already builds — so it
//! needs no settled external RDF namespace.
//!
//! The projection is **many-to-one and lossy by construction**: `gmeow:Emotion`,
//! `gmeow:AffectiveExperience`, `gmeow:Appraisal`, and `gmeow:AffectClassifierOutput` all
//! collapse into a single EmotionML `<emotion>` envelope. That collapse MUST be recorded
//! in the loss ledger — a projection that emits the envelope *without* the loss annotation
//! is a hard fail (the affect design's rule 9). Enforcement is by construction: the
//! emitter attaches the collapse record and asserts [`assert_records_collapse`] before
//! returning, and the guard's negative unit test proves an unrecorded collapse reds.

use crate::ingest::DslView;
use crate::loss_ledger::LossLedger;
use crate::projections::{OverclaimError, ProjectionResult, target_meta};

const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const GM_EMOTION_TYPE: &str = "https://blackcatinformatics.ca/gmeow/EmotionType";
const GM_APPRAISAL_DIMENSION: &str = "https://blackcatinformatics.ca/gmeow/AppraisalDimension";
const GM_CORE_AFFECT_DIMENSION: &str = "https://blackcatinformatics.ca/gmeow/CoreAffectDimension";

const EMOTIONML_NS: &str = "http://www.w3.org/2009/10/emotionml";
use gmeow_ns::GMEOW_NS;

const CATEGORY_SET_ID: &str = "gmeow-emotion-categories";
const DIMENSION_SET_ID: &str = "gmeow-appraisal-dimensions";

/// The COMPUTED worked-`<emotion>` values, projected from the canonical schadenfreude
/// worked instance shipped in the affect base graph (`slices/core/affect/module.ttl`,
/// `gmeow:schadenfreudeIntensity`). The overall
/// metric-tensor intensity `√(xᵀGx)` and every per-core-affect-dimension unit-clamp value
/// are computed by `gmeow-affect` from the schadenfreude vector over the canonical
/// `gmeow:coreAffectGram` — **nothing here is a hand-typed numeric literal**. A fabricated
/// constant in the worked envelope is an affect-honesty defect, so the emitter has no way to
/// synthesize these values itself: the caller MUST compute and supply them.
pub struct WorkedEnvelope {
    /// The overall intensity `√Q` as a fixed-precision decimal string (e.g. `"0.888819"`).
    pub intensity: String,
    /// The computed per-dimension unit-clamp values as `(appraisal-dimension IRI, value)`
    /// (e.g. `(".../dimensionValence", "0.85")`, `(".../dimensionArousal", "0.7")`),
    /// ascending by core-affect axis. The IRI joins against the enumerated dimension
    /// vocabulary to recover the EmotionML `name` attribute.
    pub dimensions: Vec<(String, String)>,
}

/// The artifact + loss-ledger row of the EmotionML lowering.
pub struct EmotionMlLowering {
    /// The `gmeow-affect.emotionml.xml` document (category + dimension vocabularies and a
    /// worked `<emotion>` envelope template).
    pub document: String,
    /// The single loss-ledger row: `SoundUnder`, carrying the many-to-one collapse record.
    pub ledger: Vec<ProjectionResult>,
    /// The loss store the many-to-one collapse record is interned into (keyed by the
    /// `emotionml` target focus). The mappings stage unions it into the single report loss
    /// store so the row's `gmeow:lossyDrop` records read back from the SAME substrate ledger.
    pub loss: LossLedger,
}

/// The IRI local segment (after the last `/` or `#`) — the fallback name when an
/// individual carries no `rdfs:label`.
fn local_name(iri: &str) -> &str {
    iri.rsplit(['/', '#']).next().unwrap_or(iri)
}

/// Escape the five XML metacharacters relevant to attribute values and element text.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// The `(name, iri)` pairs for every individual of `type_iri`; `name` is the individual's
/// `rdfs:label` (falling back to the IRI local segment). Order comes from the caller's
/// sort, so the output is deterministic.
fn labeled_individuals(view: &DslView, type_iri: &str) -> Vec<(String, String)> {
    view.subjects_of_type(type_iri)
        .into_iter()
        .map(|iri| {
            let name = view
                .object_literal(&iri, RDFS_LABEL)
                .unwrap_or_else(|| local_name(&iri).to_owned());
            (name, iri)
        })
        .collect()
}

/// Lower the affect vocabulary to an EmotionML document. `view` is the merged-ontology
/// view the correspondence lane builds (it contains the affect slice's individuals);
/// `worked` carries the COMPUTED schadenfreude worked-envelope values (intensity + per-axis
/// unit-clamp), supplied by the caller because the emitter must not fabricate them.
pub fn lower_emotionml(view: &DslView, worked: &WorkedEnvelope) -> EmotionMlLowering {
    let mut categories = labeled_individuals(view, GM_EMOTION_TYPE);
    categories.sort();
    categories.dedup();

    // A dimension is typed EITHER gmeow:AppraisalDimension OR its gmeow:CoreAffectDimension
    // subtype (the view materializes no subclass closure), so enumerate both and merge.
    let mut dimensions = labeled_individuals(view, GM_APPRAISAL_DIMENSION);
    dimensions.extend(labeled_individuals(view, GM_CORE_AFFECT_DIMENSION));
    dimensions.sort();
    dimensions.dedup();

    let document = render_document(&categories, &dimensions, worked);
    let mut loss = LossLedger::new();
    let row = ledger_row(&mut loss);
    // Enforcement by construction: the store records the many-to-one collapse. A broken
    // emitter that dropped the record would fail here (and its negative unit test).
    assert_records_collapse(&row, &loss)
        .expect("emotionml projection must record its many-to-one collapse (affect rule 9)");

    EmotionMlLowering {
        document,
        ledger: vec![row],
        loss,
    }
}

fn render_document(
    categories: &[(String, String)],
    dimensions: &[(String, String)],
    worked: &WorkedEnvelope,
) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(
        "<!-- EmotionML 1.0 projection of the GMEOW affect vocabulary. Lossy by construction:\n",
    );
    out.push_str(
        "     mode / experience / appraisal / classifier-output all collapse into one <emotion>\n",
    );
    out.push_str(
        "     envelope (the many-to-one loss is recorded in generated/logic/projection-report.ttl).\n",
    );
    out.push_str("     Generated projection — do not edit; author in slices/core/affect. -->\n");
    out.push_str(&format!(
        "<emotionml xmlns=\"{EMOTIONML_NS}\" xmlns:gmeow=\"{GMEOW_NS}\" \
         category-set=\"#{CATEGORY_SET_ID}\" dimension-set=\"#{DIMENSION_SET_ID}\">\n"
    ));

    render_vocabulary(&mut out, "category", CATEGORY_SET_ID, categories);
    render_vocabulary(&mut out, "dimension", DIMENSION_SET_ID, dimensions);

    out.push_str("  <!-- A worked <emotion> envelope: gmeow:Emotion, gmeow:AffectiveExperience,\n");
    out.push_str(
        "       gmeow:Appraisal, and gmeow:AffectClassifierOutput ALL project into one such\n",
    );
    out.push_str("       envelope — the projection is many-to-one and lossy by design. It\n");
    out.push_str(
        "       projects the canonical schadenfreude example: the <intensity> and every\n",
    );
    out.push_str("       <dimension value> are COMPUTED by gmeow-affect from the schadenfreude\n");
    out.push_str("       vector over gmeow:coreAffectGram — never a hand-typed constant. -->\n");
    out.push_str(&format!(
        "  <emotion category-set=\"#{CATEGORY_SET_ID}\" dimension-set=\"#{DIMENSION_SET_ID}\">\n"
    ));
    if let Some((name, _)) = categories.first() {
        out.push_str(&format!("    <category name=\"{}\"/>\n", xml_escape(name)));
    }
    // The overall metric-tensor intensity √(xᵀGx), computed — NOT a literal.
    out.push_str(&format!(
        "    <intensity value=\"{}\"/>\n",
        xml_escape(&worked.intensity)
    ));
    // The per-core-affect-dimension unit-clamp values, computed by gmeow-affect and joined
    // to the enumerated dimension vocabulary (by IRI) to recover the EmotionML name.
    for (dim_iri, value) in &worked.dimensions {
        let name = dimensions
            .iter()
            .find(|(_, iri)| iri == dim_iri)
            .map(|(n, _)| n.as_str())
            .unwrap_or_else(|| local_name(dim_iri));
        out.push_str(&format!(
            "    <dimension name=\"{}\" value=\"{}\"/>\n",
            xml_escape(name),
            xml_escape(value)
        ));
    }
    out.push_str("  </emotion>\n");
    out.push_str("</emotionml>\n");
    out
}

fn render_vocabulary(out: &mut String, vtype: &str, id: &str, items: &[(String, String)]) {
    out.push_str(&format!("  <vocabulary type=\"{vtype}\" id=\"{id}\">\n"));
    for (name, iri) in items {
        out.push_str(&format!(
            "    <item name=\"{}\"><info><gmeow:term>{}</gmeow:term></info></item>\n",
            xml_escape(name),
            xml_escape(iri)
        ));
    }
    out.push_str("  </vocabulary>\n");
}

fn ledger_row(loss: &mut LossLedger) -> ProjectionResult {
    let (kind, complexity, structural) = target_meta("emotionml");
    // The many-to-one collapse is a STRUCTURAL property of the target (it always happens),
    // so it is interned as this target's structural drops from `target_meta` — not per-run
    // actual drops. The rule-9 guard reads them back from the store.
    let structural: Vec<String> = structural.into_iter().map(str::to_owned).collect();
    loss.record_projection_drops("emotionml", kind, &structural, &[]);
    ProjectionResult {
        target: "emotionml".to_owned(),
        // The XML artifact is written by the mappings stage; the row is a preservation /
        // residue record, so its content is empty (as for the other correspondence rows).
        content: String::new(),
        is_rdf: false,
        preservation: kind,
        complexity: complexity.to_owned(),
    }
}

/// Affect hard-fail rule 9: an EmotionML projection collapses four affect families into one
/// `<emotion>` envelope, so its loss-ledger store MUST name that collapse. A store that records
/// the envelope without the loss annotation is a hard fail. The residue is read back from the
/// ONE loss store, keyed by the row's target focus.
pub fn assert_records_collapse(
    row: &ProjectionResult,
    loss: &LossLedger,
) -> Result<(), OverclaimError> {
    let records = loss.projection_drops_for(&row.target).iter().any(|d| {
        d.contains("Emotion")
            && d.contains("AffectiveExperience")
            && d.contains("Appraisal")
            && d.contains("AffectClassifierOutput")
            && d.contains("envelope")
    });
    if records {
        Ok(())
    } else {
        Err(OverclaimError(
            "emotionml projection collapses gmeow:Emotion / AffectiveExperience / Appraisal / \
             AffectClassifierOutput into one EmotionML <emotion> envelope, but its loss ledger \
             does not record the many-to-one collapse (affect hard-fail rule 9)"
                .to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_row_records_the_collapse() {
        let mut loss = LossLedger::new();
        let row = ledger_row(&mut loss);
        assert_records_collapse(&row, &loss).expect("the real row records the collapse");
    }

    #[test]
    fn a_row_missing_the_collapse_record_is_rule9_red() {
        // A row whose target has NO interned drops (an empty loss store) must red: the
        // many-to-one collapse is unrecorded.
        let mut loss = LossLedger::new();
        let row = ledger_row(&mut loss);
        let empty = LossLedger::new();
        assert!(
            assert_records_collapse(&row, &empty).is_err(),
            "rule 9 must red when the many-to-one collapse is unrecorded"
        );
    }

    #[test]
    fn render_emits_both_vocabularies_and_an_envelope() {
        let categories = vec![("anger".to_owned(), format!("{GMEOW_NS}emotionAnger"))];
        let dimensions = vec![
            ("valence".to_owned(), format!("{GMEOW_NS}dimensionValence")),
            ("arousal".to_owned(), format!("{GMEOW_NS}dimensionArousal")),
        ];
        // The COMPUTED schadenfreude worked values (valence 0.7 → 0.85, arousal 0.4 → 0.7,
        // intensity √(79/100) = 0.888819). The real compute + example pin live in the
        // pipeline's `correspondence_lower` test; here they are the fixed expected outputs.
        let worked = WorkedEnvelope {
            intensity: "0.888819".to_owned(),
            dimensions: vec![
                (format!("{GMEOW_NS}dimensionValence"), "0.85".to_owned()),
                (format!("{GMEOW_NS}dimensionArousal"), "0.7".to_owned()),
            ],
        };
        let doc = render_document(&categories, &dimensions, &worked);
        assert!(doc.contains("<vocabulary type=\"category\" id=\"gmeow-emotion-categories\">"));
        assert!(doc.contains("<vocabulary type=\"dimension\" id=\"gmeow-appraisal-dimensions\">"));
        assert!(doc.contains("<item name=\"anger\">"));
        // No fabricated constant: the computed unit-clamp valence + intensity, not "0.5".
        assert!(!doc.contains("value=\"0.5\""));
        assert!(doc.contains("<intensity value=\"0.888819\"/>"));
        assert!(doc.contains("<dimension name=\"valence\" value=\"0.85\"/>"));
        assert!(doc.contains("<dimension name=\"arousal\" value=\"0.7\"/>"));
        assert!(doc.trim_end().ends_with("</emotionml>"));
    }
}
