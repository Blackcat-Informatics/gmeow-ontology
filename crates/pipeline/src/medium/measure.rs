// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Does a shipped dictionary PAY FOR ITSELF? The two-part code, measured on the
//! real chain.
//!
//! A dictionary is not free: its bytes ride in band in the segment header's `"dct"`
//! map, and a consumer pays for them whether or not the frames they prime got
//! smaller. So the quantity that decides whether a dictionary belongs in the bundle
//! is the TWO-PART CODE — the classic MDL split of "the model plus the data given
//! the model":
//!
//! ```text
//! two_part_code(d) = Σ_f |enc_d(f)|   +   |dict_d|
//!                    \___________/       \_______/
//!                     the frames d         d's own
//!                     primes, encoded      in-band
//!                     THROUGH d            bytes
//! ```
//!
//! and the comparison is against the SAME frames encoded through the declared
//! no-dictionary medium (`gmeow:mediumProfileBaselineL12`):
//!
//! ```text
//! baseline(d) = Σ_f |enc_baseline(f)|
//! ```
//!
//! `d` wins iff `two_part_code(d) < baseline(d)`. There is no threshold and no
//! tolerance band: charging the dictionary's own bytes is what makes the criterion
//! non-vacuous, and a dictionary that does not clear it is dead weight a consumer
//! downloads (Constitution §18).
//!
//! # Measured on the MANDATED chain, never on a proxy
//!
//! Both arms run `zstd-rsyncable` at the medium's declared level through
//! [`purrdf::gts::codec::encode_chain_with_options`] — the SAME entry point
//! `emit_gts` writes the shipped frames with. A plain-`zstd` proxy would be cheaper
//! and would measure a codec the bundle does not use; shipping its numbers as
//! ontology content would be a claim about bytes nobody ever writes.
//!
//! # Two DECLARED populations, and why the snapshot is not in either
//!
//! * **A — [`Population::EmittedBlobFrames`]**: every blob frame THIS emission
//!   authors, grouped by the dictionary its rep is assigned. The snapshot frame is
//!   deliberately EXCLUDED, and the exclusion is not tidiness: the measurement lands
//!   in the snapshot payload, so the snapshot's own compressed length is a function
//!   of the numbers being measured. Including it would make the measurement
//!   self-referential in the one way the medium axis's two-pass fixed point cannot
//!   absorb (the envelope stratum excludes the ENVELOPES, not the payload's own
//!   size). [`DictionaryEffect::evaluated_frame_count`] is emitted so the population
//!   is visible rather than implied, and [`check`] names the exclusion in its
//!   failure message.
//! * **B — [`Population::RuntimeStoreSegments`]**: the append-only runtime store
//!   files a CONSUMER writes, replayed here through the real
//!   `Memory::store` / audit-segment / conjecture-append paths over a declared,
//!   bundle-derived corpus ([`replay_runtime_store`]). Its `bytes_on_disk` is net of
//!   the per-file in-band dictionary bytes, which is the whole question for a store:
//!   whether a dictionary paid once per FILE wins is a pure function of the record
//!   count, so a store that opened a header per record would charge those bytes
//!   again and lose.
//!
//! # The train/test overlap is RECORDED, not papered over
//!
//! For the archive-backed dictionaries the training corpus is the archive's MEMBERS
//! while the evaluated frame is the TAR of those members: train and test OVERLAP on
//! the dominant representation. This is NOT a held-out evaluation and must never be
//! described as one. What keeps the criterion from being vacuous is the two-part
//! code itself — the dictionary is charged its own bytes, so "memorize the corpus"
//! is a losing strategy past the point where the memorized bytes cost more than they
//! save. `bench/README.md` says the same thing beside the committed evidence.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::gts_compose::BlobRow;
use purrdf::{RdfLiteral, RdfQuad, RdfTerm};

use super::registry::{DictSelection, MediumRegistry};
use super::{GMEOW, MEDIUM_MEASUREMENT_GRAPH, dictionary_regression, invalid_declaration};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_NON_NEGATIVE_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const MATH: &str = "https://blackcatinformatics.ca/math/";

/// The committed path the measurement graph's fanout twin reconstructs onto.
///
/// A `.ttl`, so [`crate::stages::superset::is_header_dict_path`] (which keys on the
/// `.zdict` suffix) correctly leaves it to the `rdf-fanout` family even though it
/// shares the `generated/medium/` prefix with the projected dictionaries.
pub const MEDIUM_EFFECT_PATH: &str = "generated/medium/dictionary-effect.ttl";

/// The `gmeow:ObservationMethod` every row here carries: the numbers are produced by
/// a deterministic Rust encode of declared bytes, not by judgement or by an
/// instrument.
pub const METHOD_COMPUTATIONAL_MODEL: &str =
    "https://blackcatinformatics.ca/gmeow/methodComputationalModel";

/// Which declared body of bytes a [`DictionaryEffect`] was measured over.
///
/// An enum rather than a free string because the two populations are measured on
/// DIFFERENT chains — A is this emission's own frames, B is a consumer's store file
/// — and a reader that could not tell them apart would compare numbers that are not
/// commensurable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Population {
    /// Every blob frame this emission authors, EXCLUDING the snapshot frame.
    EmittedBlobFrames,
    /// The append-only runtime store segments a consumer writes with the shipped
    /// header entry.
    RuntimeStoreSegments,
}

