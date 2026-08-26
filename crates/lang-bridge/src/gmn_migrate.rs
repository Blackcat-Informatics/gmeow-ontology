// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN version-migration executor and the version-provenance tagging helper.
//!
//! A GMN dialect major bump is a *judged crossing* like any other: it is carried on a
//! `logic:Correspondence` individual (linked to its two majors through
//! `gmeow:gmnMigratesFrom` / `gmeow:gmnMigratesTo`) whose fidelity lives in its
//! `logic:preservationKind` — NEVER a boolean "migrated: ok" flag. The correspondence
//! aggregates one operator-grain [`GlyphRewrite`] per operator whose surface changes across
//! the crossing (a `gmeow:GmnMigrationRewrite` reifying its version-stable denoted term, its
//! source/target glyph, and its source/target binding strength).
//!
//! [`GmnMigration::from_dataset`] reads such an authored leg off the carrier — exactly the
//! graph-first discipline [`crate::gmn1_codec::resolve_dialect_acceptance`] uses for the
//! acceptance window — and [`GmnMigration::migrate`] applies it to re-emit a stored GMN
//! document (projected to its operator surface, a [`GmnRecordSet`]) at the target major.
//!
//! The one HARD FAIL the executor raises is an **unbridged glyph drop**: an operator term
//! present in the source major but absent from the target major's inventory with NO covering
//! rewrite. That is [`GmnMigrateError::UnbridgedGlyphDrop`], whose
//! [`GmnMigrateError::failure_class`] names the shipped `lang:GmnUnbridgedGlyphDrop`
//! conformance class — a dropped operator is never silently discarded.
//!
//! [`tag_schema_version`] is the reusable version-provenance stamp: it writes exactly one
//! `gmeow:gmnSchemaVersion` quad carrying the graph-resolved schema major onto an emitted
//! record, so downstream producers (token metrics, verbalizations, training-corpus rows) tag
//! every row with the codebook's resolved dialect coordinate, deterministically and
//! single-valued.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_logic_compile::ir::PreservationKind;
use purrdf::{RdfDataset, RdfLiteral, RdfQuad, RdfTerm};

use crate::gmn1_codec::GmnDictionary;

const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

const PRED_GMN_MIGRATES_FROM: &str = "https://blackcatinformatics.ca/gmeow/gmnMigratesFrom";
const PRED_GMN_MIGRATES_TO: &str = "https://blackcatinformatics.ca/gmeow/gmnMigratesTo";
const PRED_GMN_MIGRATION_REWRITE: &str = "https://blackcatinformatics.ca/gmeow/gmnMigrationRewrite";
const PRED_GMN_REWRITE_TERM: &str = "https://blackcatinformatics.ca/gmeow/gmnRewriteTerm";
const PRED_GMN_REWRITE_FROM_GLYPH: &str =
    "https://blackcatinformatics.ca/gmeow/gmnRewriteFromGlyph";
const PRED_GMN_REWRITE_TO_GLYPH: &str = "https://blackcatinformatics.ca/gmeow/gmnRewriteToGlyph";
const PRED_GMN_REWRITE_FROM_PRECEDENCE: &str =
    "https://blackcatinformatics.ca/gmeow/gmnRewriteFromPrecedence";
const PRED_GMN_REWRITE_TO_PRECEDENCE: &str =
    "https://blackcatinformatics.ca/gmeow/gmnRewriteToPrecedence";
const PRED_GMN_VERSION_DEFINES_OPERATOR: &str =
    "https://blackcatinformatics.ca/gmeow/gmnVersionDefinesOperator";
const PRED_DENOTED_FORM: &str = "https://blackcatinformatics.ca/lang/denotedForm";
const PRED_DENOTATION_TARGET: &str = "https://blackcatinformatics.ca/lang/denotationTarget";
const PRED_GMN_PRECEDENCE: &str = "https://blackcatinformatics.ca/gmeow/gmnPrecedence";
const PRED_PRESERVATION_KIND: &str = "https://blackcatinformatics.ca/logic/preservationKind";
const PRED_MNEMOMORPHIC: &str = "https://blackcatinformatics.ca/logic/mnemomorphic";
const PRED_OWL_VERSION_INFO: &str = "http://www.w3.org/2002/07/owl#versionInfo";

/// The `gmeow:gmnSchemaVersion` provenance predicate — the dialect schema major an emitted
/// record was serialized under. The SAME IRI the `gmeow:GmnEnvelope` header pins.
pub const PRED_GMN_SCHEMA_VERSION: &str = "https://blackcatinformatics.ca/gmeow/gmnSchemaVersion";

