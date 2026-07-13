// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The [`OntoLexBridge`]: lift an OntoLex-Lemon lexicon (RDF Turtle) into the `lang:` form
//! AST and its sense inventory.
//!
//! A lexicon is **somebody's claim about a language**, not unattributed fact. So this lift
//! records the SOURCE lexicon as the [`gmeow:vantage`](GMEOW_VANTAGE) that HOLDS every sense
//! it emits: an `ontolex:LexicalSense` becomes a `lang:Sense` carried FROM
//! `self.source_vantage`, never folded flat as if the language itself asserted it. Two
//! lexicons that disagree stay two vantage-held claims.
//!
//! The lift is a LOSSY LENS over the OntoLex source (`logic:LossyLens`). The form AST
//! captures the lexical entries (`ontolex:LexicalEntry` → [`Form::Lexeme`]), their inflected
//! forms (`ontolex:otherForm` → [`Form::WordForm`] with typed [`MorphFeature`]s), and the
//! sense inventory (`ontolex:sense` → `lang:Sense`, each linked to its lexeme and to the
//! `lang:LexicalConcept` it `lang:evokes`). It does NOT capture the human-readable sense
//! GLOSSES (`skos:definition`), which the form AST has no slot for — so those are recorded
//! HONESTLY as residue in the projection loss ledger rather than silently dropped,
//! and the carried correspondence declares [`PreservationKind::SoundUnder`], never `Exact`.
//!
//! OntoLex structure the lift cannot represent (an entry with no canonical form, a form with
//! no or an ambiguous written representation, a non-literal written representation) is a HARD
//! FAIL naming the exact construct — the `lang:SilentIngestDrop` floor a bridge never crosses.
//!
//! Like every `lang:` bridge, the lift CARRIES a `logic:Correspondence` rather than minting a
//! bespoke `lang:` law spine: the honest morphism class ([`MorphismClass::LossyLens`]) and
//! preservation judgment live on the correspondence, decided over the shared law machinery.

use gmeow_lang_form::{Form, MorphFeature, SurfaceForm};
use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceLaw, CorrespondenceRelation, Determinacy, DischargeVerdict,
    LawClaimIr, MorphismClass, MorphismKind, PreservationKind,
};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue, parse_dataset};

use crate::bridge::{Bridge, IngestDiagnostic, LangFailure, Lifted};
use crate::emit::{digest16, ntriples_sorted};
use crate::registry::{
    EmittedArtifact, LangEmission, LangProjectionInput, LangProjectionTarget, NamedSource,
};

/// The `lang:` namespace base, byte-identical to the other `lang:` producers so every
/// `lang:` local name resolves to the same IRI across bridges.
const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";

/// The `gmeow:vantage` property: a sense emitted by this lift is HELD FROM its source
/// lexicon through this edge, so the sense inventory stays the lexicon's perspectival claim
/// rather than an unattributed fact.
const GMEOW_VANTAGE: &str = "https://blackcatinformatics.ca/gmeow/vantage";

/// The OntoLex-Lemon core namespace.
const ONTOLEX_NS: &str = "http://www.w3.org/ns/lemon/ontolex#";

/// The LexInfo namespace — the source of `lexinfo:partOfSpeech` and the morphological
/// feature predicates (`lexinfo:number`, `lexinfo:gender`, …) an `ontolex:otherForm` carries.
const LEXINFO_NS: &str = "http://www.lexinfo.net/ontology/2.0/lexinfo#";

/// The SKOS namespace — an `ontolex:LexicalSense` glosses through `skos:definition`, the one
/// piece of OntoLex structure the form AST flattens (recorded as residue, never dropped).
const SKOS_NS: &str = "http://www.w3.org/2004/02/skos/core#";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

/// The single OntoLex sign-system IRI a lift declares for the forms it projects. A
/// per-language sign system would be more precise; a single constant is the honest
/// declaration for a lexicon whose language is not resolved here.
pub const ONTOLEX_SIGN_SYSTEM: &str =
    "https://blackcatinformatics.ca/lang/sign-system/ontolex-lemon";

/// The example-instance base the minted `lang:` individuals (lexemes, word forms, senses,
/// concepts) live under, matching the base every other `lang:` producer content-addresses
/// its individuals under.
const ONTOLEX_LIFT_BASE: &str = "http://example.org/lang/ontolex-lift/";

/// The example-instance base the carried correspondence IRI lives under.
const ONTOLEX_CORR_BASE: &str = "http://example.org/lang/ontolex-correspondence/";

/// The `logic:getLeg` program IRI: read the OntoLex source into the form view + sense
/// inventory.
const ONTOLEX_GET_LEG: &str = "https://blackcatinformatics.ca/lang/ontolexLiftLeg";

/// The `logic:putLeg` program IRI: re-render the form view back toward OntoLex (best-effort;
/// the gloss complement the get leg dropped cannot be reconstructed, so the round-trip is not
/// claimed exact).
const ONTOLEX_PUT_LEG: &str = "https://blackcatinformatics.ca/lang/ontolexRenderLeg";

/// The ledger target name for the single OntoLex-lift preservation row.
const ONTOLEX_TARGET: &str = "ontolex";

/// The example-instance base every minted OntoLex-Lemon PROJECTION individual (lexicon, entry,
/// form, sense) lives under — the forward `lang: → OntoLex` peer of [`ONTOLEX_LIFT_BASE`].
const ONTOLEX_FORWARD_BASE: &str = "http://example.org/lang/ontolex/";

/// The `logic:getLeg` program IRI for the FORWARD projection: lower the `lang:` lexeme
/// inventory out to an OntoLex-Lemon lexicon (the put leg the charter names).
const ONTOLEX_PROJECT_LEG: &str = "https://blackcatinformatics.ca/lang/ontolexProjectLeg";

