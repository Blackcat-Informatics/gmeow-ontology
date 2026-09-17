// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gmeow:DictionaryCorpus` selector vocabulary and its evaluation.
//!
//! A corpus is a SELECTOR, re-resolved on every build — never a materialized file
//! list — so a new archive member joins its dictionary's training set without an
//! authoring edit. Four selectors exist and no fifth:
//!
//! | selector | resolves to |
//! |---|---|
//! | `gmeow:corpusSelectsBlobRep` | the members of that archive, from `stage-archive-blobs` |
//! | `gmeow:corpusSelectsGraph` | that named graph's canonical N-Triples |
//! | `gmeow:corpusSelectsPathPrefix` | one exact logical file, or the artifacts / authored files under a trailing-slash prefix |
//! | `gmeow:corpusSelectsStageProduct` | that stage product's artifacts |
//!
//! # Why a stage-product selector exists at all
//!
//! This selector is the general route to a producer's byte-artifact lane when no
//! narrower graph, path-prefix, or archive representation identifies the intended
//! corpus. It preserves DAG provenance: the declaration names the producer whose
//! exact product is read, rather than reaching around the graph to a stale committed
//! file. The currently shipped corpora use narrower selectors, but the vocabulary and
//! its fail-closed evaluator remain available for a future surface that truly needs a
//! whole stage product.
//!
//! # An unrecognized selector is a HARD FAIL
//!
//! A `gmeow:corpusSelects*` predicate this module does not know is never skipped.
//! Skipping it would train the dictionary on a STRICT SUBSET of what the corpus
//! declares while every downstream digest and measurement still claimed the full
//! declaration — a silent capability degradation that no error would ever surface.
//!
//! # The held-out split
//!
//! An ARCHIVE-BACKED corpus selects the members of an archive whose TAR is the very
//! frame the dictionary is evaluated over. Trained on every member, such a dictionary
//! would be measured on the bytes it memorized. So every archive member is partitioned
//! by the ONE declared `gmeow:CorpusTrainingSplit`: the trainer sees only the training
//! side, while the evaluation still runs over the whole frame — which therefore
//! contains members the dictionary never saw. The partition is decided by the members'
//! OWN CONTENT — rank them by their `blake3:` digest, hold out every `stride`-th
//! ([`super::registry::TrainingSplitDef`]) — so it is reproducible from the corpus
//! alone and cannot be steered per dictionary.
//!
//! The split is applied HERE, ONCE over the union of every `gmeow:corpusSelectsBlobRep`
//! resolution, so no corpus can opt out of it, no caller can forget it, and a corpus
//! drawing on two archives is split as the one population it is trained and evaluated
//! as. A corpus whose archive population the split does not PARTITION — nothing held
//! out, or nothing trained — is a HARD FAIL: the first would evaluate a dictionary on
//! the bytes it memorized, the second would leave it with no dictionary at all.
//!
//! # The fixpoint exclusion
//!
//! A selector that transitively covers the medium registry's own output closes the
//! loop dictionary → registry → corpus → dictionary. Such a build does not converge:
//! it either oscillates or settles on whichever accidental fixpoint the machine
//! reached first, which is environment-dependent and therefore not reproducible. So
//! coverage is rejected in BOTH the statically decidable cases (a graph name, a path
//! prefix) and the ones only the selected material can answer (an archive whose
//! members, or a stage product whose artifacts/graphs, reach into the excluded
//! region).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use purrdf::gts_compose::BlobRow;
use purrdf::{DatasetView, RdfDataset, TermId, TermRef};

use super::registry::{MediumRegistry, gm, objects};
use super::{
    GMEOW, MEDIUM_GENERATED_PREFIX, MEDIUM_MEASUREMENT_GRAPH, MEDIUM_REGISTRY_GRAPH,
    invalid_declaration, undeclared_dictionary,
};
use crate::node::StageProduct;

/// The four recognized selector predicates, by `gmeow:` local name. Any other
/// `corpusSelects*` predicate on a corpus is a hard fail.
const RECOGNIZED_SELECTORS: [&str; 4] = [
    "corpusSelectsBlobRep",
    "corpusSelectsGraph",
    "corpusSelectsPathPrefix",
    "corpusSelectsStageProduct",
];