impl Population {
    /// The `gmeow:` individual naming this population.
    #[must_use]
    pub fn iri(self) -> String {
        format!("{GMEOW}{}", self.local())
    }

    fn local(self) -> &'static str {
        match self {
            Self::EmittedBlobFrames => "mediumPopulationEmittedBlobFrames",
            Self::RuntimeStoreSegments => "mediumPopulationRuntimeStoreSegments",
        }
    }

    /// The stable wire token used in `bench/medium-baseline.json`.
    #[must_use]
    pub fn wire(self) -> &'static str {
        match self {
            Self::EmittedBlobFrames => "emitted-blob-frames",
            Self::RuntimeStoreSegments => "runtime-store-segments",
        }
    }

    /// The population a wire token names.
    #[must_use]
    pub fn from_wire(token: &str) -> Option<Self> {
        match token {
            "emitted-blob-frames" => Some(Self::EmittedBlobFrames),
            "runtime-store-segments" => Some(Self::RuntimeStoreSegments),
            _ => None,
        }
    }
}

/// One dictionary's measured effect over one declared population.
///
/// Every field is a COUNT, never a ratio: the one derived ratio
/// ([`Self::gain_fraction`]) is bounded in `[0, 1]` by construction and computed from
/// these counts, so a reader can re-derive it and an unbounded "x times smaller"
/// number never enters the ontology.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryEffect {
    /// The `gmeow:dictionaryId` this row is about.
    pub dictionary_id: String,
    /// Which declared body of bytes was measured.
    pub population: Population,
    /// `Σ_f |enc_d(f)|` — the population's frames encoded THROUGH the dictionary,
    /// NOT counting the dictionary's own in-band bytes.
    pub bytes_on_disk: u64,
    /// `Σ_f |enc_baseline(f)|` — the SAME frames encoded through
    /// `gmeow:mediumProfileBaselineL12` (the declared no-dictionary medium).
    pub bytes_on_disk_baseline: u64,
    /// The dictionary's own bytes as the population actually carries them: once for
    /// a bundle (one segment header), once PER FILE HEADER for a runtime store.
    pub dictionary_in_band_bytes: u64,
    /// How many samples the dictionary was TRAINED over — recorded because the
    /// train/test overlap (see the module docs) is only interpretable beside it.
    pub corpus_sample_count: u64,
    /// How many frames of the population were evaluated. Emitted so the DECLARED
    /// exclusion of the snapshot frame is visible in the data rather than only in
    /// prose.
    pub evaluated_frame_count: u64,
}

impl DictionaryEffect {
    /// `Σ_f |enc_d(f)| + |dict_d|` — the whole code a consumer pays.
    #[must_use]
    pub fn two_part_code_bytes(&self) -> u64 {
        self.bytes_on_disk
            .saturating_add(self.dictionary_in_band_bytes)
    }

    /// Whether the dictionary pays for itself on this population.
    #[must_use]
    pub fn wins(&self) -> bool {
        self.two_part_code_bytes() < self.bytes_on_disk_baseline
    }

    /// `(baseline − two-part code) / baseline`, clamped into `[0, 1]`.
    ///
    /// A BOUNDED fraction rather than a ratio: an unbounded "n× smaller" number has
    /// no maximum, so it can neither be compared across dictionaries nor placed on a
    /// scale. A losing dictionary would push this negative — it is clamped to `0`
    /// and [`check`] refuses to emit the row at all, so a clamped zero never ships.
    #[must_use]
    pub fn gain_fraction(&self) -> f64 {
        if self.bytes_on_disk_baseline == 0 {
            return 0.0;
        }
        let saved = self
            .bytes_on_disk_baseline
            .saturating_sub(self.two_part_code_bytes());
        (saved as f64 / self.bytes_on_disk_baseline as f64).clamp(0.0, 1.0)
    }

    /// The gain fraction as the fixed six-decimal lexical form the projection emits.
    ///
    /// Fixed precision on purpose: an `f64`'s shortest round-trip rendering is a
    /// function of the exact bit pattern, so two builds that agree on every COUNT
    /// could still disagree on the string. Six decimals over integer inputs is a
    /// pure function of those integers.
    #[must_use]
    pub fn gain_fraction_lexical(&self) -> String {
        format!("{:.6}", self.gain_fraction())
    }
}

/// The encoded length of `payload` under one codec / level / dictionary selection.
///
/// # Errors
/// The codec name is not one this build can encode with, or the encoder refuses the
/// payload. Both are HARD FAILS: a measurement that silently fell back to a different
/// chain would report a number for bytes nobody writes.
pub fn encoded_len(
    codec: &str,
    level: i32,
    dict: Option<&[u8]>,
    payload: &[u8],
) -> Result<u64, gmeow_errors::Diag> {
    let chain = [codec.to_string()];
    let encoded = purrdf::gts::codec::encode_chain_with_options(
        &chain,
        payload,
        purrdf::gts::codec::EncodeOptions {
            zstd_level: Some(level),
            dict,
        },
    )
    .map_err(|err| {
        invalid_declaration(format!(
            "the medium measurement cannot encode a {} byte payload through the declared chain \
             [{codec}] at level {level}: {err} — there is no proxy codec to fall back to, because \
             the number would then describe bytes this build never writes",
            payload.len()
        ))
    })?;
    Ok(encoded.len() as u64)
}