/// Hard-fail helper: an OntoLex construct the bridge cannot represent is named exactly, never
/// silently dropped (the `lang:SilentIngestDrop` floor).
fn unrepresentable(construct: impl Into<String>) -> IngestDiagnostic {
    IngestDiagnostic {
        failure_class: LangFailure::SilentIngestDrop,
        construct: construct.into(),
    }
}

/// A short display string for a term (an IRI in angle brackets, else a blank-node marker) —
/// used only to name the offending construct in a hard-fail diagnostic.
fn term_label(ds: &RdfDataset, id: TermId) -> String {
    match ds.resolve(id) {
        TermRef::Iri(iri) => format!("<{iri}>"),
        TermRef::Blank { label, .. } => format!("_:{label}"),
        TermRef::Literal { lexical, .. } => format!("\"{lexical}\""),
        TermRef::Triple { .. } => "<<triple term>>".to_owned(),
    }
}

/// Resolve an IRI to its interned [`TermId`] in `ds`, if the IRI is present at all.
fn iri_id(ds: &RdfDataset, iri: &str) -> Option<TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

/// The object [`TermId`]s of every `(subject, predicate)` quad, in a deterministic order
/// (sorted by their resolved display string, so authoring/interning order is immaterial).
fn objects(ds: &RdfDataset, subject: TermId, predicate_iri: &str) -> Vec<TermId> {
    let Some(pid) = iri_id(ds, predicate_iri) else {
        return Vec::new();
    };
    let mut out: Vec<TermId> = ds
        .quads_for_pattern(Some(subject), Some(pid), None, GraphMatch::Any)
        .map(|q| q.o)
        .collect();
    out.sort_by_cached_key(|&o| term_label(ds, o));
    out.dedup();
    out
}

/// The IRI string of a term, or `None` if it is not an IRI.
fn iri_of(ds: &RdfDataset, id: TermId) -> Option<String> {
    match ds.resolve(id) {
        TermRef::Iri(iri) => Some(iri.to_owned()),
        _ => None,
    }
}

/// The lexical text of a literal term, or `None` if it is not a literal.
fn literal_of(ds: &RdfDataset, id: TermId) -> Option<String> {
    match ds.resolve(id) {
        TermRef::Literal { lexical, .. } => Some(lexical.to_owned()),
        _ => None,
    }
}

/// The local name of an IRI (the segment after the last `#` or `/`) — used to key a lexinfo
/// morphological feature by its bare name (`number`, `gender`, `plural`).
fn local_name(iri: &str) -> &str {
    iri.rsplit(['#', '/']).next().unwrap_or(iri)
}

/// The single written representation of a form node (`ontolex:canonicalForm` /
/// `ontolex:otherForm` target): exactly one `ontolex:writtenRep` literal. Zero, more than
/// one, or a non-literal written representation is a HARD FAIL naming the construct — the
/// form AST's lemma is a single string, so an ambiguous or missing written form cannot be
/// represented and must never be silently coalesced.
fn written_rep(ds: &RdfDataset, form_node: TermId, role: &str) -> Result<String, IngestDiagnostic> {
    let reps = objects(ds, form_node, &format!("{ONTOLEX_NS}writtenRep"));
    match reps.as_slice() {
        [] => Err(unrepresentable(format!(
            "ontolex:{role} {} has no ontolex:writtenRep",
            term_label(ds, form_node)
        ))),
        [one] => literal_of(ds, *one).ok_or_else(|| {
            unrepresentable(format!(
                "ontolex:writtenRep on {} is not a literal ({})",
                term_label(ds, form_node),
                term_label(ds, *one)
            ))
        }),
        _ => Err(unrepresentable(format!(
            "ontolex:{role} {} has {} ontolex:writtenRep values; a single lemma cannot \
             represent an ambiguous written form",
            term_label(ds, form_node),
            reps.len()
        ))),
    }
}

/// Collect the typed morphological features an `ontolex:otherForm` node carries: every lexinfo
/// predicate on the form node (other than `ontolex:writtenRep`) becomes a [`MorphFeature`]
/// keyed by the predicate's local name, its value the object's local name (an IRI value such
/// as `lexinfo:plural`) or lexical form (a literal value). Deterministic: features sort by
/// key, values by value.
fn morph_features(ds: &RdfDataset, form_node: TermId) -> Vec<MorphFeature> {
    let mut by_key: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for q in ds.quads_for_pattern(Some(form_node), None, None, GraphMatch::Any) {
        let Some(pred_iri) = iri_of(ds, q.p) else {
            continue;
        };
        if !pred_iri.starts_with(LEXINFO_NS) {
            continue;
        }
        let value = iri_of(ds, q.o)
            .map(|iri| local_name(&iri).to_owned())
            .or_else(|| literal_of(ds, q.o));
        if let Some(value) = value {
            by_key
                .entry(local_name(&pred_iri).to_owned())
                .or_default()
                .insert(value);
        }
    }
    by_key
        .into_iter()
        .map(|(key, values)| MorphFeature {
            key,
            values: values.into_iter().collect(),
            layer: None,
        })
        .collect()
}

/// A lifted lexical entry: its lexeme form, the inflected word forms, and the sense inventory
/// (each sense with its optional evoked concept and the glosses the form AST flattens).
struct LiftedEntry {
    /// The entry's dictionary-word form.
    lexeme: Form,
    /// The part-of-speech IRI declared through `lexinfo:partOfSpeech`, where present.
    part_of_speech: Option<String>,
    /// The inflected forms (`ontolex:otherForm`), each a [`Form::WordForm`] over the lexeme.
    word_forms: Vec<Form>,
    /// The sense inventory: `(ontolex sense identity, optional evoked concept IRI, glosses)`.
    senses: Vec<LiftedSense>,
}

