// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The off-gate dictionary SWEEP and its committed winner table.
//!
//! Which `(strategy, target length)` a dictionary should be trained at is a
//! MEASUREMENT, not an assumption — a term table is the right guess for a payload
//! dominated by vocabulary and the wrong one for a payload dominated by repeated
//! structure, and the same is true of every target length on the grid. But the
//! measurement is expensive: it re-trains each dictionary once per grid cell and
//! re-encodes that dictionary's whole frame population against it.
//!
//! So the sweep follows the pattern this repo already uses for expensive evidence
//! (`bench/baseline.json` ← `make maint-bench-baseline`, `bench/cost-baseline.json` ←
//! `make maint-bench-cost-baseline`): it runs in a MAINTAINER lane
//! (`make maint-medium-sweep`), writes its whole grid plus the winner it selected to a
//! COMMITTED artifact ([`MEDIUM_BASELINE_PATH`]), and the build then consumes only the
//! winners. `stage-medium-dictionaries` therefore stays deterministic and
//! sweep-free — a per-build sweep would make the shipped dictionaries a function of
//! how much CPU the machine had.
//!
//! # The winner table ↔ the DECLARED registry is a bijection
//!
//! [`check_bijection`] hard-fails in BOTH directions: a declared dictionary with no
//! committed row (a dictionary nobody measured) and a committed row for a dictionary
//! the registry no longer declares (a stale winner still steering a trainer). Neither
//! is a warning.
//!
//! The measurable set ([`measurable_ids`]) is every declared dictionary, with no
//! exemption: what a training point steers is the SHIPPED bytes, and every declared
//! dictionary reaches its consumer as shipped bytes — through the bundle's own payload
//! frames, or through the in-band `"dct"` map a runtime store primes from.
//!
//! # What the split holds out, and what the overlap still is
//!
//! Each grid cell trains on the TRAINING SIDE of the dictionary's declared corpus —
//! the declared `gmeow:CorpusTrainingSplit` holds a content-addressed share of every
//! archive's members out — and scores on the WHOLE frame, which is the tar of all of
//! them. So every archive-backed cell is scored partly on members that cell never saw.
//! That is a real held-out share, and it is the precise claim: NOT that the frame is
//! held out (it is not — most of its bytes are training material), and NOT that the
//! dictionary generalizes to material outside the bundle (no experiment here could say
//! that, because the population a bundle dictionary primes IS the bundle's own
//! frames). The two-part code still charges the dictionary its own bytes, so a
//! "memorize everything" cell loses once the memorized bytes cost more than they save.
//! `bench/README.md` states the same beside the artifact.
//!
//! # The committed table is pinned to the corpus it was measured over
//!
//! A corpus is a SELECTOR, so an archive that gains or loses a member moves the corpus
//! without moving the table. Every row therefore carries `corpus_digest`, the identity
//! of the whole resolution the grid was searched over, and [`check_corpus_digests`]
//! HARD-FAILS the build when it is not the identity of the corpus this build resolved.
//! A stale table reds; it never grades.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::measure::{DictionaryEffect, Population, encoded_len};
use super::registry::{DictionaryStrategy, MediumRegistry};
use super::{invalid_declaration, undeclared_dictionary};

/// The committed winner table + evidence — the ONE producer is
/// `make maint-medium-sweep`.
pub const MEDIUM_BASELINE_PATH: &str = "bench/medium-baseline.json";

/// The artifact's schema token, carried in band so a reader never has to guess which
/// shape it holds.
/// v2 added `corpus_digest` to every dictionary row — the identity of the corpus the
/// sweep actually grid-searched over. A v1 table has no way to say which corpus its
/// numbers describe, so it is REFUSED rather than read leniently: reading it would put
/// the build back in the state this field exists to remove.
pub const MEDIUM_BASELINE_SCHEMA: &str = "gmeow.medium-baseline.v2";

/// The DECLARED target-length grid, in bytes.
///
/// Four points spanning two orders of magnitude rather than every power of two: each
/// cell costs one training pass plus a full re-encode of the dictionary's frame
/// population, and the response surface over target length is monotone-then-flat (a
/// dictionary stops learning once it has the corpus's repeated structure, after which
/// the extra in-band bytes are pure cost). A denser grid would multiply the lane's
/// cost to resolve differences the two-part code cannot see.
pub const SWEEP_TARGET_LENGTHS: [usize; 4] = [4096, 16384, 65536, 262_144];

/// The grid a dictionary is actually swept over: [`SWEEP_TARGET_LENGTHS`] UNION the
/// dictionary's own DECLARED target.
///
/// The incumbent is always priced. A grid that omitted the declared target would make
/// "the declaration is not the argmin" a statement about the grid rather than about
/// the declaration — the incumbent would lose by never having been measured — and the
/// build trains at the declaration, so the one cell that must be on the grid is the
/// one the shipped bytes come from.
#[must_use]
pub fn target_grid(declared: usize) -> Vec<usize> {
    let mut grid: BTreeSet<usize> = SWEEP_TARGET_LENGTHS.into_iter().collect();
    grid.insert(declared);
    grid.into_iter().collect()
}

/// The DECLARED strategy grid: every `gmeow:DictionaryStrategy` individual, so the
/// sweep can never quietly stop considering one.
pub const SWEEP_STRATEGIES: [DictionaryStrategy; 3] = [
    DictionaryStrategy::Trained,
    DictionaryStrategy::RawContent,
    DictionaryStrategy::TermTable,
];

/// The codecs the GLOBAL codec sweep compares the mandated one against.
pub const SWEEP_CODECS: [&str; 2] = ["zstd", "zstd-rsyncable"];

/// The levels the global codec sweep compares the mandated one against.
pub const SWEEP_LEVELS: [i32; 4] = [3, 9, 12, 19];

/// The committed sweep artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediumBaseline {
    /// [`MEDIUM_BASELINE_SCHEMA`].
    pub schema: String,
    /// The global codec × level evidence behind the mandated Rule 6 chain.
    pub codec_sweep: CodecSweep,
    /// One row per MEASURABLE dictionary, sorted by id.
    pub dictionaries: Vec<DictionaryBaseline>,
}

/// The global codec × level sweep: the evidence that the mandated
/// `zstd-rsyncable` @ 12 chain is not dominated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodecSweep {
    /// The codec the media declare.
    pub mandated_codec: String,
    /// The level the media declare.
    pub mandated_level: i32,
    /// How many frames the sweep corpus held.
    pub corpus_frame_count: u64,
    /// The sweep corpus's uncompressed size.
    pub corpus_bytes: u64,
    /// The reps DECLARED-excluded from the sweep corpus, so the exclusion is data
    /// rather than an unstated habit.
    pub excluded_reps: Vec<String>,
    /// One row per `(codec, level)`, sorted.
    pub rows: Vec<CodecRow>,
    /// Whether the mandated chain is the smallest cell on the grid.
    ///
    /// It is `false`, and that is a PERMANENT, RECORDED fact rather than an open
    /// finding: the mandated `zstd-rsyncable` @ 12 chain costs materially more than
    /// plain `zstd` at the same level, and it is KEPT anyway. The grid prices SIZE
    /// ONLY, while GTS §8.4 rsyncable framing buys delta-transfer locality no size grid
    /// can see, and the mandated profile is normative Rule 6 doctrine. `bench/README.md`
    /// records the decision, the two facts that keep the tradeoff live, and what would
    /// have to be measured to reopen it.
    ///
    /// The flag is never silent: the sweep binary prints it on every run and
    /// `the_codec_grid_prices_the_mandated_cell_and_the_flag_matches_it` refuses a
    /// value that disagrees with the grid committed beside it.
    pub mandated_is_argmin: bool,
}

/// One cell of the codec × level grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodecRow {
    /// The wire codec name.
    pub codec: String,
    /// The zstd level.
    pub level: i32,
    /// The encoded size of the whole sweep corpus at this cell.
    pub bytes: u64,
}