/// The `(codec, zstd level)` chain the emission's OWN media declare — the chain the
/// production measurement must run, read from the registry rather than restated.
///
/// Every rep the emission authors resolves to a declared `gmeow:Medium`; all of them
/// must agree, exactly as [`MediumRegistry::medium_plan`] already requires of the
/// level. A disagreement has no answer: a two-part code summed across two codecs
/// would be a number no artifact ever exhibits.
///
/// # Errors
/// An unregistered/unassigned rep, an assignment naming an undeclared medium, a codec
/// outside the registered catalog, or two assigned media declaring different chains.
pub fn mandated_chain(
    registry: &MediumRegistry,
    frames: &[&BlobRow],
) -> Result<(String, i32), gmeow_errors::Diag> {
    let mut chain: Option<(String, i32, String)> = None;
    for row in frames {
        let assignment = registry.assignment_for(&row.rep)?;
        let medium = registry.media().get(&assignment.medium).ok_or_else(|| {
            invalid_declaration(format!(
                "the assignment for rep {:?} names medium <{}>, which is not a declared \
                 gmeow:Medium",
                row.rep, assignment.medium
            ))
        })?;
        let codec = medium.codec_wire_name()?.to_string();
        match &chain {
            None => chain = Some((codec, medium.zstd_level, medium.iri.clone())),
            Some((first_codec, first_level, first_iri))
                if *first_codec != codec || *first_level != medium.zstd_level =>
            {
                return Err(invalid_declaration(format!(
                    "media <{first_iri}> ({first_codec} level {first_level}) and <{}> ({codec} \
                     level {}) are both assigned in this emission — the dictionary-effect \
                     measurement sums a two-part code over one chain, and two chains in one sum \
                     describe an artifact that does not exist",
                    medium.iri, medium.zstd_level
                )));
            }
            Some(_) => {}
        }
    }
    chain
        .map(|(codec, level, _)| (codec, level))
        .ok_or_else(|| {
            invalid_declaration(
            "this emission authors no payload frame, so it declares no chain to measure over — an \
             empty bundle is a build failure, not a population of size zero"
                .to_string(),
        )
        })
}

/// The emission's blob frames, grouped by the dictionary their rep is ASSIGNED — the
/// population-A partition, shared by the live measurement and the off-gate sweep so
/// the two can never grade different frame sets.
///
/// A frame whose rep resolves to [`DictSelection::Baseline`] belongs to no group: it
/// is already written through the declared no-dictionary medium, so it has no
/// dictionary to pay for.
///
/// # Errors
/// An unregistered or unassigned rep, or an assignment naming an unregistered
/// dictionary.
pub fn frames_by_dictionary<'a>(
    registry: &MediumRegistry,
    frames: &[&'a BlobRow],
) -> Result<BTreeMap<String, Vec<&'a BlobRow>>, gmeow_errors::Diag> {
    let mut by_dictionary: BTreeMap<String, Vec<&'a BlobRow>> = BTreeMap::new();
    for row in frames {
        let assignment = registry.assignment_for(&row.rep)?;
        let DictSelection::Named(iri) = &assignment.dictionary else {
            continue;
        };
        let def = registry.dictionaries().get(iri).ok_or_else(|| {
            super::unknown_dictionary(format!(
                "rep {:?} selects <{iri}>, which is not a registered \
                 gmeow:CompressionDictionary",
                row.rep
            ))
        })?;
        by_dictionary.entry(def.id.clone()).or_default().push(row);
    }
    Ok(by_dictionary)
}

/// Population **A**: every blob frame this emission authors, grouped by the
/// dictionary its rep is assigned, measured on the mandated chain.
///
/// `frames` is the emission's blob-row set — the snapshot frame is not among them,
/// which is exactly the declared exclusion (see the module docs). A frame whose rep
/// resolves to [`DictSelection::Baseline`] contributes to no dictionary's population:
/// it is already written through the no-dictionary medium, so it has no dictionary to
/// pay for.
///
/// `sample_counts` maps a `gmeow:dictionaryId` to the number of corpus samples the
/// producer trained it over (read back off the realization records, never re-derived
/// here — re-deriving would be a second computation of one measurement).
///
/// # Errors
/// An unregistered or unassigned rep, a selected dictionary with no trained bytes, or
/// an encoder refusal.
pub fn population_a(
    registry: &MediumRegistry,
    frames: &[&BlobRow],
    trained: &BTreeMap<String, Vec<u8>>,
    sample_counts: &BTreeMap<String, u64>,
    codec: &str,
    level: i32,
) -> Result<Vec<DictionaryEffect>, gmeow_errors::Diag> {
    // Group first, encode second: a dictionary's population is a SET of frames, and
    // the two-part code is a sum over that set rather than a per-frame verdict.
    let by_dictionary = frames_by_dictionary(registry, frames)?;
    let mut out: Vec<DictionaryEffect> = Vec::with_capacity(by_dictionary.len());
    for (id, rows) in by_dictionary {
        let dict = trained.get(&id).ok_or_else(|| {
            super::undeclared_dictionary(format!(
                "dictionary {id:?} primes {} emitted frame(s) but no trained bytes were supplied \
                 — the effect measurement cannot charge a dictionary whose bytes it cannot see",
                rows.len()
            ))
        })?;
        let mut bytes_on_disk = 0u64;
        let mut baseline = 0u64;
        for row in &rows {
            bytes_on_disk += encoded_len(codec, level, Some(dict), &row.data)?;
            baseline += encoded_len(codec, level, None, &row.data)?;
        }
        out.push(DictionaryEffect {
            dictionary_id: id.clone(),
            population: Population::EmittedBlobFrames,
            bytes_on_disk,
            bytes_on_disk_baseline: baseline,
            dictionary_in_band_bytes: dict.len() as u64,
            corpus_sample_count: sample_counts.get(&id).copied().unwrap_or_default(),
            evaluated_frame_count: rows.len() as u64,
        });
    }
    Ok(out)
}