/// A lifted sense: the OntoLex sense's stable identity, the concept it evokes (where
/// declared), and any `skos:definition` glosses (recorded as residue, never carried as form).
struct LiftedSense {
    /// A stable identity string for the sense (its OntoLex IRI, or a content key for a blank).
    identity: String,
    /// The `lang:LexicalConcept` this sense evokes (`ontolex:evokes`), where declared.
    evokes: Option<String>,
    /// The `skos:definition` gloss texts — dropped from the form view, flagged as residue.
    glosses: Vec<String>,
}

/// Lift one `ontolex:LexicalEntry` subject into its lexeme, word forms, and sense inventory,
/// or HARD FAIL naming the construct the lift cannot represent.
fn lift_entry(ds: &RdfDataset, entry: TermId) -> Result<LiftedEntry, IngestDiagnostic> {
    // The lemma: the single canonical form's single written representation.
    let canonical = objects(ds, entry, &format!("{ONTOLEX_NS}canonicalForm"));
    let canonical = match canonical.as_slice() {
        [] => {
            return Err(unrepresentable(format!(
                "ontolex:LexicalEntry {} has no ontolex:canonicalForm; a lexeme cannot be \
                 lifted without a canonical lemma",
                term_label(ds, entry)
            )));
        }
        [one] => *one,
        _ => {
            return Err(unrepresentable(format!(
                "ontolex:LexicalEntry {} has {} ontolex:canonicalForm values; the canonical \
                 lemma is singular",
                term_label(ds, entry),
                canonical.len()
            )));
        }
    };
    let lemma = written_rep(ds, canonical, "canonicalForm")?;

    // The part of speech: at most one lexinfo:partOfSpeech (an IRI).
    let pos_ids = objects(ds, entry, &format!("{LEXINFO_NS}partOfSpeech"));
    let part_of_speech = match pos_ids.as_slice() {
        [] => None,
        [one] => Some(iri_of(ds, *one).ok_or_else(|| {
            unrepresentable(format!(
                "lexinfo:partOfSpeech on {} is not an IRI ({})",
                term_label(ds, entry),
                term_label(ds, *one)
            ))
        })?),
        _ => {
            return Err(unrepresentable(format!(
                "ontolex:LexicalEntry {} has {} lexinfo:partOfSpeech values; a lexeme declares \
                 a single part of speech",
                term_label(ds, entry),
                pos_ids.len()
            )));
        }
    };

    let lexeme = Form::Lexeme {
        sign_system: ONTOLEX_SIGN_SYSTEM.to_owned(),
        lemma,
        part_of_speech: part_of_speech.clone(),
    };

    // The inflected forms: each ontolex:otherForm's written representation + morph features.
    let mut word_forms = Vec::new();
    for other in objects(ds, entry, &format!("{ONTOLEX_NS}otherForm")) {
        // The written rep is validated (single literal) even though it is not stored on the
        // form: an otherForm with an ambiguous/missing surface is a hard fail, not a silent
        // skip, and the surface itself lives on the SurfaceForm stratum, never on the form.
        let _surface = written_rep(ds, other, "otherForm")?;
        word_forms.push(Form::WordForm {
            sign_system: ONTOLEX_SIGN_SYSTEM.to_owned(),
            lexeme: Box::new(lexeme.clone()),
            features: morph_features(ds, other),
        });
    }

    // The sense inventory: each ontolex:sense, its evoked concept, and its glosses.
    let mut senses = Vec::new();
    for sense in objects(ds, entry, &format!("{ONTOLEX_NS}sense")) {
        let identity = iri_of(ds, sense).unwrap_or_else(|| {
            // A blank sense node keys on the entry + a content-address of its written lemma so
            // it stays stable across runs of the same lexicon.
            format!(
                "{}#sense-{}",
                term_label(ds, entry),
                digest16("lang-ontolex-blank-sense", &lexeme.content_key())
            )
        });
        let evokes = objects(ds, sense, &format!("{ONTOLEX_NS}evokes"))
            .first()
            .and_then(|&c| iri_of(ds, c));
        let mut glosses: Vec<String> = objects(ds, sense, &format!("{SKOS_NS}definition"))
            .into_iter()
            .filter_map(|g| literal_of(ds, g))
            .collect();
        glosses.sort();
        glosses.dedup();
        senses.push(LiftedSense {
            identity,
            evokes,
            glosses,
        });
    }

    Ok(LiftedEntry {
        lexeme,
        part_of_speech,
        word_forms,
        senses,
    })
}