/// The named graphs no corpus may reach: the registry's own projection and the
/// medium measurement graph. Both are DOWNSTREAM of the dictionaries, so selecting
/// them closes the training loop.
const EXCLUDED_GRAPHS: [&str; 2] = [MEDIUM_REGISTRY_GRAPH, MEDIUM_MEASUREMENT_GRAPH];

/// One declared corpus selector.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum CorpusSelector {
    /// `gmeow:corpusSelectsBlobRep` — an archive rep; every member is one sample.
    BlobRep(String),
    /// `gmeow:corpusSelectsGraph` — a named graph; its canonical N-Triples is one
    /// sample.
    Graph(String),
    /// `gmeow:corpusSelectsPathPrefix` — one repo-relative file, or a
    /// trailing-slash path family.
    PathPrefix(String),
    /// `gmeow:corpusSelectsStageProduct` — a `gmeow:PipelineStage`; every artifact
    /// on its product's byte lane is one sample.
    StageProduct(String),
}

impl CorpusSelector {
    /// The stage id a stage-product selector names (`gmeow:stage-reason` →
    /// `stage-reason`).
    fn stage_id(iri: &str) -> Option<&str> {
        iri.strip_prefix(GMEOW)
    }
}

impl std::fmt::Display for CorpusSelector {
    /// Render a selector as `<predicate local name> <value>` — the AUTHORED form, so a
    /// consumer reading `gmeow medium explain` sees the declaration rather than a
    /// paraphrase of it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (predicate, value) = match self {
            Self::BlobRep(value) => ("corpusSelectsBlobRep", value),
            Self::Graph(value) => ("corpusSelectsGraph", value),
            Self::PathPrefix(value) => ("corpusSelectsPathPrefix", value),
            Self::StageProduct(value) => ("corpusSelectsStageProduct", value),
        };
        write!(f, "gmeow:{predicate} {value}")
    }
}

/// Everything a corpus can be resolved against — all of it IN MEMORY, save the one
/// documented exception.
pub struct CorpusSources<'a> {
    /// The repository root. Read ONLY for a path prefix naming an AUTHORED source
    /// tree; a `generated/` prefix never touches disk (see [`Self::artifacts`]).
    pub root: &'a Path,
    /// The live carrier dataset — the named-graph selector's only source.
    pub dataset: &'a RdfDataset,
    /// The `stage-archive-blobs` product's rows
    /// ([`crate::stages::archive_blobs::archive_blobs_from_product`]).
    pub archives: &'a [BlobRow],
    /// Additional in-memory artifacts used by focused fixtures. Production corpus
    /// resolution reads the exact borrowed artifact lanes from [`Self::upstream`]
    /// and leaves this map empty, so it never clones every consumed producer merely
    /// to select one path prefix. A `generated/` path prefix never falls back to disk.
    pub artifacts: &'a BTreeMap<String, Vec<u8>>,
    /// Upstream products by stage id, for the stage-product selector.
    pub upstream: &'a BTreeMap<String, StageProduct>,
}

/// Reusable bounded intermediates shared by corpus resolutions in one stage run.
///
/// Two shipped dictionaries select the same authoring-briefs graph. Canonicalizing
/// that graph twice was both duplicated work and duplicated peak memory. The cache
/// holds only the canonical bytes of explicitly selected graphs, keyed by graph IRI;
/// archive members and source files remain per-corpus and are released immediately
/// after that dictionary is trained.
#[derive(Default)]
pub struct CorpusAssemblyCache {
    graph_samples: BTreeMap<String, Arc<[u8]>>,
}

fn selected_archive_bytes<'a>(
    sources: &CorpusSources<'a>,
    rep: &str,
    corpus_iri: &str,
) -> Result<&'a [u8], gmeow_errors::Diag> {
    if let Some(row) = sources.archives.iter().find(|row| row.rep == rep) {
        return Ok(row.data.as_slice());
    }
    crate::stages::archive_blobs::archive_blob_bytes_from_product(sources.upstream, rep).map_err(
        |err| {
            invalid_declaration(format!(
                "<{corpus_iri}> selects blob rep {rep:?}, which the stage-archive-blobs \
                 product does not carry: {err}"
            ))
        },
    )
}