/// One dictionary's committed winner + the grid it was chosen from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictionaryBaseline {
    /// `gmeow:dictionaryId`.
    pub id: String,
    /// Which declared population the winner was selected over
    /// ([`Population::wire`]).
    pub population: String,
    /// The AUTHORED `gmeow:dictionaryStrategy`.
    pub declared_strategy: String,
    /// The AUTHORED `gmeow:dictionaryTargetLength`.
    pub declared_target_length: u64,
    /// The argmin strategy the sweep measured.
    pub winning_strategy: String,
    /// The argmin target length the sweep measured.
    pub winning_target_length: u64,
    /// Whether the AUTHORED declaration IS the measured argmin. `false` is a
    /// REPORTABLE FINDING, never a silent overwrite of the slice: the sweep prints it
    /// and `the_declared_training_points_are_the_committed_winners` reds until a human
    /// reconciles the declaration with the evidence.
    pub declared_is_argmin: bool,
    /// The winner's `Σ_f |enc_d(f)|`.
    pub bytes_on_disk: u64,
    /// The same frames through `gmeow:mediumProfileBaselineL12`.
    pub bytes_on_disk_baseline: u64,
    /// The winner's own in-band bytes.
    pub dictionary_in_band_bytes: u64,
    /// `bytes_on_disk + dictionary_in_band_bytes`.
    pub two_part_code_bytes: u64,
    /// The bounded `[0, 1]` gain fraction, as its fixed six-decimal lexical form.
    pub dictionary_gain_fraction: String,
    /// How many samples the trainer was handed — the TRAINING side of the declared
    /// held-out split, not the whole resolved corpus.
    pub corpus_sample_count: u64,
    /// How many archive members the declared `gmeow:CorpusTrainingSplit` held out of
    /// training. They are still in the frame every grid cell was scored over, which is
    /// what makes the evaluation include material the dictionary never saw.
    pub held_out_sample_count: u64,
    /// The identity of the WHOLE resolved corpus the grid was searched over, held-out
    /// members included ([`crate::medium::corpus::CorpusResolution::digest`]).
    ///
    /// Without it the committed table can rot in silence: a corpus is a SELECTOR
    /// re-resolved every build, so an archive that gains or loses a member moves the
    /// corpus while the table sits still — and the table would keep grading. The build
    /// re-derives this digest and refuses on any difference
    /// ([`check_corpus_digests`]).
    pub corpus_digest: String,
    /// How many frames of the population were evaluated.
    pub evaluated_frame_count: u64,
    /// Every cell of the `(strategy, target length)` grid, sorted.
    pub grid: Vec<GridRow>,
}

impl DictionaryBaseline {
    /// The committed row read back as a [`DictionaryEffect`], so the projection and
    /// the gate treat a committed measurement exactly like a live one.
    ///
    /// # Errors
    /// The row names a population token this build does not know.
    pub fn effect(&self) -> Result<DictionaryEffect, gmeow_errors::Diag> {
        let population = Population::from_wire(&self.population).ok_or_else(|| {
            invalid_declaration(format!(
                "{MEDIUM_BASELINE_PATH} row {:?} names population {:?}, which is not a declared \
                 medium measurement population",
                self.id, self.population
            ))
        })?;
        Ok(DictionaryEffect {
            dictionary_id: self.id.clone(),
            population,
            bytes_on_disk: self.bytes_on_disk,
            bytes_on_disk_baseline: self.bytes_on_disk_baseline,
            dictionary_in_band_bytes: self.dictionary_in_band_bytes,
            corpus_sample_count: self.corpus_sample_count,
            evaluated_frame_count: self.evaluated_frame_count,
        })
    }

    /// The `(strategy, target length)` the build must train this dictionary at.
    ///
    /// # Errors
    /// The committed winning strategy is not a recognized
    /// `gmeow:DictionaryStrategy`.
    pub fn winner(&self) -> Result<(DictionaryStrategy, usize), gmeow_errors::Diag> {
        let strategy = strategy_from_wire(&self.winning_strategy).ok_or_else(|| {
            invalid_declaration(format!(
                "{MEDIUM_BASELINE_PATH} row {:?} names winning strategy {:?}, which is not a \
                 recognized gmeow:DictionaryStrategy — there is no default strategy to fall back \
                 to, because the three produce different decode-side expectations",
                self.id, self.winning_strategy
            ))
        })?;
        Ok((
            strategy,
            usize::try_from(self.winning_target_length).unwrap_or(usize::MAX),
        ))
    }
}

/// One `(strategy, target length)` cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridRow {
    /// The strategy this cell trained under.
    pub strategy: String,
    /// The target length this cell asked the trainer for.
    pub target_length: u64,
    /// What the trainer ACTUALLY returned, which may be under the target.
    pub dictionary_bytes: u64,
    /// The cell's two-part code — the quantity the argmin is taken over.
    pub two_part_code_bytes: u64,
}

/// The wire token for a strategy — the same spelling [`DictionaryStrategy`]'s
/// `Display` uses, so the artifact and the diagnostics agree.
#[must_use]
pub fn strategy_wire(strategy: DictionaryStrategy) -> String {
    strategy.to_string()
}

/// The strategy a wire token names.
#[must_use]
pub fn strategy_from_wire(token: &str) -> Option<DictionaryStrategy> {
    match token {
        "trained" => Some(DictionaryStrategy::Trained),
        "raw-content" => Some(DictionaryStrategy::RawContent),
        "term-table" => Some(DictionaryStrategy::TermTable),
        _ => None,
    }
}

impl MediumBaseline {
    /// Render the artifact as its committed bytes: pretty JSON with a trailing
    /// newline, every collection already in canonical order.
    ///
    /// # Errors
    /// Serialization failure.
    pub fn to_json(&self) -> Result<String, gmeow_errors::Diag> {
        let mut json = serde_json::to_string_pretty(self)
            .map_err(|e| invalid_declaration(format!("serialize {MEDIUM_BASELINE_PATH}: {e}")))?;
        json.push('\n');
        Ok(json)
    }

    /// Parse the committed artifact.
    ///
    /// # Errors
    /// Malformed JSON, or a schema token this build does not know (a version skew is
    /// a HARD FAIL: reading an unknown shape leniently would steer the trainers off
    /// half-understood evidence).
    pub fn from_json(text: &str) -> Result<Self, gmeow_errors::Diag> {
        // The schema token FIRST, off a probe that reads nothing else. A full
        // deserialization of an older shape fails on whichever field this version
        // added — "missing field `held_out_sample_count`" — which sends a reader
        // hunting for a corrupt file instead of telling them the artifact is a
        // version behind and naming the lane that refreshes it.
        #[derive(Deserialize)]
        struct SchemaProbe {
            schema: String,
        }
        let probe: SchemaProbe = serde_json::from_str(text).map_err(|e| {
            invalid_declaration(format!(
                "parse {MEDIUM_BASELINE_PATH}: {e} — the artifact does not even carry a `schema` \
                 token, so it is not a winner table this build can identify"
            ))
        })?;
        if probe.schema != MEDIUM_BASELINE_SCHEMA {
            return Err(invalid_declaration(format!(
                "{MEDIUM_BASELINE_PATH} declares schema {:?}, but this build reads only \
                 {MEDIUM_BASELINE_SCHEMA:?} — re-run `make maint-medium-sweep` rather than \
                 reading an unknown shape leniently",
                probe.schema
            )));
        }
        let parsed: Self = serde_json::from_str(text)
            .map_err(|e| invalid_declaration(format!("parse {MEDIUM_BASELINE_PATH}: {e}")))?;
        Ok(parsed)
    }

