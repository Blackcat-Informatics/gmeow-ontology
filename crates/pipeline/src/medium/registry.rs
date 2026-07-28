// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The carrier dataset → the typed medium registry.
//!
//! Everything here is read off the IN-MEMORY [`RdfDataset`] a stage was handed —
//! never re-parsed from `slices/core/gts/module.ttl` on disk. A disk re-parse would
//! read the committed tree, which is not flushed until the post-run reconcile
//! returns, so a source edit could never reach the dictionaries in one pass: the
//! same stale-disk-fold class every stage in this crate refuses (see
//! [`crate::docs_measure`]).
//!
//! # Every field is exactly-one, and absence is a hard fail
//!
//! `slices/core/gts/module.ttl` declares each medium-axis class's cardinality in
//! the canonical `logic:` restriction vocabulary. This reader enforces the same
//! contract structurally: a dictionary without a corpus, a schema without a wire
//! label, a medium without a level — each is a HARD FAIL rather than a defaulted
//! value, because every one of those defaults would be a silent decision about
//! bytes that ship.
//!
//! # The rep→medium assignment is TOTAL
//!
//! The assignment is AUTHORED on the `gmeow:PayloadSchema` registry itself —
//! `gmeow:payloadSchemaMedium` (exactly-one) plus `gmeow:payloadSchemaDictionary`
//! (the per-rep selection out of that medium's declared bound) — and it is read
//! from there, never from an emitted `gmeow:MediumEnvelope`. That direction is
//! load-bearing: an envelope is the PROJECTION of a frame this build actually
//! wrote, so sourcing the assignment from one would make the assignment a product
//! of the emission it governs, and a rep that happened not to be emitted this run
//! would have no medium at all. Reading the schema makes the assignment TOTAL by
//! construction, because every emittable rep already has a registered schema.
//! [`MediumRegistry::assignment_for`] therefore answers with a
//! [`RepAssignment`] or a named failure, never with a default:
//!
//! * a rep with no registered `gmeow:PayloadSchema` → `MediumUnknownSchema`;
//! * a registered rep with no assignment → `MediumUndeclaredDictionary`.
//!
//! [`DictSelection::Baseline`] is a SELECTION, not an absence: it is reachable only
//! when the assigned medium declares an empty `gmeow:mediumDictionary` set — the
//! explicitly-declared no-dictionary medium (`gmeow:mediumProfileBaselineL12`)
//! whose whole purpose is to make "no dictionary" a thing a producer NAMES rather
//! than a state a frame falls into.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use purrdf::gts_compose::{
    DictSelection as WireDictSelection, FrameSlot, MediumPlan as WireMediumPlan,
};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue};

use super::corpus::CorpusSelector;
use super::{GMEOW, SNAPSHOT_WIRE_REP, invalid_declaration, undeclared_dictionary, unknown_schema};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The family a compression dictionary is built under (`gmeow:DictionaryStrategy`).
///
/// Both an AUTHORED intent on `gmeow:CompressionDictionary` and a MEASURED fact on
/// `gmeow:CompressionDictionaryRealization` — a trainer that fell back to raw
/// content must say so, because the two produce different decode-side expectations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DictionaryStrategy {
    /// The zstd FastCOVER trainer.
    Trained,
    /// A canonical trailing window of the corpus, used verbatim.
    RawContent,
    /// A dictionary synthesized from the bundle's own interned term vocabulary.
    TermTable,
}

impl DictionaryStrategy {
    /// The `gmeow:` individual naming this strategy.
    #[must_use]
    pub fn iri(self) -> String {
        let local = match self {
            Self::Trained => "dictStrategyTrained",
            Self::RawContent => "dictStrategyRawContent",
            Self::TermTable => "dictStrategyTermTable",
        };
        format!("{GMEOW}{local}")
    }

    fn from_iri(iri: &str) -> Option<Self> {
        match iri.strip_prefix(GMEOW)? {
            "dictStrategyTrained" => Some(Self::Trained),
            "dictStrategyRawContent" => Some(Self::RawContent),
            "dictStrategyTermTable" => Some(Self::TermTable),
            _ => None,
        }
    }
}

impl fmt::Display for DictionaryStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Trained => "trained",
            Self::RawContent => "raw-content",
            Self::TermTable => "term-table",
        })
    }
}

/// How a medium resolves the dictionary for a payload (`gmeow:MediumSourceKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MediumSourceKind {
    /// One dictionary per registered payload schema.
    PerRep,
    /// The dictionary the segment header pins.
    HeaderDict,
    /// One medium for the whole artifact.
    WholeArtifact,
}

impl MediumSourceKind {
    fn from_iri(iri: &str) -> Option<Self> {
        match iri.strip_prefix(GMEOW)? {
            "mediumSourcePerRep" => Some(Self::PerRep),
            "mediumSourceHeaderDict" => Some(Self::HeaderDict),
            "mediumSourceWholeArtifact" => Some(Self::WholeArtifact),
            _ => None,
        }
    }
}

/// The AUTHORED half of a dictionary: everything a human writes down, and nothing
/// the build measures. Deliberately no digest / byte length / `Dictionary_ID` —
/// those live on the generated realization, because requiring them here would be an
/// unsatisfiable obligation (the digest cannot exist before training).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryDef {
    /// The `gmeow:CompressionDictionary` individual's IRI.
    pub iri: String,
    /// `gmeow:dictionaryId` — the stable pack-dictionary name a frame cites.
    pub id: String,
    /// `gmeow:dictionaryVersion`. A dictionary id WITHOUT its version does not
    /// identify a decodable dictionary: a zstd dictionary is a verbatim substring
    /// table of its corpus, so two versions of one id share no guarantee.
    pub version: String,
    /// `gmeow:dictionaryStrategy` — the AUTHORED intent.
    pub strategy: DictionaryStrategy,
    /// `gmeow:dictionaryTargetLength` — the requested dictionary size in bytes.
    pub target_length: usize,
    /// `gmeow:trainsOverCorpus` — the corpus IRI whose selectors name the samples.
    pub corpus: String,
}

/// A declared training corpus: a SELECTOR, re-resolved every build, never a frozen
/// file list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusDef {
    /// The `gmeow:DictionaryCorpus` individual's IRI.
    pub iri: String,
    /// Its selectors, in canonical order. At least one — a corpus with none leaves
    /// its dictionary's training set undefined
    /// (`logic:DictionaryCorpusSelectorConstraint`).
    pub selectors: Vec<CorpusSelector>,
}

/// A registered payload representation (`gmeow:PayloadSchema`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDef {
    /// The `gmeow:PayloadSchema` individual's IRI.
    pub iri: String,
    /// `gmeow:payloadSchemaId` — the EXACT wire label the emitter writes.
    pub rep: String,
}