/// A GMN migration that could not be executed. `MalformedLeg` is a leg-authoring precondition
/// error (the carrier's own SHACL/round-trip gates are the primary authority — this is the
/// executor's read-back safety net); `UnbridgedGlyphDrop` is the one runtime CONFORMANCE
/// failure the executor raises, naming the `lang:GmnUnbridgedGlyphDrop` class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GmnMigrateError {
    /// The authored `logic:Correspondence` migration leg is structurally incomplete — a
    /// missing/duplicate `gmeow:gmnMigratesFrom`/`gmnMigratesTo`, a version entity with
    /// no/many `logic:versionInfo`, an absent/unknown `logic:preservationKind`, or a
    /// `gmeow:GmnMigrationRewrite` lacking its term or either glyph. Detail names the defect.
    MalformedLeg(String),
    /// `lang:GmnUnbridgedGlyphDrop` — an operator `term` (source glyph `glyph`) present in the
    /// source major is absent from the target major's inventory and NO `gmeow:GmnMigrationRewrite`
    /// on the migration bridges it. A HARD FAIL, never a silent drop.
    UnbridgedGlyphDrop {
        /// The version-stable denoted term the dropped operator names.
        term: String,
        /// The source-major glyph spelling that has no target home.
        glyph: String,
    },
}

impl GmnMigrateError {
    /// The full `lang:` conformance-class IRI for the unbridged-drop failure — the SAME IRI
    /// `slices/grounding/lang/module.ttl` mints under `lang:LangConformanceFailure`.
    pub const CLASS_UNBRIDGED_GLYPH_DROP: &'static str =
        "https://blackcatinformatics.ca/lang/GmnUnbridgedGlyphDrop";

    /// The `lang:` conformance class this failure resolves to, when it is a document-conformance
    /// failure. A [`Self::MalformedLeg`] is a leg-authoring precondition error (like
    /// [`crate::gmn1_codec::DictionaryError`]), not a conformance class, so it carries none.
    #[must_use]
    pub fn failure_class(&self) -> Option<&'static str> {
        match self {
            Self::UnbridgedGlyphDrop { .. } => Some(Self::CLASS_UNBRIDGED_GLYPH_DROP),
            Self::MalformedLeg(_) => None,
        }
    }
}

impl std::fmt::Display for GmnMigrateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // A leg-authoring precondition error (not a conformance class), mirroring the
            // codec's `DictionaryError` — the detail names the structural defect.
            Self::MalformedLeg(detail) => write!(f, "malformed migration leg: {detail}"),
            // The one runtime conformance failure — named exactly as its `lang:` class so the
            // shipped `failure_class()` and the human message never drift apart.
            Self::UnbridgedGlyphDrop { term, glyph } => write!(
                f,
                "lang:GmnUnbridgedGlyphDrop: operator {term} (source glyph {glyph:?}) is absent \
                 from the target major's inventory with no covering gmeow:GmnMigrationRewrite"
            ),
        }
    }
}

impl std::error::Error for GmnMigrateError {}

/// One operator-grain rewrite leg of a version migration — the in-memory carrier of a
/// `gmeow:GmnMigrationRewrite`. Anchored on the version-STABLE denoted term, it carries the
/// operator's source and target glyph, and (when it changes) its source and target binding
/// strength. It carries NO preservation judgment: that lives on the owning [`GmnMigration`]'s
/// `logic:Correspondence`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphRewrite {
    /// `gmeow:gmnRewriteTerm` — the version-stable denoted GMEOW term (the identity anchor).
    pub term: String,
    /// `gmeow:gmnRewriteFromGlyph` — the operator's source-major glyph.
    pub from_glyph: String,
    /// `gmeow:gmnRewriteToGlyph` — the operator's target-major glyph (a RENAME iff it differs).
    pub to_glyph: String,
    /// `gmeow:gmnRewriteFromPrecedence` — the source-major binding strength, if authored.
    pub from_precedence: Option<i64>,
    /// `gmeow:gmnRewriteToPrecedence` — the target-major binding strength, if authored (a
    /// precedence CHANGE iff present and differing from [`Self::from_precedence`]).
    pub to_precedence: Option<i64>,
}

impl GlyphRewrite {
    /// Whether this rewrite renames the operator's glyph.
    #[must_use]
    pub fn is_rename(&self) -> bool {
        self.from_glyph != self.to_glyph
    }

    /// Whether this rewrite changes the operator's binding strength (both sides authored and
    /// unequal).
    #[must_use]
    pub fn is_precedence_change(&self) -> bool {
        matches!((self.from_precedence, self.to_precedence), (Some(a), Some(b)) if a != b)
    }
}

/// An authored GMN version-migration leg, read off the carrier: the two majors, the
/// preservation JUDGMENT (never a boolean), and the operator rewrites the crossing bridges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmnMigration {
    correspondence: String,
    from_version: String,
    to_version: String,
    preservation: PreservationKind,
    mnemomorphic: bool,
    /// term IRI → its rewrite; a `BTreeMap` so lookup is deterministic and a term is bridged
    /// at most once.
    rewrites: BTreeMap<String, GlyphRewrite>,
}

