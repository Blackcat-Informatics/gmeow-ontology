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
//! the shipped bundle — the eleven archives plus the language-surface, reasoning,
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
use crate::medium::envelope::{
    DigestStratum, FrameDigestFacts, FrameFacts, MediumEnvelope, seal, seal_digests,
};
use crate::medium::rdf::{DictionaryRealization, check_dictionary_retention, realize};
use crate::medium::registry::{DictionaryDef, DictionaryStrategy, MediumRegistry, MediumSelection};
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
/// * `stage-reason` — the proof-trace corpus's three exact `generated/logic/`
///   report paths (the sink-folded reasoning archive members, excluding the closure
///   that already rides the snapshot graph);
/// * `stage-snapshot` — the `graph/authoring-briefs` named-graph selector and the
///   `generated/briefs/` path prefix, both of which first exist on the assembled
///   carrier;
/// * `stage-statements` — the `generated/statements/` path prefix.
///
/// `stage-mappings` is NOT an edge, and its absence is derived rather than chosen:
/// `gmeow-lang-ast-v1` selected that product back when its deliverables had no rep of
/// their own. They have one now (`lang-projections-archive`), so the corpus selects the
/// ARCHIVE, which arrives on `stage-archive-blobs` — one authority on what is in that
/// archive (Principle 4), and one fewer edge. The retired `gmeow-claims-v1` selected
/// `statements-archive` + `yaml-ld-archive` the same way rather than a
/// `generated/statements/` path prefix; those reps now ride the dictionary-less medium.
/// (`stage-statements` stays an edge regardless — `gmeow-memory-hot-v1` selects that
/// prefix.)
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
/// committed winner be chosen over material the build never sees. The same sharing is
/// what makes the corpus-identity gate meaningful: the digest the sweep committed and
/// the digest this build checks come out of ONE resolution path.
///
/// # Errors
/// A corpus that resolves to nothing, an archive-backed corpus the declared split
/// holds nothing out of, a selector this build cannot evaluate, or a selector that
/// closes the training fixpoint.
pub fn corpus_samples(
    registry: &MediumRegistry,
    sources: &CorpusSources<'_>,
) -> Result<BTreeMap<String, corpus::CorpusResolution>, gmeow_errors::Diag> {
    let mut out = BTreeMap::new();
    let mut cache = corpus::CorpusAssemblyCache::default();
    for def in registry.dictionaries().values() {
        out.insert(
            def.id.clone(),
            corpus::assemble_with_cache(registry, &def.corpus, sources, &mut cache)?,
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
    let archives = Vec::new();
    let artifacts = BTreeMap::new();
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
    /// Every dictionary's resolved corpus, by `gmeow:dictionaryId`: the training side
    /// of the declared held-out split, the number of members it held out, and the
    /// identity of the WHOLE resolution.
    pub corpora: BTreeMap<String, corpus::CorpusResolution>,
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
/// The corpus a dictionary trains over is the TRAINING SIDE of the declared held-out
/// split ([`crate::medium::corpus`]): for an archive-backed corpus the trainer never
/// sees the members the split holds out, while the frame the dictionary is evaluated
/// over — the tar of ALL the members — still carries them.
///
/// # Errors
/// A trainer refusal, or a dictionary with no entry in `corpora`.
fn train_declared_dictionary(
    def: &DictionaryDef,
    dataset: &purrdf::RdfDataset,
    resolved: &corpus::CorpusResolution,
    term_table: &mut Option<Vec<u8>>,
) -> Result<TrainedDictionary, gmeow_errors::Diag> {
    let term_table_sample = if def.strategy == DictionaryStrategy::TermTable {
        Some(
            term_table
                .get_or_insert_with(|| corpus::term_table_sample(dataset))
                .as_slice(),
        )
    } else {
        None
    };
    let samples: Vec<&[u8]> = match term_table_sample {
        Some(sample) => vec![sample],
        None => resolved.training.iter().map(AsRef::as_ref).collect(),
    };
    let bytes = train::build(def.strategy, &samples, def.target_length)?;
    Ok(TrainedDictionary {
        bytes,
        measured: crate::medium::rdf::Measured {
            strategy: def.strategy,
            target_length: def.target_length,
            corpus_sample_count: resolved.training.len() as u64,
            corpus_digest: resolved.digest.clone(),
        },
    })
}

/// One trained dictionary and the measured facts about the run that produced it.
struct TrainedDictionary {
    bytes: Vec<u8>,
    measured: crate::medium::rdf::Measured,
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
/// `snapshot_content_digest` is the content id of the ENVELOPE-FREE snapshot payload;
/// `snapshot_strata_digest` is the identity of that same payload's canonical quad
/// set, which is the region `gmeow:stratumPayloadExcludingMediumEnvelope` names.
/// The terminal computes and releases each large preimage separately before calling
/// here. Adding the sealed envelopes cannot change either value, so emission reaches
/// its fixed point in exactly two passes rather than iterating.
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
    snapshot_content_digest: &str,
    snapshot_strata_digest: &str,
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

    envelopes.push(seal_digests(
        registry,
        selection,
        &FrameDigestFacts {
            frame: &frame_iri(SNAPSHOT_WIRE_REP, snapshot_content_digest),
            rep: SNAPSHOT_WIRE_REP,
            content_digest: snapshot_content_digest,
            strata_digest: snapshot_strata_digest,
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
    carrier_consumes: Vec<String>,
    attaches_graphs: Vec<String>,
    resources: Vec<String>,
}

impl MediumDictionariesStage {
    /// Construct the stage over the [`CONSUMES`] edge set.
    #[must_use]
    pub fn new() -> Self {
        Self {
            consumes: CONSUMES.iter().map(|s| (*s).to_string()).collect(),
            // Archive bytes live on the archive producer's representation-blob lane,
            // and declarations/named-graph samples live on the snapshot dataset. The
            // reason and statements inputs contribute committed generated/* artifacts
            // only, so their multi-million-quad datasets may be released earlier.
            carrier_consumes: vec![
                "stage-archive-blobs".to_string(),
                "stage-snapshot".to_string(),
            ],
            attaches_graphs: crate::stages::attach::graphs(STAGE_ID).to_vec(),
            // Named-graph corpus resolution canonicalizes a carrier-scale selection
            // and dictionary training materializes compressed candidates. That live
            // set is independently bounded, but overlaps additively with either
            // whole-dataset export serializer on a cold run. Model the measured peak
            // conflict through the same declarative resource, never a fixed thread cap.
            resources: vec![crate::node::SERIALIZATION_BUFFER_RESOURCE.to_string()],
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
    fn carrier_consumes(&self) -> &[String] {
        &self.carrier_consumes
    }
    fn attaches_graphs(&self) -> &[String] {
        &self.attaches_graphs
    }
    fn resources(&self) -> &[String] {
        &self.resources
    }
    fn impl_version(&self) -> &str {
        // v4: as v3 (the declared dictionaries trained at the COMMITTED sweep winner
        // from bench/medium-baseline.json and measured into
        // gmeow:CompressionDictionaryRealization records), plus the declared held-out
        // split — an archive-backed corpus trains on the TRAINING SIDE only — and the
        // resolved-corpus identity, recorded on the realization and gated against the
        // committed table. Both change the shipped dictionary BYTES, so the key moves.
        // The inventory itself is DATA — it is read off the carrier's
        // gmeow:CompressionDictionary individuals — so growing or shrinking it does not
        // move this key; only the training/measurement code does.
        // v5: resolve, evidence-check, and train one dictionary at a time; share only
        // bounded selected-graph bytes; borrow archive/artifact lanes before selection;
        // and declare the exact two carrier inputs independently from the two
        // committed-artifact inputs. Dictionary bytes remain a pure function of the
        // same corpus multiset and authored training point.
        // v6: declare the carrier-materialization resource shared with the two
        // whole-dataset exporters. A measured cold run showed their otherwise valid
        // same-wave overlap crossing 16 GiB; serialization changes, not output bytes.
        "medium-dictionaries.v6-serialized-carrier-materialization"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let registry = registry_from_carrier(input.upstream)?;
        let carrier = input
            .upstream
            .get("stage-snapshot")
            .ok_or_else(|| stage_err("missing stage-snapshot product"))?;
        // Production resolves archive and artifact selectors by borrowing the exact
        // upstream lanes. These owned maps are fixture overlays and intentionally empty
        // here: cloning all eleven archives and every generated artifact before a
        // selector ran was the required-path memory spike this stage now avoids.
        let archives = Vec::new();
        let artifacts = BTreeMap::new();
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
        // …and, last, the three checks above must be grading THIS build's corpus. A
        // gmeow:DictionaryCorpus is a SELECTOR re-resolved every build, so an archive
        // that gained or lost a member moves the corpus without moving the table, and
        // every verdict above would then be about a sweep nobody re-ran. Resolve the
        // corpora once, HERE, and refuse when the recorded identity is not the
        // resolved one — the same resolution the trainer then consumes, so the digest
        // that was gated and the bytes that ship cannot come from two different reads.
        let mut corpus_cache = corpus::CorpusAssemblyCache::default();
        let mut term_table: Option<Vec<u8>> = None;
        let mut trained: BTreeMap<String, TrainedDictionary> = BTreeMap::new();
        for def in registry.dictionaries().values() {
            let resolved =
                corpus::assemble_with_cache(&registry, &def.corpus, &sources, &mut corpus_cache)?;
            // Grade this exact resolution before its bytes reach the trainer. On a
            // clean build the resolution is consumed immediately after training; on
            // drift it is dropped before any dictionary based on stale evidence can
            // be produced.
            crate::medium::sweep::check_corpus_digest(&baseline, &def.id, &resolved)?;
            trained.insert(
                def.id.clone(),
                train_declared_dictionary(def, carrier.dataset(), &resolved, &mut term_table)?,
            );
        }

        let mut realizations: Vec<DictionaryRealization> = Vec::with_capacity(trained.len());
        let mut lane: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for (id, dictionary) in &trained {
            let def = registry.dictionary_by_id(id)?;
            realizations.push(realize(
                def,
                &dictionary.bytes,
                dictionary.measured.clone(),
            )?);
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
                corpus_digest: crate::medium::blake3_digest(&owned.concat()),
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

    /// The declared consumes set covers every stage-product selector, and the
    /// sink-folded proof-trace corpus names exactly the three report artifacts owned
    /// by stage-reason rather than selecting its closure-bearing whole product.
    #[test]
    fn the_consumes_set_covers_shipped_corpus_producers_exactly() {
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
        for stage in &selected {
            assert!(
                CONSUMES.contains(&stage.as_str()),
                "corpus selector names `{stage}`, which {STAGE_ID} does not consume — \
                 corpus::assemble would hard-fail on the missing gmeow:dataflowConsumes edge"
            );
        }

        let prooftrace = registry
            .corpora()
            .get(&format!("{GMEOW}corpusGmeowProoftraceV1"))
            .expect("the proof-trace corpus is declared");
        let paths: Vec<&str> = prooftrace
            .selectors
            .iter()
            .filter_map(|selector| match selector {
                crate::medium::corpus::CorpusSelector::PathPrefix(path) => Some(path.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            paths,
            [
                crate::stages::reason::LEDGER_PATH,
                crate::stages::reason::PERF_LEDGER_PATH,
                crate::stages::reason::EXPLANATIONS_PATH,
            ]
        );
        assert!(!paths.contains(&crate::stages::reason::CLOSURE_PATH));
        assert!(CONSUMES.contains(&"stage-reason"));
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
        assert_eq!(
            stage.carrier_consumes(),
            ["stage-archive-blobs", "stage-snapshot"]
        );
        assert_eq!(
            stage.resources(),
            [crate::node::SERIALIZATION_BUFFER_RESOURCE]
        );
        assert!(stage.consumes().iter().any(|id| id == "stage-reason"));
        assert!(stage.consumes().iter().any(|id| id == "stage-statements"));
        assert!(
            !stage
                .carrier_consumes()
                .iter()
                .any(|id| id == "stage-reason")
        );
        assert!(
            !stage
                .carrier_consumes()
                .iter()
                .any(|id| id == "stage-statements")
        );
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
