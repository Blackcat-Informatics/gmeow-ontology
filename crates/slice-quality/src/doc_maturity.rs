// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `DocMaturity` axis — documentation-maturity coverage, CONSUMED from the
//! `gmeow-docs` computation, never recomputed here.
//!
//! The documentation-maturity standard (the Formal-Concept coverage lattice and the
//! bounded per-slice `gmeow:coverageFraction ∈ [0,1]`) is owned end-to-end by
//! [`gmeow_docs`]: [`DocsModel::discover`] builds the typed model and
//! [`documentation_graph`] projects the per-slice covered-dimension incidence and its
//! bounded coverage fraction. This axis reads that projection AS-IS — the axis score
//! IS the slice's `coverage_fraction` and the advisories name the FULL-anchor
//! dimensions the slice does not yet cover. It defines no dimension, no intent, and no
//! fraction of its own (Principle 17: `crates/docs` is the single owner; this is a
//! consumer).
//!
//! # Cost & caching
//!
//! [`DocsModel::discover`] is a repo-wide sweep, so it is built ONCE per repo root and
//! memoized: every slice the quality sweep scores reads the same in-memory
//! documentation model. The cost is the model the regenerate pipeline builds anyway,
//! paid once behind a `make check` gate.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use gmeow_docs::maturity::{Dimension, MaturityAnchor};
use gmeow_docs::model::DocsModel;
use gmeow_docs::rdf::{DocSliceFacts, documentation_graph};

use crate::axes::repo_root_of;
use crate::score::{AxisScore, ScoreContext, ScoringEnv, advisory};

/// The documentation-maturity axis producer. A struct (not a free `fn`) so the
/// producer symbol the rubric names — `"DocMaturity"` — resolves to a real Rust
/// *item* under the constitution-gate AST resolver, satisfying both the
/// axis↔producer binding gate and the exemption-staleness trigger.
pub struct DocMaturity;

impl DocMaturity {
    /// Score the slice's documentation maturity: the axis score is the slice's
    /// bounded `gmeow:coverageFraction` (already `[0,1]`), consumed verbatim from the
    /// documentation model; the advisories name the FULL-anchor coverage dimensions
    /// the slice does not yet cover, in stable dimension order (the incremental uplift
    /// targets the ratchet drives). A slice with no resolvable repo root, an
    /// un-buildable documentation model, or no record in the model is scored the
    /// crate's neutral vacuous `1.0` WITH an advisory naming the reason — never a
    /// silent false-positive "fully documented".
    ///
    /// The documentation model's SOURCE branches on the scoring environment:
    /// [`ScoringEnv::Repo`] reads the memoized repo-wide model ([`Self::axis_repo`]);
    /// [`ScoringEnv::Bundle`] builds a fresh single-slice model from the slice's own
    /// directory ([`Self::axis_external`]). The `Bundle` payload (the gmn1 dictionary)
    /// is irrelevant to documentation maturity, so it is ignored here.
    #[must_use]
    pub fn axis(ctx: &ScoreContext) -> AxisScore {
        match &ctx.env {
            ScoringEnv::Repo => Self::axis_repo(ctx),
            ScoringEnv::Bundle(_) => Self::axis_external(ctx),
        }
    }

    /// Repo-mode documentation maturity: read the memoized repo-wide documentation
    /// model (built once per repo root by [`DocsModel::discover`]) and look the slice
    /// up by its IRI. This is the verbatim pre-seam behaviour.
    fn axis_repo(ctx: &ScoreContext) -> AxisScore {
        let Some(root) = repo_root_of(&ctx.slice_dir) else {
            return AxisScore {
                score: 1.0,
                findings: vec![advisory(
                    "slice-quality.doc-maturity.model-unavailable",
                    "the slice directory carries no resolvable slices/ path prefix — documentation maturity cannot be measured (vacuous 1.0).".to_owned(),
                )],
            };
        };
        let facts = repo_facts(&root);
        match &*facts {
            RepoFacts::Failed(err) => AxisScore {
                score: 1.0,
                findings: vec![advisory(
                    "slice-quality.doc-maturity.model-unavailable",
                    format!(
                        "the documentation model could not be built ({err}) — documentation maturity cannot be measured (vacuous 1.0)."
                    ),
                )],
            },
            RepoFacts::Ready(by_slice) => match by_slice.get(&ctx.slice_iri) {
                Some(fact) => score_and_advice(fact),
                None => AxisScore {
                    score: 1.0,
                    findings: vec![advisory(
                        "slice-quality.doc-maturity.slice-untracked",
                        format!(
                            "{} carries no record in the documentation model (no documented terms) — documentation maturity is vacuously 1.0.",
                            ctx.slice_iri
                        ),
                    )],
                },
            },
        }
    }