/// The get/put [`LegPath`](gmeow_logic_compile::ir::LegPath) analogue for the OntoLex lift is
/// NOT an exact inverse pair — the lift is a lossy lens whose gloss complement cannot be
/// reconstructed — so no `exact_round_trip_holds` claim is made. The leg IRIs are carried on
/// the correspondence for provenance only.
///
/// Build the LOSSY-LENS `logic:Correspondence` a lift carries for a lexicon whose canonical
/// lift product hashes to `source_key`: a [`MorphismClass::LossyLens`] (the OntoLex→form get
/// is non-injective — distinct lexicons with different glosses map to the same form view), NOT
/// `mnemomorphic` (the whole source is not retained; glosses are shed), whose `GetPut` law is
/// carried forward as [`DischargeVerdict::ObligationUnknown`] — the honest verdict for a law
/// this lift does not discharge. It is therefore never an exact correspondence.
pub fn ontolex_correspondence(source_key: &str) -> Correspondence {
    let iri = format!(
        "{ONTOLEX_CORR_BASE}{}",
        digest16("lang-ontolex-corr", source_key)
    );
    Correspondence::new(
        iri,
        // The form view is a partial, related counterpart of the richer OntoLex source, not
        // an equivalence or a crisp subsumption.
        CorrespondenceRelation::RelatedMatch,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        // The lift sheds the gloss complement, so it does NOT retain the whole source witness.
        false,
        Some(Determinacy::Crisp),
        Some(ONTOLEX_GET_LEG.to_owned()),
        Some(ONTOLEX_PUT_LEG.to_owned()),
        vec![LawClaimIr {
            law: CorrespondenceLaw::GetPut,
            verdict: DischargeVerdict::ObligationUnknown,
            condition: None,
        }],
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("lossy OntoLex correspondence is well-formed by construction")
}

/// Escape a string literal for an N-Triples object (`"..."`): backslash, double-quote, and the
/// line-ending controls, per the N-Triples grammar.
fn escape_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// Render the vantage as an N-Triples object: an absolute IRI (a scheme-bearing source
/// identifier) is emitted as an IRI reference; anything else is a plain string literal, so a
/// human-readable source label is still carried faithfully.
fn vantage_object(source_vantage: &str) -> String {
    if source_vantage.contains("://") {
        format!("<{source_vantage}>")
    } else {
        format!("\"{}\"", escape_literal(source_vantage))
    }
}

/// Render the lifted entries to the deterministic `lang:` N-Triples line set: one
/// `lang:Lexeme` per entry (its lemma label + declared `lang:partOfSpeech`), one
/// `lang:WordForm` per inflected form (linked back through `lang:lexemeOf`), and one
/// `lang:Sense` per sense — each sense HELD FROM `source_vantage` through `gmeow:vantage`,
/// and linked to the `lang:LexicalConcept` it `lang:evokes` where declared. The declared POS
/// is the lexinfo IRI carried verbatim (the lexicon's own assignment), emitted as an IRI.
fn lift_to_lines(entries: &[LiftedEntry], source_vantage: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let vantage = vantage_object(source_vantage);
    for entry in entries {
        let lexeme_iri = format!(
            "{ONTOLEX_LIFT_BASE}lexeme/{}",
            digest16("lang-ontolex-lexeme", &entry.lexeme.content_key())
        );
        lines.push(format!("<{lexeme_iri}> <{RDF_TYPE}> <{LANG_NS}Lexeme> ."));
        if let Form::Lexeme { lemma, .. } = &entry.lexeme {
            lines.push(format!(
                "<{lexeme_iri}> <{RDFS_LABEL}> \"{}\" .",
                escape_literal(lemma)
            ));
        }
        if let Some(pos) = &entry.part_of_speech {
            lines.push(format!("<{lexeme_iri}> <{LANG_NS}partOfSpeech> <{pos}> ."));
        }

        for word_form in &entry.word_forms {
            let wf_iri = format!(
                "{ONTOLEX_LIFT_BASE}wordform/{}",
                digest16("lang-ontolex-wordform", &word_form.content_key())
            );
            lines.push(format!("<{wf_iri}> <{RDF_TYPE}> <{LANG_NS}WordForm> ."));
            lines.push(format!("<{wf_iri}> <{LANG_NS}lexemeOf> <{lexeme_iri}> ."));
        }

        for sense in &entry.senses {
            let sense_iri = format!(
                "{ONTOLEX_LIFT_BASE}sense/{}",
                digest16(
                    "lang-ontolex-sense",
                    &format!("{}\u{1f}{}", entry.lexeme.content_key(), sense.identity)
                )
            );
            lines.push(format!("<{sense_iri}> <{RDF_TYPE}> <{LANG_NS}Sense> ."));
            lines.push(format!("<{sense_iri}> <{LANG_NS}senseOf> <{lexeme_iri}> ."));
            // The sense is HELD FROM its source lexicon: the sense inventory is the lexicon's
            // vantage-held claim, never an unattributed fact.
            lines.push(format!("<{sense_iri}> <{GMEOW_VANTAGE}> {vantage} ."));
            if let Some(concept_iri) = &sense.evokes {
                let concept_lift_iri = format!(
                    "{ONTOLEX_LIFT_BASE}concept/{}",
                    digest16("lang-ontolex-concept", concept_iri)
                );
                lines.push(format!(
                    "<{concept_lift_iri}> <{RDF_TYPE}> <{LANG_NS}LexicalConcept> ."
                ));
                lines.push(format!(
                    "<{sense_iri}> <{LANG_NS}evokes> <{concept_lift_iri}> ."
                ));
            }
        }
    }
    lines
}

/// The concrete gloss residue this lift drops: each `skos:definition` the form view flattens,
/// attributed to the sense it glossed, so the loss ledger names exactly what was shed.
fn gloss_drops(entries: &[LiftedEntry]) -> Vec<String> {
    let mut drops = Vec::new();
    for entry in entries {
        for sense in &entry.senses {
            for gloss in &sense.glosses {
                drops.push(format!(
                    "sense gloss dropped (form AST has no gloss slot): sense '{}' \
                     skos:definition \"{}\"",
                    sense.identity, gloss
                ));
            }
        }
    }
    drops.sort();
    drops.dedup();
    drops
}

/// Parse an OntoLex-Lemon lexicon into the ordered lifted entries, or HARD FAIL. Non-UTF-8
/// bytes are a [`LangFailure::NonUtf8Surface`]; a Turtle syntax error or an OntoLex construct
/// the lift cannot represent is a [`LangFailure::SilentIngestDrop`] naming the construct; a
/// source with no `ontolex:LexicalEntry` at all is a hard fail (an empty "lexicon" is not one).
fn parse_lexicon(bytes: &[u8]) -> Result<Vec<LiftedEntry>, IngestDiagnostic> {
    // A clean non-UTF-8 diagnostic before handing the bytes to the Turtle parser.
    if let Err(e) = std::str::from_utf8(bytes) {
        return Err(IngestDiagnostic {
            failure_class: LangFailure::NonUtf8Surface,
            construct: format!(
                "non-UTF-8 OntoLex input: {} byte(s), first invalid byte at index {}",
                bytes.len(),
                e.valid_up_to()
            ),
        });
    }
    let ds = parse_dataset(bytes, "text/turtle", None)
        .map_err(|e| unrepresentable(format!("OntoLex lexicon does not parse as Turtle: {e}")))?;

    let (Some(type_id), Some(entry_class)) = (
        iri_id(&ds, RDF_TYPE),
        iri_id(&ds, &format!("{ONTOLEX_NS}LexicalEntry")),
    ) else {
        return Err(unrepresentable(
            "OntoLex lexicon declares no ontolex:LexicalEntry".to_owned(),
        ));
    };

    let mut entry_ids: Vec<TermId> = ds
        .quads_for_pattern(None, Some(type_id), Some(entry_class), GraphMatch::Any)
        .map(|q| q.s)
        .collect();
    entry_ids.sort_by_cached_key(|&e| term_label(&ds, e));
    entry_ids.dedup();
    if entry_ids.is_empty() {
        return Err(unrepresentable(
            "OntoLex lexicon declares no ontolex:LexicalEntry".to_owned(),
        ));
    }

    entry_ids
        .into_iter()
        .map(|entry| lift_entry(&ds, entry))
        .collect()
}

/// The OntoLex-Lemon bridge: lift a lexicon into `lang:` lexemes, word forms, and a
/// vantage-held sense inventory under a lossy-lens `logic:Correspondence`. The `source_vantage`
/// is the source lexicon's identifier — the vantage that HOLDS every emitted sense.
pub struct OntoLexBridge {
    /// The source lexicon identifier (an IRI or a label) held as the vantage of the sense
    /// inventory this lift emits.
    pub source_vantage: String,
}

impl OntoLexBridge {
    /// Lift the lexicon to the deterministic N-Triples byte stream: the `lang:Lexeme`,
    /// `lang:WordForm`, and `lang:Sense` triples (each sense carrying `gmeow:vantage`), sorted
    /// and deduped so the same lexicon always serializes byte-identically. HARD FAILS naming
    /// the construct on any OntoLex structure the lift cannot represent.
    pub fn lift_to_ntriples(&self, bytes: &[u8]) -> Result<Vec<u8>, IngestDiagnostic> {
        let entries = parse_lexicon(bytes)?;
        Ok(ntriples_sorted(lift_to_lines(
            &entries,
            &self.source_vantage,
        )))
    }
}

impl Bridge for OntoLexBridge {
    fn lift(&self, bytes: &[u8]) -> Result<Lifted, IngestDiagnostic> {
        let entries = parse_lexicon(bytes)?;

        // The forms: the lexemes and their inflected word forms (the sense inventory is emitted
        // as RDF, not held as form identity — a sense is not a form).
        let mut forms: Vec<Form> = Vec::new();
        for entry in &entries {
            forms.push(entry.lexeme.clone());
            forms.extend(entry.word_forms.iter().cloned());
        }

        // The surfaces: the lemma of each lexeme is its canonical written surface.
        let surfaces: Vec<SurfaceForm> = entries
            .iter()
            .filter_map(|e| match &e.lexeme {
                Form::Lexeme { lemma, .. } => Some(SurfaceForm {
                    text: lemma.clone(),
                    script: "Zyyy".to_owned(),
                    encoding: "UTF-8".to_owned(),
                    normalization: "NFC".to_owned(),
                    collation: "und".to_owned(),
                }),
                _ => None,
            })
            .collect();

        let ntriples = ntriples_sorted(lift_to_lines(&entries, &self.source_vantage));
        let content = String::from_utf8(ntriples).map_err(|e| IngestDiagnostic {
            failure_class: LangFailure::NonUtf8Surface,
            construct: format!(
                "OntoLex N-Triples projection is not UTF-8: first invalid byte at index {}",
                e.utf8_error().valid_up_to()
            ),
        })?;
        let correspondence = ontolex_correspondence(&content);

        // The honest loss ledger: the gloss complement the form view flattens is recorded as
        // residue, and the preservation is SoundUnder (never Exact — the lift drops glosses).
        let actual_drops = gloss_drops(&entries);
        let mut loss = crate::registry::LossLedger::new();
        let ledger = vec![crate::registry::emit_ledger_row(
            &mut loss,
            ONTOLEX_TARGET.to_owned(),
            content,
            true,
            PreservationKind::SoundUnder,
            "n/a".to_owned(),
            vec![
                "sense glosses (skos:definition) are not representable in the form AST".to_owned(),
            ],
            actual_drops,
        )];

        Ok(Lifted {
            forms,
            surfaces,
            correspondence,
            ledger,
            loss,
        })
    }

    fn emit(&self, lifted: &Lifted) -> Vec<u8> {
        // Best-effort re-render off the lifted product: the emitted N-Triples the lift already
        // produced (the gloss complement it shed cannot be reconstructed, so this is a lossy
        // view, not a byte-exact OntoLex round-trip).
        lifted
            .ledger
            .iter()
            .find(|row| row.target == ONTOLEX_TARGET)
            .map(|row| row.content.clone().into_bytes())
            .unwrap_or_default()
    }
}

// ── Forward projection: lang: model → OntoLex-Lemon ─────────────────────────────────

/// Every epistemic stratum OntoLex-Lemon has no slot for — enumerated per emission so the
/// form-view flattening is carried and flagged, never hidden (charter §OntoLex declared loss).
const ONTOLEX_FLATTENED_STRATA: &[&str] = &[
    "lang:vantage / perspectival standpoint has no OntoLex-Lemon target: the sense inventory \
     flattens to unattributed lexical structure",
    "lang:InterpretationAct and co-resident lang:Reading alternatives with held support have no \
     OntoLex-Lemon form",
    "preservation-judged lang:Translation has no OntoLex-Lemon target",
    "lang:denotationKind beyond entity/class reference (lang:denotesLogicFormula and kin) has no \
     ontolex:denotes / ontolex:reference target",
];

/// The OntoLex-Lemon lexical PROJECTION target: lowers the canonical `lang:Lexeme` /
/// `lang:WordForm` / `lang:Sense` inventory in the composed model FORWARD to an OntoLex-Lemon
/// lexicon (the charter's primary lexical projection). A lossy lens (`SoundUnder`): the
/// form/sense/reference structure is faithful, the epistemic layer flattens (enumerated).
///
/// This is the forward peer of [`OntoLexBridge`] (which lifts OntoLex INTO the model for the
/// ingestion/runtime surface); it reads the model through the shared [`crate::rdf_scan`] surface
/// exactly as [`crate::tei`] / [`crate::semaf`] do, and never re-implements the scan.
pub struct OntoLexTarget;

impl LangProjectionTarget for OntoLexTarget {
    fn name(&self) -> &'static str {
        "ontolex-lemon"
    }

    fn emit(&self, input: &LangProjectionInput) -> Result<Vec<LangEmission>, IngestDiagnostic> {
        let mut emissions = Vec::new();
        for source in &input.lang_models {
            if let Some(emission) = emit_lexicon(source)? {
                emissions.push(emission);
            }
        }
        Ok(emissions)
    }
}