/// Every dictionary the EMITTED bundle must publish a reading for: the ones a
/// registered `gmeow:PayloadSchema` selects, minus the declared-unmeasurable set.
///
/// DERIVED from the registry rather than listed, so a dictionary added together with
/// the rep it primes is covered by [`check`] without anyone remembering to extend a
/// constant. The runtime-store dictionaries are absent by construction: no bundle rep
/// selects them, and a bundle cannot honestly publish a reading over a file a CONSUMER
/// wrote.
#[must_use]
pub fn required_measurements(registry: &MediumRegistry) -> BTreeSet<String> {
    registry
        .schemas()
        .values()
        .filter_map(
            |schema| match &registry.assignment_for(&schema.rep).ok()?.dictionary {
                DictSelection::Named(iri) => {
                    registry.dictionaries().get(iri).map(|def| def.id.clone())
                }
                DictSelection::Baseline => None,
            },
        )
        .filter(|id| !super::sweep::UNMEASURABLE_DICTIONARIES.contains(&id.as_str()))
        .collect()
}

/// The GATE: every measured dictionary pays for itself on the population it primes.
///
/// Iterates the rows it is HANDED and additionally requires every id in `required` to
/// have one, so a dictionary added to the registry without a measurement is a failure
/// rather than a silently uncovered row.
///
/// There is no threshold to relax. The criterion is `two-part code < baseline`, both
/// over the same frames on the same chain, and the dictionary's own bytes are on the
/// paying side.
///
/// # Errors
/// `MediumDictionaryRegression` for a dictionary that does not clear the criterion,
/// or for one in `required` with no measured row.
pub fn check(
    effects: &[DictionaryEffect],
    required: &BTreeSet<String>,
) -> Result<(), gmeow_errors::Diag> {
    let measured: BTreeSet<&str> = effects
        .iter()
        .map(|e| e.dictionary_id.as_str())
        .collect::<BTreeSet<&str>>();
    for id in required {
        if !measured.contains(id.as_str()) {
            return Err(dictionary_regression(format!(
                "dictionary {id:?} is declared and measurable but the dictionary-effect \
                 measurement carries no row for it — a dictionary with no measured population is \
                 a dictionary whose cost nobody checked. Population A is the emitted BLOB frames \
                 (the snapshot frame is DECLARED-excluded: its compressed length is a function of \
                 the very payload this measurement lands in); population B is the runtime store. \
                 Assign the dictionary to a rep, or retire it"
            )));
        }
    }
    for effect in effects {
        if effect.wins() {
            continue;
        }
        return Err(dictionary_regression(format!(
            "dictionary {:?} does NOT pay for itself over population `{}`: two-part code {} B \
             (= {} B of frames + {} B of in-band dictionary) is not strictly less than the \
             gmeow:mediumProfileBaselineL12 code {} B for the SAME {} frame(s). There is no \
             threshold to relax — charging the dictionary its own bytes is what makes the \
             criterion non-vacuous. Retire the dictionary, re-sweep its strategy/target-length \
             winner (`make maint-medium-sweep`), or widen the population it primes; do NOT \
             weaken this check. (Population A EXCLUDES the snapshot frame by declaration: its \
             compressed length is a function of the whole payload that carries this measurement.)",
            effect.dictionary_id,
            effect.population.wire(),
            effect.two_part_code_bytes(),
            effect.bytes_on_disk,
            effect.dictionary_in_band_bytes,
            effect.bytes_on_disk_baseline,
            effect.evaluated_frame_count,
        )));
    }
    Ok(())
}