    /// External-mode documentation maturity for a foreign slice pulled in on its own:
    /// build a FRESH single-slice documentation model from the slice's OWN directory
    /// ([`DocsModel::from_slice_dir`]), read back that one slice's [`DocSliceFacts`] via
    /// [`documentation_graph`], and hand them to the SAME [`score_and_advice`] the repo
    /// arm uses — so an off-repo slice and an in-repo slice earn the score by the very
    /// same measure. On a model that will not build → the existing `model-unavailable`
    /// advisory; on the slice carrying no record in its own model → the existing
    /// `slice-untracked` advisory.
    ///
    /// The single-slice model's `term_loss` is deliberately `None` (see
    /// [`DocsModel::from_slice_dir`]): a foreign slice was never compiled through the
    /// pipeline's stage-mappings, so it has no dynamic projection-loss ledger. That is
    /// the correct off-repo scope boundary — a not-applicable fact, never a failed join.
    fn axis_external(ctx: &ScoreContext) -> AxisScore {
        let model = match DocsModel::from_slice_dir(&ctx.slice_dir) {
            Ok(model) => model,
            Err(err) => {
                return AxisScore {
                    score: 1.0,
                    findings: vec![advisory(
                        "slice-quality.doc-maturity.model-unavailable",
                        format!(
                            "the documentation model could not be built ({err}) — documentation maturity cannot be measured (vacuous 1.0)."
                        ),
                    )],
                };
            }
        };
        let graph = documentation_graph(&model);
        match graph.slices.iter().find(|s| s.documents == ctx.slice_iri) {
            Some(fact) => score_and_advice(fact),
            None => AxisScore {
                score: 1.0,
                findings: vec![advisory(
                    "slice-quality.doc-maturity.slice-untracked",
                    format!(
                        "{} carries no record in the documentation model (no documented terms) — documentation maturity is vacuously 1.0.",
                        ctx.slice_iri
                    ),
                )],
            },
        }
    }
}

/// Turn one slice's documentation facts into the axis score + uplift advisories.
///
/// The score IS the slice's bounded `gmeow:coverageFraction` (measured against the
/// FULL anchor's intent by `crates/docs`), consumed verbatim. The advisories are the
/// FULL-anchor dimensions the slice does not `gmeow:docCoversDimension`, in stable
/// [`Dimension::ALL`] order — the top missing uplift targets, deterministic.
fn score_and_advice(fact: &DocSliceFacts) -> AxisScore {
    let covered: BTreeSet<&str> = fact.covers.iter().map(String::as_str).collect();
    let full_intent = MaturityAnchor::Full.intent();
    let mut findings = Vec::new();
    for dim in Dimension::ALL {
        if !full_intent.contains(&dim) {
            continue; // measure against the FULL floor the fraction is taken over
        }
        if !covered.contains(dim.local_name()) {
            findings.push(advisory(
                "slice-quality.doc-maturity.missing-dimension",
                format!(
                    "{} does not yet cover documentation-maturity dimension gmeow:{} — cover it to raise the slice's documentation-maturity coverage (the standard is owned by slices/core/documentation; DocMaturity consumes it as-is).",
                    fact.documents,
                    dim.local_name()
                ),
            ));
        }
    }
    AxisScore {
        score: fact.coverage_fraction,
        findings,
    }
}