/// A declared medium: the lawful `(encode, decode)` pair's coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediumDef {
    /// The `gmeow:Medium` / `gmeow:ZstdDictMedium` individual's IRI.
    pub iri: String,
    /// `gmeow:mediumCodec`.
    pub codec: String,
    /// `gmeow:mediumZstdLevel` — declared on the MEDIUM, never passed per call:
    /// changing it changes every byte the medium produces.
    pub zstd_level: i32,
    /// `gmeow:mediumSourceKind` — the dictionary-resolution rule, as data.
    pub source_kind: MediumSourceKind,
    /// `gmeow:mediumDictionary` — the BOUND on what this medium may prime with.
    /// Empty is meaningful: it is the explicitly-declared no-dictionary medium.
    pub dictionaries: BTreeSet<String>,
    /// `gmeow:requiresReaderCapability` — the reader contract this medium raises.
    pub reader_capabilities: BTreeSet<String>,
}

/// Which dictionary primes a rep — TOTAL, never `Option`.
///
/// Mirrors [`purrdf::gts_compose::DictSelection`] deliberately: an `Option` would
/// let "the registry forgot this rep" and "this rep is deliberately unprimed" be the
/// same value, and they are not — the first is a bug that silently costs density,
/// the second is a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictSelection {
    /// Prime with this `gmeow:CompressionDictionary` (by IRI).
    Named(String),
    /// The declared no-dictionary selection: the assigned medium declares an empty
    /// `gmeow:mediumDictionary` set.
    Baseline,
}

/// One row of the rep→medium assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepAssignment {
    /// The `gmeow:PayloadSchema` IRI this row is keyed on.
    pub schema: String,
    /// The `gmeow:Medium` IRI the rep's payloads are written through.
    pub medium: String,
    /// The dictionary selection — never an absence.
    pub dictionary: DictSelection,
}

/// The typed medium registry: the whole medium axis, read once off the carrier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediumRegistry {
    dictionaries: BTreeMap<String, DictionaryDef>,
    dictionary_by_id: BTreeMap<String, String>,
    corpora: BTreeMap<String, CorpusDef>,
    schemas: BTreeMap<String, SchemaDef>,
    schema_by_rep: BTreeMap<String, String>,
    media: BTreeMap<String, MediumDef>,
    assignment: BTreeMap<String, RepAssignment>,
}

impl MediumRegistry {
    /// Read the whole medium axis off the in-memory carrier.
    ///
    /// # Errors
    /// Any declaration defect: a missing or duplicated exactly-one field, an
    /// unrecognized `gmeow:DictionaryStrategy` / `gmeow:MediumSourceKind` /
    /// selector individual, a dictionary whose corpus is not declared, an envelope
    /// naming an unregistered schema or dictionary, or two envelopes over one schema
    /// naming different media.
    pub fn from_dataset(ds: &RdfDataset) -> Result<Self, gmeow_errors::Diag> {
        let mut registry = Self::default();
        registry.read_corpora(ds)?;
        registry.read_dictionaries(ds)?;
        registry.read_schemas(ds)?;
        registry.read_media(ds)?;
        registry.read_assignment(ds)?;
        Ok(registry)
    }

    /// Every declared dictionary, by IRI.
    #[must_use]
    pub fn dictionaries(&self) -> &BTreeMap<String, DictionaryDef> {
        &self.dictionaries
    }

    /// Every declared corpus, by IRI.
    #[must_use]
    pub fn corpora(&self) -> &BTreeMap<String, CorpusDef> {
        &self.corpora
    }

    /// Every registered payload schema, by IRI.
    #[must_use]
    pub fn schemas(&self) -> &BTreeMap<String, SchemaDef> {
        &self.schemas
    }

    /// Every declared medium, by IRI.
    #[must_use]
    pub fn media(&self) -> &BTreeMap<String, MediumDef> {
        &self.media
    }

    /// The rep→medium assignment, keyed by `gmeow:payloadSchemaId`.
    #[must_use]
    pub fn assignment(&self) -> &BTreeMap<String, RepAssignment> {
        &self.assignment
    }

    /// The dictionary an id resolves to.
    ///
    /// # Errors
    /// `MediumUnknownDictionary` when no registered dictionary carries that id.
    pub fn dictionary_by_id(&self, id: &str) -> Result<&DictionaryDef, gmeow_errors::Diag> {
        self.dictionary_by_id
            .get(id)
            .and_then(|iri| self.dictionaries.get(iri))
            .ok_or_else(|| {
                super::unknown_dictionary(format!(
                    "dictionary id {id:?} resolves to no registered gmeow:CompressionDictionary \
                     (registered: {:?}) — there is NO fallback to a dictionary-less decode, \
                     because priming changes the code, not the framing",
                    self.dictionary_by_id.keys().collect::<Vec<_>>()
                ))
            })
    }

    /// The registered schema a wire rep label resolves to.
    ///
    /// # Errors
    /// `MediumUnknownSchema` when the rep is not registered. There is no defaultable
    /// answer: an unregistered rep would decode as an unclassified blob whose medium
    /// assignment is UNDEFINED.
    pub fn schema_for(&self, rep: &str) -> Result<&SchemaDef, gmeow_errors::Diag> {
        self.schema_by_rep
            .get(rep)
            .and_then(|iri| self.schemas.get(iri))
            .ok_or_else(|| {
                unknown_schema(format!(
                    "blob representation {rep:?} has no registered gmeow:PayloadSchema — mint the \
                     schema individual in the same change that adds the archive; there is no \
                     default medium for an unknown rep"
                ))
            })
    }

    /// The assignment for a wire rep label. TOTAL by contract, so both ways of
    /// failing are NAMED rather than defaulted.
    ///
    /// # Errors
    /// `MediumUnknownSchema` when the rep is unregistered;
    /// `MediumUndeclaredDictionary` when it is registered but its schema declares
    /// no `gmeow:payloadSchemaMedium`.
    pub fn assignment_for(&self, rep: &str) -> Result<&RepAssignment, gmeow_errors::Diag> {
        // Order matters: an UNREGISTERED rep is a registry defect, and reporting it
        // as "undeclared" would send the reader to the wrong fix.
        self.schema_for(rep)?;
        self.assignment.get(rep).ok_or_else(|| {
            undeclared_dictionary(format!(
                "blob representation {rep:?} is registered but its gmeow:PayloadSchema declares no \
                 gmeow:payloadSchemaMedium — the rep→medium assignment is TOTAL, so a missing row \
                 is a missing DECLARATION, never permission to encode the payload unprimed"
            ))
        })
    }