/// Project measured effects into the [`MEDIUM_MEASUREMENT_GRAPH`] named graph as
/// canonical quads in deterministic order.
///
/// One `gmeow:Measurement` (also typed `gmeow:MediumDictionaryEffectMeasurement`) per
/// row: `gmeow:observedFeature` is the authored `gmeow:CompressionDictionary`,
/// `gmeow:observationMethod` is `gmeow:methodComputationalModel` (exactly one — the
/// class's qualified cardinality), and `gmeow:observationResult` is a `math:Quantity`
/// wrapping the bounded gain fraction, because `gmeow:observationResult`'s range is
/// `logic:Individual` and a bare literal is forbidden there.
///
/// The graph is `graph/medium-measurement`, NOT `graph/medium-registry`: the registry
/// describes what the dictionaries ARE, the measurement describes what they DO, and
/// the two have different refresh cadences and different readers.
///
/// # Errors
/// A row naming a dictionary the registry does not declare.
pub fn project(
    registry: &MediumRegistry,
    effects: &[DictionaryEffect],
) -> Result<Vec<RdfQuad>, gmeow_errors::Diag> {
    let graph = RdfTerm::iri(MEDIUM_MEASUREMENT_GRAPH);
    let mut quads: Vec<RdfQuad> = Vec::new();
    let mut emit = |subject: &str, predicate: String, object: RdfTerm| {
        quads.push(RdfQuad::new(RdfTerm::iri(subject), predicate, object).in_graph(graph.clone()));
    };

    // Canonical (dictionary id, population) order, so the projection is a pure
    // function of the measured set rather than of the caller's push order.
    let ordered: BTreeMap<(&str, Population), &DictionaryEffect> = effects
        .iter()
        .map(|e| ((e.dictionary_id.as_str(), e.population), e))
        .collect();

    for effect in ordered.into_values() {
        let def = registry.dictionary_by_id(&effect.dictionary_id)?;
        let subject = measurement_iri(&effect.dictionary_id, effect.population);
        let quantity = format!("{subject}/gain");

        emit(
            &subject,
            RDF_TYPE.to_string(),
            RdfTerm::iri(gm("Measurement")),
        );
        emit(
            &subject,
            RDF_TYPE.to_string(),
            RdfTerm::iri(gm("MediumDictionaryEffectMeasurement")),
        );
        emit(&subject, gm("observedFeature"), RdfTerm::iri(&def.iri));
        emit(&subject, gm("measuresDictionary"), RdfTerm::iri(&def.iri));
        emit(
            &subject,
            gm("observationMethod"),
            RdfTerm::iri(METHOD_COMPUTATIONAL_MODEL),
        );
        emit(
            &subject,
            gm("measurementPopulation"),
            RdfTerm::iri(effect.population.iri()),
        );
        emit(&subject, gm("observationResult"), RdfTerm::iri(&quantity));
        for (predicate, value) in [
            ("measurementBytesOnDisk", effect.bytes_on_disk),
            (
                "measurementBytesOnDiskBaseline",
                effect.bytes_on_disk_baseline,
            ),
            (
                "measurementDictionaryInBandBytes",
                effect.dictionary_in_band_bytes,
            ),
            ("measurementTwoPartCodeBytes", effect.two_part_code_bytes()),
            ("measurementCorpusSampleCount", effect.corpus_sample_count),
            (
                "measurementEvaluatedFrameCount",
                effect.evaluated_frame_count,
            ),
        ] {
            emit(&subject, gm(predicate), non_negative_integer(value));
        }
        emit(
            &subject,
            "http://www.w3.org/2000/01/rdf-schema#label".to_string(),
            RdfTerm::literal(RdfLiteral::language_tagged(
                format!(
                    "dictionary effect: {} over {}",
                    effect.dictionary_id,
                    effect.population.wire()
                ),
                "x-gmeow-english",
            )),
        );
        emit(
            &subject,
            "http://www.w3.org/2004/02/skos/core#definition".to_string(),
            RdfTerm::literal(RdfLiteral::language_tagged(
                effect_prose(effect),
                "x-gmeow-english",
            )),
        );
        emit(&subject, gm("graphBoxRole"), RdfTerm::iri(gm("boxABox")));

        // The bounded gain fraction, wrapped as a dimensionless math:Quantity.
        emit(
            &quantity,
            RDF_TYPE.to_string(),
            RdfTerm::iri(format!("{MATH}Quantity")),
        );
        emit(
            &quantity,
            format!("{MATH}quantityValue"),
            RdfTerm::literal(RdfLiteral::typed(
                effect.gain_fraction_lexical(),
                XSD_DECIMAL,
            )),
        );
        emit(
            &quantity,
            format!("{MATH}hasDimension"),
            RdfTerm::iri(format!("{MATH}dimensionless")),
        );
        emit(
            &quantity,
            "http://www.w3.org/2000/01/rdf-schema#label".to_string(),
            RdfTerm::literal(RdfLiteral::language_tagged(
                format!(
                    "dictionary gain fraction: {} over {}",
                    effect.dictionary_id,
                    effect.population.wire()
                ),
                "x-gmeow-english",
            )),
        );
        emit(&quantity, gm("graphBoxRole"), RdfTerm::iri(gm("boxABox")));
    }
    Ok(quads)
}

/// The prose every projected row carries — the place the honesty caveats live in the
/// DATA rather than only in a README a consumer of `gmeow.gts` never sees.
fn effect_prose(effect: &DictionaryEffect) -> String {
    let overlap = match effect.population {
        Population::EmittedBlobFrames => {
            "The training corpus and the evaluated frame OVERLAP on the dominant representation: \
             the corpus selects an archive's MEMBERS while the evaluated frame is the tar of \
             those members. This is NOT a held-out evaluation and must not be read as one — what \
             keeps the criterion non-vacuous is that the two-part code charges the dictionary its \
             own in-band bytes. The snapshot frame is DECLARED-excluded from this population: its \
             compressed length is a function of the very payload this measurement lands in."
        }
        Population::RuntimeStoreSegments => {
            "Measured over a declared, bundle-derived replay corpus written through the real \
             append-only store paths, net of the per-file in-band dictionary bytes. Whether a \
             dictionary paid once per FILE wins is a pure function of the record count, so the \
             cardinality and byte size of the replay corpus are recorded beside the result. The \
             numbers are a projection of the committed bench/medium-baseline.json evidence \
             because a runtime store's records carry a wall clock and are therefore not a \
             function of this build."
        }
    };
    format!(
        "Two-part code for {}: {} B of frames + {} B of in-band dictionary = {} B, against a \
         gmeow:mediumProfileBaselineL12 code of {} B over the same {} frame(s) — a bounded gain \
         fraction of {}. Trained over {} corpus sample(s). {overlap}",
        effect.dictionary_id,
        effect.bytes_on_disk,
        effect.dictionary_in_band_bytes,
        effect.two_part_code_bytes(),
        effect.bytes_on_disk_baseline,
        effect.evaluated_frame_count,
        effect.gain_fraction_lexical(),
        effect.corpus_sample_count,
    )
}