/// The memoized per-repo documentation facts: either the per-slice-IRI map read from
/// the `graph/documentation` projection, or the error string from a failed model
/// build (surfaced as an advisory, never a silent skip).
enum RepoFacts {
    /// Slice IRI (`DocSliceFacts::documents`) → its documentation facts.
    Ready(HashMap<String, DocSliceFacts>),
    /// The documentation model could not be built; the message is surfaced.
    Failed(String),
}

/// The process-wide cache keyed by repo root, so [`DocsModel::discover`] — a repo-wide
/// sweep — runs at most ONCE per root across the whole quality assessment even though
/// every scored slice invokes the axis.
static REPO_FACTS: LazyLock<Mutex<HashMap<PathBuf, Arc<RepoFacts>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Resolve (building on first use) the documentation facts for `root`. The lock is
/// held across the build so the expensive model is constructed exactly once; a second
/// caller blocks on the mutex and then reads the shared result.
fn repo_facts(root: &Path) -> Arc<RepoFacts> {
    // The bare axis lookup has no live catalog bytes to offer, so a cache MISS falls
    // back to the disk-sourced model build — the legitimate post-pipeline / CLI path
    // over a materialized tree. The in-pipeline sweep always PRIMES the cache with the
    // live catalog bytes first ([`prime_repo_facts`]), so this lookup finds that entry
    // and never touches disk on a cold tree.
    facts_or_build(root, None)
}

/// Resolve (building on first use) the documentation facts for `root`, sourcing the
/// constraint catalog from `catalog_bytes` when supplied (the live in-pipeline bytes)
/// or from disk otherwise. The lock is held across the build so the expensive model is
/// constructed exactly once; a second caller blocks on the mutex and then reads the
/// shared result — and a caller with live bytes and a caller without both settle on
/// the SAME already-cached entry (whichever primed first).
fn facts_or_build(root: &Path, catalog_bytes: Option<&[u8]>) -> Arc<RepoFacts> {
    let mut guard = REPO_FACTS
        .lock()
        .expect("doc-maturity repo-facts cache mutex is not poisoned");
    let key = root.to_path_buf();
    if let Some(existing) = guard.get(&key) {
        return existing.clone();
    }
    let built = Arc::new(build_repo_facts(root, catalog_bytes));
    guard.insert(key, built.clone());
    built
}

/// Prime the immutable repo-wide documentation facts before a parallel slice sweep,
/// sourcing the constraint catalog from `catalog_bytes` (THIS run's freshly-rendered
/// `stage-constraint-catalog` bytes) when the caller is the in-pipeline sweep, or from
/// disk (`None`) for the post-pipeline / CLI sweep over a materialized tree.
///
/// The ordinary axis lookup remains the single cache authority. This entry point only
/// moves its first construction ahead of the worker fan-out so workers never occupy a
/// Rayon thread while waiting on the cache mutex — and, crucially, seeds the cache with
/// the live catalog bytes so a cold tree's absent `generated/catalog/constraint-catalog.nq`
/// never fails the model build (which would collapse every slice to a vacuous 1.0 and
/// diverge from a warm run).
pub(crate) fn prime_repo_facts(root: &Path, catalog_bytes: Option<&[u8]>) {
    drop(facts_or_build(root, catalog_bytes));
}

