// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `medium-dictionaries` stage: the SINGLE producer of the bundle's zstd
//! dictionaries and of the `graph/medium-registry` named graph they are described
//! in.
//!
//! [`crate::medium`] is the executable twin of `slices/core/gts/module.ttl`'s
//! medium axis; this stage is where that twin RUNS. It reads the declared
//! dictionaries, corpora, payload schemas, media, and the authored rep→medium
//! assignment off the in-memory carrier, resolves each corpus's selectors against
//! THIS run's upstream products, trains one dictionary per declaration, and
//! measures each into a `gmeow:CompressionDictionaryRealization`.
//!
//! # Why the trained bytes ride an INTERNAL artifact path
//!
//! The dictionaries are emitted under the `pipeline/` logical prefix, the lane the
//! reconcile treats as in-memory dataflow rather than a committed output. That is
//! deliberate: a zstd dictionary's shipping channel is the GTS segment header's
//! in-band `"dct"` map (spec §5), which is where a consumer — including one priming
//! its OWN runtime store with `gmeow-memory-hot-v1` — actually reads it from. Tarring
//! a second copy into the generated-opaque archive would put the same bytes in two
//! places, re-fold a blob the snapshot already carries (Constitution §18), and hand
//! high-entropy bytes to a compressor.
//!
//! The committed `generated/medium/<dict-id>.zdict` files are not that second copy:
//! they are a PROJECTION of the one header entry, reconstructed by the superset
//! gate's `header-dict` fanout family
//! ([`crate::stages::superset`]) exactly as an EDOAL file is reconstructed from its
//! named graph. Because those files are materialized,
//! [`crate::medium::MEDIUM_GENERATED_PREFIX`] is a LIVE corpus-fixpoint hazard — a
//! selector covering it would train the next build's dictionary on this build's
//! dictionaries — which is why [`crate::medium::corpus`] refuses it statically.
//!
//! # Why the envelopes are sealed at the SINK and not here
//!
//! A `gmeow:MediumEnvelope` is the projection of an EMITTED FRAME. The frame set of
//! the shipped bundle — the nine archives plus the language-surface, reasoning,
//! opaque-fanout, typed-validation and SHACL-report blobs, plus the snapshot itself
//! — is assembled by the terminal, and several of those blobs exist nowhere else.
//! Recomputing that assembly here to seal envelopes early would be a second source
//! of truth for "what frames does this bundle carry" (Principle 4). So this module
//! OWNS the sealing ([`seal_bundle_envelopes`]) and the terminal CALLS it at the one
//! point the frame set exists; the realizations, which depend on nothing downstream,
//! are this stage's own product.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use purrdf::RdfQuad;
use purrdf::gts_compose::{BlobRow, MediumPlan};
use purrdf::provenance::DatasetProvenance;

use crate::bundle::bundle_from_artifacts_over;
use crate::medium::corpus::{self, CorpusSources};
use crate::medium::envelope::{DigestStratum, FrameFacts, MediumEnvelope, seal};
use crate::medium::rdf::{DictionaryRealization, check_dictionary_retention, realize};
use crate::medium::registry::{DictionaryStrategy, MediumRegistry, MediumSelection};
use crate::medium::{GMEOW, MEDIUM_REGISTRY_GRAPH, SNAPSHOT_WIRE_REP, blake3_digest, train};
use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// The stage id — matches the `gmeow:stage-medium-dictionaries` individual.
pub const STAGE_ID: &str = "stage-medium-dictionaries";

/// The INTERNAL logical-path family the trained dictionaries ride on this stage's
/// byte-artifact lane. Under `pipeline/`, so the reconcile passes them between
/// stages without writing a committed file (see the module docs).
pub const DICT_ARTIFACT_PREFIX: &str = "pipeline/medium/";