/// One OntoLex-Lemon lexicon emission per `lang:` surface that carries ≥1 `lang:Lexeme`; a
/// surface with no lexemes yields `None` (the target simply does not fire for it — no empty
/// lexicon artifact). HARD FAILS naming the construct on model structure it cannot represent.
fn emit_lexicon(source: &NamedSource) -> Result<Option<LangEmission>, IngestDiagnostic> {
    let ds = crate::rdf_scan::parse_lang_turtle(&source.bytes, &source.name)?;
    let lexemes = crate::rdf_scan::subjects_of_type(&ds, &format!("{LANG_NS}Lexeme"));
    if lexemes.is_empty() {
        return Ok(None);
    }

    let mut lines: Vec<String> = Vec::new();
    let mut unmapped: Vec<String> = Vec::new();

    for lex in lexemes {
        let lex_iri = crate::rdf_scan::iri_of(&ds, lex).unwrap_or_else(|| {
            format!(
                "{ONTOLEX_FORWARD_BASE}anon/{}",
                digest16("lang-ontolex-anon", &crate::rdf_scan::term_label(&ds, lex))
            )
        });
        let entry_iri = format!(
            "{ONTOLEX_FORWARD_BASE}entry/{}",
            digest16("lang-ontolex-fwd-entry", &lex_iri)
        );
        let form_iri = format!(
            "{ONTOLEX_FORWARD_BASE}form/{}",
            digest16("lang-ontolex-fwd-canon", &lex_iri)
        );
        let lemma = crate::rdf_scan::label_of(&ds, lex).unwrap_or_default();

        lines.push(format!(
            "<{entry_iri}> <{RDF_TYPE}> <{ONTOLEX_NS}LexicalEntry> ."
        ));
        if !lemma.is_empty() {
            lines.push(format!(
                "<{entry_iri}> <{RDFS_LABEL}> \"{}\" .",
                escape_literal(&lemma)
            ));
        }
        // Part of speech: the UD-aligned lexinfo class where the lang: POS maps, else the lang:
        // POS IRI carried verbatim + enumerated as an unmapped residue (never silently dropped).
        if let Some(pos_iri) =
            crate::rdf_scan::object_iri(&ds, lex, &format!("{LANG_NS}partOfSpeech"))
        {
            match map_pos(crate::rdf_scan::local_name(&pos_iri)) {
                Some(lexinfo_pos) => lines.push(format!(
                    "<{entry_iri}> <{LEXINFO_NS}partOfSpeech> <{LEXINFO_NS}{lexinfo_pos}> ."
                )),
                None => {
                    lines.push(format!(
                        "<{entry_iri}> <{LANG_NS}partOfSpeech> <{pos_iri}> ."
                    ));
                    unmapped.push(format!(
                        "lang:partOfSpeech <{pos_iri}> has no lexinfo class mapping; carried \
                         verbatim, not lowered to a UD-aligned lexinfo:partOfSpeech"
                    ));
                }
            }
        }
        // Canonical form: the lemma surface.
        lines.push(format!(
            "<{entry_iri}> <{ONTOLEX_NS}canonicalForm> <{form_iri}> ."
        ));
        lines.push(format!("<{form_iri}> <{RDF_TYPE}> <{ONTOLEX_NS}Form> ."));
        if !lemma.is_empty() {
            lines.push(format!(
                "<{form_iri}> <{ONTOLEX_NS}writtenRep> \"{}\" .",
                escape_literal(&lemma)
            ));
        }

        // Inflected forms: every lang:WordForm whose lang:inflectionOf is this lexeme, with its
        // morphological features lowered to lexinfo properties where mapped.
        for wf in crate::rdf_scan::subjects_with_object(&ds, &format!("{LANG_NS}inflectionOf"), lex)
        {
            let wf_iri = crate::rdf_scan::iri_of(&ds, wf).unwrap_or_else(|| {
                format!(
                    "{ONTOLEX_FORWARD_BASE}anon-wf/{}",
                    digest16(
                        "lang-ontolex-anon-wf",
                        &crate::rdf_scan::term_label(&ds, wf)
                    )
                )
            });
            let other_iri = format!(
                "{ONTOLEX_FORWARD_BASE}form/{}",
                digest16("lang-ontolex-fwd-other", &wf_iri)
            );
            let wf_label = crate::rdf_scan::label_of(&ds, wf).unwrap_or_default();
            lines.push(format!(
                "<{entry_iri}> <{ONTOLEX_NS}otherForm> <{other_iri}> ."
            ));
            lines.push(format!("<{other_iri}> <{RDF_TYPE}> <{ONTOLEX_NS}Form> ."));
            if !wf_label.is_empty() {
                lines.push(format!(
                    "<{other_iri}> <{ONTOLEX_NS}writtenRep> \"{}\" .",
                    escape_literal(&wf_label)
                ));
            }
            for feat in crate::rdf_scan::objects(&ds, wf, &format!("{LANG_NS}morphFeature")) {
                let key = crate::rdf_scan::object_iri(&ds, feat, &format!("{LANG_NS}featureKey"));
                let val = crate::rdf_scan::object_iri(&ds, feat, &format!("{LANG_NS}featureValue"));
                match (key.as_deref(), val.as_deref()) {
                    (Some(k), Some(v)) => {
                        let kl = crate::rdf_scan::local_name(k);
                        let vl = crate::rdf_scan::local_name(v);
                        match map_feature(kl, vl) {
                            Some((prop, value)) => lines.push(format!(
                                "<{other_iri}> <{LEXINFO_NS}{prop}> <{LEXINFO_NS}{value}> ."
                            )),
                            None => unmapped.push(format!(
                                "lang:MorphFeature {kl}={vl} on word form <{wf_iri}> has no lexinfo \
                                 mapping; not lowered to a UD-aligned lexinfo property"
                            )),
                        }
                    }
                    _ => unmapped.push(format!(
                        "lang:MorphFeature on word form <{wf_iri}> is missing lang:featureKey or \
                         lang:featureValue; not lowered"
                    )),
                }
            }
        }

        // Senses: every lang:Sense whose lang:senseOf is this lexeme, its gloss carried as
        // skos:definition (the forward projection DOES carry the gloss the ingest lift shed).
        for sense in crate::rdf_scan::subjects_with_object(&ds, &format!("{LANG_NS}senseOf"), lex) {
            let sense_src = crate::rdf_scan::iri_of(&ds, sense).unwrap_or_else(|| {
                format!(
                    "{ONTOLEX_FORWARD_BASE}anon-sense/{}",
                    digest16(
                        "lang-ontolex-anon-sense",
                        &crate::rdf_scan::term_label(&ds, sense)
                    )
                )
            });
            let sense_iri = format!(
                "{ONTOLEX_FORWARD_BASE}sense/{}",
                digest16("lang-ontolex-fwd-sense", &sense_src)
            );
            lines.push(format!("<{entry_iri}> <{ONTOLEX_NS}sense> <{sense_iri}> ."));
            lines.push(format!(
                "<{sense_iri}> <{RDF_TYPE}> <{ONTOLEX_NS}LexicalSense> ."
            ));
            if let Some(gloss) = crate::rdf_scan::label_of(&ds, sense) {
                lines.push(format!(
                    "<{sense_iri}> <{SKOS_NS}definition> \"{}\" .",
                    escape_literal(&gloss)
                ));
            }
        }
    }

    unmapped.sort();
    unmapped.dedup();

    let source_iri = format!(
        "{ONTOLEX_FORWARD_BASE}lexicon/{}",
        digest16("lang-ontolex-fwd-lexicon", &source.name)
    );
    let bytes = ntriples_sorted(lines);
    let content_key = format!("{source_iri}\u{1f}{}", String::from_utf8_lossy(&bytes));

    let mut residue: Vec<String> = ONTOLEX_FLATTENED_STRATA
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    residue.extend(unmapped);
    residue.sort();
    residue.dedup();

    let corr = crate::rdf_scan::lossy_lens_correspondence(
        ONTOLEX_CORR_BASE,
        &content_key,
        ONTOLEX_PROJECT_LEG,
        None,
    );

    let mut loss = crate::registry::LossLedger::new();
    Ok(Some(LangEmission {
        artifacts: vec![EmittedArtifact {
            path_suffix: format!("ontolex-lemon/{}.ttl", source.name),
            bytes,
            is_rdf: true,
        }],
        correspondence: corr,
        ledger: vec![crate::registry::emit_ledger_row(
            &mut loss,
            format!("ontolex-lemon:{}", source.name),
            String::new(),
            true,
            PreservationKind::SoundUnder,
            "n/a".to_owned(),
            Vec::new(),
            residue.clone(),
        )],
        loss,
        leg_pair: None,
        emitted_reading_count: None,
        source_iri,
        unsupported: residue,
        round_trip_holds: false,
        lossy_kind: PreservationKind::SoundUnder,
        source_rdf: Vec::new(),
    }))
}