/// Borrow only the artifact entries one selector can observe. Later producer ids
/// overwrite earlier equal paths in the upstream map's canonical order, matching the
/// previous owned-union semantics without copying every unselected payload.
fn selected_artifacts<'a>(
    sources: &CorpusSources<'a>,
    prefix: &str,
) -> Result<BTreeMap<&'a str, &'a [u8]>, gmeow_errors::Diag> {
    let mut selected: BTreeMap<&'a str, &'a [u8]> = BTreeMap::new();
    for (path, bytes) in sources.artifacts.range(prefix.to_string()..) {
        if !logical_path_selected(path, prefix) {
            break;
        }
        selected.insert(path.as_str(), bytes.as_slice());
    }
    for product in sources.upstream.values() {
        for (path, bytes) in product.artifact_refs()? {
            if logical_path_selected(path, prefix) {
                selected.insert(path, bytes);
            }
        }
    }
    Ok(selected)
}

/// A trailing slash declares a family; every other value declares one exact logical
/// file. This keeps a full filename from accidentally admitting a later `.sig`, `.bak`,
/// or similarly prefixed sibling.
fn logical_path_selected(path: &str, selector: &str) -> bool {
    if selector.ends_with('/') {
        path.starts_with(selector)
    } else {
        path == selector
    }
}

/// Read a corpus individual's selectors off the carrier.
///
/// # Errors
/// An unrecognized `gmeow:corpusSelects*` predicate, a selector with a term of the
/// wrong kind, a corpus with no selector at all, or a selector that transitively
/// covers the medium registry's own output.
pub(crate) fn selectors_of(
    ds: &RdfDataset,
    subject: TermId,
    corpus_iri: &str,
) -> Result<Vec<CorpusSelector>, gmeow_errors::Diag> {
    reject_unrecognized_selectors(ds, subject, corpus_iri)?;

    let mut selectors: BTreeSet<CorpusSelector> = BTreeSet::new();
    for object in objects(ds, subject, &gm("corpusSelectsBlobRep")) {
        selectors.insert(CorpusSelector::BlobRep(literal(
            ds,
            object,
            corpus_iri,
            "corpusSelectsBlobRep",
        )?));
    }
    for object in objects(ds, subject, &gm("corpusSelectsGraph")) {
        selectors.insert(CorpusSelector::Graph(iri(
            ds,
            object,
            corpus_iri,
            "corpusSelectsGraph",
        )?));
    }
    for object in objects(ds, subject, &gm("corpusSelectsPathPrefix")) {
        selectors.insert(CorpusSelector::PathPrefix(literal(
            ds,
            object,
            corpus_iri,
            "corpusSelectsPathPrefix",
        )?));
    }
    for object in objects(ds, subject, &gm("corpusSelectsStageProduct")) {
        selectors.insert(CorpusSelector::StageProduct(iri(
            ds,
            object,
            corpus_iri,
            "corpusSelectsStageProduct",
        )?));
    }

    if selectors.is_empty() {
        return Err(invalid_declaration(format!(
            "<{corpus_iri}> declares no selector — a corpus with none leaves its dictionary's \
             training set undefined, so neither a reviewer nor the bundle-internal check has \
             anything to read (logic:DictionaryCorpusSelectorConstraint)"
        )));
    }
    let selectors: Vec<CorpusSelector> = selectors.into_iter().collect();
    for selector in &selectors {
        reject_fixpoint(selector, corpus_iri)?;
    }
    Ok(selectors)
}

/// A `gmeow:corpusSelects*` predicate outside [`RECOGNIZED_SELECTORS`] is a hard
/// fail — never a silent skip.
fn reject_unrecognized_selectors(
    ds: &RdfDataset,
    subject: TermId,
    corpus_iri: &str,
) -> Result<(), gmeow_errors::Diag> {
    let mut unknown: BTreeSet<String> = BTreeSet::new();
    for quad in ds.quads_for_pattern(Some(subject), None, None, purrdf::GraphMatch::Any) {
        let TermRef::Iri(predicate) = ds.resolve(quad.p) else {
            continue;
        };
        let Some(local) = predicate.strip_prefix(GMEOW) else {
            continue;
        };
        if local.starts_with("corpusSelects") && !RECOGNIZED_SELECTORS.contains(&local) {
            unknown.insert(predicate.to_string());
        }
    }
    if unknown.is_empty() {
        return Ok(());
    }
    Err(invalid_declaration(format!(
        "<{corpus_iri}> declares unrecognized corpus selector(s) {unknown:?} — a selector this \
         build cannot evaluate is a HARD FAIL, never a silent skip: skipping it would train the \
         dictionary on a strict SUBSET of what the corpus declares while every downstream digest \
         still claimed the full declaration. Recognized selectors: {RECOGNIZED_SELECTORS:?}"
    )))
}