impl GmnMigration {
    /// Read the migration leg named by `correspondence` off `ds`. The correspondence must
    /// carry exactly one `gmeow:gmnMigratesFrom` and one `gmeow:gmnMigratesTo` (each pointing
    /// at a version entity with exactly one `logic:versionInfo`), exactly one
    /// `logic:preservationKind`, and zero or more well-formed `gmeow:GmnMigrationRewrite`
    /// legs. `logic:mnemomorphic` defaults `false` when absent (as in the IR).
    ///
    /// # Errors
    /// [`GmnMigrateError::MalformedLeg`] when the authored leg is structurally incomplete.
    pub fn from_dataset(ds: &RdfDataset, correspondence: &str) -> Result<Self, GmnMigrateError> {
        // Single pass, collecting only the edges we need, keyed by subject.
        let mut migrates_from = BTreeSet::<String>::new();
        let mut migrates_to = BTreeSet::<String>::new();
        let mut preservation_iris = BTreeSet::<String>::new();
        let mut mnemomorphic_lex = BTreeSet::<String>::new();
        let mut rewrite_nodes = BTreeSet::<String>::new();
        let mut version_infos = BTreeMap::<String, BTreeSet<String>>::new();
        // rewrite node IRI → each authored field
        let mut r_term = BTreeMap::<String, BTreeSet<String>>::new();
        let mut r_from_glyph = BTreeMap::<String, BTreeSet<String>>::new();
        let mut r_to_glyph = BTreeMap::<String, BTreeSet<String>>::new();
        let mut r_from_prec = BTreeMap::<String, BTreeSet<String>>::new();
        let mut r_to_prec = BTreeMap::<String, BTreeSet<String>>::new();

        for quad in ds.owned_quads() {
            let RdfTerm::Iri(subject) = &quad.subject else {
                continue;
            };
            let pred = quad.predicate.as_str();
            if subject == correspondence {
                match pred {
                    PRED_GMN_MIGRATES_FROM => {
                        if let RdfTerm::Iri(o) = &quad.object {
                            migrates_from.insert(o.clone());
                        }
                    }
                    PRED_GMN_MIGRATES_TO => {
                        if let RdfTerm::Iri(o) = &quad.object {
                            migrates_to.insert(o.clone());
                        }
                    }
                    PRED_PRESERVATION_KIND => {
                        if let RdfTerm::Iri(o) = &quad.object {
                            preservation_iris.insert(o.clone());
                        }
                    }
                    PRED_MNEMOMORPHIC => {
                        if let RdfTerm::Literal(l) = &quad.object {
                            mnemomorphic_lex.insert(l.lexical_form.clone());
                        }
                    }
                    PRED_GMN_MIGRATION_REWRITE => {
                        if let RdfTerm::Iri(o) = &quad.object {
                            rewrite_nodes.insert(o.clone());
                        }
                    }
                    _ => {}
                }
            }
            match pred {
                // A version entity spells its version in the canonical
                // `logic:versionInfo`; its generated OWL view uses
                // `owl:versionInfo`. Read both so a re-authored migration leg is
                // never seen as missing its version.
                PRED_OWL_VERSION_INFO | gmeow_ns::LOGIC_VERSION_INFO => {
                    if let RdfTerm::Literal(l) = &quad.object {
                        version_infos
                            .entry(subject.clone())
                            .or_default()
                            .insert(l.lexical_form.clone());
                    }
                }
                PRED_GMN_REWRITE_TERM => {
                    if let RdfTerm::Iri(o) = &quad.object {
                        r_term.entry(subject.clone()).or_default().insert(o.clone());
                    }
                }
                PRED_GMN_REWRITE_FROM_GLYPH => {
                    if let RdfTerm::Literal(l) = &quad.object {
                        r_from_glyph
                            .entry(subject.clone())
                            .or_default()
                            .insert(l.lexical_form.clone());
                    }
                }
                PRED_GMN_REWRITE_TO_GLYPH => {
                    if let RdfTerm::Literal(l) = &quad.object {
                        r_to_glyph
                            .entry(subject.clone())
                            .or_default()
                            .insert(l.lexical_form.clone());
                    }
                }
                PRED_GMN_REWRITE_FROM_PRECEDENCE => {
                    if let RdfTerm::Literal(l) = &quad.object {
                        r_from_prec
                            .entry(subject.clone())
                            .or_default()
                            .insert(l.lexical_form.clone());
                    }
                }
                PRED_GMN_REWRITE_TO_PRECEDENCE => {
                    if let RdfTerm::Literal(l) = &quad.object {
                        r_to_prec
                            .entry(subject.clone())
                            .or_default()
                            .insert(l.lexical_form.clone());
                    }
                }
                _ => {}
            }
        }

        let from_entity = exactly_one(&migrates_from, correspondence, "gmeow:gmnMigratesFrom")?;
        let to_entity = exactly_one(&migrates_to, correspondence, "gmeow:gmnMigratesTo")?;
        let from_version =
            exactly_one_owned(version_infos.get(&from_entity), &from_entity, "versionInfo")?;
        let to_version =
            exactly_one_owned(version_infos.get(&to_entity), &to_entity, "versionInfo")?;
        let preservation_iri =
            exactly_one(&preservation_iris, correspondence, "logic:preservationKind")?;
        let preservation = preservation_iri
            .strip_prefix(LOGIC_NS)
            .and_then(PreservationKind::from_local)
            .ok_or_else(|| {
                GmnMigrateError::MalformedLeg(format!(
                    "migration {correspondence} logic:preservationKind {preservation_iri:?} is not a known logic:PreservationKind"
                ))
            })?;
        // `logic:mnemomorphic` is optional and defaults false (mirroring the Correspondence IR).
        let mnemomorphic = match mnemomorphic_lex.len() {
            0 => false,
            1 => {
                let lex = mnemomorphic_lex
                    .iter()
                    .next()
                    .expect("the singleton set is non-empty");
                match lex.as_str() {
                    "true" => true,
                    "false" => false,
                    other => {
                        return Err(GmnMigrateError::MalformedLeg(format!(
                            "migration {correspondence} logic:mnemomorphic {other:?} is not a boolean"
                        )));
                    }
                }
            }
            n => {
                return Err(GmnMigrateError::MalformedLeg(format!(
                    "migration {correspondence} declares {n} logic:mnemomorphic values, expected at most one"
                )));
            }
        };

        let mut rewrites = BTreeMap::<String, GlyphRewrite>::new();
        for node in &rewrite_nodes {
            let term = exactly_one_owned(r_term.get(node), node, "gmeow:gmnRewriteTerm")?;
            let from_glyph =
                exactly_one_owned(r_from_glyph.get(node), node, "gmeow:gmnRewriteFromGlyph")?;
            let to_glyph =
                exactly_one_owned(r_to_glyph.get(node), node, "gmeow:gmnRewriteToGlyph")?;
            let from_precedence = optional_int(
                r_from_prec.get(node),
                node,
                "gmeow:gmnRewriteFromPrecedence",
            )?;
            let to_precedence =
                optional_int(r_to_prec.get(node), node, "gmeow:gmnRewriteToPrecedence")?;
            let rewrite = GlyphRewrite {
                term: term.clone(),
                from_glyph,
                to_glyph,
                from_precedence,
                to_precedence,
            };
            if rewrites.insert(term.clone(), rewrite).is_some() {
                return Err(GmnMigrateError::MalformedLeg(format!(
                    "migration {correspondence} carries two rewrites for term {term}"
                )));
            }
        }

        Ok(Self {
            correspondence: correspondence.to_owned(),
            from_version,
            to_version,
            preservation,
            mnemomorphic,
            rewrites,
        })
    }

