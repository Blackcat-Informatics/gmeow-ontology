// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Selected-dictionary record observations; all inputs are explicit, bounded models.

use gmeow_lang_bridge::{
    ConstructCoverageTally, Gmn0Model, Gmn1Document, Gmn1Error, GmnDictionary, QuadCoverage,
    classify_model, gmn0_canonically_equal, gmn1_read, gmn1_write,
};
use purrdf::{RdfDatasetBuilder, RdfLiteral};
use serde::{Deserialize, Serialize};

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
const MATH_NS: &str = "https://blackcatinformatics.ca/math/";
const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const CLASS_GMN_ERR: &str = "https://blackcatinformatics.ca/gmeow/GmnErr";
const CLASS_GMN_PATCH: &str = "https://blackcatinformatics.ca/gmeow/GmnPatch";
const CLASS_GMN_RETRACT: &str = "https://blackcatinformatics.ca/gmeow/GmnRetract";
const PRED_GMN_REPAIR_ID: &str = "https://blackcatinformatics.ca/gmeow/gmnRepairId";
const PRED_GMN_REPAIR_CLASS: &str = "https://blackcatinformatics.ca/gmeow/gmnRepairClass";
const PRED_OCCURRENT_BOUNDARY: &str = "https://blackcatinformatics.ca/logic/occurrentBoundary";

#[derive(Serialize, Deserialize)]
pub(in crate::stages::conformance) struct Records {
    pub sigils: Result<Written, Gmn1Error>,
    pub scoped: Result<Scoped, Gmn1Error>,
    pub process: Result<Process, Gmn1Error>,
}

#[derive(Serialize, Deserialize)]
pub(in crate::stages::conformance) struct Written {
    pub text: String,
    pub canonical_equal: bool,
}

#[derive(Serialize, Deserialize)]
pub(in crate::stages::conformance) struct Scoped {
    pub written: Written,
    pub fallback: Result<bool, Gmn1Error>,
    pub wrong_scope: Result<(), Gmn1Error>,
    pub wrong_fallback_scope: Result<(), Gmn1Error>,
}

#[derive(Serialize, Deserialize)]
pub(in crate::stages::conformance) struct Process {
    pub written: Written,
    pub primary: Option<QuadCoverage>,
    pub glyph_count: usize,
}

fn written(model: &Gmn0Model, dict: &GmnDictionary) -> Result<Written, Gmn1Error> {
    let document = gmn1_write(model, dict)?;
    let reconstructed = gmn1_read(&document, dict)?;
    Ok(Written {
        text: document.text,
        canonical_equal: gmn0_canonically_equal(model, &reconstructed),
    })
}

fn scoped(model: &Gmn0Model, dict: &GmnDictionary) -> Result<Scoped, Gmn1Error> {
    let written = written(model, dict)?;
    let fallback = Gmn1Document::from_text(
        written
            .text
            .replace("s: π", "s: pi")
            .replace("o: +", "o: add")
            .replace("p: ¬", "p: not"),
    );
    let wrong = |token| {
        Gmn1Document::from_text(format!(
            "@gmn{{v: 1, aliases: dict-v3, glyphs: 2}}\n@ℒ{{s: logic__Formula, p: {token}, o: logic__Formula}}\n"
        ))
    };
    Ok(Scoped {
        written,
        fallback: gmn1_read(&fallback, dict).map(|back| gmn0_canonically_equal(model, &back)),
        wrong_scope: gmn1_read(&wrong("+"), dict).map(|_| ()),
        wrong_fallback_scope: gmn1_read(&wrong("add"), dict).map(|_| ()),
    })
}

fn process(model: &Gmn0Model, dict: &GmnDictionary) -> Result<Process, Gmn1Error> {
    let written = written(model, dict)?;
    let classifications = classify_model(model, dict);
    let primary = model
        .quads
        .iter()
        .zip(&classifications)
        .find(|(quad, _)| quad.predicate == format!("{GMEOW_NS}hasState"))
        .map(|(_, coverage)| coverage.clone());
    let mut tally = ConstructCoverageTally::default();
    tally.absorb_classifications(classifications);
    Ok(Process {
        written,
        primary,
        glyph_count: tally.count(gmeow_lang_bridge::Gmn1ConstructCategory::IriGlyph),
    })
}

pub(super) fn record(dict: &GmnDictionary) -> gmeow_errors::Result<Records> {
    Ok(Records {
        sigils: written(&sigil_model()?, dict),
        scoped: scoped(&glyph_model()?, dict),
        process: process(&process_model()?, dict),
    })
}