/// Reject a selector that STATICALLY covers the medium pass's own output.
fn reject_fixpoint(selector: &CorpusSelector, corpus_iri: &str) -> Result<(), gmeow_errors::Diag> {
    let covered = match selector {
        CorpusSelector::Graph(graph) => EXCLUDED_GRAPHS
            .iter()
            .find(|excluded| *excluded == graph)
            .map(|excluded| format!("the named graph <{excluded}>")),
        CorpusSelector::PathPrefix(prefix) => {
            // Family coverage runs BOTH ways: `generated/` contains
            // `generated/medium/`, and `generated/medium/x/` is inside it. An exact
            // file selector is a cycle only when that file is inside the emitted
            // family.
            (if prefix.ends_with('/') {
                MEDIUM_GENERATED_PREFIX.starts_with(prefix.as_str())
                    || prefix.starts_with(MEDIUM_GENERATED_PREFIX)
            } else {
                prefix.starts_with(MEDIUM_GENERATED_PREFIX)
            })
            .then(|| format!("the emitted path family `{MEDIUM_GENERATED_PREFIX}`"))
        }
        // A blob rep and a stage product name a container, not a region: whether
        // they reach the excluded material is a question only their CONTENTS can
        // answer, so it is asked at assembly time (`reject_covering_bytes`).
        CorpusSelector::BlobRep(_) | CorpusSelector::StageProduct(_) => None,
    };
    match covered {
        None => Ok(()),
        Some(what) => Err(invalid_declaration(format!(
            "<{corpus_iri}> selector {selector:?} covers {what}, which the medium pass itself \
             emits — dictionary → registry → corpus → dictionary would close a cycle, and such a \
             build does not converge: it oscillates or settles on an environment-dependent \
             accidental fixpoint. Narrow the selector to material the medium pass does not produce"
        ))),
    }
}

/// Reject material that reaches into the excluded region — the dynamic half of the
/// fixpoint exclusion, for selectors whose coverage only their contents can answer.
fn reject_covering_bytes<'a>(
    names: impl IntoIterator<Item = &'a str>,
    selector: &CorpusSelector,
    corpus_iri: &str,
) -> Result<(), gmeow_errors::Diag> {
    for name in names {
        if name.starts_with(MEDIUM_GENERATED_PREFIX) {
            return Err(invalid_declaration(format!(
                "<{corpus_iri}> selector {selector:?} resolves to `{name}`, inside the \
                 `{MEDIUM_GENERATED_PREFIX}` family the medium pass emits — training a dictionary \
                 on its own output closes a cycle the build cannot converge out of"
            )));
        }
    }
    Ok(())
}

/// One declared corpus, RESOLVED against this build and split.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusResolution {
    /// The samples the trainer sees: the resolved corpus MINUS the archive members
    /// the declared `gmeow:CorpusTrainingSplit` holds out.
    pub training: BTreeSet<Arc<[u8]>>,
    /// How many archive members the declared split held out — the members the
    /// dictionary never saw, which the evaluated tar still contains.
    pub held_out_count: u64,
    /// A `blake3:` digest over the WHOLE resolved corpus, held-out members included:
    /// every sample's own canonical digest, in canonical digest order, hashed again.
    ///
    /// The whole corpus rather than the training side alone, because the sweep's
    /// argmin is a function of both — a held-out member that changes leaves the
    /// training set alone while moving the frame the grid is scored over.
    pub digest: String,
}