    /// The migration's `logic:Correspondence` IRI.
    #[must_use]
    pub fn correspondence(&self) -> &str {
        &self.correspondence
    }

    /// The source major (`logic:versionInfo` of the `gmeow:gmnMigratesFrom` entity).
    #[must_use]
    pub fn from_version(&self) -> &str {
        &self.from_version
    }

    /// The target major (`logic:versionInfo` of the `gmeow:gmnMigratesTo` entity).
    #[must_use]
    pub fn to_version(&self) -> &str {
        &self.to_version
    }

    /// The crossing's preservation JUDGMENT — asserted on emitted output, never reduced to a
    /// boolean.
    #[must_use]
    pub fn preservation(&self) -> PreservationKind {
        self.preservation
    }

    /// Whether the crossing retains a source witness (`logic:mnemomorphic`).
    #[must_use]
    pub fn mnemomorphic(&self) -> bool {
        self.mnemomorphic
    }

    /// The rewrite bridging `term`, if the migration authors one.
    #[must_use]
    pub fn rewrite_for(&self, term: &str) -> Option<&GlyphRewrite> {
        self.rewrites.get(term)
    }

    /// The authored rewrites, in deterministic term order.
    pub fn rewrites(&self) -> impl Iterator<Item = &GlyphRewrite> {
        self.rewrites.values()
    }

