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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
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

    /// The strategy a `gmeow:DictionaryStrategy` individual names, or `None` when the
    /// IRI is outside the declared vocabulary.
    pub(super) fn from_iri(iri: &str) -> Option<Self> {
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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CorpusDef {
    /// The `gmeow:DictionaryCorpus` individual's IRI.
    pub iri: String,
    /// Its selectors, in canonical order. At least one — a corpus with none leaves
    /// its dictionary's training set undefined
    /// (`logic:DictionaryCorpusSelectorConstraint`).
    pub selectors: Vec<CorpusSelector>,
}

/// The DECLARED held-out split (`gmeow:CorpusTrainingSplit`) every archive-backed
/// corpus is resolved under.
///
/// ONE individual governs EVERY corpus. It is declared once rather than per corpus
/// because a per-corpus knob is a per-dictionary carve-out with extra steps: the
/// dictionary whose evaluation most needs a held-out member is exactly the one whose
/// author would be tempted to widen its own training set.
///
/// The rule is a STRIDE over the corpus's members in canonical CONTENT-DIGEST order:
/// rank every member by its own `blake3:` digest, then hold out the ones whose rank is
/// congruent to [`Self::offset`] modulo [`Self::stride`]. Nothing about the archive's
/// order, its member names, or the build machine enters it, so the partition is
/// reproducible from the corpus alone.
///
/// A stride over the RANKS rather than over the digest VALUES, because the properness
/// of the split has to be a theorem rather than a probability. A residue rule
/// (`digest mod 8 == 0`) holds nothing out of a four-member corpus about 59% of the
/// time — and the shipped claim corpus is exactly four members — so on most builds the
/// split would silently do nothing, which is the failure mode it exists to prevent.
/// Ranking makes the held-out side non-empty for every corpus with more members than
/// the offset, and the training side non-empty for every corpus with at least two.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TrainingSplitDef {
    /// The `gmeow:CorpusTrainingSplit` individual's IRI.
    pub iri: String,
    /// `gmeow:splitHeldOutStride` — one member in every `stride`, in digest order, is
    /// held out.
    pub stride: u64,
    /// `gmeow:splitHeldOutOffset` — which position within each stride is the held-out
    /// one.
    pub offset: u64,
}

impl TrainingSplitDef {
    /// Rank a corpus's members: their canonical `blake3:` digests, ascending.
    ///
    /// The digest is the same [`super::blake3_digest`] rendering every other
    /// medium-axis digest uses, so a reader who can print a corpus's member digests
    /// can reproduce the ranking — and therefore the partition — without running this
    /// code.
    #[must_use]
    pub fn holds_out_rank(&self, rank: usize) -> bool {
        (rank as u64) % self.stride == self.offset
    }
}

/// A registered payload representation (`gmeow:PayloadSchema`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SchemaDef {
    /// The `gmeow:PayloadSchema` individual's IRI.
    pub iri: String,
    /// `gmeow:payloadSchemaId` — the EXACT wire label the emitter writes.
    pub rep: String,
}

/// A declared medium: the lawful `(encode, decode)` pair's coordinates.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

impl MediumDef {
    /// The WIRE codec name [`purrdf::gts::codec`] encodes with, derived from the
    /// declared `gmeow:mediumCodec` individual.
    ///
    /// The mapping is total over the catalog the GTS spec §8 registers and HARD-FAILS
    /// on anything else. There is no "unknown codec → skip" arm: a medium whose codec
    /// this build cannot spell would silently measure (or write) a chain the shipped
    /// artifact does not use.
    ///
    /// # Errors
    /// `InvalidDeclaration` when the codec individual is outside the registered
    /// catalog.
    pub fn codec_wire_name(&self) -> Result<&'static str, gmeow_errors::Diag> {
        let local = self.codec.strip_prefix(GMEOW).unwrap_or(&self.codec);
        match local {
            "codecIdentity" => Ok("identity"),
            "codecGzip" => Ok("gzip"),
            "codecZstd" => Ok("zstd"),
            "codecZstdRsyncable" => Ok("zstd-rsyncable"),
            _ => Err(invalid_declaration(format!(
                "<{}> gmeow:mediumCodec <{}> is not a codec this build can spell on the wire — \
                 there is no fallback chain, because a measurement (or an emission) through a \
                 different codec would describe bytes the shipped artifact never carries",
                self.iri, self.codec
            ))),
        }
    }
}

/// Which dictionary primes a rep — TOTAL, never `Option`.
///
/// Mirrors [`purrdf::gts_compose::DictSelection`] deliberately: an `Option` would
/// let "the registry forgot this rep" and "this rep is deliberately unprimed" be the
/// same value, and they are not — the first is a bug that silently costs density,
/// the second is a declaration.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DictSelection {
    /// Prime with this `gmeow:CompressionDictionary` (by IRI).
    Named(String),
    /// The declared no-dictionary selection: the assigned medium declares an empty
    /// `gmeow:mediumDictionary` set.
    Baseline,
}