/// Resolve a declared corpus to its training samples.
///
/// Samples are collected in `BTreeSet` order, which makes assembly deterministic
/// without making the order load-bearing: [`super::train::build`] is a pure function
/// of the sample MULTISET (upstream canonically sorts before concatenating), so the
/// set's order can never leak into the dictionary bytes.
///
/// Archive members are partitioned by the ONE declared `gmeow:CorpusTrainingSplit`
/// before they reach the trainer (see the module docs): the returned `training` set is
/// a PROPER subset of the archive material, and the frame the dictionary is later
/// evaluated over still carries the members it excludes.
///
/// An EMPTY corpus for a declared dictionary is a HARD FAIL: the dictionary would
/// have no bytes, and a frame primed with the id it was supposed to carry would be
/// permanently undecodable.
///
/// # Errors
/// An empty training result, an archive-backed corpus the declared split holds
/// nothing out of, a missing archive rep / stage product, a carrier with no declared
/// split, or material that reaches into the excluded fixpoint region.
pub fn assemble(
    registry: &MediumRegistry,
    corpus_iri: &str,
    sources: &CorpusSources<'_>,
) -> Result<CorpusResolution, gmeow_errors::Diag> {
    assemble_with_cache(
        registry,
        corpus_iri,
        sources,
        &mut CorpusAssemblyCache::default(),
    )
}

/// Resolve a declared corpus while sharing bounded graph serializations with other
/// dictionary resolutions in the same run.
///
/// # Errors
/// The same conditions as [`assemble`].
pub fn assemble_with_cache(
    registry: &MediumRegistry,
    corpus_iri: &str,
    sources: &CorpusSources<'_>,
    cache: &mut CorpusAssemblyCache,
) -> Result<CorpusResolution, gmeow_errors::Diag> {
    let corpus = registry.corpora().get(corpus_iri).ok_or_else(|| {
        invalid_declaration(format!(
            "<{corpus_iri}> is not a declared gmeow:DictionaryCorpus"
        ))
    })?;

    let mut samples: BTreeSet<Arc<[u8]>> = BTreeSet::new();
    // Every resolved sample's own digest — held-out members included, because the
    // corpus IDENTITY is about what the corpus holds, not about what the trainer saw.
    let mut digests: BTreeSet<String> = BTreeSet::new();
    // Every archive member this corpus selects, keyed by its own content digest. The
    // split is a stride over THIS map's key order, so it is applied once over the
    // corpus's whole archive material rather than once per selector — a corpus that
    // draws on two archives is split as one population, exactly as it is trained and
    // evaluated as one.
    let mut archive_members: BTreeMap<String, Arc<[u8]>> = BTreeMap::new();
    for selector in &corpus.selectors {
        match selector {
            CorpusSelector::BlobRep(rep) => {
                let archive = selected_archive_bytes(sources, rep, corpus_iri)?;
                let members = purrdf::ustar::read_archive(archive).map_err(|err| {
                    invalid_declaration(format!(
                        "<{corpus_iri}> selects blob rep {rep:?}, which does not read as a USTAR \
                         archive: {err}"
                    ))
                })?;
                let members: BTreeMap<String, Vec<u8>> = members.into_iter().collect();
                reject_covering_bytes(members.keys().map(String::as_str), selector, corpus_iri)?;
                for bytes in members.into_values().filter(|b| !b.is_empty()) {
                    // Keyed by digest, so a member two archives both carry is ONE
                    // member of the corpus and is split once.
                    archive_members.insert(super::blake3_digest(&bytes), Arc::from(bytes));
                }
            }
            CorpusSelector::Graph(graph) => {
                let sample = match cache.graph_samples.get(graph) {
                    Some(sample) => Arc::clone(sample),
                    None => {
                        let projected = sources.dataset.project_named_graph(graph);
                        let ntriples =
                            purrdf::canonical_flat_nquads(&projected).map_err(|err| {
                                invalid_declaration(format!(
                                    "<{corpus_iri}> selects graph <{graph}>, which does not \
                                     canonicalize: {err}"
                                ))
                            })?;
                        let sample: Arc<[u8]> = Arc::from(ntriples.into_bytes());
                        cache
                            .graph_samples
                            .insert(graph.clone(), Arc::clone(&sample));
                        sample
                    }
                };
                if !sample.is_empty() {
                    admit(&mut samples, &mut digests, sample);
                }
            }
            CorpusSelector::PathPrefix(prefix) => {
                for bytes in selected_artifacts(sources, prefix)?.into_values() {
                    if !bytes.is_empty() {
                        admit(&mut samples, &mut digests, Arc::from(bytes));
                    }
                }
                // An AUTHORED tree is legitimately on disk (it is what
                // `stage-archive-blobs` tars for the same reason); a `generated/`
                // prefix is NOT, and resolves from the in-memory lane above alone.
                if !prefix.starts_with("generated/") {
                    collect_authored_files(&sources.root.join(prefix), &mut samples, &mut digests);
                }
            }
            CorpusSelector::StageProduct(stage_iri) => {
                let stage = CorpusSelector::stage_id(stage_iri).ok_or_else(|| {
                    invalid_declaration(format!(
                        "<{corpus_iri}> selects stage product <{stage_iri}>, which is not a \
                         gmeow: stage individual"
                    ))
                })?;
                let product = sources.upstream.get(stage).ok_or_else(|| {
                    invalid_declaration(format!(
                        "<{corpus_iri}> selects the product of `{stage}`, which is not among this \
                         stage's upstream products — add the gmeow:dataflowConsumes edge rather \
                         than resolving the corpus from disk"
                    ))
                })?;
                let artifacts = product.artifact_refs()?;
                reject_covering_bytes(artifacts.keys().copied(), selector, corpus_iri)?;
                reject_covering_graphs(product, selector, corpus_iri)?;
                for bytes in artifacts.into_values().filter(|b| !b.is_empty()) {
                    admit(&mut samples, &mut digests, Arc::from(bytes));
                }
            }
        }
    }

    // The split, applied ONCE over the corpus's whole archive population: rank the
    // members by their own content digest (the BTreeMap's key order IS that ranking)
    // and hold out every `stride`-th of them.
    let mut held_out_count: u64 = 0;
    let mut trained_members: u64 = 0;
    let member_count = archive_members.len();
    if member_count > 0 {
        let split = registry.training_split()?;
        for (rank, (digest, bytes)) in archive_members.into_iter().enumerate() {
            digests.insert(digest);
            if split.holds_out_rank(rank) {
                held_out_count += 1;
            } else {
                trained_members += 1;
                samples.insert(bytes);
            }
        }
        if held_out_count == 0 || trained_members == 0 {
            return Err(invalid_declaration(format!(
                "<{corpus_iri}> resolves {member_count} archive member(s), and the declared split \
                 <{}> (stride {}, offset {}) does not partition them ({trained_members} trained, \
                 {held_out_count} held out) — a corpus with nothing held out is evaluated on the \
                 bytes its dictionary memorized, and one with nothing trained has no dictionary at \
                 all. Widen the corpus or redeclare the split; do NOT exempt the corpus",
                split.iri, split.stride, split.offset
            )));
        }
    }

    if samples.is_empty() {
        return Err(undeclared_dictionary(format!(
            "<{corpus_iri}> resolves to ZERO training samples over selectors {:?} — a declared \
             dictionary with an empty corpus has no bytes, so every frame primed with the id it \
             was supposed to carry would be permanently undecodable",
            corpus.selectors
        )));
    }
    Ok(CorpusResolution {
        training: samples,
        held_out_count,
        digest: resolution_digest(&digests),
    })
}