/// The upstream stages this stage consumes, sorted. Every one of them is REQUIRED
/// by a declared corpus selector, and the set is derived from the corpora rather
/// than guessed:
///
/// * `stage-archive-blobs` — `gmeow:corpusSelectsBlobRep "cells-archive"` /
///   `"axioms-archive"`;
/// * `stage-reason` — `gmeow:corpusSelectsStageProduct gmeow:stage-reason`
///   (the proof-trace corpus, whose archive is sink-folded and so exists as a
///   product only mid-DAG);
/// * `stage-snapshot` — the named-graph selectors
///   (`graph/statements`, `graph/authoring-briefs`) and the `generated/briefs/`
///   path prefix, all of which first exist on the assembled carrier;
/// * `stage-statements` — the `generated/statements/` path prefix.
///
/// `stage-mappings` was an edge here for exactly one reason — the retired
/// `gmeow-lang-ast-v1` corpus selected its product — and went out with that
/// dictionary rather than being left as an input nothing reads.
const CONSUMES: [&str; 4] = [
    "stage-archive-blobs",
    "stage-reason",
    "stage-snapshot",
    "stage-statements",
];

fn stage_err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: STAGE_ID.to_string(),
        message: message.into(),
    })
}

/// The medium registry read off the assembled carrier — the declarations, never a
/// disk re-parse of `slices/core/gts/module.ttl` (see [`crate::medium::registry`]).
///
/// # Errors
/// A missing `stage-snapshot` product, or any medium-axis declaration defect.
pub fn registry_from_carrier(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<MediumRegistry, gmeow_errors::Diag> {
    let carrier = upstream
        .get("stage-snapshot")
        .ok_or_else(|| stage_err("missing stage-snapshot product for the medium registry"))?;
    MediumRegistry::from_dataset(carrier.dataset())
}

/// Every declared dictionary's training corpus, resolved against THIS run's in-memory
/// products and keyed by `gmeow:dictionaryId`.
///
/// Shared with the off-gate sweep (`make maint-medium-sweep`), which must grid-search
/// over EXACTLY the corpus the build trains from — resolving it twice would let the
/// committed winner be chosen over material the build never sees.
///
/// # Errors
/// A corpus that resolves to nothing, a selector this build cannot evaluate, or a
/// selector that closes the training fixpoint.
pub fn corpus_samples(
    registry: &MediumRegistry,
    sources: &CorpusSources<'_>,
) -> Result<BTreeMap<String, std::collections::BTreeSet<Vec<u8>>>, gmeow_errors::Diag> {
    let mut out = BTreeMap::new();
    for def in registry.dictionaries().values() {
        out.insert(
            def.id.clone(),
            corpus::assemble(registry, &def.corpus, sources)?,
        );
    }
    Ok(out)
}

/// The medium registry, every dictionary's RESOLVED training corpus, and the bundle's
/// own canonical term rendering — everything the off-gate sweep
/// (`make maint-medium-sweep`) needs to grid-search over EXACTLY what the build trains
/// from.
///
/// It re-uses this stage's own source assembly rather than restating it, so the sweep
/// cannot pick a winner over material the build never sees: that divergence would be
/// invisible (both would "work") and would make every committed number wrong.
///
/// # Errors
/// A missing upstream product, an unreadable medium declaration, or a corpus that
/// resolves to nothing.
pub fn resolved_corpora(
    root: &Path,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<ResolvedCorpora, gmeow_errors::Diag> {
    let registry = registry_from_carrier(upstream)?;
    let carrier = upstream
        .get("stage-snapshot")
        .ok_or_else(|| stage_err("missing stage-snapshot product"))?;
    let archives = crate::stages::archive_blobs::archive_blobs_from_product(upstream)?;
    let artifacts = upstream_artifacts(upstream);
    let sources = CorpusSources {
        root,
        dataset: carrier.dataset(),
        archives: &archives,
        artifacts: &artifacts,
        upstream,
    };
    let corpora = corpus_samples(&registry, &sources)?;
    let term_table = crate::medium::corpus::term_table_sample(carrier.dataset());
    Ok(ResolvedCorpora {
        registry,
        corpora,
        term_table,
    })
}

/// What [`resolved_corpora`] hands the off-gate sweep.
pub struct ResolvedCorpora {
    /// The medium axis, read off the assembled carrier.
    pub registry: MediumRegistry,
    /// Every dictionary's resolved training corpus, by `gmeow:dictionaryId`.
    pub corpora: BTreeMap<String, std::collections::BTreeSet<Vec<u8>>>,
    /// The bundle's own canonical term rendering — the `term-table` strategy's corpus.
    pub term_table: Vec<u8>,
}

/// Train one dictionary per declaration, over its declared corpus resolved against
/// THIS run's in-memory products.
///
/// The training point is the AUTHORED `(gmeow:dictionaryStrategy,
/// gmeow:dictionaryTargetLength)`, and the COMMITTED sweep winner is what makes that
/// declaration honest: [`crate::medium::sweep::check_declared_matches_winners`] runs
/// before this and HARD-FAILS when the two differ. Reading the declaration rather than
/// steering from the table is deliberate — the sweep must never silently overwrite
/// what a human wrote down, and the authored value stays the reviewable one — but the
/// gate means the build can only ever train at a MEASURED point. It is also what keeps
/// the graph acyclic: the sweep runs the whole DAG to measure, so a stage that could
/// not run until the table existed could never produce the table.
///
/// The `term-table` strategy trains over the bundle's OWN canonical term rendering
/// rather than over the declared corpus: the two strategies differ in WHAT is fed in,
/// not in how the trainer runs (see [`crate::medium::corpus::term_table_sample`]), and
/// feeding it the declared corpus would make it a duplicate of `raw-content` while
/// still claiming to be a vocabulary dictionary.
///
/// The MEASURED point is returned beside the bytes: it is what the trainer ACTUALLY
/// ran, which the realization records under its own predicates.
///
/// # Errors
/// A corpus that resolves to nothing, a selector this build cannot evaluate, a
/// selector that closes the training fixpoint, or a trainer refusal.
fn train_declared_dictionaries(
    registry: &MediumRegistry,
    sources: &CorpusSources<'_>,
) -> Result<BTreeMap<String, TrainedDictionary>, gmeow_errors::Diag> {
    let corpora = corpus_samples(registry, sources)?;
    let term_table = crate::medium::corpus::term_table_sample(sources.dataset);
    let mut trained: BTreeMap<String, TrainedDictionary> = BTreeMap::new();
    for def in registry.dictionaries().values() {
        let samples = corpora
            .get(&def.id)
            .ok_or_else(|| stage_err(format!("no resolved corpus for dictionary {:?}", def.id)))?;
        let owned: Vec<&[u8]> = match def.strategy {
            DictionaryStrategy::TermTable => vec![term_table.as_slice()],
            _ => samples.iter().map(Vec::as_slice).collect(),
        };
        let bytes = train::build(def.strategy, &owned, def.target_length)?;
        trained.insert(
            def.id.clone(),
            TrainedDictionary {
                bytes,
                measured: crate::medium::rdf::Measured {
                    strategy: def.strategy,
                    target_length: def.target_length,
                    corpus_sample_count: samples.len() as u64,
                },
            },
        );
    }
    Ok(trained)
}

/// One trained dictionary and the measured facts about the run that produced it.
struct TrainedDictionary {
    bytes: Vec<u8>,
    measured: crate::medium::rdf::Measured,
}

/// Every consumed product's byte-artifact lane, unioned by logical path — the
/// in-memory resolution source for a `gmeow:corpusSelectsPathPrefix` naming a
/// `generated/` family. A disk read there would train on the PREVIOUS build's
/// bytes, which is the stale-disk-fold class this crate refuses.
fn upstream_artifacts(upstream: &BTreeMap<String, StageProduct>) -> BTreeMap<String, Vec<u8>> {
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for id in CONSUMES {
        if let Some(product) = upstream.get(id) {
            out.extend(product.artifacts());
        }
    }
    out
}

/// The trained dictionary bytes this stage produced, read back off its product's
/// byte-artifact lane keyed by `gmeow:dictionaryId`.
///
/// # Errors
/// A missing product — the consumer forgot the `gmeow:dataflowConsumes` edge, which
/// is never permission to emit an unprimed bundle.
pub(crate) fn trained_dictionaries(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let product = upstream.get(STAGE_ID).ok_or_else(|| {
        stage_err(format!(
            "missing {STAGE_ID} product — the medium plan has no dictionaries to pin, and an \
             unprimed emission is a silent capability degradation rather than a fallback"
        ))
    })?;
    Ok(product
        .artifacts()
        .into_iter()
        .filter_map(|(path, bytes)| {
            path.strip_prefix(DICT_ARTIFACT_PREFIX)
                .and_then(|name| name.strip_suffix(".zdict"))
                .map(|id| (id.to_string(), bytes))
        })
        .collect())
}

/// The `gmeow:CompressionDictionaryRealization` quads this stage projected, read
/// back verbatim off its product's `graph/medium-registry` named graph.
///
/// Read back rather than recomputed: the realization is the MEASURED record of the
/// bytes this stage trained, so re-deriving it at the terminal would be a second
/// computation of the same measurement and the two could silently disagree.
///
/// # Errors
/// A missing product.
pub(crate) fn realization_quads(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    let product = upstream.get(STAGE_ID).ok_or_else(|| {
        stage_err(format!(
            "missing {STAGE_ID} product for the medium-registry graph"
        ))
    })?;
    // Filter rather than `project_named_graph`: projecting RE-ROOTS the quads into
    // the default graph, so the realizations would fold into the snapshot payload
    // unrooted — present in the bundle but no longer IN graph/medium-registry, which
    // is exactly where a consumer looks for them.
    let quads: Vec<RdfQuad> = purrdf::flat_rdf_quads_from_dataset(product.dataset())
        .into_iter()
        .filter(|quad| quad.graph_name == Some(purrdf::RdfTerm::iri(MEDIUM_REGISTRY_GRAPH)))
        .collect();
    if quads.is_empty() {
        return Err(stage_err(format!(
            "the {STAGE_ID} product carries no <{MEDIUM_REGISTRY_GRAPH}> quads — the shipped \
             bundle would name dictionaries no consumer could resolve back to their authored \
             gmeow:CompressionDictionary"
        )));
    }
    purrdf::dataset_from_quads(&quads)
        .map_err(|err| stage_err(format!("freeze the medium-registry graph: {err}")))
}

/// The IRI naming the frame a `gmeow:MediumEnvelope` describes.
///
/// CONTENT-ADDRESSED on `(rep, content digest)` rather than taken from the GTS
/// frame's own `"id"`. A frame id is a hash over the frame INCLUDING its `prev`
/// link, and the snapshot frame's payload carries the very envelope that would name
/// it — so citing frame ids would make the snapshot envelope's subject depend on
/// itself. `(rep, digest)` is available before emission, stable across it, and
/// identifies exactly the same frame.
#[must_use]
pub fn frame_iri(rep: &str, content_digest: &str) -> String {
    let key = format!("{rep}\u{0}{content_digest}");
    format!(
        "{GMEOW}frame/{}",
        blake3_digest(key.as_bytes())
            .strip_prefix("blake3:")
            .expect("blake3_digest always carries the prefix")
    )
}

/// Seal one `gmeow:MediumEnvelope` per payload-bearing frame of the emission: every
/// blob row, and the snapshot frame itself.
///
/// `snapshot_payload` is the canonical CBOR of the ENVELOPE-FREE snapshot payload,
/// so its digest is `snapshot_content_id()` verbatim; `snapshot_stratum` is the
/// canonical serialization of that same payload's quad set, which is the region
/// `gmeow:stratumPayloadExcludingMediumEnvelope` names. Adding the sealed envelopes
/// to the payload cannot change either value — the first is taken before they
/// exist, the second over a region that excludes them — so the emission reaches its
/// fixed point in exactly two passes rather than iterating.
///
/// # Errors
/// A blob whose rep is unregistered or unassigned, a plan that primes a frame with a
/// dictionary its rep is not assigned, or two frames sharing one `(rep, digest)`
/// identity (which would collapse two envelopes onto one subject).
pub(crate) fn seal_bundle_envelopes(
    registry: &MediumRegistry,
    selection: &MediumSelection,
    plan: &MediumPlan,
    blobs: &[&BlobRow],
    snapshot_payload: &[u8],
    snapshot_stratum: &[u8],
) -> Result<Vec<MediumEnvelope>, gmeow_errors::Diag> {
    use purrdf::gts_compose::{DictSelection as WireDictSelection, FrameSlot};

    let dictionary_of = |slot: &FrameSlot| -> Option<&str> {
        match plan.assignment.get(slot) {
            Some(WireDictSelection::Named(id)) => Some(id.as_str()),
            Some(WireDictSelection::Baseline) | None => None,
        }
    };

    let mut envelopes: Vec<MediumEnvelope> = Vec::with_capacity(blobs.len() + 1);
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for blob in blobs {
        // The frame's OWN in-band digest: `emit_gts` writes exactly this string into
        // `pub.digest`, so the envelope projects it rather than recomputing a second
        // identity for the same bytes.
        let digest = blake3_digest(&blob.data);
        let frame = frame_iri(&blob.rep, &digest);
        if let Some(previous) = seen.insert(frame.clone(), blob.rep.clone()) {
            return Err(stage_err(format!(
                "two payload frames share the identity (rep {:?} / {previous:?}, digest {digest}) \
                 — their envelopes would collapse onto one subject, so the bundle would describe \
                 fewer frames than it carries",
                blob.rep
            )));
        }
        envelopes.push(seal(
            registry,
            selection,
            &FrameFacts {
                frame: &frame,
                rep: &blob.rep,
                payload: &blob.data,
                stratum_bytes: &blob.data,
                stratum: DigestStratum::WholePayload,
                dictionary_id: dictionary_of(&FrameSlot::Blob(blob.rep.clone())),
            },
        )?);
    }

    let snapshot_digest = blake3_digest(snapshot_payload);
    envelopes.push(seal(
        registry,
        selection,
        &FrameFacts {
            frame: &frame_iri(SNAPSHOT_WIRE_REP, &snapshot_digest),
            rep: SNAPSHOT_WIRE_REP,
            payload: snapshot_payload,
            stratum_bytes: snapshot_stratum,
            stratum: DigestStratum::PayloadExcludingMediumEnvelope,
            dictionary_id: dictionary_of(&FrameSlot::Snapshot),
        },
    )?);
    Ok(envelopes)
}

/// Project sealed envelopes into the [`MEDIUM_REGISTRY_GRAPH`] quads the terminal
/// folds beside the realizations.
///
/// # Errors
/// A malformed digest on any envelope (refused at emission rather than shipped).
pub(crate) fn envelope_quads(
    registry: &MediumRegistry,
    envelopes: &[MediumEnvelope],
) -> Result<Vec<RdfQuad>, gmeow_errors::Diag> {
    crate::medium::rdf::project(registry, &[], envelopes)
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `medium-dictionaries` pipeline stage.
pub struct MediumDictionariesStage {
    consumes: Vec<String>,
    attaches_graphs: Vec<String>,
}

impl MediumDictionariesStage {
    /// Construct the stage over the [`CONSUMES`] edge set.
    #[must_use]
    pub fn new() -> Self {
        Self {
            consumes: CONSUMES.iter().map(|s| (*s).to_string()).collect(),
            attaches_graphs: crate::stages::attach::graphs(STAGE_ID).to_vec(),
        }
    }
}

impl Default for MediumDictionariesStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for MediumDictionariesStage {
    fn id(&self) -> &str {
        STAGE_ID
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn attaches_graphs(&self) -> &[String] {
        &self.attaches_graphs
    }
    fn impl_version(&self) -> &str {
        // v3: the FIVE declared dictionaries trained over their declared corpora at
        // the COMMITTED sweep winner (bench/medium-baseline.json), measured into
        // gmeow:CompressionDictionaryRealization records carrying the measured
        // strategy / target length / corpus size, and projected into
        // graph/medium-registry. v2 trained seven, two of which the sweep showed
        // could not pay for their own in-band bytes over the frames they primed.
        "medium-dictionaries.v3"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let registry = registry_from_carrier(input.upstream)?;
        let carrier = input
            .upstream
            .get("stage-snapshot")
            .ok_or_else(|| stage_err("missing stage-snapshot product"))?;
        let archives = crate::stages::archive_blobs::archive_blobs_from_product(input.upstream)?;
        let artifacts = upstream_artifacts(input.upstream);
        let sources = CorpusSources {
            root: input.root,
            dataset: carrier.dataset(),
            archives: &archives,
            artifacts: &artifacts,
            upstream: input.upstream,
        };
        // The COMMITTED sweep winners, read as a GATE rather than as a steering wheel:
        // the bijection against the MEASURABLE registry, then the requirement that the
        // authored training point of every measurable dictionary IS the committed
        // argmin. A build can therefore only ever train at a point somebody measured,
        // while the authored declaration stays the single reviewable source and is
        // never silently overwritten by a sweep.
        let baseline = crate::medium::sweep::load(input.root)?;
        crate::medium::sweep::check_bijection(&registry, &baseline)?;
        crate::medium::sweep::check_declared_matches_winners(&registry, &baseline)?;
        // …and the evidence must say every shipped dictionary earns the in-band bytes
        // a consumer downloads with it. Checked HERE rather than at the emission: the
        // emitter also serializes fixture-scale folds, where nothing of any size pays
        // for itself, whereas the committed evidence is about the real deliverable.
        crate::medium::sweep::check_dictionaries_pay_for_themselves(&baseline)?;
        let trained = train_declared_dictionaries(&registry, &sources)?;

        let mut realizations: Vec<DictionaryRealization> = Vec::with_capacity(trained.len());
        let mut lane: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for (id, dictionary) in &trained {
            let def = registry.dictionary_by_id(id)?;
            realizations.push(realize(def, &dictionary.bytes, dictionary.measured)?);
            lane.insert(
                format!("{DICT_ARTIFACT_PREFIX}{id}.zdict"),
                dictionary.bytes.clone(),
            );
        }
        // Every emitted version is still declared by the definition it realizes:
        // retiring one orphans every artifact already primed with it, so the check
        // runs HERE, where the emission can still be stopped.
        check_dictionary_retention(&registry, &realizations)?;

        let quads = crate::medium::rdf::project(&registry, &realizations, &[])?;
        let graph = purrdf::dataset_from_quads(&quads)
            .map_err(|err| stage_err(format!("freeze the medium-registry projection: {err}")))?;
        let bundle = bundle_from_artifacts_over(graph, lane, DatasetProvenance::new());
        Ok(StageOutput::new(StageProduct::from_bundle(
            self.id(),
            Arc::new(bundle),
        )))
    }

    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // The AUTHORED source trees three corpora select by path prefix
        // (`slices/core/`, `slices/grounding/lang|logic|math/`). They are read from
        // the repo exactly as `stage-archive-blobs` reads the trees it tars, so an
        // edit to any of them must bust this stage's cache key — otherwise a source
        // change would silently ship the previous run's dictionary bytes.
        let mut files = Vec::new();
        for prefix in [
            "slices/core",
            "slices/grounding/lang",
            "slices/grounding/logic",
            "slices/grounding/math",
        ] {
            collect_files(&root.join(prefix), &mut files);
        }
        // The COMMITTED sweep winner table this stage TRAINS AT. Without it in the
        // cache key, refreshing the winners (`make maint-medium-sweep`) would leave the
        // previous build's dictionary bytes in place — the shipped bytes would then
        // disagree with the committed evidence describing them.
        files.push(root.join(crate::medium::sweep::MEDIUM_BASELINE_PATH));
        files.sort();
        files.dedup();
        Ok(files)
    }
}

/// Every regular file under `dir`, recursively, skipping symlinks in both positions
/// (a symlinked directory could cycle; a symlinked file would fold twice).
fn collect_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| !path.is_symlink())
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// A `stage-medium-dictionaries` product over a carrier that carries the medium
/// DECLARATIONS but not the real corpora — the shape every focused sink test
/// has, since none of them assembles the whole DAG.
///
/// It trains each declared dictionary over one small synthetic corpus instead of
/// its declared one. That substitution is scoped to the corpus and NOTHING else:
/// the declarations, the strategies, the target lengths, the realization
/// measurement, the projection, and every downstream plan/envelope path are the
/// production ones, so a test built on this exercises the real wiring and only the
/// dictionary BYTES differ from a full run's. Corpus resolution itself is covered
/// where it belongs — `medium::corpus`'s own tests and the whole-DAG bundle gate.
///
/// # Errors
/// The carrier carries an unreadable medium declaration, or a trainer refusal.
#[cfg(test)]
pub(crate) fn test_product_over(
    carrier: &purrdf::RdfDataset,
) -> Result<StageProduct, gmeow_errors::Diag> {
    let registry = MediumRegistry::from_dataset(carrier)?;
    let owned: Vec<Vec<u8>> = (0..512u32)
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

    let mut realizations: Vec<DictionaryRealization> = Vec::new();
    let mut lane: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for def in registry.dictionaries().values() {
        let bytes = train::build(def.strategy, &corpus, def.target_length)?;
        realizations.push(realize(
            def,
            &bytes,
            crate::medium::rdf::Measured {
                strategy: def.strategy,
                target_length: def.target_length,
                corpus_sample_count: corpus.len() as u64,
            },
        )?);
        lane.insert(format!("{DICT_ARTIFACT_PREFIX}{}.zdict", def.id), bytes);
    }
    let quads = crate::medium::rdf::project(&registry, &realizations, &[])?;
    let graph = purrdf::dataset_from_quads(&quads)
        .map_err(|err| stage_err(format!("freeze the fixture medium registry: {err}")))?;
    Ok(StageProduct::from_bundle(
        STAGE_ID,
        Arc::new(bundle_from_artifacts_over(
            graph,
            lane,
            DatasetProvenance::new(),
        )),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The declared consumes set covers EVERY stage a shipped corpus selector names.
    /// `corpus::assemble` hard-fails on a stage-product selector whose product is not
    /// among this stage's upstream, so a missing edge here is a build failure — but
    /// only once the whole DAG runs. Pin it against the live declaration instead.
    #[test]
    fn the_consumes_set_covers_every_stage_product_a_shipped_corpus_selects() {
        let module = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("the pipeline crate lives under crates/")
            .join("slices/core/gts/module.ttl");
        let text = std::fs::read_to_string(&module).expect("the gts slice is readable");
        let ds = purrdf::parse_dataset(text.as_bytes(), "text/turtle", Some(GMEOW))
            .expect("the gts slice parses as Turtle");
        let registry = MediumRegistry::from_dataset(&ds).expect("the live medium axis reads");

        let mut selected: Vec<String> = Vec::new();
        for corpus in registry.corpora().values() {
            for selector in &corpus.selectors {
                if let crate::medium::corpus::CorpusSelector::StageProduct(iri) = selector {
                    let stage = iri
                        .strip_prefix(GMEOW)
                        .expect("a stage-product selector names a gmeow: individual");
                    selected.push(stage.to_string());
                }
            }
        }
        selected.sort();
        selected.dedup();
        assert!(
            !selected.is_empty(),
            "the shipped corpora must exercise the stage-product selector at all"
        );
        for stage in &selected {
            assert!(
                CONSUMES.contains(&stage.as_str()),
                "corpus selector names `{stage}`, which {STAGE_ID} does not consume — \
                 corpus::assemble would hard-fail on the missing gmeow:dataflowConsumes edge"
            );
        }
    }

    /// The Rust `consumes()` is sorted and matches the declared constant, which is the
    /// half of the three-way declaration (`Stage::consumes`, `run::full_spec`,
    /// `module.ttl`) this crate can check without running the loader.
    #[test]
    fn the_consumes_set_is_sorted_and_bound() {
        let stage = MediumDictionariesStage::new();
        let mut sorted = stage.consumes().to_vec();
        sorted.sort();
        assert_eq!(stage.consumes(), sorted.as_slice());
        assert_eq!(stage.id(), STAGE_ID);
    }

    /// The frame identity is a pure function of `(rep, digest)` and separates frames
    /// that share either coordinate alone — the property that keeps one envelope per
    /// frame rather than per rep.
    #[test]
    fn the_frame_identity_separates_rep_and_digest() {
        let a = frame_iri("cells-archive", "blake3:aa");
        let b = frame_iri("cells-archive", "blake3:bb");
        let c = frame_iri("axioms-archive", "blake3:aa");
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_eq!(a, frame_iri("cells-archive", "blake3:aa"));
        assert!(a.starts_with("https://blackcatinformatics.ca/gmeow/frame/"));
    }
}