    /// The committed row for a dictionary id.
    ///
    /// # Errors
    /// No row carries that id.
    pub fn row(&self, id: &str) -> Result<&DictionaryBaseline, gmeow_errors::Diag> {
        self.dictionaries
            .iter()
            .find(|row| row.id == id)
            .ok_or_else(|| {
                undeclared_dictionary(format!(
                    "{MEDIUM_BASELINE_PATH} carries no winner row for dictionary {id:?} — the \
                     build consumes the COMMITTED winners, so a dictionary with no row has no \
                     measured (strategy, target length) to train at. Re-run \
                     `make maint-medium-sweep`"
                ))
            })
    }
}

/// Read the committed winner table from a repository root.
///
/// # Errors
/// The file is missing (the build consumes committed winners, so its absence is a
/// HARD FAIL rather than permission to fall back to the authored declaration),
/// unreadable, or malformed.
pub fn load(root: &Path) -> Result<MediumBaseline, gmeow_errors::Diag> {
    let path = root.join(MEDIUM_BASELINE_PATH);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        undeclared_dictionary(format!(
            "cannot read the committed medium winner table {}: {e} — the build trains at the \
             COMMITTED sweep winners, and falling back to the authored declaration would ship \
             dictionaries nobody measured. Run `make maint-medium-sweep`",
            path.display()
        ))
    })?;
    MediumBaseline::from_json(&text)
}

/// Every declared dictionary the sweep is required to measure: ALL of them.
///
/// There is no exemption. Every declared dictionary reaches its consumer as SHIPPED
/// bytes — the emitted-frame dictionaries through the bundle's own payload frames, the
/// runtime-store ones through the in-band `"dct"` map a consumer primes from
/// (`gmeow:dictionaryId` → exactly one byte sequence, including the compaction lane's,
/// which hands purrdf the shipped bytes as `DictStrategy::Pinned`). So `(strategy,
/// target length)` steers real bytes for every one of them, and a dictionary with no
/// measured row would be trained at a guess.
#[must_use]
pub fn measurable_ids(registry: &MediumRegistry) -> BTreeSet<String> {
    registry
        .dictionaries()
        .values()
        .map(|def| def.id.clone())
        .collect()
}

/// The winner table ↔ the DECLARED registry is a BIJECTION, and failing in either
/// direction is a hard fail.
///
/// * a declared dictionary with no committed row would be trained at an unmeasured
///   `(strategy, target length)` — the state this whole lane exists to remove;
/// * a committed row for a dictionary the registry no longer declares is a stale
///   winner: harmless-looking, and exactly how a retired dictionary's evidence
///   outlives it and gets cited.
///
/// # Errors
/// `MediumUndeclaredDictionary` in either direction, naming the offending ids.
pub fn check_bijection(
    registry: &MediumRegistry,
    baseline: &MediumBaseline,
) -> Result<(), gmeow_errors::Diag> {
    let measurable = measurable_ids(registry);
    let committed: BTreeSet<String> = baseline
        .dictionaries
        .iter()
        .map(|row| row.id.clone())
        .collect();
    if committed.len() != baseline.dictionaries.len() {
        return Err(undeclared_dictionary(format!(
            "{MEDIUM_BASELINE_PATH} carries a duplicated dictionary id — a winner table that \
             names one dictionary twice has no single answer for what to train it at"
        )));
    }
    let missing: Vec<&String> = measurable.difference(&committed).collect();
    let stale: Vec<&String> = committed.difference(&measurable).collect();
    if missing.is_empty() && stale.is_empty() {
        return Ok(());
    }
    Err(undeclared_dictionary(format!(
        "the committed winner table and the DECLARED dictionary registry are not a bijection: \
         declared-but-unmeasured {missing:?}, committed-but-undeclared {stale:?}. Both directions \
         are hard fails — an unmeasured dictionary would be trained at a guess, and a stale row \
         would steer a trainer for a dictionary the bundle no longer ships. There is no exempt \
         dictionary: every declared one reaches its consumer as SHIPPED bytes, so every declared \
         one is measured. Re-run `make maint-medium-sweep`"
    )))
}

/// The AUTHORED training point of every measurable dictionary IS the committed
/// argmin.
///
/// This is what lets the build train from the DECLARATION while still guaranteeing it
/// only ever trains at a MEASURED point. The direction matters: the sweep never
/// rewrites `slices/core/gts/module.ttl` — a measurement silently editing a human's
/// declaration is exactly the kind of invisible authority this repository refuses — so
/// a divergence is reported by the lane and reds here until a person reconciles the
/// two.
///
/// It is also what keeps the pipeline graph acyclic. The sweep runs the whole DAG in
/// order to measure, so a stage that STEERED off the table could not run until the
/// table existed, and the table could not exist until that stage ran.
///
/// # Errors
/// `MediumDictionaryRegression` naming every dictionary whose declaration is not the
/// committed winner.
pub fn check_declared_matches_winners(
    registry: &MediumRegistry,
    baseline: &MediumBaseline,
) -> Result<(), gmeow_errors::Diag> {
    let mut drift: Vec<String> = Vec::new();
    for id in measurable_ids(registry) {
        let def = registry.dictionary_by_id(&id)?;
        let row = baseline.row(&id)?;
        let (strategy, target) = row.winner()?;
        if def.strategy != strategy || def.target_length != target {
            drift.push(format!(
                "{id}: declared {}/{} but the committed argmin is {strategy}/{target}",
                def.strategy, def.target_length
            ));
        }
    }
    if drift.is_empty() {
        return Ok(());
    }
    Err(super::dictionary_regression(format!(
        "the AUTHORED dictionary training points have drifted from the committed sweep winners: \
         {drift:?}. The build trains from the DECLARATION — a sweep must never silently overwrite \
         what a human wrote down — so this is reconciled by editing gmeow:dictionaryStrategy / \
         gmeow:dictionaryTargetLength in slices/core/gts/module.ttl to the measured argmin, or by \
         re-running `make maint-medium-sweep` if the corpus has moved. Do NOT weaken this check: \
         without it the build would train at a point nobody measured"
    )))
}

/// Every committed reading says its dictionary PAID FOR ITSELF.
///
/// The build-time half of the criterion, and the one that can run cheaply on every
/// build: the committed evidence is a deterministic artifact, so refusing to build
/// against evidence that says a shipped dictionary loses costs nothing and cannot be
/// confused by scale. (The emission itself does NOT refuse — it serializes fixture-scale
/// folds too, where no dictionary of any size can pay for itself over a few hundred
/// bytes, so a refusal there would red on artifacts the criterion was never about. The
/// LIVE half, over the whole DAG's real output, is `tests/medium_bundle.rs`.)
///
/// There is no threshold. A dictionary that loses is retired or re-scoped by a HUMAN —
/// retiring one orphans every artifact already primed with it — so this states the
/// numbers and stops.
///
/// # Errors
/// `MediumDictionaryRegression` naming every committed reading that does not clear the
/// two-part code.
pub fn check_dictionaries_pay_for_themselves(
    baseline: &MediumBaseline,
) -> Result<(), gmeow_errors::Diag> {
    // A BOOTSTRAP seed ([`seed_from_registry`]) carries a zero baseline on every row.
    // It is refused here — a build must never train off a table nobody measured — but
    // with its own diagnosis, because "your dictionary does not pay for itself" would
    // send a reader hunting for a regression that is really a missing measurement.
    let seeded: Vec<&str> = baseline
        .dictionaries
        .iter()
        .filter(|row| row.bytes_on_disk_baseline == 0)
        .map(|row| row.id.as_str())
        .collect();
    if !seeded.is_empty() {
        return Err(super::dictionary_regression(format!(
            "{MEDIUM_BASELINE_PATH} carries a ZERO no-dictionary baseline for {seeded:?} — that \
             is the declaration-derived BOOTSTRAP seed (`medium-sweep --seed`), not evidence, and \
             a build must never train off a table nobody measured. Run `make maint-medium-sweep`, \
             which overwrites the seed with the real grid"
        )));
    }
    let losing: Vec<String> = baseline
        .dictionaries
        .iter()
        .filter(|row| row.two_part_code_bytes >= row.bytes_on_disk_baseline)
        .map(|row| {
            format!(
                "{} over `{}`: two-part {} B (= {} B of frames + {} B of in-band dictionary) vs \
                 baseline {} B over {} frame(s)",
                row.id,
                row.population,
                row.two_part_code_bytes,
                row.bytes_on_disk,
                row.dictionary_in_band_bytes,
                row.bytes_on_disk_baseline,
                row.evaluated_frame_count
            )
        })
        .collect();
    if losing.is_empty() {
        return Ok(());
    }
    Err(super::dictionary_regression(format!(
        "the committed sweep evidence says {} shipped dictionary/dictionaries do NOT pay for \
         themselves at their BEST measured cell: {losing:?}. There is no threshold to relax — \
         charging a dictionary its own in-band bytes is what makes the criterion non-vacuous. \
         Resolving it is a HUMAN decision (retire the dictionary, widen the population it primes, \
         or record why it stands), because retiring a shipped dictionary orphans every artifact \
         already primed with it. Do NOT weaken this check",
        losing.len()
    )))
}