fn sigil_model() -> gmeow_errors::Result<Gmn0Model> {
    let mut builder = RdfDatasetBuilder::new();
    let rdf_type = builder.intern_iri(RDF_TYPE);
    for (local, class) in [
        ("evidence", format!("{GMEOW_NS}EvidenceSpan")),
        ("standpoint", format!("{GMEOW_NS}Standpoint")),
        ("process", format!("{LOGIC_NS}Process")),
        ("proof", format!("{MATH_NS}Proof")),
        ("defeater", format!("{GMEOW_NS}Defeater")),
        ("modal", format!("{GMEOW_NS}ModalForce")),
    ] {
        let subject = builder.intern_iri(&format!("{GMEOW_NS}{local}SigilProbe"));
        let object = builder.intern_iri(&class);
        builder.push_quad(subject, rdf_type, object, None);
    }
    // The three in-band repair sigils: each probe is a repair-class-typed subject
    // that also names its target id (and, for @err, its failure class), so the
    // writer folds it to its repair sigil rather than to flat `@c` records.
    for (local, class) in [
        ("err", CLASS_GMN_ERR),
        ("patch", CLASS_GMN_PATCH),
        ("retract", CLASS_GMN_RETRACT),
    ] {
        let subject = builder.intern_iri(&format!("{GMEOW_NS}{local}SigilProbe"));
        let object = builder.intern_iri(class);
        builder.push_quad(subject, rdf_type, object, None);
        let repair_id = builder.intern_iri(PRED_GMN_REPAIR_ID);
        let target = builder.intern_literal(RdfLiteral::typed("t1", XSD_STRING));
        builder.push_quad(subject, repair_id, target, None);
    }
    let err_probe = builder.intern_iri(&format!("{GMEOW_NS}errSigilProbe"));
    let repair_class = builder.intern_iri(PRED_GMN_REPAIR_CLASS);
    let failure_class = builder.intern_iri(&format!("{LANG_NS}GmnMalformedNumber"));
    builder.push_quad(err_probe, repair_class, failure_class, None);
    for (local, predicate, object) in [
        (
            "claim",
            format!("{GMEOW_NS}hasState"),
            format!("{GMEOW_NS}State"),
        ),
        (
            "math",
            format!("{MATH_NS}operatorDomain"),
            format!("{MATH_NS}realNumbers"),
        ),
        (
            "lang",
            format!("{LANG_NS}denotedForm"),
            format!("{LANG_NS}Form"),
        ),
        (
            "logic",
            format!("{LOGIC_NS}and"),
            format!("{LOGIC_NS}Formula"),
        ),
    ] {
        let subject = builder.intern_iri(&format!("{GMEOW_NS}{local}Probe"));
        let predicate = builder.intern_iri(&predicate);
        let object = builder.intern_iri(&object);
        builder.push_quad(subject, predicate, object, None);
    }
    let dataset = builder
        .freeze()
        .map_err(|error| super::super::stage_err(&error.to_string()))?;
    Ok(Gmn0Model::from_dataset(&dataset))
}

fn glyph_model() -> gmeow_errors::Result<Gmn0Model> {
    let mut builder = RdfDatasetBuilder::new();
    let pi = builder.intern_iri(&format!("{MATH_NS}pi"));
    let math_predicate = builder.intern_iri(&format!("{MATH_NS}operatorDomain"));
    let addition = builder.intern_iri(&format!("{MATH_NS}Addition"));
    builder.push_quad(pi, math_predicate, addition, None);
    let formula = builder.intern_iri(&format!("{LOGIC_NS}Formula"));
    let not = builder.intern_iri(&format!("{LOGIC_NS}not"));
    let operand = builder.intern_iri(&format!("{LOGIC_NS}AtomicFormula"));
    builder.push_quad(formula, not, operand, None);
    let dataset = builder
        .freeze()
        .map_err(|error| super::super::stage_err(&error.to_string()))?;
    Ok(Gmn0Model::from_dataset(&dataset))
}

fn process_model() -> gmeow_errors::Result<Gmn0Model> {
    let mut builder = RdfDatasetBuilder::new();
    let subject = builder.intern_iri(&format!("{GMEOW_NS}processCoverageProbe"));
    let predicate = builder.intern_iri(&format!("{GMEOW_NS}hasState"));
    let addition = builder.intern_iri(&format!("{MATH_NS}Addition"));
    builder.push_quad(subject, predicate, addition, None);

    let boundary = builder.intern_iri(PRED_OCCURRENT_BOUNDARY);
    let open = builder.intern_iri(&format!("{LOGIC_NS}Open"));
    builder.push_quad(subject, boundary, open, None);

    let dataset = builder
        .freeze()
        .map_err(|error| super::super::stage_err(&error.to_string()))?;
    Ok(Gmn0Model::from_dataset(&dataset))
}