/// Map a `lang:partOfSpeech` local name to its UD-aligned LexInfo class local name, or `None`
/// where no faithful mapping exists (the caller carries the lang: POS verbatim + flags it).
fn map_pos(local: &str) -> Option<&'static str> {
    Some(match local {
        "noun" => "noun",
        "verb" => "verb",
        "adjective" => "adjective",
        "adverb" => "adverb",
        "pronoun" => "pronoun",
        "adposition" => "adposition",
        "determiner" => "determiner",
        "numeral" => "numeral",
        "conjunction" => "conjunction",
        "interjection" => "interjection",
        _ => return None,
    })
}

/// Map a `lang:MorphFeature` (`featureKey` / `featureValue` local names) to a LexInfo
/// (property, value) local-name pair, or `None` where no faithful mapping exists.
fn map_feature(key: &str, value: &str) -> Option<(&'static str, &'static str)> {
    let prop = match key {
        "featNumber" => "number",
        "featTense" => "tense",
        "featGender" => "gender",
        "featPerson" => "person",
        "featCase" => "case",
        _ => return None,
    };
    let val = match (key, value) {
        ("featNumber", "valPlur") => "plural",
        ("featNumber", "valSing") => "singular",
        ("featNumber", "valDual") => "dual",
        ("featTense", "valPres") => "present",
        ("featTense", "valPast") => "past",
        ("featTense", "valFut") => "future",
        ("featGender", "valMasc") => "masculine",
        ("featGender", "valFem") => "feminine",
        ("featGender", "valNeut") => "neuter",
        _ => return None,
    };
    Some((prop, val))
}