    /// Apply the migration to a stored source-major document (projected to its operator
    /// surface) and re-emit it at the target major.
    ///
    /// `target_inventory` is the set of operator terms the target major defines natively. For
    /// each source occurrence: an authored rewrite bridges it (renaming its glyph / re-binding
    /// its precedence, or re-surfacing an otherwise-dropped operator); a term still in
    /// `target_inventory` with no rewrite survives unchanged; a term ABSENT from
    /// `target_inventory` with NO rewrite is an unbridged drop and a HARD FAIL.
    ///
    /// # Errors
    /// [`GmnMigrateError::UnbridgedGlyphDrop`] on the first source operator dropped by the
    /// target major without a covering rewrite.
    pub fn migrate(
        &self,
        doc: &GmnRecordSet,
        target_inventory: &BTreeSet<String>,
    ) -> Result<MigratedRecordSet, GmnMigrateError> {
        let mut operators = Vec::with_capacity(doc.operators.len());
        for occ in &doc.operators {
            match self.rewrites.get(&occ.term) {
                Some(rewrite) => operators.push(MigratedOperator {
                    term: occ.term.clone(),
                    glyph: rewrite.to_glyph.clone(),
                    precedence: rewrite.to_precedence.or(occ.precedence),
                    rewritten: true,
                }),
                None if target_inventory.contains(&occ.term) => operators.push(MigratedOperator {
                    term: occ.term.clone(),
                    glyph: occ.glyph.clone(),
                    precedence: occ.precedence,
                    rewritten: false,
                }),
                None => {
                    return Err(GmnMigrateError::UnbridgedGlyphDrop {
                        term: occ.term.clone(),
                        glyph: occ.glyph.clone(),
                    });
                }
            }
        }
        Ok(MigratedRecordSet {
            target_version: self.to_version.clone(),
            preservation: self.preservation,
            operators,
        })
    }
}

/// One operator occurrence in a stored GMN document, at the SOURCE dialect major: the
/// version-stable term it denotes and the surface (glyph, binding strength) it was written
/// under. A [`GmnRecordSet`] is the operator-surface projection of a stored GMN document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorOccurrence {
    /// The GMEOW term the operator denotes.
    pub term: String,
    /// The source-major glyph the operator was written with.
    pub glyph: String,
    /// The source-major binding strength, if the document records it.
    pub precedence: Option<i64>,
}

impl OperatorOccurrence {
    /// A convenience constructor.
    #[must_use]
    pub fn new(term: impl Into<String>, glyph: impl Into<String>, precedence: Option<i64>) -> Self {
        Self {
            term: term.into(),
            glyph: glyph.into(),
            precedence,
        }
    }
}

/// The operator-surface projection of a stored GMN document — the operator occurrences the
/// migration executor re-emits at the target major.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GmnRecordSet {
    /// The operator occurrences, in document order.
    pub operators: Vec<OperatorOccurrence>,
}

/// One re-emitted operator at the target major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigratedOperator {
    /// The version-stable denoted term (unchanged by the crossing).
    pub term: String,
    /// The target-major glyph (renamed iff a rewrite applied).
    pub glyph: String,
    /// The target-major binding strength.
    pub precedence: Option<i64>,
    /// Whether an authored rewrite leg applied to this occurrence.
    pub rewritten: bool,
}

/// A stored GMN document re-emitted at the target major, carrying the crossing's preservation
/// JUDGMENT so consumers read fidelity from the correspondence, never from a boolean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigratedRecordSet {
    /// The target major the document was re-emitted at.
    pub target_version: String,
    /// The migration's preservation judgment.
    pub preservation: PreservationKind,
    /// The re-emitted operators, in document order.
    pub operators: Vec<MigratedOperator>,
}

// ── Version-provenance tagging (req #20 infra; Tasks 7/8/10 consume it) ──────────────

/// The graph-resolved schema major to stamp on emitted records — the codebook's latest major
/// via [`GmnDictionary::schema_major`] / `resolve_dialect_acceptance`, never a Rust constant.
/// Single-valued and deterministic.
#[must_use]
pub fn resolved_schema_version(dict: &GmnDictionary) -> String {
    dict.schema_major()
}

/// Stamp exactly one `gmeow:gmnSchemaVersion` provenance quad onto `record`, carrying the
/// graph-resolved schema major. Deterministic and single-valued: one call yields one quad,
/// and its value is the codebook's resolved latest major, so tagging a record is idempotent.
#[must_use]
pub fn tag_schema_version(record: &str, dict: &GmnDictionary) -> RdfQuad {
    RdfQuad::new(
        RdfTerm::Iri(record.to_owned()),
        PRED_GMN_SCHEMA_VERSION,
        RdfTerm::Literal(RdfLiteral {
            lexical_form: resolved_schema_version(dict),
            datatype: Some(XSD_STRING.to_owned()),
            language: None,
            direction: None,
        }),
    )
}

// ── Document-level migration (the production `gmeow gmn migrate` plumbing) ───────────
//
// The executor above works over the operator-surface projection ([`GmnRecordSet`]); the CLI
// leg needs three graph-derived steps around it: PROJECT a stored GMN-1 document to that
// surface, DERIVE the target major's native operator inventory, and RE-EMIT the document at
// the target major. Every step reads its operator vocabulary FROM THE GRAPH — the dictionary's
// executable glyph registry and the migration leg's own authored rewrites — never a hardcoded
// glyph/term/version table.
//
// The surface scan (not `gmn1_read`) is deliberate: a migration exists precisely to carry
// operators whose glyph the CURRENT dictionary no longer lists (a retired/bridged operator like
// the demonstrator's ⊻), so `gmn1_read` would reject the very documents this leg must migrate.
// The scan is still exact — it matches ONLY whole delimiter-bounded tokens against the
// graph-derived source glyph table — so every non-operator byte is passed through untouched.