/// WHICH declared medium an emission writes its frames through.
///
/// A first-class NAMED selection, never a boolean and never an empty registry. The
/// counterfactual half of the medium axis — "the same claim, written through the
/// declared no-dictionary medium" — has to be reachable to be checkable, and the only
/// honest way to reach it is to NAME the medium it is written through. Reaching it by
/// handing the emitter an empty [`MediumRegistry`] (or a bare
/// [`purrdf::gts_compose::MediumPlan::undicted`]) would be the legacy no-dict mode this
/// axis exists to remove: nothing on the artifact would say which medium it is, and a
/// registry that silently lost its dictionaries would look identical to a deliberate
/// baseline emission.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MediumSelection {
    /// The AUTHORED rep→medium assignment (`gmeow:payloadSchemaMedium` +
    /// `gmeow:payloadSchemaDictionary`) — the shipped distribution selection, under
    /// which every primed blob rep names `gmeow:mediumProfileDistL12`.
    Authored,
    /// ONE declared `gmeow:Medium` applied to EVERY frame slot.
    ///
    /// Legal only for a medium whose `gmeow:mediumDictionary` set is EMPTY — the
    /// explicitly-declared no-dictionary media, of which `gmeow:mediumProfileBaselineL12`
    /// is the shipped one. A dictionary-DECLARING medium resolves its dictionary per
    /// registered `gmeow:PayloadSchema` (`gmeow:mediumSourcePerRep`), which IS
    /// [`Self::Authored`]; applying it "uniformly" would have to invent a per-rep answer,
    /// so it is refused rather than defaulted.
    Uniform(String),
}

impl MediumSelection {
    /// The `gmeow:mediumProfileBaselineL12` uniform selection — the declared
    /// no-dictionary medium, spelled once so no caller has to spell the IRI.
    #[must_use]
    pub fn baseline_profile() -> Self {
        Self::Uniform(gm("mediumProfileBaselineL12"))
    }
}

/// One row of the rep→medium assignment.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RepAssignment {
    /// The `gmeow:PayloadSchema` IRI this row is keyed on.
    pub schema: String,
    /// The `gmeow:Medium` IRI the rep's payloads are written through.
    pub medium: String,
    /// The dictionary selection — never an absence.
    pub dictionary: DictSelection,
}

