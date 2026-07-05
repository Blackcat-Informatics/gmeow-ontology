// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The [`SemafBridge`]: lower a `lang:Denotation` whose `lang:denotationTarget` is a `logic:`
//! formula to an ISO-24617 SemAF / AMR meaning-graph annotation, and a `lang:CommunicativeAct`'s
//! `lang:communicativeForce` to a SemAF dialogue-act label.
//!
//! This projection is PROGRAM-DEPENDENT and DISCLOSED, exactly as the Nemo projection is: the
//! preservation is whatever the SPECIFIC denotation supports, judged per emission, never treated
//! as the meaning itself. Only a `lang:denotesLogicFormula` denotation has a meaning-graph form
//! (AMR is a coarse predicate-argument surface); a denotation of any other kind — a type, a
//! query, an entity, a description — has NO AMR target and is emitted as `logic:Unsupported`
//! with the reason enumerated, never a fabricated best-effort graph. Even for the formula case,
//! AMR has no quantifier scope, no modality depth, and no vantage, and its role inventory is
//! coarser than the full FOL predicate-argument structure — so the judgment is SoundUnder and
//! those gaps are enumerated on every emission.

use gmeow_logic_compile::ir::PreservationKind;
use gmeow_logic_compile::projections::ProjectionResult;
use purrdf::{RdfDataset, TermId};

use crate::bridge::IngestDiagnostic;
use crate::emit::digest16;
use crate::rdf_scan::{
    iri_of, label_of, local_name, lossy_lens_correspondence, object_iri, object_literal, objects,
    parse_lang_turtle, subjects_with_object, term_label, unrepresentable, EXAMPLE_BASE, LANG_NS,
    LOGIC_NS,
};
use crate::registry::{
    EmittedArtifact, LangEmission, LangProjectionInput, LangProjectionTarget, NamedSource,
};

/// The `logic:getLeg` program IRI: lower a logic-formula denotation into an AMR/SemAF graph.
const SEMAF_GET_LEG: &str = "https://blackcatinformatics.ca/lang/semafLowerLeg";
/// The content-address base of the carried SemAF correspondence.
const SEMAF_CORR_BASE: &str = "http://example.org/lang/semaf-correspondence/";

/// The structure AMR/SemAF cannot carry — enumerated on every meaning-graph emission.
const SEMAF_UNSUPPORTED: &[&str] = &[
    "AMR has no quantifier scope: quantifier scoping in the logic: formula is not carried",
    "AMR has no modality depth: modal operators in the denotation collapse",
    "AMR / SemAF carry no vantage: perspectival / standpoint support is dropped",
    "SemAF role inventory is coarser than the full logic: predicate-argument structure",
];

/// A parsed denotation ready to judge and (where lowerable) project.
struct Denotation {
    iri: String,
    /// The `lang:denotationKind` local name (`denotesLogicFormula`, `denotesQuery`, …).
    kind: String,
    /// The `lang:denotationTarget` IRI (a `logic:` formula for the lowerable case).
    target: Option<String>,
    /// The denoted form's label, for the AMR `::snt` header.
    form_label: Option<String>,
    /// Whether the denotation is indexical (`lang:isIndexical` = true) — indexicality is not
    /// carried into a context-free AMR graph.
    indexical: bool,
    /// The SemAF dialogue-act label derived from a `lang:CommunicativeAct` performed on the
    /// denoted form, where one is declared.
    dialogue_act: Option<String>,
}

/// The SemAF / AMR meaning-annotation projection target.
pub struct SemafBridge;

impl LangProjectionTarget for SemafBridge {
    fn name(&self) -> &'static str {
        "semaf"
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
    for den in crate::rdf_scan::subjects_of_type(&ds, &format!("{LANG_NS}Denotation")) {
        let parsed = parse_denotation(&ds, den)?;
        emissions.push(emit_denotation(source, &parsed));
    }
    Ok(emissions)
}