    /// Render the assignment as the [`purrdf::gts_compose::MediumPlan`] the
    /// authorship door ([`gmeow_gts_profile::emit_gmeow_gts_with_medium`]) consumes.
    ///
    /// `reps` is the set of blob representations the emission will actually author;
    /// every one of them must resolve, and the snapshot slot is taken from the
    /// [`SNAPSHOT_WIRE_REP`] assignment rather than from a hardcoded default — the
    /// snapshot frame is a payload like any other, and giving it a private rule
    /// would put a second source of truth beside the registry.
    ///
    /// `trained` maps a `gmeow:dictionaryId` to its trained bytes. EVERY declared
    /// dictionary is pinned in the pack's `"dct"` map, not merely the ones this
    /// emission's reps select: the bundle is the distribution channel for the
    /// dictionary family itself. `gmeow-memory-hot-v1` and
    /// `gmeow-memory-compact-v1` prime a consumer's OWN runtime store, so the only
    /// place a consumer can obtain them is the shipped header — pinning only the
    /// selected ones would ship a bundle that declares seven dictionaries and
    /// carries five, leaving the other two nameable but unobtainable.
    ///
    /// # Errors
    /// An unregistered or unassigned rep, a declared dictionary with no trained
    /// bytes, or two assigned media declaring different zstd levels (the plan
    /// carries ONE level, so a disagreement has no answer).
    pub fn medium_plan(
        &self,
        reps: &[String],
        trained: &BTreeMap<String, Vec<u8>>,
    ) -> Result<WireMediumPlan, gmeow_errors::Diag> {
        let mut assignment: BTreeMap<FrameSlot, WireDictSelection> = BTreeMap::new();
        let mut pinned: BTreeSet<String> = BTreeSet::new();
        let mut level: Option<(i32, String)> = None;

        for def in self.dictionaries.values() {
            if !trained.contains_key(&def.id) {
                return Err(undeclared_dictionary(format!(
                    "dictionary <{}> ({:?}) is declared but no trained bytes were supplied — the \
                     shipped pack pins every declared dictionary, so a declaration with no bytes \
                     would ship a header naming a dictionary the pack does not carry",
                    def.iri, def.id
                )));
            }
            pinned.insert(def.id.clone());
        }

        let slots = reps
            .iter()
            .map(|rep| (FrameSlot::Blob(rep.clone()), rep.as_str()))
            .chain(std::iter::once((FrameSlot::Snapshot, SNAPSHOT_WIRE_REP)));

        for (slot, rep) in slots {
            let row = self.assignment_for(rep)?;
            let medium = self.media.get(&row.medium).ok_or_else(|| {
                invalid_declaration(format!(
                    "the assignment for rep {rep:?} names medium <{}>, which is not a declared \
                     gmeow:Medium",
                    row.medium
                ))
            })?;
            match &level {
                None => level = Some((medium.zstd_level, medium.iri.clone())),
                Some((declared, first)) if *declared != medium.zstd_level => {
                    return Err(invalid_declaration(format!(
                        "media <{first}> (level {declared}) and <{}> (level {}) are both assigned \
                         in this emission, but a GTS segment declares ONE zstd level in its codec \
                         catalog — two levels in one bundle has no answer",
                        medium.iri, medium.zstd_level
                    )));
                }
                Some(_) => {}
            }

            let selection = match &row.dictionary {
                DictSelection::Baseline => WireDictSelection::Baseline,
                DictSelection::Named(iri) => {
                    let def = self.dictionaries.get(iri).ok_or_else(|| {
                        super::unknown_dictionary(format!(
                            "the assignment for rep {rep:?} names dictionary <{iri}>, which is \
                             not a registered gmeow:CompressionDictionary"
                        ))
                    })?;
                    if !trained.contains_key(&def.id) {
                        return Err(undeclared_dictionary(format!(
                            "rep {rep:?} selects dictionary {:?}, but no trained bytes were \
                             supplied for it — a selected dictionary with no bytes would emit a \
                             frame citing a dictionary the pack does not carry",
                            def.id
                        )));
                    }
                    WireDictSelection::Named(def.id.clone())
                }
            };
            assignment.insert(slot, selection);
        }

        let dicts = pinned
            .into_iter()
            .map(|id| {
                let bytes = trained.get(&id).cloned().expect("checked above");
                (id, bytes)
            })
            .collect();

        Ok(WireMediumPlan {
            dicts,
            assignment,
            zstd_level: level.map(|(level, _)| level),
        })
    }

    fn read_corpora(&mut self, ds: &RdfDataset) -> Result<(), gmeow_errors::Diag> {
        for subject in subjects_of_type(ds, &gm("DictionaryCorpus")) {
            let iri = require_iri(ds, subject)?;
            let selectors = super::corpus::selectors_of(ds, subject, &iri)?;
            self.corpora
                .insert(iri.clone(), CorpusDef { iri, selectors });
        }
        Ok(())
    }

    fn read_dictionaries(&mut self, ds: &RdfDataset) -> Result<(), gmeow_errors::Diag> {
        for subject in subjects_of_type(ds, &gm("CompressionDictionary")) {
            let iri = require_iri(ds, subject)?;
            let id = one_literal(ds, subject, &gm("dictionaryId"), &iri)?;
            let version = one_literal(ds, subject, &gm("dictionaryVersion"), &iri)?;
            let target = one_literal(ds, subject, &gm("dictionaryTargetLength"), &iri)?;
            let target_length: usize = target.parse().map_err(|_| {
                invalid_declaration(format!(
                    "<{iri}> gmeow:dictionaryTargetLength {target:?} is not a non-negative integer"
                ))
            })?;
            let strategy_iri = one_iri(ds, subject, &gm("dictionaryStrategy"), &iri)?;
            let strategy = DictionaryStrategy::from_iri(&strategy_iri).ok_or_else(|| {
                invalid_declaration(format!(
                    "<{iri}> gmeow:dictionaryStrategy <{strategy_iri}> is not a recognized \
                     gmeow:DictionaryStrategy individual (trained / raw-content / term-table)"
                ))
            })?;
            let corpus = one_iri(ds, subject, &gm("trainsOverCorpus"), &iri)?;
            if !self.corpora.contains_key(&corpus) {
                return Err(invalid_declaration(format!(
                    "<{iri}> gmeow:trainsOverCorpus <{corpus}>, which is not a declared \
                     gmeow:DictionaryCorpus — an untrained dictionary id names nothing a decoder \
                     can resolve"
                )));
            }
            if let Some(previous) = self.dictionary_by_id.insert(id.clone(), iri.clone()) {
                return Err(invalid_declaration(format!(
                    "dictionary id {id:?} is declared by both <{previous}> and <{iri}> — an id \
                     that resolves to two definitions cannot prime a decode"
                )));
            }
            self.dictionaries.insert(
                iri.clone(),
                DictionaryDef {
                    iri,
                    id,
                    version,
                    strategy,
                    target_length,
                    corpus,
                },
            );
        }
        Ok(())
    }