/// Every committed row describes THE CORPUS THIS BUILD RESOLVED.
///
/// The three checks above — the bijection, the declared-is-argmin agreement, and the
/// pays-for-itself criterion — all grade the build against committed numbers. None of
/// them re-derives the argmin, and none of them can: the sweep costs a whole DAG run
/// plus a grid of trainings. What they CAN do is refuse to grade against numbers taken
/// over different material, and that is this check.
///
/// A `gmeow:DictionaryCorpus` is a SELECTOR re-resolved on every build, so an archive
/// gaining or losing one member moves the corpus without moving the table. Before this
/// existed, such a build kept grading: the winner table's byte columns were read by
/// nothing at all, so they could drift arbitrarily while `(strategy, target_length)`
/// still steered the trainer. A stale table must RED, not grade.
///
/// The digest compared here is the one the SWEEP recorded, against the one THIS build's
/// [`crate::stages::medium_dictionaries::corpus_samples`] resolved — one resolution
/// path, used by both, so agreement means the same material and disagreement means the
/// evidence is about something else.
///
/// # Errors
/// `MediumCorpusDrift` naming every dictionary whose recorded corpus identity is not
/// the resolved one, or that has no resolved corpus at all.
pub fn check_corpus_digests(
    registry: &MediumRegistry,
    baseline: &MediumBaseline,
    resolved: &std::collections::BTreeMap<String, super::corpus::CorpusResolution>,
) -> Result<(), gmeow_errors::Diag> {
    let mut drift: Vec<String> = Vec::new();
    for id in measurable_ids(registry) {
        let row = baseline.row(&id)?;
        let Some(live) = resolved.get(&id) else {
            return Err(super::corpus_drift(format!(
                "dictionary {id:?} has a committed winner row but this build resolved no corpus \
                 for it — there is nothing to check the evidence against, and grading against \
                 unanchored numbers is what this check exists to stop"
            )));
        };
        if row.corpus_digest != live.digest {
            drift.push(format!(
                "{id}: the committed evidence was measured over corpus {} ({} training sample(s), \
                 {} held out) but this build resolved {} ({} training sample(s), {} held out)",
                if row.corpus_digest.is_empty() {
                    "<none recorded>"
                } else {
                    row.corpus_digest.as_str()
                },
                row.corpus_sample_count,
                row.held_out_sample_count,
                live.digest,
                live.training.len(),
                live.held_out_count
            ));
        }
    }
    if drift.is_empty() {
        return Ok(());
    }
    Err(super::corpus_drift(format!(
        "the committed sweep evidence describes a corpus this build did not resolve: {drift:?}. A \
         gmeow:DictionaryCorpus is a SELECTOR re-resolved every build, so an archive that gained \
         or lost a member moves the corpus while {MEDIUM_BASELINE_PATH} sits still — and every \
         verdict read out of that table (the bijection, the declared-is-argmin agreement, the \
         pays-for-itself criterion) would then be about a sweep nobody re-ran. Re-run \
         `make maint-medium-sweep`. Do NOT weaken this check: stale evidence is not weaker \
         evidence, it is evidence about a different corpus"
    )))
}

/// A BOOTSTRAP winner table derived from the authored declarations alone, with every
/// measured field left at zero.
///
/// It exists to break a start-up cycle and nothing else: [`run_sweep`] runs the whole
/// DAG to measure, and `stage-medium-dictionaries` refuses to run without a committed
/// table, so the very first sweep in a fresh tree (or after the file is deleted) has no
/// way in. `medium-sweep --seed` writes this, the sweep immediately OVERWRITES it with
/// real numbers, and `the_committed_winner_table_carries_real_measurements` refuses a
/// seed that was ever committed — a row whose baseline is zero cannot have been
/// measured.
#[must_use]
pub fn seed_from_registry(registry: &MediumRegistry) -> MediumBaseline {
    let primed: BTreeSet<String> = registry
        .schemas()
        .values()
        .filter_map(
            |schema| match &registry.assignment_for(&schema.rep).ok()?.dictionary {
                super::registry::DictSelection::Named(iri) => {
                    registry.dictionaries().get(iri).map(|def| def.id.clone())
                }
                super::registry::DictSelection::Baseline => None,
            },
        )
        .collect();
    let dictionaries = measurable_ids(registry)
        .into_iter()
        .filter_map(|id| {
            let def = registry.dictionary_by_id(&id).ok()?;
            let population = if primed.contains(&id) {
                Population::EmittedBlobFrames
            } else {
                Population::RuntimeStoreSegments
            };
            Some(DictionaryBaseline {
                id: id.clone(),
                population: population.wire().to_string(),
                declared_strategy: strategy_wire(def.strategy),
                declared_target_length: def.target_length as u64,
                winning_strategy: strategy_wire(def.strategy),
                winning_target_length: def.target_length as u64,
                declared_is_argmin: true,
                bytes_on_disk: 0,
                bytes_on_disk_baseline: 0,
                dictionary_in_band_bytes: 0,
                two_part_code_bytes: 0,
                dictionary_gain_fraction: "0.000000".to_string(),
                corpus_sample_count: 0,
                held_out_sample_count: 0,
                // The seed resolves NO corpus (it never runs the DAG), so it records
                // no corpus identity. The empty string is not a digest and cannot
                // match one; `check_dictionaries_pay_for_themselves` refuses the seed
                // first, with the diagnosis a reader actually needs.
                corpus_digest: String::new(),
                evaluated_frame_count: 0,
                grid: Vec::new(),
            })
        })
        .collect();
    MediumBaseline {
        schema: MEDIUM_BASELINE_SCHEMA.to_string(),
        codec_sweep: CodecSweep {
            mandated_codec: "zstd-rsyncable".to_string(),
            mandated_level: 12,
            corpus_frame_count: 0,
            corpus_bytes: 0,
            excluded_reps: Vec::new(),
            rows: Vec::new(),
            mandated_is_argmin: true,
        },
        dictionaries,
    }
}

// ── The sweep itself ─────────────────────────────────────────────────────────