/// Admit one non-archive sample: it joins the training set and the corpus identity.
///
/// Non-archive material is not split. A named graph resolves to ONE canonical
/// serialization and an authored source tree is not what the frame carries, so neither
/// is the train-equals-test case the split exists to break — holding either out would
/// shrink the training set without adding an unseen member to any evaluated frame.
fn admit(samples: &mut BTreeSet<Arc<[u8]>>, digests: &mut BTreeSet<String>, bytes: Arc<[u8]>) {
    digests.insert(super::blake3_digest(&bytes));
    samples.insert(bytes);
}

/// The identity of a RESOLVED corpus: every sample's own canonical `blake3:` digest,
/// one per line in canonical digest order, hashed again.
///
/// Over the digests rather than over the concatenated samples so the identity costs a
/// bounded amount of memory on a corpus whose members run to hundreds of megabytes,
/// and so the value is explainable member by member: a reader who prints one member's
/// digest can see it in the input to this one.
fn resolution_digest(digests: &BTreeSet<String>) -> String {
    let mut joined = String::with_capacity(digests.len() * 72);
    for digest in digests {
        joined.push_str(digest);
        joined.push('\n');
    }
    super::blake3_digest(joined.as_bytes())
}

/// A stage product whose dataset carries quads in an excluded graph covers the
/// medium pass's own output transitively, even though its selector named only a
/// stage.
fn reject_covering_graphs(
    product: &StageProduct,
    selector: &CorpusSelector,
    corpus_iri: &str,
) -> Result<(), gmeow_errors::Diag> {
    let dataset = product.dataset();
    for excluded in EXCLUDED_GRAPHS {
        let Some(graph) = dataset.term_id_by_value(&purrdf::TermValue::iri(excluded)) else {
            continue;
        };
        if dataset
            .quads_for_pattern(None, None, None, purrdf::GraphMatch::Named(graph))
            .next()
            .is_some()
        {
            return Err(invalid_declaration(format!(
                "<{corpus_iri}> selector {selector:?} resolves to a product whose dataset carries \
                 <{excluded}>, which the medium pass itself emits — training a dictionary on its \
                 own registry closes a cycle the build cannot converge out of"
            )));
        }
    }
    Ok(())
}

