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
    /// no/many `owl:versionInfo`, an absent/unknown `logic:preservationKind`, or a
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
    /// at a version entity with exactly one `owl:versionInfo`), exactly one
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
                PRED_OWL_VERSION_INFO => {
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
        let from_version = exactly_one_owned(
            version_infos.get(&from_entity),
            &from_entity,
            "owl:versionInfo",
        )?;
        let to_version =
            exactly_one_owned(version_infos.get(&to_entity), &to_entity, "owl:versionInfo")?;
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

    /// The source major (`owl:versionInfo` of the `gmeow:gmnMigratesFrom` entity).
    #[must_use]
    pub fn from_version(&self) -> &str {
        &self.from_version
    }

    /// The target major (`owl:versionInfo` of the `gmeow:gmnMigratesTo` entity).
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
