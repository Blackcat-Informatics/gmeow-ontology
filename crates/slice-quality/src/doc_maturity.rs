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

use gmeow_docs_model::maturity::{Dimension, MaturityAnchor};
use gmeow_docs_model::model::DocsModel;
use gmeow_docs_model::rdf::{DocSliceFacts, documentation_graph};

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
    /// targets the ratchet drives).
    ///
    /// A condition under which the axis CANNOT BE MEASURED — no resolvable repo root, or
    /// a documentation model that will not build — scores [`unmeasurable`]'s `0.0` with
    /// an advisory naming the reason. A slice that IS measurable but carries no record in
    /// the model (no documented terms) is genuinely vacuous and keeps the neutral `1.0`:
    /// having nothing to document is a different fact from not having been able to look.
    ///
    /// The documentation model's SOURCE branches on the scoring environment:
    /// [`ScoringEnv::Repo`] reads the memoized repo-wide model ([`Self::axis_repo`]);
    /// [`ScoringEnv::Bundle`] builds a fresh single-slice model from the slice's own
    /// carried files ([`Self::axis_external`]). The `Bundle` payload (the gmn1
    /// dictionary) is irrelevant to documentation maturity, so it is ignored here.
    #[must_use]
    pub fn axis(ctx: &ScoreContext) -> AxisScore {
        match &ctx.env {
            ScoringEnv::Repo { slice_dir } => Self::axis_repo(ctx, slice_dir),
            ScoringEnv::Bundle(_) => Self::axis_external(ctx),
        }
    }

    /// Repo-mode documentation maturity: read the memoized repo-wide documentation
    /// model (built once per repo root by [`DocsModel::discover`]) and look the slice
    /// up by its IRI. This is the verbatim pre-seam behaviour — the repo root is
    /// resolved from the CHECKOUT anchor the [`ScoringEnv::Repo`] environment carries,
    /// the only path any axis is allowed to read.
    fn axis_repo(ctx: &ScoreContext, slice_dir: &Path) -> AxisScore {
        let Some(root) = repo_root_of(slice_dir) else {
            return unmeasurable(
                "the slice directory carries no resolvable slices/ path prefix".to_owned(),
            );
        };
        let facts = repo_facts(&root);
        match &*facts {
            RepoFacts::Failed(err) => unmeasurable(format!(
                "the documentation model could not be built ({err})"
            )),
            RepoFacts::Ready(by_slice) => match by_slice.get(&ctx.slice_iri) {
                Some(fact) => score_and_advice(fact),
                None => slice_untracked(&ctx.slice_iri),
            },
        }
    }

    /// External-mode documentation maturity for a foreign slice pulled in on its own:
    /// build a FRESH single-slice documentation model from the slice's OWN carried
    /// files ([`DocsModel::from_slice_dir`]), read back that one slice's
    /// [`DocSliceFacts`] via [`documentation_graph`], and hand them to the SAME
    /// [`score_and_advice`] the repo arm uses — so an off-repo slice and an in-repo
    /// slice earn the score by the very same measure. On a model that will not build →
    /// the existing `model-unavailable` advisory; on the slice carrying no record in
    /// its own model → the existing `slice-untracked` advisory.
    ///
    /// The single-slice model's `term_loss` is deliberately `None` (see
    /// [`DocsModel::from_slice_dir`]): a foreign slice was never compiled through the
    /// pipeline's stage-mappings, so it has no dynamic projection-loss ledger. That is
    /// the correct off-repo scope boundary — a not-applicable fact, never a failed join.
    ///
    /// # Why this one axis materializes the file map back to a directory
    ///
    /// Every other axis reads `ctx.files` directly. This one cannot: the documentation
    /// model is built by `gmeow_docs_model` on top of `purrdf`'s `SliceCatalog`, whose
    /// `records`/`vocab` fields are PRIVATE and whose only constructors are
    /// `SliceCatalog::discover(root, vocab)` and `SliceCatalog::from_slice_dir(dir,
    /// vocab)` — both of which take a `&Path` and walk it with `std::fs`. There is no
    /// in-memory constructor to hand a byte map to. Rather than fork a second,
    /// silently-divergent documentation model here (a forbidden second source of
    /// truth), the native arm writes the map back out to a temp directory and calls the
    /// SAME real loader, so the off-repo score stays byte-for-byte what it was before
    /// the map existed. See [`Self::axis_external`]'s `wasm32` twin for what happens
    /// where no filesystem exists at all.
    #[cfg(not(target_arch = "wasm32"))]
    fn axis_external(ctx: &ScoreContext) -> AxisScore {
        let staged = match stage_slice_files(ctx.files) {
            Ok(staged) => staged,
            // A temp-dir or write failure is a real inability to measure, surfaced with
            // the SAME advisory a failed model build carries — never a silent pass.
            Err(err) => {
                return unmeasurable(format!(
                    "the documentation model could not be built ({err})"
                ));
            }
        };
        let model = match DocsModel::from_slice_dir(staged.path()) {
            Ok(model) => model,
            Err(err) => {
                return unmeasurable(format!(
                    "the documentation model could not be built ({err})"
                ));
            }
        };
        let graph = documentation_graph(&model);
        match graph.slices.iter().find(|s| s.documents == ctx.slice_iri) {
            Some(fact) => score_and_advice(fact),
            None => slice_untracked(&ctx.slice_iri),
        }
    }

    /// External-mode documentation maturity on a target with NO FILESYSTEM.
    ///
    /// The documentation model cannot be built here, and the reason is a precise,
    /// nameable upstream gap rather than a local shortcut: `purrdf`'s `SliceCatalog`
    /// keeps its `records` and `vocab` fields private and exposes exactly two
    /// constructors — `SliceCatalog::discover(&Path, SliceVocab)` and
    /// `SliceCatalog::from_slice_dir(&Path, &SliceVocab)` — both of which read the
    /// tree with `std::fs`. `gmeow_docs_model::model::DocsModel::from_slice_dir` is a
    /// thin wrapper over the first of those, so with no `SliceCatalog` constructor
    /// that accepts already-loaded artifact bytes there is no way to build a
    /// documentation model from `ctx.files` on `wasm32`.
    ///
    /// The unblocking upstream capability is therefore exactly one thing: a
    /// `purrdf::slice::SliceCatalog` constructor taking pre-read
    /// `(logical_path, bytes)` artifacts (equivalently, a public `SliceRecord`
    /// constructor). The moment that lands, this arm becomes the same three lines as
    /// the native one with the staging step deleted, and the advisory disappears.
    /// Until then the axis is UNMEASURABLE here and scores [`unmeasurable`]'s `0.0`
    /// with the reason named — never a silent false-positive "fully documented", and
    /// never the neutral `1.0` either: not having been able to look is not the same
    /// fact as having nothing to document.
    #[cfg(target_arch = "wasm32")]
    fn axis_external(_ctx: &ScoreContext) -> AxisScore {
        unmeasurable(
            "purrdf's SliceCatalog has no in-memory constructor (its fields are private and \
             SliceCatalog::discover / SliceCatalog::from_slice_dir are the only entries, both \
             std::fs path readers), so a documentation model cannot be built from bytes on a \
             target with no filesystem"
                .to_owned(),
        )
    }
}