/// One exact AUTHORED file, or every regular file under an authored directory,
/// recursively. Symlinks are skipped in both positions: a symlinked directory could
/// form a cycle, and a symlinked file would fold the same bytes twice under two names.
fn collect_authored_files(
    path: &Path,
    samples: &mut BTreeSet<Arc<[u8]>>,
    digests: &mut BTreeSet<String>,
) {
    if path.is_symlink() {
        return;
    }
    if path.is_file() {
        if let Ok(bytes) = std::fs::read(path)
            && !bytes.is_empty()
        {
            admit(samples, digests, Arc::from(bytes));
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| !path.is_symlink())
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_authored_files(&path, samples, digests);
        } else if let Ok(bytes) = std::fs::read(&path)
            && !bytes.is_empty()
        {
            admit(samples, digests, Arc::from(bytes));
        }
    }
}

/// A canonical rendering of the bundle's OWN interned term table — the corpus the
/// `gmeow:dictStrategyTermTable` strategy trains over.
///
/// Every distinct interned term is rendered once, sorted, one per line. This makes
/// the `.zdict` a projection of `gmeow.gts` in the STRONG sense: its content is
/// derived from the bundle's vocabulary rather than sampled from its bytes, so the
/// dictionary is explainable term by term and involves no RNG at all.
///
/// It COMPETES with the trained strategy rather than replacing it: a term table is
/// the right guess when a payload is dominated by vocabulary and the wrong one when
/// it is dominated by repeated structure. Which wins is a MEASUREMENT, not an
/// assumption.
#[must_use]
pub fn term_table_sample(dataset: &RdfDataset) -> Vec<u8> {
    let mut rendered: BTreeSet<String> = BTreeSet::new();
    for index in 0..dataset.term_count() {
        let id = purrdf::TermId::from_index(
            u32::try_from(index).expect("a dataset term table is addressed by u32 index"),
        );
        match dataset.resolve(id) {
            TermRef::Iri(value) => {
                rendered.insert(format!("<{value}>"));
            }
            TermRef::Literal {
                lexical, language, ..
            } => {
                // Blank-node labels and literal DATATYPE ids are deliberately
                // excluded: a blank label is a parse-local artifact and a datatype id
                // is already rendered as the IRI it interns to, so including either
                // would make the rendering depend on interning order rather than on
                // the vocabulary.
                match language {
                    Some(tag) => rendered.insert(format!("\"{lexical}\"@{tag}")),
                    None => rendered.insert(format!("\"{lexical}\"")),
                };
            }
            TermRef::Blank { .. } | TermRef::Triple { .. } => {}
        }
    }
    let mut out = String::new();
    for line in rendered {
        out.push_str(&line);
        out.push('\n');
    }
    out.into_bytes()
}

fn literal(
    ds: &RdfDataset,
    object: TermId,
    corpus_iri: &str,
    local: &str,
) -> Result<String, gmeow_errors::Diag> {
    match ds.resolve(object) {
        TermRef::Literal { lexical, .. } => Ok(lexical.to_string()),
        other => Err(invalid_declaration(format!(
            "<{corpus_iri}> gmeow:{local} carries {other:?}, which is not a literal"
        ))),
    }
}

fn iri(
    ds: &RdfDataset,
    object: TermId,
    corpus_iri: &str,
    local: &str,
) -> Result<String, gmeow_errors::Diag> {
    match ds.resolve(object) {
        TermRef::Iri(value) => Ok(value.to_string()),
        other => Err(invalid_declaration(format!(
            "<{corpus_iri}> gmeow:{local} carries {other:?}, which is not an IRI"
        ))),
    }
}

#[path = "corpus.tests.rs"]
#[cfg(test)]
mod tests;