/// A GMN-1 surface delimiter (mirroring the codec's record grammar): an operator/value token
/// is a maximal run of characters that are neither whitespace nor one of these delimiters.
fn is_gmn_delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '{' | '}' | '[' | ']' | ',' | ':' | '(' | ')')
}

/// The delimiter-and-whitespace-split value/operator tokens of a GMN-1 document, in document
/// order (empty tokens dropped).
fn surface_tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(is_gmn_delimiter)
        .filter(|token| !token.is_empty())
}

/// The source operator surface table: source glyph → its [`OperatorOccurrence`] (denoted term
/// + source binding strength), derived ENTIRELY from the graph. It unions two authored sources:
///
/// * the dictionary's executable glyph registry — every operator the SOURCE major defines
///   natively ([`crate::gmn1_codec::GmnGlyphRegistry::bindings`]), its source precedence read off the operator's
///   `lang:denotedForm` `gmeow:gmnPrecedence` in `ds`; and
/// * the migration leg's own rewrites — whose `gmeow:gmnRewriteFromGlyph` /
///   `gmeow:gmnRewriteFromPrecedence` carry the source surface of the bridged/renamed operators
///   the current registry no longer lists. These OVERRIDE the registry, because the leg is the
///   graph authority for the source surface of an operator its crossing rewrites.
///
/// Never a hardcoded glyph table: removing a Denotation or a rewrite from the graph removes the
/// glyph from this table.
#[must_use]
pub fn source_operator_table(
    dict: &GmnDictionary,
    migration: &GmnMigration,
    ds: &RdfDataset,
) -> BTreeMap<String, OperatorOccurrence> {
    let mut glyph_terms = BTreeMap::<String, BTreeSet<String>>::new();
    for (_sigil, glyph, _fixity, _arity, term) in dict.glyph_registry().bindings() {
        glyph_terms.entry(glyph).or_default().insert(term);
    }
    let precedences = operator_precedences(ds);
    let mut table = BTreeMap::<String, OperatorOccurrence>::new();
    for (glyph, terms) in glyph_terms {
        // A glyph denoting more than one term across sigil scopes cannot be resolved by a
        // scope-free surface scan; such a token is passed through as ordinary content (the
        // migration's own rewrites, keyed unambiguously by term, still bridge it below).
        if terms.len() == 1 {
            let term = terms
                .into_iter()
                .next()
                .expect("the singleton term set is non-empty");
            let precedence = precedences.get(&term).copied();
            table.insert(
                glyph.clone(),
                OperatorOccurrence::new(term, glyph, precedence),
            );
        }
    }
    for rewrite in migration.rewrites() {
        table.insert(
            rewrite.from_glyph.clone(),
            OperatorOccurrence::new(
                rewrite.term.clone(),
                rewrite.from_glyph.clone(),
                rewrite.from_precedence,
            ),
        );
    }
    table
}

/// Operator term IRI → its source-major `gmeow:gmnPrecedence`, read off each operator's
/// `lang:Denotation` → `lang:denotedForm` → `gmeow:gmnPrecedence` chain in `ds`. A term with no
/// authored precedence (or whose glyph carries no denoted form) is simply absent.
fn operator_precedences(ds: &RdfDataset) -> BTreeMap<String, i64> {
    let mut den_target = BTreeMap::<String, String>::new();
    let mut den_form = BTreeMap::<String, String>::new();
    let mut form_precedence = BTreeMap::<String, i64>::new();
    for quad in ds.owned_quads() {
        let RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        match quad.predicate.as_str() {
            PRED_DENOTATION_TARGET => {
                if let RdfTerm::Iri(target) = &quad.object {
                    den_target.insert(subject.clone(), target.clone());
                }
            }
            PRED_DENOTED_FORM => {
                if let RdfTerm::Iri(form) = &quad.object {
                    den_form.insert(subject.clone(), form.clone());
                }
            }
            PRED_GMN_PRECEDENCE => {
                if let RdfTerm::Literal(lit) = &quad.object
                    && let Ok(value) = lit.lexical_form.parse::<i64>()
                {
                    form_precedence.insert(subject.clone(), value);
                }
            }
            _ => {}
        }
    }
    let mut out = BTreeMap::<String, i64>::new();
    for (denotation, target) in den_target {
        if let Some(form) = den_form.get(&denotation)
            && let Some(precedence) = form_precedence.get(form)
        {
            out.insert(target, *precedence);
        }
    }
    out
}