#[cfg(test)]
mod forward_tests {
    use super::*;
    use crate::is_exact_correspondence;

    const LEXICON: &str = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .

ex:lexCat a lang:Lexeme ; rdfs:label "cat" ; lang:partOfSpeech lang:noun .
ex:senseCat a lang:Sense ; rdfs:label "the animal sense of 'cat'" ; lang:senseOf ex:lexCat .
ex:wfCats a lang:WordForm ; rdfs:label "cats" ; lang:inflectionOf ex:lexCat ;
    lang:morphFeature ex:featPlur .
ex:featPlur a lang:MorphFeature ; lang:featureKey lang:featNumber ; lang:featureValue lang:valPlur .
"#;

    fn source() -> NamedSource {
        NamedSource {
            name: "lex".to_owned(),
            bytes: LEXICON.as_bytes().to_vec(),
        }
    }

    #[test]
    fn lexeme_inventory_projects_forward_to_ontolex() {
        let input = LangProjectionInput {
            lang_models: vec![source()],
            ..Default::default()
        };
        let emissions = OntoLexTarget.emit(&input).expect("emit");
        assert_eq!(
            emissions.len(),
            1,
            "one lexicon emission for a lexeme-bearing surface"
        );
        let e = &emissions[0];
        let ttl = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();

        // Faithful form/sense/reference structure: entry, canonical form + writtenRep, sense.
        assert!(
            ttl.contains(&format!("<{ONTOLEX_NS}LexicalEntry>")),
            "{ttl}"
        );
        assert!(
            ttl.contains(&format!("<{ONTOLEX_NS}canonicalForm>")),
            "{ttl}"
        );
        assert!(
            ttl.contains(&format!("<{ONTOLEX_NS}writtenRep> \"cat\"")),
            "{ttl}"
        );
        assert!(ttl.contains(&format!("<{ONTOLEX_NS}otherForm>")), "{ttl}");
        assert!(
            ttl.contains(&format!("<{ONTOLEX_NS}writtenRep> \"cats\"")),
            "{ttl}"
        );
        assert!(
            ttl.contains(&format!("<{ONTOLEX_NS}LexicalSense>")),
            "{ttl}"
        );
        // The gloss the ingest lift shed is carried FORWARD as skos:definition.
        assert!(
            ttl.contains(&format!(
                "<{SKOS_NS}definition> \"the animal sense of 'cat'\""
            )),
            "{ttl}"
        );
        // UD-aligned features lowered to lexinfo: POS + number/plural.
        assert!(
            ttl.contains(&format!("<{LEXINFO_NS}partOfSpeech> <{LEXINFO_NS}noun>")),
            "{ttl}"
        );
        assert!(
            ttl.contains(&format!("<{LEXINFO_NS}number> <{LEXINFO_NS}plural>")),
            "{ttl}"
        );
        assert!(e.artifacts[0].is_rdf);
        assert!(e.artifacts[0].path_suffix.starts_with("ontolex-lemon/"));

        // Honest preservation: never exact; SoundUnder with the flattened epistemic strata named.
        assert!(!is_exact_correspondence(&e.correspondence));
        assert_eq!(e.lossy_kind, PreservationKind::SoundUnder);
        let joined = e.unsupported.join("\n");
        assert!(joined.contains("vantage"), "{joined}");
        assert!(joined.contains("lang:InterpretationAct"), "{joined}");
    }