/// The `slice-quality.doc-maturity.slice-untracked` advisory: the model built, but
/// carries no record for this slice (it documents no terms).
fn slice_untracked(slice_iri: &str) -> AxisScore {
    AxisScore {
        score: 1.0,
        findings: vec![advisory(
            "slice-quality.doc-maturity.slice-untracked",
            format!(
                "{slice_iri} carries no record in the documentation model (no documented terms) — documentation maturity is vacuously 1.0."
            ),
        )],
    }
}

/// Write an in-memory slice file map back out to a fresh temp directory, preserving
/// the key paths verbatim, and return the owning [`TempDir`] (deleted on drop).
///
/// Only [`DocMaturity::axis_external`] needs this, and only because the upstream
/// documentation-model loader is path-shaped (see that function's doc comment). Keys
/// are slice-relative forward-slash paths produced by
/// [`crate::report::slice_files_from_dir`] or an equivalent in-memory author; a key
/// that escapes the staging root (an absolute path or a `..` component) is REJECTED
/// rather than written, because materializing attacker-influenced bytes outside the
/// temp dir is a real write-outside-root hazard, not a scoring concern.
///
/// # Errors
/// A [`crate::error::Io`] diagnostic naming the failure when the temp directory cannot
/// be created, a key is not a safe relative path, or a file cannot be written.
#[cfg(not(target_arch = "wasm32"))]
fn stage_slice_files(
    files: &std::collections::BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<tempfile::TempDir> {
    /// Raise a staging failure as the crate's typed I/O diagnostic — `Diag` is the sole
    /// first-party error type, so this path carries no bare `String` error.
    fn io(detail: String) -> gmeow_errors::Diag {
        gmeow_errors::Diag::of_kind(crate::error::Io { detail })
    }

    let dir = tempfile::Builder::new()
        .prefix("gmeow-slice-quality-docs-")
        .tempdir()
        .map_err(|e| {
            io(format!(
                "cannot create a staging directory for the slice files: {e}"
            ))
        })?;
    for (key, bytes) in files {
        let rel = Path::new(key);
        if rel
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(io(format!(
                "slice file key {key:?} is not a safe slice-relative path (absolute or \
                 parent-directory components are rejected)"
            )));
        }
        let target = dir.path().join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                io(format!(
                    "cannot create staging directory {}: {e}",
                    parent.display()
                ))
            })?;
        }
        std::fs::write(&target, bytes)
            .map_err(|e| io(format!("cannot stage slice file {}: {e}", target.display())))?;
    }
    Ok(dir)
}