/// The typed medium registry: the whole medium axis, read once off the carrier.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MediumRegistry {
    dictionaries: BTreeMap<String, DictionaryDef>,
    dictionary_by_id: BTreeMap<String, String>,
    corpora: BTreeMap<String, CorpusDef>,
    split: Option<TrainingSplitDef>,
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
        registry.read_training_split(ds)?;
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

    /// The DECLARED held-out split every archive-backed corpus is resolved under.
    ///
    /// # Errors
    /// `InvalidDeclaration` when the carrier declares no `gmeow:CorpusTrainingSplit`.
    /// There is no implicit "train on everything" fallback: that is precisely the
    /// self-measured evaluation the split exists to remove, and it would be invisible.
    pub fn training_split(&self) -> Result<&TrainingSplitDef, gmeow_errors::Diag> {
        self.split.as_ref().ok_or_else(|| {
            invalid_declaration(
                "the carrier declares no gmeow:CorpusTrainingSplit — every archive-backed corpus \
                 is resolved under a DECLARED held-out split, and defaulting to 'train on every \
                 member' would silently restore the self-measured evaluation the split exists to \
                 remove",
            )
        })
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

    /// The assignment a rep's frames are written under for `selection`.
    ///
    /// TOTAL for both selections, and both ways of failing stay NAMED:
    /// [`MediumSelection::Authored`] answers with the authored row verbatim, while
    /// [`MediumSelection::Uniform`] answers with the named medium and its own
    /// (necessarily [`DictSelection::Baseline`]) selection — after proving the medium is
    /// DECLARED and declares no dictionary. The rep must still be registered and
    /// assigned under either selection: a uniform medium changes which medium the frame
    /// is written through, never whether the rep is one the registry knows.
    ///
    /// # Errors
    /// `MediumUnknownSchema` (unregistered rep), `MediumUndeclaredDictionary` (registered
    /// but unassigned), or `InvalidDeclaration` (the uniform medium is undeclared, or
    /// declares a dictionary set and therefore has no uniform per-rep answer).
    pub fn resolved_assignment(
        &self,
        selection: &MediumSelection,
        rep: &str,
    ) -> Result<RepAssignment, gmeow_errors::Diag> {
        let authored = self.assignment_for(rep)?;
        match selection {
            MediumSelection::Authored => Ok(authored.clone()),
            MediumSelection::Uniform(iri) => {
                let medium = self.media.get(iri).ok_or_else(|| {
                    invalid_declaration(format!(
                        "the uniform medium selection names <{iri}>, which is not a declared \
                         gmeow:Medium — a medium nothing declares cannot be the one an emission \
                         was written through"
                    ))
                })?;
                if !medium.dictionaries.is_empty() {
                    return Err(invalid_declaration(format!(
                        "<{iri}> declares {} gmeow:mediumDictionary value(s), so applying it \
                         uniformly to every frame slot has no derivable per-rep selection — a \
                         dictionary-declaring medium resolves its dictionary per registered \
                         gmeow:PayloadSchema (gmeow:mediumSourcePerRep), which IS the authored \
                         assignment. There is nothing to pick between its dictionaries here",
                        medium.dictionaries.len()
                    )));
                }
                Ok(RepAssignment {
                    schema: authored.schema.clone(),
                    medium: iri.clone(),
                    dictionary: DictSelection::Baseline,
                })
            }
        }
    }

    /// Render the assignment as the [`purrdf::gts_compose::MediumPlan`] the
    /// authorship door ([`gmeow_gts_profile::emit_gmeow_gts`]) consumes.
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
    /// selected ones would ship a bundle that declares five dictionaries and
    /// carries three, leaving the other two nameable but unobtainable.
    ///
    /// `selection` names WHICH declared medium the emission is written through. There
    /// is ONE plan door, taken by the shipped emission at
    /// [`MediumSelection::Authored`] and by the counterfactual baseline emission at
    /// [`MediumSelection::baseline_profile`]; a second, selection-free entry point
    /// would let the counterfactual be produced by a path the deliverable never takes.
    ///
    /// The PINNED dictionary set is the same under every selection: `gmeow:dicts` is the
    /// bundle's DISTRIBUTION channel for the dictionary family (a consumer priming its own
    /// runtime store reads them out of the shipped header), while the assignment is what
    /// PRIMES a frame. A baseline emission therefore still carries every declared
    /// dictionary and primes nothing with it — which is exactly the counterfactual the
    /// two-part code is judged against, and what makes the two emissions differ in
    /// priming ALONE.
    ///
    /// # Errors
    /// An unregistered or unassigned rep, a declared dictionary with no trained bytes,
    /// two assigned media declaring different zstd levels (the plan carries ONE level,
    /// so a disagreement has no answer), or an undeclared or dictionary-declaring
    /// uniform medium (see [`Self::resolved_assignment`]).
    pub fn medium_plan_under(
        &self,
        selection: &MediumSelection,
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
            let row = self.resolved_assignment(selection, rep)?;
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

    /// Read the ONE declared `gmeow:CorpusTrainingSplit`.
    ///
    /// Two declared splits are a hard fail rather than a precedence rule: which
    /// members a dictionary never saw would then depend on which individual a reader
    /// picked, and the held-out claim would be about neither.
    fn read_training_split(&mut self, ds: &RdfDataset) -> Result<(), gmeow_errors::Diag> {
        for subject in subjects_of_type(ds, &gm("CorpusTrainingSplit")) {
            let iri = require_iri(ds, subject)?;
            let stride = one_u64(ds, subject, &gm("splitHeldOutStride"), &iri)?;
            let offset = one_u64(ds, subject, &gm("splitHeldOutOffset"), &iri)?;
            if stride < 2 {
                return Err(invalid_declaration(format!(
                    "<{iri}> gmeow:splitHeldOutStride {stride} does not split anything — a stride \
                     of 0 or 1 holds out every member, leaving no training set at all, and the \
                     held-out claim would be false rather than weaker"
                )));
            }
            if offset >= stride {
                return Err(invalid_declaration(format!(
                    "<{iri}> gmeow:splitHeldOutOffset {offset} is not a position within a stride \
                     of {stride} — no rank could ever land on it, so the split would hold nothing \
                     out while still claiming to"
                )));
            }
            if let Some(previous) = self.split.replace(TrainingSplitDef {
                iri: iri.clone(),
                stride,
                offset,
            }) {
                return Err(invalid_declaration(format!(
                    "<{previous}> and <{iri}> both declare a gmeow:CorpusTrainingSplit — the split \
                     is ONE rule over EVERY archive-backed corpus, so two of them leave 'which \
                     members did this dictionary never see' with two answers and the held-out \
                     claim about neither",
                    previous = previous.iri
                )));
            }
        }
        Ok(())
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

/// The single non-negative integer value of an exactly-one datatype property.
fn one_u64(
    ds: &RdfDataset,
    subject: TermId,
    predicate: &str,
    subject_iri: &str,
) -> Result<u64, gmeow_errors::Diag> {
    let lexical = one_literal(ds, subject, predicate, subject_iri)?;
    lexical.parse::<u64>().map_err(|_| {
        invalid_declaration(format!(
            "<{subject_iri}> <{predicate}> {lexical:?} is not a non-negative integer"
        ))
    })
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

#[path = "registry.fixture.rs"]
#[cfg(test)]
pub(crate) mod fixture;

#[path = "registry.tests.rs"]
#[cfg(test)]
mod tests;