/// Everything a grid sweep needs about ONE dictionary, grouped so the call site cannot
/// transpose its coordinates.
///
/// A struct rather than a positional list because three of the fields are the same
/// shape (`&[u8]`-ish corpora) and two more are same-typed integers: `corpus` and
/// `term_table` are both byte material but only one of them is the DECLARED corpus,
/// and `declared_target_length` and `corpus_sample_count` are both counts that mean
/// entirely different things.
pub struct DictionarySweepInputs<'a> {
    /// `gmeow:dictionaryId`.
    pub id: &'a str,
    /// The AUTHORED `gmeow:dictionaryStrategy` — the incumbent the grid prices.
    pub declared_strategy: DictionaryStrategy,
    /// The AUTHORED `gmeow:dictionaryTargetLength` — likewise always on the grid.
    pub declared_target_length: usize,
    /// The dictionary's DECLARED training corpus, already resolved against this run.
    pub corpus: &'a [&'a [u8]],
    /// The bundle's own canonical term rendering — the corpus the `term-table`
    /// strategy trains over (it differs from `raw-content` in WHAT is fed in, not in
    /// how the trainer runs).
    pub term_table: &'a [u8],
    /// How many samples `corpus` holds, recorded on every cell's reading.
    pub corpus_sample_count: u64,
    /// How many archive members the declared split held OUT of `corpus` — the members
    /// the evaluated frame still carries and no cell was trained on.
    pub held_out_sample_count: u64,
    /// The identity of the WHOLE resolved corpus (held-out members included), carried
    /// onto the committed row so a later build can prove the table is about the corpus
    /// it is grading.
    pub corpus_digest: &'a str,
}

/// One dictionary's whole grid, and the argmin it selects.
///
/// `corpus` is the dictionary's DECLARED training corpus, already resolved; `frames`
/// are the population-A frames the dictionary primes. `term_table` is the bundle's own
/// canonical term rendering, which is the corpus the `term-table` strategy trains over
/// (that strategy differs from `raw-content` in WHAT is fed in, not in how it trains).
///
/// # Errors
/// A trainer refusal on every cell (a dictionary with no buildable cell has no
/// winner), or an encoder refusal.
pub fn sweep_dictionary(
    inputs: &DictionarySweepInputs<'_>,
    frames: &[&[u8]],
    codec: &str,
    level: i32,
) -> Result<DictionaryBaseline, gmeow_errors::Diag> {
    let DictionarySweepInputs {
        id,
        declared_strategy,
        declared_target_length,
        corpus,
        term_table,
        corpus_sample_count,
        held_out_sample_count,
        corpus_digest,
    } = *inputs;
    // The baseline arm is dictionary-INDEPENDENT, so it is computed once rather than
    // once per cell: every cell is compared against the same declared no-dictionary
    // code over the same frames.
    let mut baseline_bytes = 0u64;
    for frame in frames {
        baseline_bytes += encoded_len(codec, level, None, frame)?;
    }

    let targets = target_grid(declared_target_length);
    let mut grid: Vec<GridRow> = Vec::new();
    let mut best: Option<(DictionaryStrategy, usize, u64, u64, u64)> = None;
    for strategy in SWEEP_STRATEGIES {
        let cell_corpus: Vec<&[u8]> = match strategy {
            DictionaryStrategy::TermTable => vec![term_table],
            _ => corpus.to_vec(),
        };
        for target in targets.iter().copied() {
            // A cell the trainer refuses (a target too small to hold the finalized
            // header, a corpus too thin for FastCOVER) is RECORDED as absent from the
            // grid rather than treated as an infinite code: the grid is evidence, and
            // an invented number would be worse than a missing row.
            let Ok(dict) = super::train::build(strategy, &cell_corpus, target) else {
                continue;
            };
            let mut encoded = 0u64;
            for frame in frames {
                encoded += encoded_len(codec, level, Some(&dict), frame)?;
            }
            let two_part = encoded + dict.len() as u64;
            grid.push(GridRow {
                strategy: strategy_wire(strategy),
                target_length: target as u64,
                dictionary_bytes: dict.len() as u64,
                two_part_code_bytes: two_part,
            });
            // Ties break toward the SMALLER dictionary, then the earlier strategy:
            // two cells with the same code are not equally good, because the smaller
            // one raises the same reader contract for fewer in-band bytes.
            let better = match &best {
                None => true,
                Some((_, _, best_code, best_dict, _)) => {
                    two_part < *best_code
                        || (two_part == *best_code && (dict.len() as u64) < *best_dict)
                }
            };
            if better {
                best = Some((strategy, target, two_part, dict.len() as u64, encoded));
            }
        }
    }

    let (strategy, target, two_part, dict_bytes, encoded) = best.ok_or_else(|| {
        undeclared_dictionary(format!(
            "no ({}) grid cell produced a buildable dictionary for {id:?} — every strategy at \
             every declared target length was refused, so there is no winner to commit",
            SWEEP_STRATEGIES.len() * targets.len()
        ))
    })?;

    let effect = DictionaryEffect {
        dictionary_id: id.to_string(),
        population: Population::EmittedBlobFrames,
        bytes_on_disk: encoded,
        bytes_on_disk_baseline: baseline_bytes,
        dictionary_in_band_bytes: dict_bytes,
        corpus_sample_count,
        evaluated_frame_count: frames.len() as u64,
    };
    let _ = two_part;

    Ok(DictionaryBaseline {
        id: id.to_string(),
        population: Population::EmittedBlobFrames.wire().to_string(),
        declared_strategy: strategy_wire(declared_strategy),
        declared_target_length: declared_target_length as u64,
        winning_strategy: strategy_wire(strategy),
        winning_target_length: target as u64,
        declared_is_argmin: declared_strategy == strategy && declared_target_length == target,
        bytes_on_disk: effect.bytes_on_disk,
        bytes_on_disk_baseline: effect.bytes_on_disk_baseline,
        dictionary_in_band_bytes: effect.dictionary_in_band_bytes,
        two_part_code_bytes: effect.two_part_code_bytes(),
        dictionary_gain_fraction: effect.gain_fraction_lexical(),
        corpus_sample_count,
        held_out_sample_count,
        corpus_digest: corpus_digest.to_string(),
        evaluated_frame_count: effect.evaluated_frame_count,
        grid,
    })
}

/// One dictionary's whole grid over population **B** — the runtime store.
///
/// Structurally the same argmin as [`sweep_dictionary`], but the "encode" step is a
/// REPLAY: each cell writes the declared corpus into a temp store primed with that
/// cell's dictionary and prices the resulting file. That is the only faithful way to
/// price a store dictionary, because its in-band cost is paid PER SEGMENT HEADER
/// rather than once per artifact, and a synthetic sum over frame payloads would
/// silently drop that term.
///
/// # Errors
/// A store refusal, or a grid with no buildable cell.
pub fn sweep_dictionary_runtime_store(
    inputs: &DictionarySweepInputs<'_>,
    replay: &[String],
    dir: &Path,
) -> Result<DictionaryBaseline, gmeow_errors::Diag> {
    let DictionarySweepInputs {
        id,
        declared_strategy,
        declared_target_length,
        corpus,
        term_table,
        corpus_sample_count,
        held_out_sample_count,
        corpus_digest,
    } = *inputs;
    let targets = target_grid(declared_target_length);
    let mut grid: Vec<GridRow> = Vec::new();
    let mut best: Option<(DictionaryStrategy, usize, DictionaryEffect)> = None;
    for strategy in SWEEP_STRATEGIES {
        let cell_corpus: Vec<&[u8]> = match strategy {
            DictionaryStrategy::TermTable => vec![term_table],
            _ => corpus.to_vec(),
        };
        for target in targets.iter().copied() {
            let Ok(dict) = super::train::build(strategy, &cell_corpus, target) else {
                continue;
            };
            let cell_dir = dir.join(format!("{strategy}-{target}"));
            std::fs::create_dir_all(&cell_dir).map_err(|err| {
                invalid_declaration(format!(
                    "cannot open the population-B replay directory {}: {err}",
                    cell_dir.display()
                ))
            })?;
            let (primed, baseline) = replay_runtime_store(&cell_dir, id, &dict, replay)?;
            let effect = super::measure::population_b(
                id,
                &primed,
                &baseline,
                corpus_sample_count,
                replay.len() as u64,
            )?;
            grid.push(GridRow {
                strategy: strategy_wire(strategy),
                target_length: target as u64,
                dictionary_bytes: dict.len() as u64,
                two_part_code_bytes: effect.two_part_code_bytes(),
            });
            let better = match &best {
                None => true,
                Some((_, _, incumbent)) => {
                    effect.two_part_code_bytes() < incumbent.two_part_code_bytes()
                }
            };
            if better {
                best = Some((strategy, target, effect));
            }
        }
    }
    let (strategy, target, effect) = best.ok_or_else(|| {
        undeclared_dictionary(format!(
            "no grid cell produced a buildable runtime-store dictionary for {id:?}"
        ))
    })?;
    Ok(DictionaryBaseline {
        id: id.to_string(),
        population: Population::RuntimeStoreSegments.wire().to_string(),
        declared_strategy: strategy_wire(declared_strategy),
        declared_target_length: declared_target_length as u64,
        winning_strategy: strategy_wire(strategy),
        winning_target_length: target as u64,
        declared_is_argmin: declared_strategy == strategy && declared_target_length == target,
        bytes_on_disk: effect.bytes_on_disk,
        bytes_on_disk_baseline: effect.bytes_on_disk_baseline,
        dictionary_in_band_bytes: effect.dictionary_in_band_bytes,
        two_part_code_bytes: effect.two_part_code_bytes(),
        dictionary_gain_fraction: effect.gain_fraction_lexical(),
        corpus_sample_count,
        held_out_sample_count,
        corpus_digest: corpus_digest.to_string(),
        evaluated_frame_count: effect.evaluated_frame_count,
        grid,
    })
}