/// The measurement individual's IRI: derived from `(dictionary id, population)`, so
/// two populations of one dictionary never collapse onto one subject and a re-run
/// over the same coordinates lands on the same node.
#[must_use]
pub fn measurement_iri(dictionary_id: &str, population: Population) -> String {
    format!(
        "{GMEOW}medium-measurement/{dictionary_id}/{}",
        population.wire()
    )
}

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn non_negative_integer(value: u64) -> RdfTerm {
    RdfTerm::literal(RdfLiteral::typed(
        value.to_string(),
        XSD_NON_NEGATIVE_INTEGER,
    ))
}

// ── Population B: the runtime store replay ───────────────────────────────────

/// How many segment headers of `store` pin `dictionary` in their in-band `"dct"` map,
/// and the byte length of the entry each of them carries.
///
/// A store file is APPEND-ONLY and may hold several segments; each segment header
/// re-pins the whole dictionary. That is the property the runtime-store criterion
/// turns on, so it is counted from the wire rather than assumed to be one.
///
/// # Errors
/// The store is a torn CBOR sequence, or a header pins the dictionary with no bytes.
pub fn in_band_dictionary_bytes(store: &[u8], dictionary: &str) -> Result<u64, gmeow_errors::Diag> {
    use ciborium::value::Value;
    use purrdf::gts::wire::{iter_items, map_get, unwrap_header};

    let (items, torn) = iter_items(store);
    if torn.is_some() {
        return Err(invalid_declaration(
            "the runtime store is a torn CBOR sequence — its in-band dictionary cost cannot be \
             counted, and guessing it would understate the two-part code"
                .to_string(),
        ));
    }
    let mut total = 0u64;
    for (_, item) in &items {
        let is_header = match item {
            Value::Tag(tag, _) => *tag == 55799,
            Value::Map(entries) => {
                matches!(map_get(entries, "gts"), Some(Value::Text(magic)) if magic == "GTS1")
            }
            _ => false,
        };
        if !is_header {
            continue;
        }
        let Ok(head) = unwrap_header(item) else {
            continue;
        };
        let Some(Value::Map(dicts)) = map_get(head, "dct") else {
            continue;
        };
        for (name, value) in dicts {
            let Value::Text(name) = name else { continue };
            if name != dictionary {
                continue;
            }
            match value {
                Value::Bytes(bytes) if !bytes.is_empty() => total += bytes.len() as u64,
                _ => {
                    return Err(invalid_declaration(format!(
                        "a runtime-store segment header pins {dictionary:?} with no bytes — a \
                         named-but-empty dictionary primes nothing while still raising the reader \
                         contract"
                    )));
                }
            }
        }
    }
    Ok(total)
}

