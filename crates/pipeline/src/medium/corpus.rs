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

/// Resolve a declared corpus to its training samples.
///
/// Samples are collected in `BTreeSet` order, which makes assembly deterministic
/// without making the order load-bearing: [`super::train::build`] is a pure function
/// of the sample MULTISET (upstream canonically sorts before concatenating), so the
/// set's order can never leak into the dictionary bytes.
///
/// An EMPTY corpus for a declared dictionary is a HARD FAIL: the dictionary would
/// have no bytes, and a frame primed with the id it was supposed to carry would be
/// permanently undecodable.
///
/// # Errors
/// An empty result, a missing archive rep / stage product, or material that reaches
/// into the excluded fixpoint region.
pub fn assemble(
    registry: &MediumRegistry,
    corpus_iri: &str,
    sources: &CorpusSources<'_>,
) -> Result<BTreeSet<Vec<u8>>, gmeow_errors::Diag> {
    let corpus = registry.corpora().get(corpus_iri).ok_or_else(|| {
        invalid_declaration(format!(
            "<{corpus_iri}> is not a declared gmeow:DictionaryCorpus"
        ))
    })?;

    let mut samples: BTreeSet<Vec<u8>> = BTreeSet::new();
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
                samples.extend(members.into_values().filter(|b| !b.is_empty()));
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
                    samples.insert(ntriples.into_bytes());
                }
            }
            CorpusSelector::PathPrefix(prefix) => {
                for (path, bytes) in sources.artifacts.range(prefix.clone()..) {
                    if !path.starts_with(prefix.as_str()) {
                        break;
                    }
                    if !bytes.is_empty() {
                        samples.insert(bytes.clone());
                    }
                }
                // An AUTHORED tree is legitimately on disk (it is what
                // `stage-archive-blobs` tars for the same reason); a `generated/`
                // prefix is NOT, and resolves from the in-memory lane above alone.
                if !prefix.starts_with("generated/") {
                    collect_authored_files(&sources.root.join(prefix), &mut samples);
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
                samples.extend(artifacts.into_values().filter(|b| !b.is_empty()));
            }
        }
    }

    if samples.is_empty() {
        return Err(undeclared_dictionary(format!(
            "<{corpus_iri}> resolves to ZERO samples over selectors {:?} — a declared dictionary \
             with an empty corpus has no bytes, so every frame primed with the id it was supposed \
             to carry would be permanently undecodable",
            corpus.selectors
        )));
    }
    Ok(samples)
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
fn collect_authored_files(dir: &Path, samples: &mut BTreeSet<Vec<u8>>) {
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
            collect_authored_files(&path, samples);
        } else if let Ok(bytes) = std::fs::read(&path)
            && !bytes.is_empty()
        {
            samples.insert(bytes);
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

            let tar: Vec<(String, Vec<u8>)> = vec![(
                "slices/core/gts/cell.ttl".to_string(),
                b"<https://e/s> <https://e/p> <https://e/o> .\n".to_vec(),
            )];
            let archive = purrdf::ustar::write_archive(&tar).expect("fixture archive");

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
        let samples = assemble(&registry, &gm("corpusCore"), &harness.sources())
            .expect("the core corpus resolves");
        // The archive MEMBER (not the tar) and the authored source file.
        assert!(
            samples.iter().any(|s| s.starts_with(b"<https://e/s>")),
            "the archive's members are the samples, not the tar itself"
        );
        assert!(
            samples
                .iter()
                .any(|s| s.starts_with(b"# an authored source file")),
            "an AUTHORED path prefix reads the repo tree"
        );
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
        let samples = assemble(&registry, &gm("corpusReason"), &harness.sources())
            .expect("the stage-product corpus resolves");
        assert!(samples.iter().any(|s| s.starts_with(b"<https://e/why>")));
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