    fn read_schemas(&mut self, ds: &RdfDataset) -> Result<(), gmeow_errors::Diag> {
        for subject in subjects_of_type(ds, &gm("PayloadSchema")) {
            let iri = require_iri(ds, subject)?;
            let rep = one_literal(ds, subject, &gm("payloadSchemaId"), &iri)?;
            if let Some(previous) = self.schema_by_rep.insert(rep.clone(), iri.clone()) {
                return Err(invalid_declaration(format!(
                    "wire rep {rep:?} is registered by both <{previous}> and <{iri}> — the \
                     rep→schema map is the join key the carrier's representation constants are \
                     enumerated against, so it must be injective"
                )));
            }
            self.schemas.insert(iri.clone(), SchemaDef { iri, rep });
        }
        Ok(())
    }

    fn read_media(&mut self, ds: &RdfDataset) -> Result<(), gmeow_errors::Diag> {
        // `gmeow:ZstdDictMedium` is the zstd specialization of `gmeow:Medium`; the
        // carrier may carry either or both type assertions, so collect the union
        // rather than picking one and silently missing the other spelling.
        let mut subjects: BTreeSet<TermId> = BTreeSet::new();
        for class in ["Medium", "ZstdDictMedium"] {
            subjects.extend(subjects_of_type(ds, &gm(class)));
        }
        for subject in subjects {
            let iri = require_iri(ds, subject)?;
            let codec = one_iri(ds, subject, &gm("mediumCodec"), &iri)?;
            let source_iri = one_iri(ds, subject, &gm("mediumSourceKind"), &iri)?;
            let source_kind = MediumSourceKind::from_iri(&source_iri).ok_or_else(|| {
                invalid_declaration(format!(
                    "<{iri}> gmeow:mediumSourceKind <{source_iri}> is not a recognized \
                     gmeow:MediumSourceKind individual (per-rep / header-dict / whole-artifact)"
                ))
            })?;
            let level = one_literal(ds, subject, &gm("mediumZstdLevel"), &iri)?;
            let zstd_level: i32 = level.parse().map_err(|_| {
                invalid_declaration(format!(
                    "<{iri}> gmeow:mediumZstdLevel {level:?} is not an integer (the zstd level \
                     space includes negative fast levels, so xsd:integer is deliberate)"
                ))
            })?;
            let dictionaries = iri_objects(ds, subject, &gm("mediumDictionary"));
            let reader_capabilities = literal_objects(ds, subject, &gm("requiresReaderCapability"));
            self.media.insert(
                iri.clone(),
                MediumDef {
                    iri,
                    codec,
                    zstd_level,
                    source_kind,
                    dictionaries,
                    reader_capabilities,
                },
            );
        }
        Ok(())
    }

    /// Read the AUTHORED rep→medium assignment off the `gmeow:PayloadSchema`
    /// registry: `gmeow:payloadSchemaMedium` (exactly-one) plus, under a medium
    /// that declares a dictionary set, `gmeow:payloadSchemaDictionary`.
    ///
    /// The assignment is read from the SCHEMA, never from an emitted
    /// `gmeow:MediumEnvelope`. An envelope is the PROJECTION of a frame this build
    /// actually wrote, so sourcing the assignment from one would make the
    /// assignment a product of the emission it is supposed to govern — and a rep
    /// that happened not to be emitted this run would have no medium at all.
    /// Reading the schema instead makes the assignment TOTAL by construction:
    /// every emittable rep already has a registered schema.
    fn read_assignment(&mut self, ds: &RdfDataset) -> Result<(), gmeow_errors::Diag> {
        let schemas: Vec<(String, String)> = self
            .schemas
            .values()
            .map(|schema| (schema.iri.clone(), schema.rep.clone()))
            .collect();
        for (schema_iri, rep) in schemas {
            let Some(subject) = iri_id(ds, &schema_iri) else {
                continue;
            };
            let declared_media: Vec<String> = iri_objects(ds, subject, &gm("payloadSchemaMedium"))
                .into_iter()
                .collect();
            let medium_iri = match declared_media.len() {
                // No row: the schema is registered but UNASSIGNED. That is a
                // declaration gap, and it is reported where it is actionable — at
                // `assignment_for`, naming the rep the emission actually reached
                // for — rather than by refusing to build the registry at all,
                // which would hide every other row behind one missing one.
                0 => continue,
                1 => declared_media.into_iter().next().expect("length checked"),
                n => {
                    return Err(undeclared_dictionary(format!(
                        "<{schema_iri}> (rep {rep:?}) declares {n} gmeow:payloadSchemaMedium \
                         values — the rep→medium map is a FUNCTION \
                         (logic:MediumSchemaMediumFunctionalityConstraint), so a rep two \
                         assignments disagree about has no derivable medium and there is nothing \
                         to pick between them"
                    )));
                }
            };
            let medium = self.media.get(&medium_iri).ok_or_else(|| {
                invalid_declaration(format!(
                    "<{schema_iri}> gmeow:payloadSchemaMedium <{medium_iri}>, which is not a \
                     declared gmeow:Medium"
                ))
            })?;

            // `gmeow:payloadSchemaDictionary` is exactly-one on a rep assigned a
            // dictionary-declaring medium. Its absence is legal in exactly one
            // situation — the assigned medium declares an empty dictionary set,
            // i.e. the explicitly-declared no-dictionary medium, where "no
            // dictionary" IS the selection. Anywhere else it is
            // gmeow:MediumUndeclaredDictionary.
            let declared = iri_objects(ds, subject, &gm("payloadSchemaDictionary"));
            let dictionary = match declared.len() {
                0 if medium.dictionaries.is_empty() => DictSelection::Baseline,
                0 => {
                    return Err(undeclared_dictionary(format!(
                        "<{schema_iri}> (rep {rep:?}) declares no gmeow:payloadSchemaDictionary, \
                         but its medium <{medium_iri}> declares {} — an undeclared dictionary is \
                         undiscoverable, so every payload it primes would be permanently \
                         undecodable even with its bytes intact",
                        medium.dictionaries.len()
                    )));
                }
                1 => {
                    let dict_iri = declared.into_iter().next().expect("length checked");
                    if !self.dictionaries.contains_key(&dict_iri) {
                        return Err(super::unknown_dictionary(format!(
                            "<{schema_iri}> gmeow:payloadSchemaDictionary <{dict_iri}>, which is \
                             not a registered gmeow:CompressionDictionary"
                        )));
                    }
                    if !medium.dictionaries.contains(&dict_iri) {
                        return Err(super::unknown_dictionary(format!(
                            "<{schema_iri}> selects <{dict_iri}>, which its medium <{medium_iri}> \
                             does not declare — the medium's gmeow:mediumDictionary set is the \
                             BOUND on what it may prime with"
                        )));
                    }
                    DictSelection::Named(dict_iri)
                }
                n => {
                    return Err(undeclared_dictionary(format!(
                        "<{schema_iri}> declares {n} gmeow:payloadSchemaDictionary values — a \
                         payload primed with two dictionaries is incoherent, and a declaration \
                         missing (or doubling) a coordinate is a DIFFERENT claim rather than a \
                         weaker one"
                    )));
                }
            };

            self.assignment.insert(
                rep,
                RepAssignment {
                    schema: schema_iri,
                    medium: medium_iri,
                    dictionary,
                },
            );
        }
        Ok(())
    }
}