/// The measured effect of `dictionary` on a runtime store, from the two store files a
/// replay produced.
///
/// `primed` is the store written with the dictionary pinned; `baseline` is the SAME
/// replay corpus written through the declared no-dictionary medium. `bytes_on_disk` is
/// net of the primed file's in-band dictionary bytes, so the two-part code
/// reconstitutes the file's actual length — which is what a consumer's disk holds.
///
/// # Errors
/// Either store is unreadable as a CBOR sequence, or the primed store does not pin the
/// dictionary at all (a store that pinned nothing would trivially "win").
pub fn population_b(
    dictionary: &str,
    primed: &[u8],
    baseline: &[u8],
    corpus_sample_count: u64,
    evaluated_frame_count: u64,
) -> Result<DictionaryEffect, gmeow_errors::Diag> {
    let in_band = in_band_dictionary_bytes(primed, dictionary)?;
    if in_band == 0 {
        return Err(invalid_declaration(format!(
            "the replayed runtime store pins no {dictionary:?} entry in any segment header — its \
             two-part code would charge nothing for a dictionary the file does not carry, which \
             would make the criterion vacuous"
        )));
    }
    Ok(DictionaryEffect {
        dictionary_id: dictionary.to_string(),
        population: Population::RuntimeStoreSegments,
        bytes_on_disk: (primed.len() as u64).saturating_sub(in_band),
        bytes_on_disk_baseline: baseline.len() as u64,
        dictionary_in_band_bytes: in_band,
        corpus_sample_count,
        evaluated_frame_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::medium::registry::fixture;

    fn registry() -> MediumRegistry {
        MediumRegistry::from_dataset(&fixture::dataset("")).expect("fixture registry")
    }

    fn effect(bytes: u64, baseline: u64, in_band: u64) -> DictionaryEffect {
        DictionaryEffect {
            dictionary_id: "gmeow-core-v1".to_string(),
            population: Population::EmittedBlobFrames,
            bytes_on_disk: bytes,
            bytes_on_disk_baseline: baseline,
            dictionary_in_band_bytes: in_band,
            corpus_sample_count: 12,
            evaluated_frame_count: 3,
        }
    }

    /// The gain fraction is BOUNDED in `[0, 1]` in every direction, including the
    /// degenerate ones — an unbounded ratio would be unusable on a scale.
    #[test]
    fn the_gain_fraction_is_bounded_in_zero_one() {
        assert_eq!(effect(400, 1000, 100).gain_fraction_lexical(), "0.500000");
        // A dictionary that costs more than it saves is clamped, never negative.
        assert_eq!(effect(900, 1000, 500).gain_fraction_lexical(), "0.000000");
        // An empty baseline is zero rather than a division by zero.
        assert_eq!(effect(0, 0, 0).gain_fraction_lexical(), "0.000000");
        // A free dictionary saving everything saturates at 1.
        assert_eq!(effect(0, 1000, 0).gain_fraction_lexical(), "1.000000");
    }

    /// The two-part code charges the dictionary's OWN bytes — the property that keeps
    /// the criterion non-vacuous under a train/test overlap.
    #[test]
    fn the_two_part_code_charges_the_dictionary_its_own_bytes() {
        let saves_but_costs_more = effect(500, 1000, 600);
        assert_eq!(saves_but_costs_more.two_part_code_bytes(), 1100);
        assert!(
            !saves_but_costs_more.wins(),
            "a dictionary whose bytes cost more than it saves must lose even though the frames \
             got smaller"
        );
        assert!(effect(500, 1000, 400).wins());
        // Strictly less: a tie is a loss (the dictionary raised the reader contract for
        // nothing).
        assert!(!effect(500, 1000, 500).wins());
    }

    /// The gate names the DECLARED snapshot exclusion in its failure message, so a
    /// reader is never left to infer which frames were evaluated.
    #[test]
    fn the_gate_reds_and_names_the_declared_exclusion() {
        let diag = check(&[effect(900, 1000, 500)], &BTreeSet::new())
            .expect_err("a losing dictionary must hard-fail");
        assert_eq!(
            diag.code(),
            crate::error::MediumDictionaryRegression::register(),
            "{diag}"
        );
        let text = diag.to_string();
        assert!(text.contains("snapshot frame"), "{text}");
        assert!(text.contains("no threshold to relax"), "{text}");
        check(&[effect(400, 1000, 100)], &BTreeSet::new()).expect("a winning dictionary passes");
    }

    /// A declared dictionary with NO measured row is a failure, not an uncovered row:
    /// that is exactly how a dictionary would slip in without gate coverage.
    #[test]
    fn a_declared_dictionary_with_no_measured_row_hard_fails() {
        let required: BTreeSet<String> =
            ["gmeow-core-v1".to_string(), "gmeow-terms-v1".to_string()]
                .into_iter()
                .collect();
        let diag = check(&[effect(400, 1000, 100)], &required)
            .expect_err("an unmeasured declared dictionary must hard-fail");
        assert_eq!(
            diag.code(),
            crate::error::MediumDictionaryRegression::register(),
            "{diag}"
        );
        assert!(diag.to_string().contains("gmeow-terms-v1"), "{diag}");
    }

    /// The encode runs the MANDATED chain: `zstd-rsyncable` primes EVERY independent
    /// block with the dictionary, so a dictionary-primed encode of repetitive RDF is
    /// materially smaller than the unprimed one at the same level.
    #[test]
    fn the_measured_chain_is_zstd_rsyncable_and_dictionary_primed() {
        let owned: Vec<Vec<u8>> = (0..400u32)
            .map(|i| {
                format!(
                    "<https://blackcatinformatics.ca/gmeow/term{}> \
                     <https://blackcatinformatics.ca/gmeow/definition> \
                     \"a definition of term {i} in the gmeow ontology\" .\n",
                    i % 37
                )
                .into_bytes()
            })
            .collect();
        let corpus: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
        let dict = crate::medium::train::build(
            crate::medium::registry::DictionaryStrategy::Trained,
            &corpus,
            4096,
        )
        .expect("train");
        let payload = owned[0].clone();
        let primed = encoded_len("zstd-rsyncable", 12, Some(&dict), &payload).expect("primed");
        let bare = encoded_len("zstd-rsyncable", 12, None, &payload).expect("bare");
        assert!(
            primed < bare,
            "a primed encode of one small RDF record must beat the unprimed one: {primed} vs \
             {bare}"
        );
        // A codec the writer cannot encode with is a HARD FAIL, never a proxy.
        assert!(encoded_len("brotli", 12, None, &payload).is_err());
    }

    /// Population A groups by the ASSIGNED dictionary and skips baseline-assigned reps
    /// — a rep already written through the no-dictionary medium has no dictionary to
    /// pay for.
    #[test]
    fn population_a_groups_by_the_assigned_dictionary() {
        let registry = registry();
        let owned: Vec<Vec<u8>> = (0..400u32)
            .map(|i| format!("<https://e/s{}> <https://e/p> \"v{i}\" .\n", i % 29).into_bytes())
            .collect();
        let corpus: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
        let dict = crate::medium::train::build(
            crate::medium::registry::DictionaryStrategy::Trained,
            &corpus,
            4096,
        )
        .expect("train");
        let rows = [BlobRow {
            data: owned.concat(),
            media_type: "application/x-tar".to_string(),
            rep: "cells-archive".to_string(),
        }];
        let borrowed: Vec<&BlobRow> = rows.iter().collect();
        let trained: BTreeMap<String, Vec<u8>> = [("gmeow-core-v1".to_string(), dict)].into();
        let sample_counts: BTreeMap<String, u64> = [("gmeow-core-v1".to_string(), 400)].into();
        let effects = population_a(
            &registry,
            &borrowed,
            &trained,
            &sample_counts,
            "zstd-rsyncable",
            12,
        )
        .expect("population A");
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].dictionary_id, "gmeow-core-v1");
        assert_eq!(effects[0].evaluated_frame_count, 1);
        assert_eq!(effects[0].corpus_sample_count, 400);
        assert!(effects[0].bytes_on_disk > 0 && effects[0].bytes_on_disk_baseline > 0);
    }

    /// The projection lands entirely in `graph/medium-measurement` — NOT in
    /// `graph/medium-registry` — and carries exactly one `gmeow:observationMethod`.
    #[test]
    fn the_projection_lands_in_the_measurement_graph_with_one_method() {
        let registry = registry();
        let quads = project(&registry, &[effect(400, 1000, 100)]).expect("project");
        assert!(!quads.is_empty());
        assert!(
            quads
                .iter()
                .all(|q| q.graph_name == Some(RdfTerm::iri(MEDIUM_MEASUREMENT_GRAPH))),
            "a measurement must never land in graph/medium-registry: the registry says what the \
             dictionaries ARE, the measurement says what they DO"
        );
        let subject = measurement_iri("gmeow-core-v1", Population::EmittedBlobFrames);
        let methods = quads
            .iter()
            .filter(|q| {
                q.subject == RdfTerm::iri(subject.as_str())
                    && q.predicate == gm("observationMethod")
            })
            .count();
        assert_eq!(
            methods, 1,
            "gmeow:Measurement carries a min/max-1 qualified cardinality on \
             gmeow:observationMethod"
        );
        let predicates: Vec<&str> = quads.iter().map(|q| q.predicate.as_str()).collect();
        for required in [
            "measurementBytesOnDisk",
            "measurementBytesOnDiskBaseline",
            "measurementDictionaryInBandBytes",
            "measurementTwoPartCodeBytes",
            "measurementCorpusSampleCount",
            "measurementEvaluatedFrameCount",
            "measurementPopulation",
            "measuresDictionary",
            "observedFeature",
        ] {
            assert!(
                predicates.contains(&gm(required).as_str()),
                "the projection must carry gmeow:{required}"
            );
        }
        // The gain fraction rides a math:Quantity, never a bare literal on the
        // observation (gmeow:observationResult's range is logic:Individual).
        assert!(
            quads
                .iter()
                .any(|q| q.predicate == format!("{MATH}quantityValue")),
            "the bounded gain fraction must ride a math:Quantity"
        );
    }

    /// The projection is a pure function of the measured SET, not of push order — the
    /// property that makes the committed `.ttl` byte-identical across two runs.
    #[test]
    fn the_projection_is_emission_order_independent() {
        let registry = registry();
        let mut a = effect(400, 1000, 100);
        a.dictionary_id = "gmeow-core-v1".to_string();
        let mut b = effect(300, 900, 90);
        b.dictionary_id = "gmeow-terms-v1".to_string();
        let forward = project(&registry, &[a.clone(), b.clone()]).expect("project");
        let reversed = project(&registry, &[b, a]).expect("project");
        assert_eq!(forward, reversed);
    }

    /// A store whose header pins the dictionary ONCE PER RECORD pays for it once per
    /// record — the exact shape that makes a runtime-store dictionary lose.
    #[test]
    fn the_in_band_cost_is_counted_per_segment_header() {
        let dict = vec![7u8; 512];
        let one = fake_store(&[("gmeow-memory-hot-v1", dict.clone())], 1);
        let many = fake_store(&[("gmeow-memory-hot-v1", dict.clone())], 8);
        assert_eq!(
            in_band_dictionary_bytes(&one, "gmeow-memory-hot-v1").expect("one header"),
            512
        );
        assert_eq!(
            in_band_dictionary_bytes(&many, "gmeow-memory-hot-v1").expect("eight headers"),
            8 * 512,
            "each segment header re-pins the whole dictionary, and the two-part code charges \
             every copy"
        );
        // A store pinning nothing cannot be measured: it would win vacuously.
        let bare = fake_store(&[], 3);
        assert_eq!(
            in_band_dictionary_bytes(&bare, "gmeow-memory-hot-v1").expect("no entry"),
            0
        );
        assert!(population_b("gmeow-memory-hot-v1", &bare, &bare, 1, 1).is_err());
    }

    /// A minimal CBOR sequence of `headers` GTS segment headers, each pinning `dicts`.
    fn fake_store(dicts: &[(&str, Vec<u8>)], headers: usize) -> Vec<u8> {
        use ciborium::value::Value;
        let mut out: Vec<u8> = Vec::new();
        for _ in 0..headers {
            let dct = Value::Map(
                dicts
                    .iter()
                    .map(|(name, bytes)| {
                        (
                            Value::Text((*name).to_string()),
                            Value::Bytes(bytes.clone()),
                        )
                    })
                    .collect(),
            );
            let header = Value::Map(vec![
                (Value::Text("gts".into()), Value::Text("GTS1".into())),
                (Value::Text("dct".into()), dct),
            ]);
            ciborium::ser::into_writer(&header, &mut out).expect("cbor");
        }
        out
    }
}
