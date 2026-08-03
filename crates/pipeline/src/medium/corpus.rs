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
//! | `gmeow:corpusSelectsPathPrefix` | the artifacts / authored files under that prefix |
//! | `gmeow:corpusSelectsStageProduct` | that stage product's artifacts |
//!
//! # Why a stage-product selector exists at all
//!
//! Two shipped corpora need material that is SINK-FOLDED — `reasoning-archive` and
//! `lang-surface-blob` are assembled only at the terminal snapshot, so they are not
//! archive reps a mid-DAG trainer can read. Re-specifying those corpora over the
//! archives' constituent inputs would duplicate archive-membership logic and create
//! a second source of truth for "what is in this archive" (Principle 4). Selecting
//! the PRODUCING STAGE's product instead reaches the same lifted surface at the
//! point in the DAG where the trainer actually runs: `gmeow-prooftrace-v1` reaches
//! `stage-reason`.
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CorpusSelector {
    /// `gmeow:corpusSelectsBlobRep` — an archive rep; every member is one sample.
    BlobRep(String),
    /// `gmeow:corpusSelectsGraph` — a named graph; its canonical N-Triples is one
    /// sample.
    Graph(String),
    /// `gmeow:corpusSelectsPathPrefix` — a repo-relative path family.
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
    /// THIS run's generated artifacts, by repo-relative logical path, off the
    /// producing stages' in-memory byte lanes. A `generated/` path prefix resolves
    /// HERE and never from disk: the committed tree is not flushed until the
    /// post-run reconcile returns, so a disk read would train on the PREVIOUS
    /// build's bytes — the stale-disk-fold class this crate refuses.
    pub artifacts: &'a BTreeMap<String, Vec<u8>>,
    /// Upstream products by stage id, for the stage-product selector.
    pub upstream: &'a BTreeMap<String, StageProduct>,
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
            // Coverage runs BOTH ways: `generated/` contains `generated/medium/`,
            // and `generated/medium/x/` is inside it. Either is a cycle.
            (MEDIUM_GENERATED_PREFIX.starts_with(prefix.as_str())
                || prefix.starts_with(MEDIUM_GENERATED_PREFIX))
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
    names: impl IntoIterator<Item = &'a String>,
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
    pub training: BTreeSet<Vec<u8>>,
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
    let corpus = registry.corpora().get(corpus_iri).ok_or_else(|| {
        invalid_declaration(format!(
            "<{corpus_iri}> is not a declared gmeow:DictionaryCorpus"
        ))
    })?;

    let mut samples: BTreeSet<Vec<u8>> = BTreeSet::new();
    // Every resolved sample's own digest — held-out members included, because the
    // corpus IDENTITY is about what the corpus holds, not about what the trainer saw.
    let mut digests: BTreeSet<String> = BTreeSet::new();
    // Every archive member this corpus selects, keyed by its own content digest. The
    // split is a stride over THIS map's key order, so it is applied once over the
    // corpus's whole archive material rather than once per selector — a corpus that
    // draws on two archives is split as one population, exactly as it is trained and
    // evaluated as one.
    let mut archive_members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for selector in &corpus.selectors {
        match selector {
            CorpusSelector::BlobRep(rep) => {
                let row = sources
                    .archives
                    .iter()
                    .find(|row| row.rep == *rep)
                    .ok_or_else(|| {
                        invalid_declaration(format!(
                            "<{corpus_iri}> selects blob rep {rep:?}, which the \
                             stage-archive-blobs product does not carry (available: {:?})",
                            sources.archives.iter().map(|r| &r.rep).collect::<Vec<_>>()
                        ))
                    })?;
                let members = purrdf::ustar::read_archive(&row.data).map_err(|err| {
                    invalid_declaration(format!(
                        "<{corpus_iri}> selects blob rep {rep:?}, which does not read as a USTAR \
                         archive: {err}"
                    ))
                })?;
                let members: BTreeMap<String, Vec<u8>> = members.into_iter().collect();
                reject_covering_bytes(members.keys(), selector, corpus_iri)?;
                for bytes in members.into_values().filter(|b| !b.is_empty()) {
                    // Keyed by digest, so a member two archives both carry is ONE
                    // member of the corpus and is split once.
                    archive_members.insert(super::blake3_digest(&bytes), bytes);
                }
            }
            CorpusSelector::Graph(graph) => {
                let projected = sources.dataset.project_named_graph(graph);
                let ntriples = purrdf::canonical_flat_nquads(&projected).map_err(|err| {
                    invalid_declaration(format!(
                        "<{corpus_iri}> selects graph <{graph}>, which does not canonicalize: \
                         {err}"
                    ))
                })?;
                if !ntriples.is_empty() {
                    admit(&mut samples, &mut digests, ntriples.into_bytes());
                }
            }
            CorpusSelector::PathPrefix(prefix) => {
                for (path, bytes) in sources.artifacts.range(prefix.clone()..) {
                    if !path.starts_with(prefix.as_str()) {
                        break;
                    }
                    if !bytes.is_empty() {
                        admit(&mut samples, &mut digests, bytes.clone());
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
                let artifacts = product.artifacts();
                reject_covering_bytes(artifacts.keys(), selector, corpus_iri)?;
                reject_covering_graphs(product, selector, corpus_iri)?;
                for bytes in artifacts.into_values().filter(|b| !b.is_empty()) {
                    admit(&mut samples, &mut digests, bytes);
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
fn admit(samples: &mut BTreeSet<Vec<u8>>, digests: &mut BTreeSet<String>, bytes: Vec<u8>) {
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

/// Every regular file under an AUTHORED source tree, recursively. Symlinks are
/// skipped in both positions: a symlinked directory could form a cycle, and a
/// symlinked file would fold the same bytes twice under two names.
fn collect_authored_files(
    dir: &Path,
    samples: &mut BTreeSet<Vec<u8>>,
    digests: &mut BTreeSet<String>,
) {
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
            collect_authored_files(&path, samples, digests);
        } else if let Ok(bytes) = std::fs::read(&path)
            && !bytes.is_empty()
        {
            admit(samples, digests, bytes);
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::medium::registry::fixture;

    fn registry_of(extra: &str) -> Result<MediumRegistry, gmeow_errors::Diag> {
        MediumRegistry::from_dataset(&fixture::dataset(extra))
    }

    /// The fixture archive's members: enough of them that the DECLARED split
    /// (modulus 8) partitions them both ways, which
    /// `the_declared_split_partitions_the_fixture_archive_both_ways` pins so a
    /// degenerate fixture can never make the split tests vacuous.
    fn archive_members() -> Vec<(String, Vec<u8>)> {
        (0..64u32)
            .map(|i| {
                (
                    format!("slices/core/gts/cell-{i:02}.ttl"),
                    format!("<https://e/s> <https://e/p> <https://e/o{i}> .\n").into_bytes(),
                )
            })
            .collect()
    }

    struct Harness {
        dataset: Arc<RdfDataset>,
        archives: Vec<BlobRow>,
        artifacts: BTreeMap<String, Vec<u8>>,
        upstream: BTreeMap<String, StageProduct>,
        root: tempfile::TempDir,
    }

    impl Harness {
        fn new() -> Self {
            let root = tempfile::tempdir().expect("temp root");
            std::fs::create_dir_all(root.path().join("slices/core/gts")).expect("authored tree");
            std::fs::write(
                root.path().join("slices/core/gts/module.ttl"),
                b"# an authored source file the path-prefix selector reads\n",
            )
            .expect("authored file");

            let archive =
                purrdf::ustar::write_archive(&archive_members()).expect("fixture archive");

            Self {
                dataset: fixture::dataset(""),
                archives: vec![BlobRow {
                    data: archive,
                    media_type: "application/x-tar".to_string(),
                    rep: "cells-archive".to_string(),
                }],
                artifacts: [(
                    "generated/statements/claims.ttl".to_string(),
                    b"<https://e/claim> <https://e/p> \"v\" .\n".to_vec(),
                )]
                .into(),
                upstream: BTreeMap::new(),
                root,
            }
        }

        fn sources(&self) -> CorpusSources<'_> {
            CorpusSources {
                root: self.root.path(),
                dataset: &self.dataset,
                archives: &self.archives,
                artifacts: &self.artifacts,
                upstream: &self.upstream,
            }
        }
    }

    #[test]
    fn a_blob_rep_and_path_prefix_corpus_resolves_to_real_bytes() {
        let harness = Harness::new();
        let registry = registry_of("").expect("registry");
        let resolved = assemble(&registry, &gm("corpusCore"), &harness.sources())
            .expect("the core corpus resolves");
        // The archive MEMBER (not the tar) and the authored source file.
        assert!(
            resolved
                .training
                .iter()
                .any(|s| s.starts_with(b"<https://e/s>")),
            "the archive's members are the samples, not the tar itself"
        );
        assert!(
            resolved
                .training
                .iter()
                .any(|s| s.starts_with(b"# an authored source file")),
            "an AUTHORED path prefix reads the repo tree"
        );
    }

    /// The members the declared split holds out of `archive_members()`, computed the
    /// way a READER would: rank by content digest, take every stride-th.
    fn expected_held_out(split: &crate::medium::registry::TrainingSplitDef) -> Vec<Vec<u8>> {
        let ranked: BTreeMap<String, Vec<u8>> = archive_members()
            .into_iter()
            .map(|(_, bytes)| (crate::medium::blake3_digest(&bytes), bytes))
            .collect();
        ranked
            .into_values()
            .enumerate()
            .filter(|(rank, _)| split.holds_out_rank(*rank))
            .map(|(_, bytes)| bytes)
            .collect()
    }

    /// The split PARTITIONS the fixture archive — both sides non-empty — and does so
    /// by a stride over content-digest rank, which is what makes properness a
    /// theorem rather than a coin flip on a small corpus.
    #[test]
    fn the_declared_split_partitions_the_fixture_archive_both_ways() {
        let registry = registry_of("").expect("registry");
        let split = registry.training_split().expect("the fixture declares one");
        let total = archive_members().len();
        let held = expected_held_out(split).len();
        assert_eq!(
            held,
            total.div_ceil(split.stride as usize),
            "one member in every {} is held out, in content-digest rank order",
            split.stride
        );
        assert!(held > 0 && held < total, "{held} of {total} held out");
    }

    /// The trainer NEVER sees the held-out members, and the resolution says how many
    /// it held out.
    #[test]
    fn the_declared_split_keeps_held_out_members_out_of_the_training_set() {
        let harness = Harness::new();
        let registry = registry_of("").expect("registry");
        let split = registry.training_split().expect("the fixture declares one");
        let resolved = assemble(&registry, &gm("corpusCore"), &harness.sources())
            .expect("the core corpus resolves");

        let expected_held = expected_held_out(split);
        assert_eq!(resolved.held_out_count, expected_held.len() as u64);
        for held in &expected_held {
            assert!(
                !resolved.training.contains(held),
                "a held-out member reached the trainer — the evaluation would be over bytes the \
                 dictionary memorized"
            );
        }
        // …and it is a PROPER subset: the training side is the larger one.
        assert!(resolved.training.len() > resolved.held_out_count as usize);
    }

    /// An archive-backed corpus the declared split does not PARTITION is a hard fail
    /// — and there is no exemption. A one-member archive is that case: whichever side
    /// it lands on, the other side is empty.
    #[test]
    fn an_archive_the_split_cannot_partition_hard_fails() {
        let mut harness = Harness::new();
        let registry = registry_of("").expect("registry");
        harness.archives = vec![BlobRow {
            data: purrdf::ustar::write_archive(&archive_members()[..1]).expect("fixture archive"),
            media_type: "application/x-tar".to_string(),
            rep: "cells-archive".to_string(),
        }];
        let diag = assemble(&registry, &gm("corpusCore"), &harness.sources())
            .expect_err("an unsplittable archive-backed corpus must hard-fail");
        assert!(
            diag.to_string().contains("does not partition them")
                && diag.to_string().contains("do NOT exempt"),
            "{diag}"
        );
    }

    /// A carrier with no declared `gmeow:CorpusTrainingSplit` refuses an
    /// archive-backed corpus rather than quietly training on every member.
    #[test]
    fn an_archive_backed_corpus_without_a_declared_split_hard_fails() {
        let harness = Harness::new();
        let text = fixture::turtle("").replace("a gmeow:CorpusTrainingSplit", "a gmeow:Retired");
        let ds = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).expect("turtle");
        let registry = MediumRegistry::from_dataset(&ds).expect("registry");
        let diag = assemble(&registry, &gm("corpusCore"), &harness.sources())
            .expect_err("a corpus with no declared split must hard-fail");
        assert!(
            diag.to_string().contains("gmeow:CorpusTrainingSplit"),
            "{diag}"
        );
    }

    /// Two declared splits leave "which members did this dictionary never see" with
    /// two answers, so the registry refuses to read at all.
    #[test]
    fn two_declared_splits_are_rejected() {
        let diag = registry_of(
            "gmeow:corpusTrainingSplitV2 a gmeow:CorpusTrainingSplit ;\n\
             \x20   gmeow:splitHeldOutStride 4 ;\n\
             \x20   gmeow:splitHeldOutOffset 1 .",
        )
        .expect_err("two splits must be rejected");
        assert!(diag.to_string().contains("two answers"), "{diag}");
    }

    /// (d) A declared dictionary whose selector matches nothing hard-fails with
    /// `pipeline.medium.undeclared-dictionary` — never a silently empty dictionary.
    #[test]
    fn a_corpus_matching_nothing_hard_fails_as_undeclared() {
        let mut harness = Harness::new();
        harness.artifacts = BTreeMap::new();
        harness.archives = Vec::new();
        let registry = registry_of(
            "gmeow:corpusEmpty a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsPathPrefix \"generated/nothing-here/\" .",
        )
        .expect("registry");
        let diag = assemble(&registry, &gm("corpusEmpty"), &harness.sources())
            .expect_err("an empty corpus must hard-fail");
        assert_eq!(
            diag.code(),
            crate::error::MediumUndeclaredDictionary::register(),
            "{diag}"
        );
        assert_eq!(
            crate::error::MediumUndeclaredDictionary::CODE,
            "pipeline.medium.undeclared-dictionary"
        );
    }

    /// (e) An unrecognized selector predicate hard-fails. Skipping it would train on
    /// a strict subset of the declaration with nothing to surface the gap.
    #[test]
    fn an_unrecognized_selector_predicate_hard_fails() {
        let diag = registry_of(
            "gmeow:corpusWeird a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsBlobRep \"cells-archive\" ;\n\
             \x20   gmeow:corpusSelectsMoonPhase \"waxing\" .",
        )
        .expect_err("an unknown selector must be rejected");
        assert!(
            diag.to_string().contains("corpusSelectsMoonPhase")
                && diag.to_string().contains("never a silent skip"),
            "{diag}"
        );
    }

    /// (f) A selector covering `graph/medium-registry` is rejected — the cycle
    /// dictionary → registry → corpus → dictionary never converges.
    #[test]
    fn a_selector_covering_the_medium_registry_graph_is_rejected() {
        let diag = registry_of(&format!(
            "gmeow:corpusLoop a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsGraph <{MEDIUM_REGISTRY_GRAPH}> ."
        ))
        .expect_err("a fixpoint selector must be rejected");
        assert!(diag.to_string().contains("close a cycle"), "{diag}");

        // The measurement graph is excluded for the same reason.
        let diag = registry_of(&format!(
            "gmeow:corpusLoop a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsGraph <{MEDIUM_MEASUREMENT_GRAPH}> ."
        ))
        .expect_err("the measurement graph is excluded too");
        assert!(diag.to_string().contains("close a cycle"), "{diag}");
    }

    /// Path-prefix coverage runs BOTH ways: a prefix INSIDE `generated/medium/` and
    /// a prefix that CONTAINS it are equally a cycle.
    #[test]
    fn a_path_prefix_covering_the_emitted_family_is_rejected_in_both_directions() {
        for prefix in ["generated/", "generated/medium/", "generated/medium/v1/"] {
            let diag = registry_of(&format!(
                "gmeow:corpusLoop a gmeow:DictionaryCorpus ;\n\
                 \x20   gmeow:corpusSelectsPathPrefix \"{prefix}\" ."
            ))
            .unwrap_err();
            assert!(
                diag.to_string().contains("close a cycle"),
                "prefix {prefix:?} must be rejected: {diag}"
            );
        }
        // A sibling family under `generated/` is NOT covered — the exclusion is
        // narrow, not a blanket ban on generated material.
        registry_of(
            "gmeow:corpusFine a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsPathPrefix \"generated/statements/\" .",
        )
        .expect("a sibling generated/ family is legitimate corpus material");
    }

    /// A blob rep names a CONTAINER, so its coverage is decided by its contents.
    #[test]
    fn an_archive_reaching_into_the_emitted_family_is_rejected() {
        let mut harness = Harness::new();
        harness.archives = vec![BlobRow {
            data: purrdf::ustar::write_archive(&[(
                format!("{MEDIUM_GENERATED_PREFIX}gmeow-core-v1.zdict"),
                vec![1, 2, 3],
            )])
            .expect("fixture archive"),
            media_type: "application/x-tar".to_string(),
            rep: "cells-archive".to_string(),
        }];
        let registry = registry_of("").expect("registry");
        let diag = assemble(&registry, &gm("corpusCore"), &harness.sources())
            .expect_err("an archive carrying the medium output must be rejected");
        assert!(diag.to_string().contains("closes a cycle"), "{diag}");
    }

    /// A stage-product selector reaches a product's artifacts — the mechanism the
    /// two SINK-FOLDED corpora depend on.
    #[test]
    fn a_stage_product_selector_reads_the_named_product() {
        let mut harness = Harness::new();
        harness.upstream.insert(
            "stage-reason".to_string(),
            StageProduct::from_artifacts(
                "stage-reason",
                [(
                    "generated/reasoning/explanations.ttl".to_string(),
                    b"<https://e/why> <https://e/because> \"rule\" .\n".to_vec(),
                )]
                .into(),
            ),
        );
        let registry = registry_of(
            "gmeow:corpusReason a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsStageProduct gmeow:stage-reason .",
        )
        .expect("registry");
        let resolved = assemble(&registry, &gm("corpusReason"), &harness.sources())
            .expect("the stage-product corpus resolves");
        assert!(
            resolved
                .training
                .iter()
                .any(|s| s.starts_with(b"<https://e/why>"))
        );
        assert_eq!(
            resolved.held_out_count, 0,
            "a stage-product corpus carries no archive members to split"
        );
    }

    /// A stage-product selector whose product carries the medium registry graph is
    /// a transitive cycle even though the selector named only a stage.
    #[test]
    fn a_stage_product_carrying_the_registry_graph_is_rejected() {
        let mut harness = Harness::new();
        let trig =
            format!("<{MEDIUM_REGISTRY_GRAPH}> {{ <https://e/real> <https://e/p> \"v\" . }}\n");
        let dataset =
            purrdf::parse_dataset(trig.as_bytes(), "application/trig", None).expect("trig");
        harness.upstream.insert(
            "stage-reason".to_string(),
            StageProduct::from_artifacts_over(
                "stage-reason",
                dataset,
                [("generated/reasoning/x.ttl".to_string(), b"# x\n".to_vec())].into(),
            ),
        );
        let registry = registry_of(
            "gmeow:corpusReason a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsStageProduct gmeow:stage-reason .",
        )
        .expect("registry");
        let diag = assemble(&registry, &gm("corpusReason"), &harness.sources())
            .expect_err("a product carrying the registry graph must be rejected");
        assert!(diag.to_string().contains("closes a cycle"), "{diag}");
    }

    /// A stage-product selector naming a stage this stage does not consume is a
    /// missing dataflow edge, not permission to read that stage's output from disk.
    #[test]
    fn a_stage_product_selector_without_an_upstream_edge_hard_fails() {
        let harness = Harness::new();
        let registry = registry_of(
            "gmeow:corpusReason a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsStageProduct gmeow:stage-reason .",
        )
        .expect("registry");
        let diag = assemble(&registry, &gm("corpusReason"), &harness.sources())
            .expect_err("a missing upstream product must hard-fail");
        assert!(
            diag.to_string().contains("gmeow:dataflowConsumes"),
            "{diag}"
        );
    }

    /// The term-table rendering is canonical, vocabulary-only, and stable across
    /// two parses of isomorphic input — the property that makes a term-table
    /// dictionary explainable and RNG-free.
    #[test]
    fn the_term_table_rendering_is_canonical_and_vocabulary_only() {
        let a = purrdf::parse_dataset(
            b"<https://e/s> <https://e/p> \"v\" .\n<https://e/s> <https://e/q> _:x .\n",
            "application/n-triples",
            None,
        )
        .expect("parse");
        let b = purrdf::parse_dataset(
            b"<https://e/s> <https://e/q> _:renamed .\n<https://e/s> <https://e/p> \"v\" .\n",
            "application/n-triples",
            None,
        )
        .expect("parse");
        let rendered = term_table_sample(&a);
        assert_eq!(
            rendered,
            term_table_sample(&b),
            "the rendering must not depend on blank labels or parse order"
        );
        let text = String::from_utf8(rendered).expect("utf-8");
        assert!(text.contains("<https://e/p>\n"), "{text}");
        assert!(text.contains("\"v\"\n"), "{text}");
        assert!(!text.contains("_:"), "blank labels are excluded: {text}");
        // Sorted, one term per line — a canonical rendering, not an interning-order dump.
        let lines: Vec<&str> = text.lines().collect();
        let mut sorted = lines.clone();
        sorted.sort_unstable();
        assert_eq!(lines, sorted);
    }
}