/// Project the operator occurrences a stored source-major GMN-1 document uses into a
/// [`GmnRecordSet`] — graph-derived: every surface token equal to a known source operator glyph
/// (via [`source_operator_table`]) maps back to its version-stable term + source precedence.
/// Distinct terms, in first-occurrence document order.
#[must_use]
pub fn extract_operators(
    doc_text: &str,
    table: &BTreeMap<String, OperatorOccurrence>,
) -> GmnRecordSet {
    let mut seen = BTreeSet::<String>::new();
    let mut operators = Vec::new();
    for token in surface_tokens(doc_text) {
        if let Some(occurrence) = table.get(token)
            && seen.insert(occurrence.term.clone())
        {
            operators.push(occurrence.clone());
        }
    }
    GmnRecordSet { operators }
}

/// The operator terms the migration's TARGET major defines NATIVELY, read from the graph: the
/// `gmeow:gmnVersionDefinesOperator` inventory authored on the correspondence's
/// `gmeow:gmnMigratesTo` version entity. NEVER a Rust constant. An operator absent from this set
/// with no covering rewrite is the unbridged-drop HARD FAIL [`GmnMigration::migrate`] raises.
///
/// # Errors
/// [`GmnMigrateError::MalformedLeg`] when the correspondence names no/many `gmeow:gmnMigratesTo`
/// entity (the same exactly-one discipline [`GmnMigration::from_dataset`] applies).
pub fn derive_target_inventory(
    ds: &RdfDataset,
    correspondence: &str,
) -> Result<BTreeSet<String>, GmnMigrateError> {
    let mut migrates_to = BTreeSet::<String>::new();
    let mut defines = BTreeMap::<String, BTreeSet<String>>::new();
    for quad in ds.owned_quads() {
        let RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        if subject == correspondence
            && quad.predicate.as_str() == PRED_GMN_MIGRATES_TO
            && let RdfTerm::Iri(entity) = &quad.object
        {
            migrates_to.insert(entity.clone());
        }
        if quad.predicate.as_str() == PRED_GMN_VERSION_DEFINES_OPERATOR
            && let RdfTerm::Iri(term) = &quad.object
        {
            defines
                .entry(subject.clone())
                .or_default()
                .insert(term.clone());
        }
    }
    let target = exactly_one(&migrates_to, correspondence, "gmeow:gmnMigratesTo")?;
    Ok(defines.get(&target).cloned().unwrap_or_default())
}

/// Re-emit a stored source-major GMN-1 document at the target major: substitute each migrated
/// operator's SOURCE glyph with its target-major glyph (from `migrated`) and re-stamp the leading
/// `@gmn{v: …}` header to `to_major`. Deterministic and faithful — a single left-to-right pass
/// replaces ONLY whole tokens equal to a source operator glyph, so every non-operator byte
/// (record structure, references, literals, delimiters, whitespace) is preserved verbatim.
///
/// `source` and `migrated` are the two ends of one [`GmnMigration::migrate`] call, so their
/// operators are the same terms in the same order; pairing them by position recovers each
/// operator's `source glyph → target glyph` edge.
#[must_use]
pub fn reemit_migrated_document(
    doc_text: &str,
    source: &GmnRecordSet,
    migrated: &MigratedRecordSet,
    to_major: &str,
) -> String {
    let mut glyph_map = BTreeMap::<String, String>::new();
    for (occurrence, operator) in source.operators.iter().zip(migrated.operators.iter()) {
        glyph_map.insert(occurrence.glyph.clone(), operator.glyph.clone());
    }
    let substituted = substitute_tokens(doc_text, &glyph_map);
    restamp_header_major(&substituted, to_major)
}

/// The `v:` dialect-major coordinate declared in the leading `@gmn{…}` header, if present.
/// `None` when the text carries no `@gmn{` header or that header pins no `v:` coordinate — a
/// malformed input the migrate leg treats as a HARD FAIL rather than migrating on a guess.
#[must_use]
pub fn header_schema_major(doc_text: &str) -> Option<String> {
    let header = doc_text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    if !header.starts_with("@gmn{") {
        return None;
    }
    header_version_span(header).map(|(start, end)| header[start..end].to_owned())
}

/// The byte range `(start, end)` of the `v:` value token inside an `@gmn{…}` header line.
fn header_version_span(header: &str) -> Option<(usize, usize)> {
    let after_colon = header.find("v:")? + "v:".len();
    let rest = &header[after_colon..];
    let leading_ws = rest.len() - rest.trim_start().len();
    let value_start = after_colon + leading_ws;
    let value_rest = &header[value_start..];
    let value_len = value_rest
        .find(is_gmn_delimiter)
        .unwrap_or(value_rest.len());
    (value_len > 0).then_some((value_start, value_start + value_len))
}