/// The score for a slice whose documentation maturity CANNOT BE MEASURED: `0.0` plus an
/// advisory naming `reason`.
///
/// It used to be `1.0`. That was a silent, maximal false positive on exactly the
/// condition under which nothing was known — and it was load-bearing, not theoretical:
/// a corpus recorded on a tree where the documentation model would not build carried a
/// ceilinged `DocMaturity` for EVERY slice, and (because the freshness fingerprint folds
/// only authored sources) verified as fresh. The unmeasured case now scores the bottom
/// of the axis, so it reds the per-axis floor instead of clearing it: an axis that could
/// not be measured is a defect to fix, never a grade to bank.
///
/// This is distinct from the genuinely vacuous case (a measurable slice that documents no
/// terms), which keeps `1.0` — there the measurement succeeded and found nothing owed.
fn unmeasurable(reason: String) -> AxisScore {
    AxisScore {
        score: 0.0,
        findings: vec![advisory(
            "slice-quality.doc-maturity.model-unavailable",
            format!(
                "{reason} — documentation maturity cannot be measured, so it is scored 0.0 \
                 (the bottom of the axis). An unmeasurable axis is never a passing one; fix \
                 the condition and re-score."
            ),
        )],
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
        // Post-pipeline / CLI over a materialized tree: build the model from disk.
        //
        // This is deliberately the UNCACHED loader, and the reason is a layering
        // constraint rather than a preference. The content-addressed
        // `.cache/docs-fixture` store that would save this a fresh ~12 s `discover()`
        // lives in `gmeow_docs::fixture` (`try_load`, byte-identical to `discover()`
        // and Result-preserving), but reaching it means an edge
        // `gmeow-slice-quality -> gmeow-docs`, and `gmeow-docs` dev-depends on
        // `gmeow-mcp` (so `shipped_queries_execute` can run every SPARQL text the site
        // ships against the real engine), while `gmeow-mcp` depends on THIS crate. That
        // closes a first-party cycle `gmeow-docs -> gmeow-mcp -> gmeow-slice-quality ->
        // gmeow-docs`, which `gmeow_validate::crate_layering` refuses — its dependency
        // scan counts `dev-dependencies`.
        //
        // Nothing about the SCORE differs: `try_load` returns what `discover` returns.
        // What is lost is only the cache hit. The forward path is to move the MODEL half
        // of `gmeow_docs::fixture` (the envelope + `cache_key` + the derived crate
        // closure) into `gmeow-docs-model`, which this crate already depends on and
        // which nothing depends on this crate for; the renderer-only `CachedSite` half
        // stays in `gmeow-docs`. Then the cache is reachable from here with no new edge.
        None => DocsModel::discover(root),
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
    /// committed `gmeow-docs-model` single-slice fixture). Returns the owning
    /// [`tempfile::TempDir`], the root, and the slice directory. No `generated/`
    /// tree is created.
    ///
    /// Two scaffolded roots never collide because each owns a distinct
    /// `TempDir`, and each tree is removed when its guard drops — on success, on
    /// panic, and on early return. The caller must bind the guard
    /// (`let (_tmp, root, slice) = scaffold_single_slice_root();`); a bare `_`
    /// binding would drop it at once and delete the tree out from under the test.
    fn scaffold_single_slice_root() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let guard = tempfile::tempdir().expect("create temp dir");
        let root = guard.path().join("gmeow-docmaturity-det");
        let slice_dir = root.join("slices").join("fixture").join("single");
        std::fs::create_dir_all(&slice_dir).expect("mkdir slice");
        // The committed fixture lives in the sibling gmeow-docs-model crate — it moved
        // there with the model when `gmeow-docs` was split, and this reader was left
        // pointing at the old `crates/docs/tests/fixtures/` path, so the copy failed and
        // the determinism check could not run at all.
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("docs-model")
            .join("tests")
            .join("fixtures")
            .join("single-slice");
        for file in ["manifest.ttl", "module.ttl"] {
            std::fs::copy(fixture.join(file), slice_dir.join(file))
                .unwrap_or_else(|e| panic!("copy fixture {file}: {e}"));
        }
        (guard, root, slice_dir)
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
    /// fails the model build and collapses the axis to the model-unavailable floor of
    /// `0.0`, while the warm run scores the real fraction — a divergent
    /// `graph/quality-assessment`.
    #[test]
    fn doc_maturity_score_identical_live_bytes_vs_disk_catalog() {
        let catalog = sample_catalog_bytes();
        let empty = purrdf::RdfDatasetBuilder::new()
            .freeze()
            .expect("empty dataset");
        let slice_iri = "https://blackcatinformatics.ca/gmeow/slices/fixture-single".to_owned();

        // The repo arm reads only the documentation model and the checkout anchor, so
        // the slice's own file map is irrelevant here — an empty map is the honest input.
        let files = std::collections::BTreeMap::new();

        // COLD tree: no generated/ on disk, catalog primed from LIVE bytes.
        let (_live_tmp, live_root, live_slice) = scaffold_single_slice_root();
        prime_repo_facts(&live_root, Some(&catalog));
        let live_ctx = ScoreContext::new(
            slice_iri.clone(),
            &files,
            &empty,
            ScoringEnv::Repo {
                slice_dir: live_slice,
            },
        );
        let live = DocMaturity::axis(&live_ctx);

        // WARM tree: the SAME catalog bytes on disk, sourced by the disk path (None).
        let (_warm_tmp, warm_root, warm_slice) = scaffold_single_slice_root();
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
        let warm_ctx = ScoreContext::new(
            slice_iri,
            &files,
            &empty,
            ScoringEnv::Repo {
                slice_dir: warm_slice,
            },
        );
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