/// Parse one `lang:Denotation`, or HARD FAIL if it declares no kind (an unkinded denotation
/// cannot be judged, and a silent skip is forbidden).
fn parse_denotation(ds: &RdfDataset, den: TermId) -> Result<Denotation, IngestDiagnostic> {
    let iri = iri_of(ds, den).unwrap_or_else(|| {
        format!(
            "{EXAMPLE_BASE}semaf/blank/{}",
            digest16("lang-semaf-blank", &term_label(ds, den))
        )
    });
    let kind = object_iri(ds, den, &format!("{LANG_NS}denotationKind"))
        .map(|k| local_name(&k).to_owned())
        .ok_or_else(|| {
            unrepresentable(format!(
                "lang:Denotation {} has no lang:denotationKind; a meaning assignment cannot be \
                 judged for projection without its kind",
                term_label(ds, den)
            ))
        })?;
    let target = object_iri(ds, den, &format!("{LANG_NS}denotationTarget"));
    let denoted = objects(ds, den, &format!("{LANG_NS}denotedForm"))
        .into_iter()
        .next();
    let form_label = denoted.and_then(|f| label_of(ds, f));
    let indexical = object_literal(ds, den, &format!("{LANG_NS}isIndexical"))
        .map(|v| v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    // The dialogue act: a lang:CommunicativeAct performed on the denoted form, mapped to a SemAF
    // DiAML dialogue-act class from its lang:communicativeForce.
    let dialogue_act = denoted.and_then(|form| {
        subjects_with_object(ds, &format!("{LANG_NS}performedOn"), form)
            .into_iter()
            .find_map(|act| object_iri(ds, act, &format!("{LANG_NS}communicativeForce")))
            .map(|force| diaml_act(local_name(&force)))
    });

    Ok(Denotation {
        iri,
        kind,
        target,
        form_label,
        indexical,
        dialogue_act,
    })
}

/// Map a `lang:communicativeForce` individual to its ISO-24617 DiAML dialogue-act class. The
/// SemAF classes are coarser than the flat force vocabulary (carried as alignment, not identity).
fn diaml_act(force_local: &str) -> String {
    match force_local {
        "assertForce" => "Inform",
        "askForce" => "Question",
        "orderForce" => "Instruct",
        "promiseForce" => "Commissive",
        "defineForce" => "Inform.Definition",
        _ => "Inform",
    }
    .to_owned()
}

/// Emit one denotation. A `denotesLogicFormula` denotation lowers to an AMR/SemAF graph
/// (SoundUnder); any other kind is `Unsupported` (no AMR target), the reason enumerated. The
/// preservation is JUDGED here, per emission — disclosed, never assumed.
fn emit_denotation(source: &NamedSource, d: &Denotation) -> LangEmission {
    let local = local_name(&d.iri).to_owned();
    let is_formula = d.kind == "denotesLogicFormula";
    // A logic: formula target is required for the lowerable case; a formula-kind denotation whose
    // target is not in the logic: namespace is treated as non-lowerable (disclosed, not faked).
    let target_is_logic = d
        .target
        .as_deref()
        .map(|t| t.starts_with(LOGIC_NS))
        .unwrap_or(false);

    if is_formula && target_is_logic && !d.indexical {
        // Lowerable: emit the coarse AMR graph + SemAF dialogue act; SoundUnder.
        let amr = render_amr(d);
        let corr = lossy_lens_correspondence(
            SEMAF_CORR_BASE,
            &format!("{}\u{1f}{amr}", d.iri),
            SEMAF_GET_LEG,
            None,
        );
        let residue: Vec<String> = SEMAF_UNSUPPORTED.iter().map(|s| (*s).to_owned()).collect();
        LangEmission {
            artifacts: vec![EmittedArtifact {
                path_suffix: format!("semaf/{}.{}.amr", source.name, local),
                bytes: amr.into_bytes(),
                is_rdf: false,
            }],
            correspondence: corr,
            ledger: vec![ProjectionResult {
                target: format!("semaf:{}#{}", source.name, local),
                content: String::new(),
                is_rdf: false,
                preservation: PreservationKind::SoundUnder,
                complexity: "n/a".to_owned(),
                lossy_drops: Vec::new(),
                actual_drops: residue.clone(),
            }],
            leg_pair: None,
            emitted_reading_count: None,
            source_iri: d.iri.clone(),
            unsupported: residue,
            round_trip_holds: false,
            lossy_kind: PreservationKind::SoundUnder,
            source_rdf: Vec::new(),
        }
    } else {
        // Not lowerable: no AMR target for this denotation. Emit no artifact (a best-effort AMR
        // would be fabrication), carry Unsupported, and enumerate exactly why.
        let reason = if is_formula && !target_is_logic {
            format!(
                "lang:Denotation <{}> is lang:denotesLogicFormula but its lang:denotationTarget is \
                 not a logic: formula; no AMR/SemAF meaning graph is well-posed",
                d.iri
            )
        } else if d.indexical {
            format!(
                "lang:Denotation <{}> is indexical (lang:isIndexical true); its referent varies \
                 with a lang:IndexicalAnchor that a context-free AMR graph cannot carry",
                d.iri
            )
        } else {
            format!(
                "lang:Denotation <{}> has lang:denotationKind lang:{} whose target has no AMR/SemAF \
                 meaning-graph form (only lang:denotesLogicFormula lowers)",
                d.iri, d.kind
            )
        };
        let mut residue: Vec<String> = SEMAF_UNSUPPORTED.iter().map(|s| (*s).to_owned()).collect();
        residue.insert(0, reason);
        let corr = lossy_lens_correspondence(SEMAF_CORR_BASE, &d.iri, SEMAF_GET_LEG, None);
        LangEmission {
            artifacts: Vec::new(),
            correspondence: corr,
            ledger: vec![ProjectionResult {
                target: format!("semaf:{}#{}", source.name, local),
                content: String::new(),
                is_rdf: false,
                preservation: PreservationKind::Unsupported,
                complexity: "n/a".to_owned(),
                lossy_drops: Vec::new(),
                actual_drops: residue.clone(),
            }],
            leg_pair: None,
            emitted_reading_count: None,
            source_iri: d.iri.clone(),
            unsupported: residue,
            round_trip_holds: false,
            lossy_kind: PreservationKind::Unsupported,
            source_rdf: Vec::new(),
        }
    }
}

/// Render a coarse deterministic PENMAN AMR fragment for a logic-formula denotation, with the
/// SemAF dialogue-act class in the header. The formula is carried by reference (its IRI), not
/// expanded — AMR is the coarse surface, and the full structure stays in logic:.
fn render_amr(d: &Denotation) -> String {
    let snt = d.form_label.as_deref().unwrap_or("(unlabelled form)");
    let act = d.dialogue_act.as_deref().unwrap_or("Inform");
    let target = d.target.as_deref().unwrap_or("(no target)");
    let mut out = String::new();
    out.push_str(&format!("# ::snt {snt}\n"));
    out.push_str(&format!("# ::semaf-dialogue-act {act}\n"));
    out.push_str(&format!("# ::denotation {}\n", d.iri));
    out.push_str("(a / assert-01\n");
    out.push_str("      :ARG1 (f / logic-formula\n");
    out.push_str(&format!("                  :value \"{target}\"))\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::is_exact_correspondence;

    const FORMULA_DEN: &str = r#"
@prefix lang:  <https://blackcatinformatics.ca/lang/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:    <http://example.org/lang/> .

ex:sent a lang:ComposedForm ; rdfs:label "cats chase mice" .
ex:act a lang:CommunicativeAct ; lang:performedOn ex:sent ; lang:communicativeForce lang:assertForce .
ex:den a lang:Denotation ;
    lang:denotedForm ex:sent ;
    lang:denotationKind lang:denotesLogicFormula ;
    lang:denotationTarget logic:catsChaseMiceFormula ;
    lang:isIndexical false .
"#;

    const QUERY_DEN: &str = r#"
@prefix lang:  <https://blackcatinformatics.ca/lang/> .
@prefix ex:    <http://example.org/lang/> .
ex:q a lang:Denotation ;
    lang:denotationKind lang:denotesQuery ;
    lang:denotationTarget ex:someQuery .
"#;

    fn src(name: &str, ttl: &str) -> NamedSource {
        NamedSource {
            name: name.to_owned(),
            bytes: ttl.as_bytes().to_vec(),
        }
    }

    #[test]
    fn logic_formula_denotation_lowers_to_amr_soundunder() {
        let input = LangProjectionInput {
            lang_models: vec![src("f", FORMULA_DEN)],
            ..Default::default()
        };
        let emissions = SemafBridge.emit(&input).expect("emit");
        assert_eq!(emissions.len(), 1);
        let e = &emissions[0];
        assert_eq!(e.artifacts.len(), 1, "one AMR graph");
        let amr = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();
        assert!(amr.contains("::snt cats chase mice"), "{amr}");
        // The communicative force lowered to a SemAF dialogue act.
        assert!(amr.contains("::semaf-dialogue-act Inform"), "{amr}");
        assert!(amr.contains("catsChaseMiceFormula"), "{amr}");

        // Program-dependent judgment: SoundUnder, never exact.
        assert!(!is_exact_correspondence(&e.correspondence));
        assert_eq!(e.lossy_kind, PreservationKind::SoundUnder);
        // The AMR gaps are enumerated (no scope, no modality, no vantage).
        let joined = e.unsupported.join("\n");
        assert!(joined.contains("no quantifier scope"), "{joined}");
        assert!(joined.contains("no modality depth"), "{joined}");
        assert!(joined.contains("no vantage"), "{joined}");
    }

    #[test]
    fn non_formula_denotation_is_unsupported_with_reason() {
        let input = LangProjectionInput {
            lang_models: vec![src("q", QUERY_DEN)],
            ..Default::default()
        };
        let e = &SemafBridge.emit(&input).expect("emit")[0];
        assert!(
            e.artifacts.is_empty(),
            "no fabricated AMR for a non-formula denotation"
        );
        assert_eq!(e.lossy_kind, PreservationKind::Unsupported);
        assert_eq!(e.ledger[0].preservation, PreservationKind::Unsupported);
        let joined = e.unsupported.join("\n");
        assert!(joined.contains("denotesQuery"), "{joined}");
        assert!(
            joined.contains("only lang:denotesLogicFormula lowers"),
            "{joined}"
        );
    }

    #[test]
    fn emitter_is_byte_reproducible() {
        let input = LangProjectionInput {
            lang_models: vec![src("f", FORMULA_DEN)],
            ..Default::default()
        };
        let a = SemafBridge.emit(&input).expect("a");
        let b = SemafBridge.emit(&input).expect("b");
        assert_eq!(a[0].artifacts[0].bytes, b[0].artifacts[0].bytes);
    }
}