/// Build the per-slice documentation facts from a fresh [`DocsModel`]. The per-slice
/// coverage fraction + covered-dimension incidence are read back from the SAME
/// `graph/documentation` N-Quads the docs projection emits (via [`documentation_graph`]),
/// so this axis and the published documentation health surface can never disagree.
///
/// `catalog_bytes` selects the constraint-catalog source: the live in-pipeline bytes
/// ([`DocsModel::discover_with_catalog`]) when supplied, else the committed on-disk
/// catalog ([`DocsModel::discover`]). The catalog content does not feed the coverage
/// fraction; supplying live bytes only guarantees the model BUILDS on a cold tree.
fn build_repo_facts(root: &Path, catalog_bytes: Option<&[u8]>) -> RepoFacts {
    let built = match catalog_bytes {
        // In-pipeline: the catalog is THIS run's freshly-rendered bytes, which are not
        // part of the on-disk content-addressed key, so the disk fixture cache cannot
        // and must not serve it.
        Some(bytes) => DocsModel::discover_with_catalog(root, bytes),
        // Post-pipeline / CLI over a materialized tree: take the model from the
        // content-addressed `.cache/docs-fixture` store instead of paying a fresh
        // ~12 s `discover()`. `fixture::try_load` is byte-identical to `discover()`
        // and preserves its Result — a build failure still becomes
        // `RepoFacts::Failed` (and thence the `doc-maturity.model-unavailable`
        // advisory), never a panic. The key folds every input `discover()` reads plus
        // gmeow-docs' whole transitive path-dependency closure, so a stale model
        // cannot be served across any edit that would change this axis's score.
        None => gmeow_docs::fixture::try_load(root),
    };
    match built {
        Ok(model) => {
            let graph = documentation_graph(&model);
            let by_slice = graph
                .slices
                .into_iter()
                .map(|s| (s.documents.clone(), s))
                .collect();
            RepoFacts::Ready(by_slice)
        }
        Err(e) => RepoFacts::Failed(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic per-slice fact: covers the given dimension local names, with the
    /// given bounded coverage fraction.
    fn fact(fraction: f64, covers: &[&str]) -> DocSliceFacts {
        DocSliceFacts {
            subject: "https://blackcatinformatics.ca/gmeow/documentation/slice/zoo".to_owned(),
            documents: "https://blackcatinformatics.ca/gmeow/slices/zoo".to_owned(),
            covers: covers.iter().map(|s| (*s).to_owned()).collect(),
            coverage_fraction: fraction,
            earned: None,
            asserted: None,
        }
    }

    #[test]
    fn score_is_the_consumed_coverage_fraction() {
        // The axis score is the slice's bounded coverage fraction verbatim — the
        // producer consumes the docs computation, never recomputes a dimension.
        let f = fact(0.5833, &["dimDefinition", "dimLabel"]);
        let scored = score_and_advice(&f);
        assert!(
            (scored.score - 0.5833).abs() < 1e-12,
            "score is the fraction"
        );
    }

    #[test]
    fn advisories_name_the_uncovered_full_dimensions() {
        // A slice covering only the two Minimal dimensions is short every other
        // FULL-anchor dimension; each missing one is a ranked uplift advisory, and a
        // covered one is not advised.
        let f = fact(0.1667, &["dimDefinition", "dimLabel"]);
        let scored = score_and_advice(&f);
        let full_extra = MaturityAnchor::Full.intent().len() - 2; // minus Definition+Label
        assert_eq!(
            scored.findings.len(),
            full_extra,
            "one advisory per uncovered FULL dimension"
        );
        // The covered dimensions are never advised.
        assert!(
            !scored
                .findings
                .iter()
                .any(|f| f.message.contains("dimDefinition") || f.message.contains("dimLabel")),
            "a covered dimension is not an uplift target"
        );
        // A genuinely-missing FULL dimension is advised, naming the slice.
        assert!(
            scored
                .findings
                .iter()
                .any(|f| f.message.contains("dimExample") && f.message.contains("slices/zoo")),
            "an uncovered FULL dimension is named for the slice"
        );
    }

    /// Scaffold a temp repo root carrying exactly one real slice (copied from the
    /// committed `gmeow-docs` single-slice fixture). Returns the root and the slice
    /// directory. No `generated/` tree is created.
    fn scaffold_single_slice_root(tag: u32) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "gmeow-docmaturity-det-{}-{tag}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&root).ok();
        let slice_dir = root.join("slices").join("fixture").join("single");
        std::fs::create_dir_all(&slice_dir).expect("mkdir slice");
        // The committed fixture lives in the sibling gmeow-docs crate.
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("docs")
            .join("tests")
            .join("fixtures")
            .join("single-slice");
        for file in ["manifest.ttl", "module.ttl"] {
            std::fs::copy(fixture.join(file), slice_dir.join(file))
                .unwrap_or_else(|e| panic!("copy fixture {file}: {e}"));
        }
        (root, slice_dir)
    }

    /// Minimal but valid constraint-catalog N-Quads: one `gmeow:ValidationRule` with a
    /// `gmeow:ruleCode`.
    fn sample_catalog_bytes() -> Vec<u8> {
        let graph =
            "https://blackcatinformatics.ca/gmeow/graph/fanout/catalog/constraint-catalog.nq";
        let rule = "https://blackcatinformatics.ca/gmeow/rule/box-roles-invalid";
        format!(
            "<{rule}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
             <https://blackcatinformatics.ca/gmeow/ValidationRule> <{graph}> .\n\
             <{rule}> <https://blackcatinformatics.ca/gmeow/ruleCode> \"box-roles.invalid\" <{graph}> .\n"
        )
        .into_bytes()
    }

    /// The DocMaturity axis score is BYTE-IDENTICAL whether the constraint catalog is
    /// sourced from THIS run's live bytes (a cold tree with no `generated/`) or read
    /// from disk (a warm tree carrying the SAME bytes). This is the determinism the
    /// two-generation sync gate needs: without the fix, the cold run's absent catalog
    /// fails the model build and collapses the axis to the vacuous model-unavailable
    /// `1.0`, while the warm run scores the real fraction — a divergent
    /// `graph/quality-assessment`.
    #[test]
    fn doc_maturity_score_identical_live_bytes_vs_disk_catalog() {
        let catalog = sample_catalog_bytes();
        let empty = purrdf::RdfDatasetBuilder::new()
            .freeze()
            .expect("empty dataset");
        let slice_iri = "https://blackcatinformatics.ca/gmeow/slices/fixture-single".to_owned();

        // COLD tree: no generated/ on disk, catalog primed from LIVE bytes.
        let (live_root, live_slice) = scaffold_single_slice_root(line!());
        prime_repo_facts(&live_root, Some(&catalog));
        let live_ctx = ScoreContext::new(slice_iri.clone(), live_slice, &empty, ScoringEnv::Repo);
        let live = DocMaturity::axis(&live_ctx);

        // WARM tree: the SAME catalog bytes on disk, sourced by the disk path (None).
        let (warm_root, warm_slice) = scaffold_single_slice_root(line!());
        std::fs::create_dir_all(warm_root.join("generated").join("catalog"))
            .expect("mkdir generated");
        std::fs::write(
            warm_root
                .join("generated")
                .join("catalog")
                .join("constraint-catalog.nq"),
            &catalog,
        )
        .expect("write catalog");
        prime_repo_facts(&warm_root, None);
        let warm_ctx = ScoreContext::new(slice_iri, warm_slice, &empty, ScoringEnv::Repo);
        let warm = DocMaturity::axis(&warm_ctx);

        assert_eq!(
            live.score.to_bits(),
            warm.score.to_bits(),
            "DocMaturity score must be byte-identical cold(live) vs warm(disk); live={} warm={}",
            live.score,
            warm.score
        );
        // The live path genuinely BUILT the model — it did not fall back to the vacuous
        // model-unavailable 1.0 the cold disk read would have forced.
        assert!(
            !live
                .findings
                .iter()
                .any(|f| f.message.contains("documentation model could not be built")),
            "the live-bytes path must build the model, never the model-unavailable fallback"
        );

        std::fs::remove_dir_all(&live_root).ok();
        std::fs::remove_dir_all(&warm_root).ok();
    }

    #[test]
    fn a_fully_covered_full_intent_has_no_advice() {
        // Covering exactly the FULL intent → fraction 1.0, no uplift advice at this
        // axis's own (FULL) measure.
        let covers: Vec<String> = MaturityAnchor::Full
            .intent()
            .iter()
            .map(|d| d.local_name().to_owned())
            .collect();
        let refs: Vec<&str> = covers.iter().map(String::as_str).collect();
        let scored = score_and_advice(&fact(1.0, &refs));
        assert!(
            scored.findings.is_empty(),
            "a FULL-covered slice has no missing-dimension advice"
        );
    }
}