    #[test]
    fn unmapped_pos_is_carried_verbatim_and_flagged() {
        let src = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .
ex:lexOnom a lang:Lexeme ; rdfs:label "meow" ; lang:partOfSpeech lang:onomatopoeia .
"#;
        let input = LangProjectionInput {
            lang_models: vec![NamedSource {
                name: "x".to_owned(),
                bytes: src.as_bytes().to_vec(),
            }],
            ..Default::default()
        };
        let e = &OntoLexTarget.emit(&input).expect("emit")[0];
        let ttl = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();
        // No lexinfo POS invented; the lang: POS is carried verbatim and flagged as residue.
        assert!(
            ttl.contains(&format!("<{LANG_NS}partOfSpeech> <{LANG_NS}onomatopoeia>")),
            "{ttl}"
        );
        assert!(
            e.unsupported
                .iter()
                .any(|r| r.contains("no lexinfo class mapping")),
            "{:?}",
            e.unsupported
        );
    }

    #[test]
    fn no_lexeme_surface_does_not_emit() {
        let src = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix ex:   <http://example.org/lang/> .
ex:sys a lang:SignSystem .
"#;
        let input = LangProjectionInput {
            lang_models: vec![NamedSource {
                name: "nolex".to_owned(),
                bytes: src.as_bytes().to_vec(),
            }],
            ..Default::default()
        };
        assert!(OntoLexTarget.emit(&input).expect("emit").is_empty());
    }

    #[test]
    fn emitter_is_byte_reproducible() {
        let input = LangProjectionInput {
            lang_models: vec![source()],
            ..Default::default()
        };
        let a = OntoLexTarget.emit(&input).expect("a");
        let b = OntoLexTarget.emit(&input).expect("b");
        assert_eq!(a[0].artifacts[0].bytes, b[0].artifacts[0].bytes);
    }
}