/// Re-stamp the leading `@gmn{v: …}` header's dialect major to `to_major`, preserving every
/// other byte. A text with no re-stampable header is returned unchanged (the caller validates
/// header presence up front via [`header_schema_major`]).
fn restamp_header_major(text: &str, to_major: &str) -> String {
    let (first, rest) = match text.find('\n') {
        Some(nl) => (&text[..nl], &text[nl..]),
        None => (text, ""),
    };
    if !first.trim_start().starts_with("@gmn{") {
        return text.to_owned();
    }
    let Some((start, end)) = header_version_span(first) else {
        return text.to_owned();
    };
    let mut out = String::with_capacity(text.len() + to_major.len());
    out.push_str(&first[..start]);
    out.push_str(to_major);
    out.push_str(&first[end..]);
    out.push_str(rest);
    out
}

/// Replace every whole delimiter-bounded token equal to a key of `map` with its value,
/// preserving all delimiters and whitespace exactly.
fn substitute_tokens(text: &str, map: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut token = String::new();
    for c in text.chars() {
        if is_gmn_delimiter(c) {
            flush_token(&mut token, map, &mut out);
            out.push(c);
        } else {
            token.push(c);
        }
    }
    flush_token(&mut token, map, &mut out);
    out
}

/// Emit the pending token — its `map` substitution when one exists, else the token verbatim —
/// and clear it.
fn flush_token(token: &mut String, map: &BTreeMap<String, String>, out: &mut String) {
    if !token.is_empty() {
        match map.get(token.as_str()) {
            Some(replacement) => out.push_str(replacement),
            None => out.push_str(token),
        }
        token.clear();
    }
}

// ── Small graph-read helpers (mirroring gmn1_codec's exactly-one discipline) ─────────

fn exactly_one(
    values: &BTreeSet<String>,
    subject: &str,
    label: &str,
) -> Result<String, GmnMigrateError> {
    match values.len() {
        1 => Ok(values
            .iter()
            .next()
            .expect("the singleton set is non-empty")
            .clone()),
        n => Err(GmnMigrateError::MalformedLeg(format!(
            "{subject} declares {n} {label} values, expected exactly one"
        ))),
    }
}

fn exactly_one_owned(
    values: Option<&BTreeSet<String>>,
    subject: &str,
    label: &str,
) -> Result<String, GmnMigrateError> {
    let empty = BTreeSet::new();
    exactly_one(values.unwrap_or(&empty), subject, label)
}

fn optional_int(
    values: Option<&BTreeSet<String>>,
    subject: &str,
    label: &str,
) -> Result<Option<i64>, GmnMigrateError> {
    let empty = BTreeSet::new();
    let values = values.unwrap_or(&empty);
    match values.len() {
        0 => Ok(None),
        1 => {
            let lex = values
                .iter()
                .next()
                .expect("the singleton set is non-empty");
            lex.parse::<i64>().map(Some).map_err(|_| {
                GmnMigrateError::MalformedLeg(format!(
                    "{subject} {label} {lex:?} is not an integer"
                ))
            })
        }
        n => Err(GmnMigrateError::MalformedLeg(format!(
            "{subject} declares {n} {label} values, expected at most one"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use purrdf::parse_dataset;

    const MIGRATION_CORR: &str =
        "https://blackcatinformatics.ca/gmeow/examples/lang/gmnMigrationVSrcToVTgt";
    const LOGIC_NOT: &str = "https://blackcatinformatics.ca/logic/not";
    const LEGACY_XOR: &str = "https://blackcatinformatics.ca/gmeow/examples/lang/gmnLegacyXorOp";

    fn demonstrator_dataset() -> Arc<RdfDataset> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../slices/grounding/lang/examples/gmn-migration.ttl"
        );
        let bytes = std::fs::read(path).expect("gmn-migration.ttl is readable");
        parse_dataset(&bytes, "text/turtle", None).expect("gmn-migration.ttl parses")
    }

    #[test]
    fn loads_the_authored_demonstrator_leg() {
        let migration =
            GmnMigration::from_dataset(&demonstrator_dataset(), MIGRATION_CORR).expect("leg loads");
        assert_eq!(migration.from_version(), "1");
        assert_eq!(migration.to_version(), "2");
        assert_eq!(migration.preservation(), PreservationKind::Exact);
        assert!(migration.mnemomorphic());
        let rename = migration.rewrite_for(LOGIC_NOT).expect("logic:not rewrite");
        assert!(rename.is_rename() && rename.is_precedence_change());
        assert_eq!(
            (rename.from_glyph.as_str(), rename.to_glyph.as_str()),
            ("¬", "!")
        );
        assert_eq!(
            (rename.from_precedence, rename.to_precedence),
            (Some(90), Some(80))
        );

        // The bridged-drop rewrite (a glyph rename with no precedence legs).
        let bridged = migration.rewrite_for(LEGACY_XOR).expect("xor rewrite");
        assert!(bridged.is_rename() && !bridged.is_precedence_change());
        assert_eq!(
            (bridged.from_glyph.as_str(), bridged.to_glyph.as_str()),
            ("⊻", "^")
        );
        assert_eq!(
            (bridged.from_precedence, bridged.to_precedence),
            (None, None)
        );
    }
}