/// The GLOBAL codec × level sweep: is the mandated Rule 6 chain the argmin over the
/// declared grid?
///
/// # Errors
/// An encoder refusal on a cell whose codec this build claims to support.
pub fn sweep_codecs(
    frames: &[&[u8]],
    excluded_reps: &[String],
    mandated_codec: &str,
    mandated_level: i32,
) -> Result<CodecSweep, gmeow_errors::Diag> {
    let mut rows: Vec<CodecRow> = Vec::new();
    for codec in SWEEP_CODECS {
        for level in SWEEP_LEVELS {
            let mut bytes = 0u64;
            for frame in frames {
                bytes += encoded_len(codec, level, None, frame)?;
            }
            rows.push(CodecRow {
                codec: codec.to_string(),
                level,
                bytes,
            });
        }
    }
    rows.sort_by(|a, b| a.codec.cmp(&b.codec).then(a.level.cmp(&b.level)));
    let mandated = rows
        .iter()
        .find(|row| row.codec == mandated_codec && row.level == mandated_level)
        .ok_or_else(|| {
            invalid_declaration(format!(
                "the codec sweep grid does not contain the MANDATED cell ({mandated_codec} level \
                 {mandated_level}) — a sweep that cannot price the chain the bundle actually uses \
                 is evidence about nothing"
            ))
        })?
        .bytes;
    let argmin = rows.iter().map(|row| row.bytes).min().unwrap_or(mandated);
    Ok(CodecSweep {
        mandated_codec: mandated_codec.to_string(),
        mandated_level,
        corpus_frame_count: frames.len() as u64,
        corpus_bytes: frames.iter().map(|f| f.len() as u64).sum(),
        excluded_reps: excluded_reps.to_vec(),
        rows,
        mandated_is_argmin: mandated == argmin,
    })
}

// ── Population B: the runtime-store replay ───────────────────────────────────

/// The DECLARED CEILING on the population-B replay corpus.
///
/// Fixed and declared because the answer the measurement gives DEPENDS on it: a
/// dictionary paid once per store file wins only once the records it primes outweigh
/// its own bytes, so "how many records" is part of the claim, never a fixture
/// author's taste. A reader who wants a different extent changes this constant and
/// re-runs the lane; they do not get a different answer by accident.
///
/// It is a CEILING, not an equality: [`replay_corpus`] takes at most this many lines
/// from the bundle's own statement layer, and that layer is currently shorter, so the
/// EFFECTIVE extent is whatever the bundle yields. That is why every reading records
/// its own `evaluated_frame_count` rather than citing this constant — the number the
/// verdict actually depends on is the one in the artifact, and the live gate in
/// `crates/pipeline/tests/medium_bundle.rs` pins the live corpus to exactly it.
pub const REPLAY_RECORD_COUNT: usize = 512;

/// Replay a declared, bundle-derived corpus through the REAL `Memory::store` path,
/// twice: once primed with `dictionary`, once through the declared no-dictionary
/// medium.
///
/// Both arms use `purrdf::gts::examples::agent_memory::Memory` with the production
/// authoring options (`ai-package`, `zstd-rsyncable`, level 12 — upstream's defaults,
/// which ARE the mandated profile), exactly as [`crate::mcp`] configures a runtime
/// store. The ONLY difference between the arms is the in-band dictionary, which is
/// what makes their difference attributable to it.
///
/// Returns `(primed bytes, baseline bytes)`.
///
/// # Errors
/// The store refuses a record, or either file cannot be read back.
pub fn replay_runtime_store(
    dir: &Path,
    dictionary: &str,
    dictionary_bytes: &[u8],
    corpus: &[String],
) -> Result<(Vec<u8>, Vec<u8>), gmeow_errors::Diag> {
    use purrdf::gts::examples::agent_memory::{Memory, MemoryOptions, StoreOptions};

    let write = |path: &Path, options: MemoryOptions| -> Result<Vec<u8>, gmeow_errors::Diag> {
        let memory = Memory::with_options(path, options);
        for text in corpus {
            memory.store(text, StoreOptions::default()).map_err(|err| {
                invalid_declaration(format!(
                    "the population-B replay store refused a record: {err} — a replay that \
                         dropped records would measure a corpus smaller than the one it declares"
                ))
            })?;
        }
        std::fs::read(path).map_err(|err| {
            invalid_declaration(format!(
                "cannot read back the replayed store {}: {err}",
                path.display()
            ))
        })
    };

    let primed = write(
        &dir.join("primed-memory.gts"),
        MemoryOptions {
            dicts: vec![(dictionary.to_string(), dictionary_bytes.to_vec())],
            dict: Some(dictionary.to_string()),
            ..MemoryOptions::default()
        },
    )?;
    let baseline = write(&dir.join("baseline-memory.gts"), MemoryOptions::default())?;
    Ok((primed, baseline))
}

/// The DECLARED, bundle-derived replay corpus: the first [`REPLAY_RECORD_COUNT`]
/// canonical N-Triples lines of the claim corpus's own named graph.
///
/// Bundle-derived rather than hand-written, so the records a runtime store is measured
/// over are the SAME statement-layer material the bundle's `yaml-ld-archive` frame
/// carries — and canonical, so the corpus is a pure function of the bundle rather than of a
/// traversal order. A short bundle simply yields a shorter corpus; the cardinality is
/// recorded beside the result either way.
///
/// # Errors
/// The graph does not canonicalize.
pub fn replay_corpus(dataset: &purrdf::RdfDataset) -> Result<Vec<String>, gmeow_errors::Diag> {
    let projected = dataset.project_named_graph(crate::stages::carrier::GRAPH_STATEMENTS);
    let ntriples = purrdf::canonical_flat_nquads(&projected).map_err(|err| {
        invalid_declaration(format!(
            "the population-B replay corpus does not canonicalize: {err}"
        ))
    })?;
    Ok(ntriples
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(REPLAY_RECORD_COUNT)
        .map(str::to_string)
        .collect())
}

// ── The maintainer lane's driver ─────────────────────────────────────────────

/// The reps DECLARED-excluded from the GLOBAL codec sweep corpus.
///
/// The documentation/export payloads are hundreds of megabytes; pricing them at eight
/// codec × level cells (including level 19, which is several times slower than the
/// mandated 12) would dominate the lane's cost without changing which cell wins — the
/// codec question is about the CHAIN, and the chain's ordering is not a function of
/// these four payloads. The exclusion is DATA (`CodecSweep::excluded_reps`) rather
/// than a habit, so a reader sees exactly which frames were priced.
pub const CODEC_SWEEP_EXCLUDED_REPS: [&str; 4] =
    ["docs-book", "docs-print", "okf-export", "ontology-docs"];