/// `gmeow:<local>`.
pub(crate) fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn iri_id(ds: &RdfDataset, iri: &str) -> Option<TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

/// Every subject asserted to be of `class`, in canonical term-id order.
fn subjects_of_type(ds: &RdfDataset, class: &str) -> Vec<TermId> {
    let (Some(type_id), Some(class_id)) = (iri_id(ds, RDF_TYPE), iri_id(ds, class)) else {
        return Vec::new();
    };
    // `BTreeSet` both deduplicates (a subject may be typed in several graphs) and
    // fixes the traversal order, so the registry is a pure function of the dataset
    // rather than of quad-table layout.
    ds.quads_for_pattern(None, Some(type_id), Some(class_id), GraphMatch::Any)
        .map(|q| q.s)
        .collect::<BTreeSet<TermId>>()
        .into_iter()
        .collect()
}

/// The objects of `subject predicate ?o`.
pub(crate) fn objects(ds: &RdfDataset, subject: TermId, predicate: &str) -> Vec<TermId> {
    let Some(p) = iri_id(ds, predicate) else {
        return Vec::new();
    };
    ds.quads_for_pattern(Some(subject), Some(p), None, GraphMatch::Any)
        .map(|q| q.o)
        .collect()
}

fn iri_objects(ds: &RdfDataset, subject: TermId, predicate: &str) -> BTreeSet<String> {
    objects(ds, subject, predicate)
        .into_iter()
        .filter_map(|o| match ds.resolve(o) {
            TermRef::Iri(iri) => Some(iri.to_string()),
            _ => None,
        })
        .collect()
}

fn literal_objects(ds: &RdfDataset, subject: TermId, predicate: &str) -> BTreeSet<String> {
    objects(ds, subject, predicate)
        .into_iter()
        .filter_map(|o| match ds.resolve(o) {
            TermRef::Literal { lexical, .. } => Some(lexical.to_string()),
            _ => None,
        })
        .collect()
}

/// A subject's own IRI. A blank-node medium individual is rejected: every record on
/// this axis is cited by IRI from somewhere else (an envelope, a realization, a
/// frame header), and a blank node cannot be cited.
fn require_iri(ds: &RdfDataset, subject: TermId) -> Result<String, gmeow_errors::Diag> {
    match ds.resolve(subject) {
        TermRef::Iri(iri) => Ok(iri.to_string()),
        other => Err(invalid_declaration(format!(
            "a medium-axis individual is not an IRI ({other:?}) — every record on this axis is \
             cited by IRI from elsewhere, and a blank node cannot be cited"
        ))),
    }
}

/// The single literal value of an exactly-one datatype property.
fn one_literal(
    ds: &RdfDataset,
    subject: TermId,
    predicate: &str,
    subject_iri: &str,
) -> Result<String, gmeow_errors::Diag> {
    exactly_one(
        literal_objects(ds, subject, predicate)
            .into_iter()
            .collect(),
        predicate,
        subject_iri,
    )
}

/// The single IRI value of an exactly-one object property.
fn one_iri(
    ds: &RdfDataset,
    subject: TermId,
    predicate: &str,
    subject_iri: &str,
) -> Result<String, gmeow_errors::Diag> {
    exactly_one(
        iri_objects(ds, subject, predicate).into_iter().collect(),
        predicate,
        subject_iri,
    )
}