/// Run the whole sweep over a repository: the real DAG once, then the
/// `(strategy × target length)` grid per measurable dictionary plus the global codec ×
/// level grid.
///
/// This is the body of `make maint-medium-sweep`. It is deliberately in the library
/// rather than the binary so the grid logic is unit-testable and so the binary stays a
/// thin argument/exit-code shell.
///
/// # Errors
/// Any DAG failure, a corpus that resolves to nothing, a trainer or encoder refusal,
/// or a dictionary whose grid has no buildable cell.
pub fn run_sweep(root: &Path) -> Result<MediumBaseline, gmeow_errors::Diag> {
    let spec = crate::full_spec();
    let graph = spec.validate()?;
    let bound = crate::bind(&spec, &graph, &crate::default_registry())?;
    let jobs = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4);
    let mut ctx = crate::RunContext::open(root, jobs)?;
    // Everything EXCEPT the two stages that CONSUME this lane's own output.
    //
    // `stage-medium-dictionaries` reads the committed winner table as a gate — the
    // bijection, the declared-is-argmin agreement, and the refusal to build against
    // evidence that says a dictionary loses — and the terminal sink emits only what
    // that stage produced. Running either here would let the gate eat its own
    // diagnosis: a sweep re-run to REPLACE a table that says a dictionary loses (or a
    // first sweep in a tree whose table is only the declaration-derived bootstrap
    // seed) could never start, because the stale table it exists to replace would
    // refuse the run.
    //
    // Skipping them is safe by construction rather than by inspection:
    // [`crate::scheduler::run_without`] hard-fails if any REMAINING stage consumes a
    // skipped one, and the only consumer of `stage-medium-dictionaries` is the sink,
    // which is skipped with it. Every product the sweep reads — the carrier, the
    // archives, the statement lane, the reasoning products — is produced upstream of
    // both.
    let skip: BTreeSet<String> = [
        crate::run::SINK_STAGE.to_string(),
        crate::stages::medium_dictionaries::STAGE_ID.to_string(),
    ]
    .into_iter()
    .collect();
    let products = crate::scheduler::run_without(&graph, &bound, &mut ctx, &skip)?.products;

    let carrier = crate::stages::carrier::snapshot_dataset(&products)?;
    let frames = crate::stages::carrier::snapshot_frames(root, &products, carrier.as_ref())?;
    let payload_frames = frames.payload_frames();
    let crate::stages::medium_dictionaries::ResolvedCorpora {
        registry,
        corpora,
        term_table,
    } = crate::stages::medium_dictionaries::resolved_corpora(root, &products)?;
    let (codec, level) = super::measure::mandated_chain(&registry, &payload_frames)?;
    let by_dictionary = super::measure::frames_by_dictionary(&registry, &payload_frames)?;

    // The codec grid, over the primed population minus the four oversized reps.
    let codec_corpus: Vec<&[u8]> = payload_frames
        .iter()
        .filter(|row| !CODEC_SWEEP_EXCLUDED_REPS.contains(&row.rep.as_str()))
        .map(|row| row.data.as_slice())
        .collect();
    let excluded: Vec<String> = CODEC_SWEEP_EXCLUDED_REPS
        .iter()
        .map(|rep| (*rep).to_string())
        .collect();
    let codec_sweep = sweep_codecs(&codec_corpus, &excluded, &codec, level)?;

    let replay = replay_corpus(carrier.as_ref())?;
    let replay_dir = tempfile::tempdir().map_err(|err| {
        invalid_declaration(format!(
            "cannot open a population-B replay directory: {err}"
        ))
    })?;

    let mut rows: Vec<DictionaryBaseline> = Vec::new();
    for id in measurable_ids(&registry) {
        let def = registry.dictionary_by_id(&id)?;
        let resolved = corpora.get(&id).ok_or_else(|| {
            undeclared_dictionary(format!("dictionary {id:?} resolved to no training corpus"))
        })?;
        let corpus: Vec<&[u8]> = resolved.training.iter().map(Vec::as_slice).collect();
        let inputs = DictionarySweepInputs {
            id: &id,
            declared_strategy: def.strategy,
            declared_target_length: def.target_length,
            corpus: &corpus,
            term_table: &term_table,
            corpus_sample_count: resolved.training.len() as u64,
            held_out_sample_count: resolved.held_out_count,
            corpus_digest: &resolved.digest,
        };
        let row = match by_dictionary.get(&id) {
            // Population A: the dictionary primes emitted blob frames.
            Some(primed) => {
                let population: Vec<&[u8]> = primed.iter().map(|row| row.data.as_slice()).collect();
                sweep_dictionary(&inputs, &population, &codec, level)?
            }
            // Population B: no bundle rep names it, so its frames are the ones a
            // CONSUMER writes into a runtime store out of the shipped header.
            None => sweep_dictionary_runtime_store(&inputs, &replay, replay_dir.path())?,
        };
        rows.push(row);
    }
    rows.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(MediumBaseline {
        schema: MEDIUM_BASELINE_SCHEMA.to_string(),
        codec_sweep,
        dictionaries: rows,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::medium::registry::fixture;

    fn registry() -> MediumRegistry {
        MediumRegistry::from_dataset(&fixture::dataset("")).expect("fixture registry")
    }

    fn row(id: &str) -> DictionaryBaseline {
        DictionaryBaseline {
            id: id.to_string(),
            population: Population::EmittedBlobFrames.wire().to_string(),
            declared_strategy: "trained".to_string(),
            declared_target_length: 4096,
            winning_strategy: "trained".to_string(),
            winning_target_length: 4096,
            declared_is_argmin: true,
            bytes_on_disk: 400,
            bytes_on_disk_baseline: 1000,
            dictionary_in_band_bytes: 100,
            two_part_code_bytes: 500,
            dictionary_gain_fraction: "0.500000".to_string(),
            corpus_sample_count: 12,
            held_out_sample_count: 2,
            corpus_digest: super::super::blake3_digest(id.as_bytes()),
            evaluated_frame_count: 3,
            grid: Vec::new(),
        }
    }

    fn baseline(ids: &[&str]) -> MediumBaseline {
        MediumBaseline {
            schema: MEDIUM_BASELINE_SCHEMA.to_string(),
            codec_sweep: CodecSweep {
                mandated_codec: "zstd-rsyncable".to_string(),
                mandated_level: 12,
                corpus_frame_count: 3,
                corpus_bytes: 4096,
                excluded_reps: Vec::new(),
                rows: Vec::new(),
                mandated_is_argmin: true,
            },
            dictionaries: ids.iter().map(|id| row(id)).collect(),
        }
    }

    /// (f) The bijection hard-fails in BOTH directions — a missing row and a stale
    /// one are each a way for a dictionary to escape gate coverage.
    #[test]
    fn the_winner_table_registry_bijection_hard_fails_in_both_directions() {
        let registry = registry();
        check_bijection(&registry, &baseline(&["gmeow-core-v1", "gmeow-terms-v1"]))
            .expect("the exact measurable set is a bijection");

        let missing = check_bijection(&registry, &baseline(&["gmeow-core-v1"]))
            .expect_err("a declared-but-unmeasured dictionary must hard-fail");
        assert_eq!(
            missing.code(),
            crate::error::MediumUndeclaredDictionary::register(),
            "{missing}"
        );
        assert!(missing.to_string().contains("gmeow-terms-v1"), "{missing}");

        let stale = check_bijection(
            &registry,
            &baseline(&["gmeow-core-v1", "gmeow-terms-v1", "gmeow-retired-v1"]),
        )
        .expect_err("a committed-but-undeclared dictionary must hard-fail");
        assert!(stale.to_string().contains("gmeow-retired-v1"), "{stale}");
    }

    /// The measurable set is EVERY declared dictionary — the sweep exempts none.
    #[test]
    fn every_declared_dictionary_is_measurable() {
        let registry = registry();
        let declared: BTreeSet<String> = registry
            .dictionaries()
            .values()
            .map(|def| def.id.clone())
            .collect();
        assert_eq!(
            measurable_ids(&registry),
            declared,
            "an id that is declared but not measurable would be trained at a guess"
        );
        assert_eq!(
            declared,
            ["gmeow-core-v1".to_string(), "gmeow-terms-v1".to_string()]
                .into_iter()
                .collect::<BTreeSet<String>>(),
            "the fixture registry's declaration set drifted, so the equality above is not the \
             statement it reads as"
        );
    }

    /// The artifact round-trips byte-identically and refuses an unknown schema token.
    #[test]
    fn the_artifact_round_trips_and_refuses_an_unknown_schema() {
        let original = baseline(&["gmeow-core-v1"]);
        let json = original.to_json().expect("serialize");
        assert!(json.ends_with('\n'), "the artifact ends with a newline");
        assert_eq!(
            MediumBaseline::from_json(&json).expect("parse"),
            original,
            "the committed artifact must round-trip"
        );
        // Byte-stability: rendering the parsed value reproduces the same bytes.
        assert_eq!(
            MediumBaseline::from_json(&json)
                .expect("parse")
                .to_json()
                .expect("serialize"),
            json
        );
        let skewed = json.replace(MEDIUM_BASELINE_SCHEMA, "gmeow.medium-baseline.v99");
        let diag =
            MediumBaseline::from_json(&skewed).expect_err("an unknown schema must hard-fail");
        assert!(diag.to_string().contains("maint-medium-sweep"), "{diag}");
    }

    /// The strategy wire tokens round-trip against `DictionaryStrategy`'s own
    /// `Display`, so the committed artifact and the diagnostics cannot drift.
    #[test]
    fn the_strategy_wire_tokens_round_trip() {
        for strategy in SWEEP_STRATEGIES {
            let token = strategy_wire(strategy);
            assert_eq!(strategy_from_wire(&token), Some(strategy), "{token}");
        }
        assert_eq!(strategy_from_wire("invented"), None);
    }

    /// The sweep selects the argmin of the TWO-PART code, and the grid it selected
    /// from is committed beside the winner — a winner with no visible grid is an
    /// assertion, not evidence.
    #[test]
    fn the_sweep_selects_the_two_part_argmin_and_commits_its_grid() {
        let owned: Vec<Vec<u8>> = (0..600u32)
            .map(|i| {
                format!(
                    "<https://blackcatinformatics.ca/gmeow/term{}> \
                     <https://blackcatinformatics.ca/gmeow/definition> \
                     \"a definition of term {i} in the gmeow ontology\" .\n",
                    i % 41
                )
                .into_bytes()
            })
            .collect();
        let corpus: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
        let frame = owned.concat();
        let frames: Vec<&[u8]> = vec![frame.as_slice()];
        let term_table = b"<https://blackcatinformatics.ca/gmeow/definition>\n".to_vec();
        let swept = sweep_dictionary(
            &DictionarySweepInputs {
                id: "gmeow-core-v1",
                declared_strategy: DictionaryStrategy::Trained,
                declared_target_length: 4096,
                corpus: &corpus,
                term_table: &term_table,
                corpus_sample_count: corpus.len() as u64,
                held_out_sample_count: 7,
                corpus_digest: "blake3:00",
            },
            &frames,
            "zstd-rsyncable",
            12,
        )
        .expect("sweep");
        assert_eq!(swept.held_out_sample_count, 7);
        assert_eq!(swept.corpus_digest, "blake3:00");
        assert!(!swept.grid.is_empty(), "the grid is the evidence");
        let argmin = swept
            .grid
            .iter()
            .map(|cell| cell.two_part_code_bytes)
            .min()
            .expect("non-empty grid");
        assert_eq!(
            swept.two_part_code_bytes, argmin,
            "the committed winner must be the grid's two-part argmin"
        );
        assert_eq!(
            swept.declared_is_argmin,
            swept.winning_strategy == "trained" && swept.winning_target_length == 4096
        );
        assert_eq!(swept.evaluated_frame_count, 1);
    }

    /// The corpus-identity gate passes on agreement and REDS the moment the resolved
    /// corpus is not the one the table was measured over — which is what a committed
    /// table quietly rotting looks like from the build's side.
    #[test]
    fn the_corpus_identity_gate_reds_when_the_resolved_corpus_moves() {
        use crate::medium::corpus::CorpusResolution;

        let registry = registry();
        let baseline = baseline(&["gmeow-core-v1", "gmeow-terms-v1"]);
        let resolution = |id: &str| CorpusResolution {
            training: [b"a sample".to_vec()].into_iter().collect(),
            held_out_count: 2,
            digest: super::super::blake3_digest(id.as_bytes()),
        };
        let mut resolved: BTreeMap<String, CorpusResolution> = ["gmeow-core-v1", "gmeow-terms-v1"]
            .into_iter()
            .map(|id| (id.to_string(), resolution(id)))
            .collect();
        check_corpus_digests(&registry, &baseline, &resolved)
            .expect("the recorded identity IS the resolved one");

        // One archive member changes: the corpus moves, the table does not.
        resolved.insert(
            "gmeow-core-v1".to_string(),
            CorpusResolution {
                training: [b"a sample".to_vec(), b"a new archive member".to_vec()]
                    .into_iter()
                    .collect(),
                held_out_count: 2,
                digest: super::super::blake3_digest(b"gmeow-core-v1 plus one member"),
            },
        );
        let diag = check_corpus_digests(&registry, &baseline, &resolved)
            .expect_err("a moved corpus must hard-fail rather than keep grading");
        assert_eq!(
            diag.code(),
            crate::error::MediumCorpusDrift::register(),
            "{diag}"
        );
        assert!(
            diag.to_string().contains("gmeow-core-v1")
                && diag.to_string().contains("maint-medium-sweep"),
            "{diag}"
        );
        assert!(
            !diag.to_string().contains("gmeow-terms-v1"),
            "only the dictionary whose corpus moved is named: {diag}"
        );
    }

    /// A dictionary the build resolves no corpus for has nothing to anchor its
    /// committed row to, which is the same defect wearing a different hat.
    #[test]
    fn a_committed_row_with_no_resolved_corpus_reds() {
        let registry = registry();
        let baseline = baseline(&["gmeow-core-v1", "gmeow-terms-v1"]);
        let diag = check_corpus_digests(&registry, &baseline, &BTreeMap::new())
            .expect_err("an unanchored row must hard-fail");
        assert_eq!(
            diag.code(),
            crate::error::MediumCorpusDrift::register(),
            "{diag}"
        );
    }

    /// The codec sweep prices the MANDATED cell and says, as data, whether it is the
    /// argmin — the STOP-and-ask signal.
    #[test]
    fn the_codec_sweep_prices_the_mandated_cell() {
        let payload = b"<https://e/s> <https://e/p> \"v\" .\n".repeat(400);
        let frames: Vec<&[u8]> = vec![payload.as_slice()];
        let sweep = sweep_codecs(
            &frames,
            &["ontology-docs".to_string()],
            "zstd-rsyncable",
            12,
        )
        .expect("codec sweep");
        assert_eq!(
            sweep.rows.len(),
            SWEEP_CODECS.len() * SWEEP_LEVELS.len(),
            "every declared cell is priced"
        );
        assert!(
            sweep
                .rows
                .iter()
                .any(|row| row.codec == "zstd-rsyncable" && row.level == 12),
            "the mandated cell is on the grid"
        );
        assert_eq!(sweep.excluded_reps, ["ontology-docs".to_string()]);
        assert_eq!(sweep.corpus_frame_count, 1);
        // A cell the sweep cannot price is a hard fail, never an omitted row.
        assert!(sweep_codecs(&frames, &[], "brotli", 12).is_err());
    }
}