fn exactly_one(
    values: Vec<String>,
    predicate: &str,
    subject_iri: &str,
) -> Result<String, gmeow_errors::Diag> {
    match values.len() {
        1 => Ok(values.into_iter().next().expect("length checked")),
        n => Err(invalid_declaration(format!(
            "<{subject_iri}> declares {n} value(s) for <{predicate}>, which is exactly-one on this \
             class — a record missing (or doubling) a coordinate is a DIFFERENT claim, not a \
             weaker one, so there is nothing to default to"
        ))),
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    //! A minimal but COMPLETE medium declaration, in the same shape
    //! `slices/core/gts/module.ttl` carries. Used by every module in `medium::`, so
    //! the tests exercise the real reader against real Turtle rather than a
    //! hand-built registry struct that could drift from what the carrier says.

    use std::sync::Arc;

    use purrdf::RdfDataset;

    /// The declaration under test, with `{extra}` spliced in for the negative cases.
    pub(crate) fn turtle(extra: &str) -> String {
        format!(
            r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:corpusCore a gmeow:DictionaryCorpus ;
    gmeow:corpusSelectsBlobRep "cells-archive" ;
    gmeow:corpusSelectsPathPrefix "slices/core/" .

gmeow:corpusTerms a gmeow:DictionaryCorpus ;
    gmeow:corpusSelectsGraph <https://blackcatinformatics.ca/gmeow/graph/statements> .

gmeow:dictCore a gmeow:CompressionDictionary ;
    gmeow:dictionaryId "gmeow-core-v1" ;
    gmeow:dictionaryVersion "1" ;
    gmeow:dictionaryStrategy gmeow:dictStrategyTrained ;
    gmeow:dictionaryTargetLength 4096 ;
    gmeow:trainsOverCorpus gmeow:corpusCore .

gmeow:dictTerms a gmeow:CompressionDictionary ;
    gmeow:dictionaryId "gmeow-terms-v1" ;
    gmeow:dictionaryVersion "1" ;
    gmeow:dictionaryStrategy gmeow:dictStrategyTermTable ;
    gmeow:dictionaryTargetLength 4096 ;
    gmeow:trainsOverCorpus gmeow:corpusTerms .

gmeow:payloadSchemaCells a gmeow:PayloadSchema ; gmeow:payloadSchemaId "cells-archive" ;
    gmeow:payloadSchemaMedium gmeow:mediumDist ;
    gmeow:payloadSchemaDictionary gmeow:dictCore .
gmeow:payloadSchemaSnapshot a gmeow:PayloadSchema ; gmeow:payloadSchemaId "gmeow:snapshot/wire" ;
    gmeow:payloadSchemaMedium gmeow:mediumBaseline .
# Registered but DELIBERATELY unassigned: the negative case for a rep that has a
# schema and no gmeow:payloadSchemaMedium.
gmeow:payloadSchemaOrphan a gmeow:PayloadSchema ; gmeow:payloadSchemaId "orphan-archive" .

gmeow:mediumDist a gmeow:ZstdDictMedium ;
    gmeow:mediumCodec gmeow:codecZstdRsyncable ;
    gmeow:mediumZstdLevel 12 ;
    gmeow:mediumSourceKind gmeow:mediumSourcePerRep ;
    gmeow:requiresReaderCapability "zstd-dictionary" , "zstd-rsyncable" ;
    gmeow:mediumDictionary gmeow:dictCore , gmeow:dictTerms .

gmeow:mediumBaseline a gmeow:ZstdDictMedium ;
    gmeow:mediumCodec gmeow:codecZstdRsyncable ;
    gmeow:mediumZstdLevel 12 ;
    gmeow:mediumSourceKind gmeow:mediumSourceWholeArtifact ;
    gmeow:requiresReaderCapability "zstd-rsyncable" .
{extra}
"#
        )
    }

    pub(crate) fn dataset(extra: &str) -> Arc<RdfDataset> {
        purrdf::parse_dataset(turtle(extra).as_bytes(), "text/turtle", None)
            .expect("the medium fixture parses as Turtle")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry(extra: &str) -> MediumRegistry {
        MediumRegistry::from_dataset(&fixture::dataset(extra)).expect("fixture registry")
    }

    fn error(extra: &str) -> gmeow_errors::Diag {
        MediumRegistry::from_dataset(&fixture::dataset(extra))
            .expect_err("the fixture addition must be rejected")
    }

    #[test]
    fn reads_dictionaries_corpora_schemas_media_and_the_assignment() {
        let registry = registry("");
        assert_eq!(registry.dictionaries().len(), 2);
        let core = registry
            .dictionary_by_id("gmeow-core-v1")
            .expect("registered by id");
        assert_eq!(core.version, "1");
        assert_eq!(core.strategy, DictionaryStrategy::Trained);
        assert_eq!(core.target_length, 4096);
        assert_eq!(core.corpus, gm("corpusCore"));
        assert_eq!(
            registry
                .dictionary_by_id("gmeow-terms-v1")
                .expect("registered")
                .strategy,
            DictionaryStrategy::TermTable
        );

        assert_eq!(registry.corpora().len(), 2);
        assert_eq!(registry.schemas().len(), 3);
        assert_eq!(registry.media().len(), 2);

        let dist = registry.media().get(&gm("mediumDist")).expect("declared");
        assert_eq!(dist.zstd_level, 12);
        assert_eq!(dist.source_kind, MediumSourceKind::PerRep);
        assert_eq!(dist.dictionaries.len(), 2);
        assert!(dist.reader_capabilities.contains("zstd-dictionary"));

        let cells = registry.assignment_for("cells-archive").expect("assigned");
        assert_eq!(cells.dictionary, DictSelection::Named(gm("dictCore")));
        // The baseline medium declares NO dictionary, and that IS its selection.
        let snapshot = registry
            .assignment_for(SNAPSHOT_WIRE_REP)
            .expect("assigned");
        assert_eq!(snapshot.dictionary, DictSelection::Baseline);
    }

    /// A rep with no registered schema and a registered rep with no assignment are
    /// DIFFERENT defects with different fixes, so they raise different classes.
    #[test]
    fn an_unknown_rep_and_an_unassigned_rep_raise_different_failures() {
        let registry = registry("");
        assert_eq!(
            registry
                .assignment_for("never-registered")
                .expect_err("unregistered rep")
                .code(),
            crate::error::MediumUnknownSchema::register()
        );
        assert_eq!(
            registry
                .assignment_for("orphan-archive")
                .expect_err("registered but unassigned rep")
                .code(),
            crate::error::MediumUndeclaredDictionary::register()
        );
    }

    /// A rep assigned a dictionary-declaring medium but selecting no dictionary is
    /// `MediumUndeclaredDictionary` — the baseline exemption is scoped to a medium
    /// that declares an EMPTY dictionary set, not to "any schema that forgot the
    /// field".
    #[test]
    fn an_assigned_rep_with_no_dictionary_selection_is_undeclared() {
        let diag = error("gmeow:payloadSchemaOrphan gmeow:payloadSchemaMedium gmeow:mediumDist .");
        assert_eq!(
            diag.code(),
            crate::error::MediumUndeclaredDictionary::register(),
            "{diag}"
        );
    }

    /// A dictionary the medium does not declare is outside the medium's BOUND.
    #[test]
    fn a_rep_selecting_outside_the_medium_bound_is_unknown() {
        let diag = error(
            "gmeow:dictStray a gmeow:CompressionDictionary ;\n\
             \x20   gmeow:dictionaryId \"stray-v1\" ; gmeow:dictionaryVersion \"1\" ;\n\
             \x20   gmeow:dictionaryStrategy gmeow:dictStrategyTrained ;\n\
             \x20   gmeow:dictionaryTargetLength 4096 ;\n\
             \x20   gmeow:trainsOverCorpus gmeow:corpusCore .\n\
             gmeow:payloadSchemaOrphan gmeow:payloadSchemaMedium gmeow:mediumDist ;\n\
             \x20   gmeow:payloadSchemaDictionary gmeow:dictStray .",
        );
        assert_eq!(
            diag.code(),
            crate::error::MediumUnknownDictionary::register(),
            "{diag}"
        );
    }

    /// Two `gmeow:payloadSchemaMedium` values on one schema leave that rep's medium
    /// underivable — which is exactly the per-call decision the axis exists to
    /// remove, so it is rejected rather than resolved by precedence.
    #[test]
    fn a_rep_assigned_two_media_is_rejected() {
        let diag =
            error("gmeow:payloadSchemaCells gmeow:payloadSchemaMedium gmeow:mediumBaseline .");
        assert_eq!(
            diag.code(),
            crate::error::MediumUndeclaredDictionary::register(),
            "{diag}"
        );
        assert!(
            diag.to_string()
                .contains("2 gmeow:payloadSchemaMedium values"),
            "{diag}"
        );
    }

    /// A rep whose assignment names an undeclared medium has no round-trip law.
    #[test]
    fn a_rep_assigned_an_undeclared_medium_is_rejected() {
        let diag =
            error("gmeow:payloadSchemaOrphan gmeow:payloadSchemaMedium gmeow:mediumInvented .");
        assert_eq!(
            diag.code(),
            crate::error::InvalidDeclaration::register(),
            "{diag}"
        );
        assert!(diag.to_string().contains("mediumInvented"), "{diag}");
    }

    /// An exactly-one field that is missing has nothing to default to.
    #[test]
    fn a_dictionary_missing_an_exactly_one_field_is_rejected() {
        let diag = error(
            "gmeow:dictNoVersion a gmeow:CompressionDictionary ;\n\
             \x20   gmeow:dictionaryId \"no-version-v1\" ;\n\
             \x20   gmeow:dictionaryStrategy gmeow:dictStrategyTrained ;\n\
             \x20   gmeow:dictionaryTargetLength 4096 ;\n\
             \x20   gmeow:trainsOverCorpus gmeow:corpusCore .",
        );
        assert!(diag.to_string().contains("dictionaryVersion"), "{diag}");
    }

    /// An unrecognized strategy individual is a hard fail, not a fallback to trained.
    #[test]
    fn an_unrecognized_strategy_individual_is_rejected() {
        let diag = error(
            "gmeow:dictWeird a gmeow:CompressionDictionary ;\n\
             \x20   gmeow:dictionaryId \"weird-v1\" ; gmeow:dictionaryVersion \"1\" ;\n\
             \x20   gmeow:dictionaryStrategy gmeow:dictStrategyInvented ;\n\
             \x20   gmeow:dictionaryTargetLength 4096 ;\n\
             \x20   gmeow:trainsOverCorpus gmeow:corpusCore .",
        );
        assert!(diag.to_string().contains("dictStrategyInvented"), "{diag}");
    }

    /// The trained bytes of every declared fixture dictionary — the plan pins them
    /// ALL, so a plan test must supply them all.
    fn trained_all() -> BTreeMap<String, Vec<u8>> {
        [
            ("gmeow-core-v1".to_string(), vec![1, 2, 3]),
            ("gmeow-terms-v1".to_string(), vec![4, 5]),
        ]
        .into()
    }

    #[test]
    fn the_medium_plan_renders_the_assignment_for_the_authorship_door() {
        let registry = registry("");
        let plan = registry
            .medium_plan(&["cells-archive".to_string()], &trained_all())
            .expect("plan");
        assert_eq!(plan.zstd_level, Some(12));
        // EVERY declared dictionary is pinned, not merely the selected one: the
        // pack is the distribution channel for the dictionary family, so a
        // declared-but-unselected dictionary must still be obtainable from it.
        assert_eq!(
            plan.dicts,
            vec![
                ("gmeow-core-v1".to_string(), vec![1, 2, 3]),
                ("gmeow-terms-v1".to_string(), vec![4, 5]),
            ]
        );
        assert_eq!(
            plan.assignment
                .get(&FrameSlot::Blob("cells-archive".into())),
            Some(&WireDictSelection::Named("gmeow-core-v1".into()))
        );
        // The snapshot slot is read from the registry, not defaulted.
        assert_eq!(
            plan.assignment.get(&FrameSlot::Snapshot),
            Some(&WireDictSelection::Baseline)
        );
    }

    /// A rep the emission actually authors but the registry does not assign is a
    /// HARD FAIL at plan time — the point where it is still fixable, rather than at
    /// decode time on a shipped artifact.
    #[test]
    fn the_medium_plan_hard_fails_on_an_unassigned_rep() {
        let registry = registry("");
        let diag = registry
            .medium_plan(&["orphan-archive".to_string()], &trained_all())
            .expect_err("an unassigned rep must not produce a plan");
        assert_eq!(
            diag.code(),
            crate::error::MediumUndeclaredDictionary::register(),
            "{diag}"
        );
        assert!(
            diag.to_string().contains("orphan-archive"),
            "the failure names the unassigned rep, not a missing dictionary: {diag}"
        );
    }

    /// The dictionary ids `slices/core/gts/module.ttl` ships, spelled out so the
    /// inventory is pinned in BOTH directions: a dropped dictionary and an added one
    /// each fail [`the_live_gts_slice_reads_as_a_complete_registry`].
    ///
    /// It is SEVEN, not eight. An earlier draft carried `gmeow-math-v1`, designed from
    /// slice names rather than from the bundle's frame layout; the mathematical named
    /// graphs are unioned into the SNAPSHOT payload, which is one frame already primed
    /// in full by `gmeow-core-v1`, so it primed zero reps. See the retirement note in
    /// the slice.
    const SHIPPED_DICTIONARY_IDS: [&str; 7] = [
        "gmeow-claims-v1",
        "gmeow-core-v1",
        "gmeow-lang-ast-v1",
        "gmeow-logic-v1",
        "gmeow-memory-compact-v1",
        "gmeow-memory-hot-v1",
        "gmeow-prooftrace-v1",
    ];

    /// NON-VACUITY: the reader is exercised against the REAL authored declaration,
    /// not only the fixture. The seven shipped dictionaries, their corpora, the
    /// payload-schema registry, and both declared media must all read cleanly — if
    /// the reader silently disagreed with `slices/core/gts/module.ttl`, every
    /// fixture-based test above would still pass.
    ///
    /// (The authored Turtle is parsed here because a unit test has no carrier to
    /// read; PRODUCTION always reads the in-memory dataset. That asymmetry is the
    /// point of the test — it proves the two agree.)
    #[test]
    fn the_live_gts_slice_reads_as_a_complete_registry() {
        let module = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("the pipeline crate lives under crates/")
            .join("slices/core/gts/module.ttl");
        let text = std::fs::read_to_string(&module).expect("the gts slice is readable");
        let ds = purrdf::parse_dataset(text.as_bytes(), "text/turtle", Some(GMEOW))
            .expect("the gts slice parses as Turtle");
        let registry = MediumRegistry::from_dataset(&ds).expect("the live medium axis reads");

        let ids: BTreeSet<&str> = registry
            .dictionaries()
            .values()
            .map(|d| d.id.as_str())
            .collect();
        // BOTH directions, and the count on its own line: a dropped dictionary orphans
        // every artifact primed with it, and a re-added eighth would be trained,
        // measured, pinned in the header, and projected onto a committed `.zdict` while
        // priming nothing — which is exactly how `gmeow-math-v1` shipped as dead weight
        // until it was measured. Neither may pass unnoticed.
        assert_eq!(
            ids.len(),
            SHIPPED_DICTIONARY_IDS.len(),
            "the bundle ships {} dictionaries; got {ids:?}",
            SHIPPED_DICTIONARY_IDS.len()
        );
        assert_eq!(
            ids,
            SHIPPED_DICTIONARY_IDS
                .into_iter()
                .collect::<BTreeSet<&str>>(),
            "the declared dictionary inventory drifted"
        );
        for id in SHIPPED_DICTIONARY_IDS {
            let def = registry.dictionary_by_id(id).expect("shipped dictionary");
            assert!(
                registry.corpora().contains_key(&def.corpus),
                "{id} trains over <{}>, which must be a declared corpus",
                def.corpus
            );
            assert!(def.target_length > 0, "{id} declares a zero target length");
        }

        // TOTALITY IN THE OTHER DIRECTION — the check that turns "gmeow-math-v1 primed
        // nothing" from a thing someone had to measure into a thing the gate refuses.
        // A dictionary has exactly two legitimate homes:
        //
        //   * it is selected by a registered `gmeow:PayloadSchema`, so some emitted
        //     frame is actually primed with it; or
        //   * it is bound by a `gmeow:mediumSourceHeaderDict` medium — the runtime-store
        //     family, whose frames are written by a CONSUMER out of the shipped header
        //     rather than by this emission, so no bundle rep names it.
        //
        // Anything else is a dictionary the bundle trains, measures, pins, and projects
        // while no payload cites it: pure dead weight, and (Constitution §18) high-entropy
        // bytes handed to a compressor for nothing.
        let primed: BTreeSet<&str> = registry
            .schemas()
            .values()
            .filter_map(
                |schema| match &registry.assignment_for(&schema.rep).ok()?.dictionary {
                    DictSelection::Named(iri) => {
                        registry.dictionaries().get(iri).map(|d| d.id.as_str())
                    }
                    DictSelection::Baseline => None,
                },
            )
            .collect();
        let consumer_primed: BTreeSet<&str> = registry
            .media()
            .values()
            .filter(|m| m.source_kind == MediumSourceKind::HeaderDict)
            .flat_map(|m| m.dictionaries.iter())
            .filter_map(|iri| registry.dictionaries().get(iri).map(|d| d.id.as_str()))
            .collect();
        for id in SHIPPED_DICTIONARY_IDS {
            assert!(
                primed.contains(id) || consumer_primed.contains(id),
                "{id} primes no registered gmeow:PayloadSchema and is bound by no \
                 header-dict medium — it would be trained, measured, pinned and projected \
                 while no frame cites it. Assign it to a rep or retire it; do NOT weaken \
                 this assertion (primed: {primed:?}, consumer-primed: {consumer_primed:?})"
            );
        }

        // The two memory dictionaries are the ones whose corpora must be
        // BUNDLE-INTERNAL rather than a user's runtime store: a zstd dictionary
        // carries verbatim substrings of its corpus, so this is a privacy property,
        // not a tidiness one.
        for id in ["gmeow-memory-compact-v1", "gmeow-memory-hot-v1"] {
            let def = registry.dictionary_by_id(id).expect("shipped dictionary");
            let corpus = registry.corpora().get(&def.corpus).expect("declared");
            for selector in &corpus.selectors {
                if let crate::medium::corpus::CorpusSelector::PathPrefix(prefix) = selector {
                    assert!(
                        !prefix.contains(".gmeow"),
                        "{id} must never train on a user's runtime store: {prefix}"
                    );
                }
            }
        }

        assert!(
            registry.schemas().len() >= 20,
            "one gmeow:PayloadSchema per emittable rep; got {}",
            registry.schemas().len()
        );
        assert!(
            registry
                .schemas()
                .values()
                .any(|s| s.rep == SNAPSHOT_WIRE_REP),
            "the snapshot wire schema is registered like any other payload"
        );
        // TOTALITY: the authored assignment covers EVERY registered rep. This is
        // the property that makes `MediumUndeclaredDictionary` unreachable on the
        // live tree — without it every real rep would raise it at emission time.
        for schema in registry.schemas().values() {
            let row = registry
                .assignment_for(&schema.rep)
                .unwrap_or_else(|err| panic!("rep {:?} is unassigned: {err}", schema.rep));
            assert!(
                registry.media().contains_key(&row.medium),
                "rep {:?} names undeclared medium <{}>",
                schema.rep,
                row.medium
            );
            match &row.dictionary {
                DictSelection::Named(iri) => assert!(
                    registry.dictionaries().contains_key(iri),
                    "rep {:?} selects unregistered dictionary <{iri}>",
                    schema.rep
                ),
                DictSelection::Baseline => assert!(
                    registry
                        .media()
                        .get(&row.medium)
                        .is_some_and(|m| m.dictionaries.is_empty()),
                    "rep {:?} is unprimed under a medium that declares dictionaries",
                    schema.rep
                ),
            }
        }
        // baseline + dist + store. The three are not decoration: each is the ONLY
        // medium under which one of the three `gmeow:MediumSourceKind` branches is
        // reachable, so a missing one would leave that branch with no live producer.
        assert_eq!(registry.media().len(), 3, "baseline + dist + store");
        let mut kinds: BTreeSet<MediumSourceKind> = BTreeSet::new();
        for medium in registry.media().values() {
            assert_eq!(
                medium.zstd_level, 12,
                "<{}> must declare the mandated level",
                medium.iri
            );
            assert!(
                !medium.reader_capabilities.is_empty(),
                "<{}> uses a non-baseline codec, so it must declare its reader contract",
                medium.iri
            );
            kinds.insert(medium.source_kind);
        }
        assert_eq!(
            kinds,
            [
                MediumSourceKind::PerRep,
                MediumSourceKind::HeaderDict,
                MediumSourceKind::WholeArtifact,
            ]
            .into_iter()
            .collect::<BTreeSet<MediumSourceKind>>(),
            "every declared gmeow:MediumSourceKind must be realized by a live medium"
        );
        // The store medium's bound is exactly the two memory dictionaries: a store
        // primed with anything else could never be re-primed by a consumer holding only
        // the shipped bundle.
        let store = registry
            .media()
            .get(&gm("mediumProfileStoreL12"))
            .expect("the store medium is declared");
        assert_eq!(store.source_kind, MediumSourceKind::HeaderDict);
        assert_eq!(
            store.dictionaries,
            [gm("dictGmeowMemoryCompactV1"), gm("dictGmeowMemoryHotV1")]
                .into_iter()
                .collect::<BTreeSet<String>>()
        );
    }

    /// A selected dictionary with no trained bytes would emit frames citing a
    /// dictionary the pack does not carry.
    #[test]
    fn the_medium_plan_hard_fails_when_a_selected_dictionary_has_no_bytes() {
        let registry = registry("");
        let diag = registry
            .medium_plan(&["cells-archive".to_string()], &BTreeMap::new())
            .expect_err("a selected dictionary with no bytes must not produce a plan");
        assert_eq!(
            diag.code(),
            crate::error::MediumUndeclaredDictionary::register(),
            "{diag}"
        );
    }
}
