// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `stage-conformance` Transform stage: the external-corpus divergence fold.
//!
//! The committed external conformance corpus
//! (`conformance/logic/cases/external/<corpus>/<case>/`) freezes, per case, both
//! the native verdict (`expected/verdicts.json`, the per-world status the native DL
//! consistency path decided at vendor time) and the published external verdict
//! (`source/manifest.ttl`, the W3C `otest:`/`mf:` declared outcome). This stage
//! grades the frozen native verdict against the frozen published verdict for every
//! case that carries a published outcome and projects each divergence into a
//! `gmeow:Finding` N-Quads graph in [`gmeow_conformance::divergence::CONFORMANCE_GRAPH`].
//!
//! The grading reuses the same divergence machinery the `ingest-external`
//! `--grade-suite` lane drives ([`gmeow_conformance::divergence::emit_divergence_nq`]
//! → [`gmeow_logic::reason::compare_external_corpus`]): an agreement folds as a
//! NON-blocking `logic:FindingCorroboration` finding (positive evidence, never
//! dropped), a native `incomplete` becomes a `DlGap` row, and a decided native verdict
//! that differs from the published expected becomes a `CorpusOnly` row. Alongside the
//! findings, every comparison is reified as a `gmeow:ConformanceComparison` individual
//! and each corpus as a `gmeow:CorpusAgreementTally` individual. The emitter sorts and
//! content-addresses every finding + individual, so the product is byte-deterministic
//! and GTS-fold-stable.
//!
//! Grading off the FROZEN committed verdicts (rather than re-running the reasoner in
//! this stage) is deterministic by construction and never couples the snapshot fold
//! to engine availability: the native token in `expected/verdicts.json` IS the
//! verdict `gmeow_logic::reason::dl_consistency` produced for that case (it is the
//! golden the conformance harness asserts). [`crate::stages::carrier`] folds this
//! stage's product into the `graph/conformance` named graph of `gmeow.gts`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_conformance::divergence::{
    AgreementTally, agreement_tally, emit_agreement_tally_nq, emit_divergence_nq,
};
use gmeow_conformance::external::{outcome_from_szs, parse_test_manifest};
use gmeow_logic::reason::ExternalComparison;
use serde::{Deserialize, Serialize};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// The in-memory logical path of the external-corpus divergence N-Quads product
/// [`crate::stages::carrier`] folds into the `graph/conformance` named graph. The
/// `pipeline/` prefix marks it as in-memory dataflow that is never written to disk
/// (the same convention the diagnostics / composed dataflow products follow).
pub const CONFORMANCE_NQ_PATH: &str = "pipeline/conformance-divergence.nq";

/// The in-memory logical path of the per-corpus agreement-tally product
/// `stage-export-agreement` consumes to render the benchmark dashboard. Like
/// [`CONFORMANCE_NQ_PATH`] the `pipeline/` prefix marks it as in-memory dataflow
/// never written to disk — it is the single graded result, attached once, that the
/// dashboard projects (PIPELINE_SPINE §3.2: consumers read the attached result, they
/// do not re-grade).
pub const AGREEMENT_TALLIES_PATH: &str = "pipeline/agreement-tallies.json";

/// One corpus's agreement counts as they ride in [`AGREEMENT_TALLIES_PATH`]. The
/// corpus name is the JSON map key, so the record itself carries only the counts and
/// the corpus lane — serialized in a `BTreeMap` (sorted keys) with integer fields, so
/// the attached bytes are deterministic (the `bench` integer-baseline discipline;
/// no `f64`).
///
/// `lane` is the `corpus.json` lane (`"a"`/`"b"`/`"divergence"`). The dashboard needs
/// it to present a `divergence`-lane corpus honestly: its `corpus_only` rows are the
/// DOCUMENTED, intended native↔published divergences (native EL correctly differs from
/// the published DL/Full answer), never engine defects — so they are excluded from the
/// headline agreement rate rather than counted as failures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TallyRecord {
    pub(crate) lane: String,
    pub(crate) cases: usize,
    pub(crate) agree: usize,
    pub(crate) corpus_only: usize,
    pub(crate) dl_gap: usize,
}

/// The committed external-corpus root: one `<corpus>/<case>/` subtree per vendored
/// suite (`w3c-owl2-el`, `w3c-mini`, `szs-mini`, …).
const EXTERNAL_ROOT: &str = "conformance/logic/cases/external";

/// One graded external case: its corpus, case id, world IRI, and the two verdict
/// tokens (native = frozen `expected/verdicts.json` status; published = the
/// `source/manifest.ttl` `otest:`/`mf:` declared outcome).
struct GradedCase {
    corpus: String,
    comparison: ExternalComparison,
}

/// Grade every committed external corpus case that carries a published verdict and
/// emit the divergences as one `graph/conformance` N-Quads document.
///
/// Cases are discovered generically under [`EXTERNAL_ROOT`]: each `<corpus>/<case>/`
/// directory carrying both an `expected/verdicts.json` and a `source/manifest.ttl`
/// is graded. A case dir that carries a `source/manifest.ttl` this stage cannot
/// parse — or an `expected/verdicts.json` it cannot read — is a HARD failure (no
/// silent skip): the corpus is a committed, drift-gated surface.
///
/// Returns the per-corpus conformance N-Quads, concatenated in corpus order (each
/// corpus's emitter output is itself sorted + content-addressed). Every comparison
/// folds (corroboration findings + reified comparison + tally individuals), so an
/// all-agree corpus now contributes a non-empty graph.
pub fn build_conformance_divergence(root: &Path) -> Result<Vec<u8>, gmeow_errors::Diag> {
    Ok(divergence_nq_from_corpora(&grade_external_corpora(root)?))
}

/// Grade every committed external corpus case once, grouped by corpus and sorted
/// deterministically within each corpus. This is the SINGLE grading walk the stage
/// runs; both the divergence findings graph and the agreement-tally dashboard project
/// from its result, never re-grading (PIPELINE_SPINE §3.2/§8, the razor).
pub fn grade_external_corpora(
    root: &Path,
) -> Result<BTreeMap<String, Vec<ExternalComparison>>, gmeow_errors::Diag> {
    let external = root.join(EXTERNAL_ROOT);
    let mut by_corpus: BTreeMap<String, Vec<ExternalComparison>> = BTreeMap::new();
    for graded in grade_external_cases(&external)? {
        by_corpus
            .entry(graded.corpus)
            .or_default()
            .push(graded.comparison);
    }
    // Deterministic per-corpus order (the case id is unique within a corpus).
    for comparisons in by_corpus.values_mut() {
        comparisons.sort_by(|a, b| a.case.cmp(&b.case).then(a.world.cmp(&b.world)));
    }
    Ok(by_corpus)
}

/// Project the graded corpora into the `graph/conformance` N-Quads, concatenated in
/// corpus order. EVERY comparison folds: the divergence `gmeow:Finding` graph (now
/// including non-blocking corroboration findings for agreements), one reified
/// `gmeow:ConformanceComparison` individual per comparison, and one aggregate
/// `gmeow:CorpusAgreementTally` individual per corpus — so an all-agree corpus now
/// contributes a non-empty graph rather than nothing.
fn divergence_nq_from_corpora(by_corpus: &BTreeMap<String, Vec<ExternalComparison>>) -> Vec<u8> {
    let mut out = String::new();
    let push = |nq: &str, out: &mut String| {
        if !nq.is_empty() {
            out.push_str(nq);
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
    };
    for (corpus, comparisons) in by_corpus {
        // The per-case findings + reified comparison individuals.
        push(&emit_divergence_nq(corpus, comparisons), &mut out);
        // The aggregate per-corpus tally individual (computed via the same grading the
        // dashboard tally JSON uses — a pure classification, not a second disk walk).
        push(
            &emit_agreement_tally_nq(&agreement_tally(corpus, comparisons)),
            &mut out,
        );
    }
    out.into_bytes()
}

/// Project the graded corpora into the deterministic per-corpus agreement-tally JSON
/// ([`AGREEMENT_TALLIES_PATH`]) `stage-export-agreement` consumes. Keyed by corpus
/// (a `BTreeMap`, so sorted), integer counts only — no `f64`, no re-grade. The corpus
/// lane is read from each `corpus.json` (a metadata read, not a re-grade of the cases).
pub(crate) fn agreement_tallies_json(
    root: &Path,
    by_corpus: &BTreeMap<String, Vec<ExternalComparison>>,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let external = root.join(EXTERNAL_ROOT);
    let mut records: BTreeMap<String, TallyRecord> = BTreeMap::new();
    for (corpus, comparisons) in by_corpus {
        let AgreementTally {
            corpus: _,
            cases,
            agree,
            corpus_only,
            dl_gap,
        } = agreement_tally(corpus, comparisons);
        let meta = gmeow_conformance::vendored::load_corpus_meta(
            &external.join(corpus).join("corpus.json"),
        )
        .map_err(|e| stage_err(&format!("load corpus.json lane for {corpus}: {e}")))?;
        records.insert(
            corpus.clone(),
            TallyRecord {
                lane: meta.lane.as_str().to_string(),
                cases,
                agree,
                corpus_only,
                dl_gap,
            },
        );
    }
    let mut json = serde_json::to_string_pretty(&records)
        .map_err(|e| stage_err(&format!("serialize agreement tallies: {e}")))?;
    json.push('\n');
    Ok(json.into_bytes())
}

/// Discover and grade every committed external case under `external`, sorted by
/// `(corpus, case)`. A case's *published* verdict comes from whichever source it
/// carries: a W3C `source/manifest.ttl` (`otest:`/`mf:` outcome) OR a TPTP
/// `source/problem.p` (`% SZS status` outcome). A case dir carrying neither — or a
/// TPTP source-only divergence case with no frozen `expected/verdicts.json` — has
/// no native/published pair to grade and is skipped (not a defect). A source that
/// is present but unparsable, or a verdicts file present but malformed, HARD-fails.
fn grade_external_cases(external: &Path) -> Result<Vec<GradedCase>, gmeow_errors::Diag> {
    let mut graded: Vec<GradedCase> = Vec::new();
    if !external.is_dir() {
        return Err(stage_err(&format!(
            "external corpus root {} is missing",
            external.display()
        )));
    }
    for corpus_dir in sorted_dirs(external)? {
        let corpus = dir_name(&corpus_dir)?;
        for case_dir in sorted_dirs(&corpus_dir)? {
            let case = dir_name(&case_dir)?;

            // OntoUML foundation-discipline cases (source/model.ttl) grade the fired
            // discipline set against the documented anti-pattern, not a consistency
            // verdict. An agreeing Lane-A case yields native == published (no row); a
            // real divergence folds as a gmeow:Finding.
            if case_dir.join("source").join("model.ttl").is_file() {
                if let Some(comparison) = ontouml_graded(&case_dir, &case)? {
                    graded.push(GradedCase {
                        corpus: corpus.clone(),
                        comparison,
                    });
                }
                continue;
            }

            let Some(published) = published_outcome(&case_dir)? else {
                // No published external verdict to grade against (README/fixture dir,
                // or a source-only divergence case) — nothing to compare, not a defect.
                continue;
            };
            // A published outcome with no frozen native verdict (e.g. a source-only
            // divergence case) has nothing to compare — skip rather than hard-fail.
            if !case_dir.join("expected").join("verdicts.json").is_file() {
                continue;
            }
            let (world, native) = native_verdict(&case_dir, &case)?;
            graded.push(GradedCase {
                corpus: corpus.clone(),
                comparison: ExternalComparison {
                    case,
                    world,
                    native,
                    published,
                },
            });
        }
    }
    Ok(graded)
}

/// Grade one OntoUML foundation-discipline case (`source/model.ttl`) into an
/// [`ExternalComparison`]. The published verdict is the model's documented
/// anti-pattern (or `"clean"` for a clean control); the native verdict is the fired
/// `logic:Discipline` set projected to the canonical comparison string — so an
/// agreeing Lane-A case has `native == published` (dropped as `Agree`), and a real
/// divergence folds as a `gmeow:Finding`.
///
/// Returns `None` for a source-only case with no frozen `expected/materialized.nq`
/// (nothing to grade — not a defect).
fn ontouml_graded(
    case_dir: &Path,
    case: &str,
) -> Result<Option<ExternalComparison>, gmeow_errors::Diag> {
    let materialized = case_dir.join("expected").join("materialized.nq");
    if !materialized.is_file() {
        return Ok(None);
    }
    // The documented anti-pattern is the verbatim provenance in profile.json; absent
    // for a clean control (the null hypothesis "clean").
    let documented = documented_antipattern(case_dir)?;
    let fired = fired_disciplines_in_golden(&materialized)?;
    let native =
        gmeow_conformance::external::ontouml::native_verdict_string(documented.as_deref(), &fired);
    let published = documented.unwrap_or_else(|| "clean".to_string());
    // The world scope: the single world in the frozen verdicts.json.
    let (world, _status) = native_verdict(case_dir, case)?;
    Ok(Some(ExternalComparison {
        case: case.to_string(),
        world,
        native,
        published,
    }))
}

/// The verbatim `documented_antipattern` provenance in a case's `profile.json`, or
/// `None` when the key is absent (a clean control). A malformed profile HARD-fails.
fn documented_antipattern(case_dir: &Path) -> Result<Option<String>, gmeow_errors::Diag> {
    let path = case_dir.join("profile.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| stage_err(&format!("read {}: {e}", path.display())))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| stage_err(&format!("parse {}: {e}", path.display())))?;
    match value.get("documented_antipattern") {
        None => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(stage_err(&format!(
            "{}: documented_antipattern must be a string",
            path.display()
        ))),
    }
}

/// The fired `logic:Discipline` local names in a committed `expected/materialized.nq`
/// (`<s> <logic:violation> <logic:{Discipline}> <g> .`).
fn fired_disciplines_in_golden(
    materialized: &Path,
) -> Result<std::collections::BTreeSet<String>, gmeow_errors::Diag> {
    const VIOLATION: &str = "<https://blackcatinformatics.ca/logic/violation>";
    const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
    let text = std::fs::read_to_string(materialized)
        .map_err(|e| stage_err(&format!("read {}: {e}", materialized.display())))?;
    let mut fired = std::collections::BTreeSet::new();
    for line in text.lines() {
        let mut toks = line.split_whitespace();
        let (_s, p, o) = (toks.next(), toks.next(), toks.next());
        if p != Some(VIOLATION) {
            continue;
        }
        if let Some(obj) = o {
            let iri = obj.trim_start_matches('<').trim_end_matches('>');
            if let Some(local) = iri.strip_prefix(LOGIC_NS) {
                fired.insert(local.to_owned());
            }
        }
    }
    Ok(fired)
}

/// The published external verdict for a case, from whichever committed source it
/// carries: a W3C `source/manifest.ttl` or a TPTP `source/problem.p`. Returns
/// `None` when the case carries neither recognized source.
fn published_outcome(case_dir: &Path) -> Result<Option<String>, gmeow_errors::Diag> {
    let manifest_path = case_dir.join("source").join("manifest.ttl");
    if manifest_path.is_file() {
        return Ok(Some(published_verdict(&manifest_path)?));
    }
    let szs_path = case_dir.join("source").join("problem.p");
    if szs_path.is_file() {
        let text = std::fs::read_to_string(&szs_path)
            .map_err(|e| stage_err(&format!("read {}: {e}", szs_path.display())))?;
        let outcome = outcome_from_szs(&text)
            .map_err(|e| stage_err(&format!("parse SZS {}: {e}", szs_path.display())))?;
        return Ok(Some(outcome.verdict_status().as_str().to_string()));
    }
    Ok(None)
}

/// The published external verdict: parse the case's `source/manifest.ttl` (the
/// committed W3C `otest:`/`mf:` declaration) and return its lowercase verdict token.
///
/// The manifest carries exactly one recognized test entry per committed case; a
/// parse failure, a zero-entry manifest, or a multi-entry manifest all HARD-fail
/// (the committed corpus is a single-test-per-case surface).
fn published_verdict(manifest_path: &Path) -> Result<String, gmeow_errors::Diag> {
    let text = std::fs::read_to_string(manifest_path)
        .map_err(|e| stage_err(&format!("read {}: {e}", manifest_path.display())))?;
    let abs = std::path::absolute(manifest_path)
        .map_err(|e| stage_err(&format!("resolve {}: {e}", manifest_path.display())))?;
    let base = format!("file://{}", abs.display());
    let entries = parse_test_manifest(&text, Some(&base))
        .map_err(|e| stage_err(&format!("parse manifest {}: {e}", manifest_path.display())))?;
    let [entry] = entries.as_slice() else {
        return Err(stage_err(&format!(
            "manifest {} carries {} recognized test entries; a committed external case is a single-test surface",
            manifest_path.display(),
            entries.len()
        )));
    };
    Ok(entry.outcome().verdict_status().as_str().to_string())
}

/// The native verdict: read the case's frozen `expected/verdicts.json` (the
/// per-world status the native DL consistency path decided at vendor time) and
/// return its single `(world, status)` pair.
///
/// `expected/verdicts.json` is a `{ "<world-iri>": { "status": "<token>", … } }`
/// object. A committed external case scopes its premises to exactly one world, so a
/// missing file, a parse failure, a zero-world object, a multi-world object, or a
/// missing/non-string `status` all HARD-fail (no silent skip).
fn native_verdict(case_dir: &Path, case: &str) -> Result<(String, String), gmeow_errors::Diag> {
    let path = case_dir.join("expected").join("verdicts.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| stage_err(&format!("read {}: {e}", path.display())))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| stage_err(&format!("parse {}: {e}", path.display())))?;
    let obj = value.as_object().ok_or_else(|| {
        stage_err(&format!(
            "{} is not a JSON object of world→verdict",
            path.display()
        ))
    })?;
    let [(world, world_verdict)] = obj.iter().collect::<Vec<_>>()[..] else {
        return Err(stage_err(&format!(
            "{} carries {} worlds; case {case} must scope to exactly one world",
            path.display(),
            obj.len()
        )));
    };
    let status = world_verdict
        .get("status")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            stage_err(&format!(
                "{} world {world} has no string \"status\"",
                path.display()
            ))
        })?;
    Ok((world.clone(), status.to_string()))
}

/// The immediate subdirectories of `dir`, sorted by path.
fn sorted_dirs(dir: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
    let mut out: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir)
        .map_err(|e| stage_err(&format!("read_dir {}: {e}", dir.display())))?
    {
        let path = entry
            .map_err(|e| stage_err(&format!("read_dir entry under {}: {e}", dir.display())))?
            .path();
        if path.is_dir() {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// The final path component as an owned `String` (HARD-fails on a non-UTF-8 name).
fn dir_name(dir: &Path) -> Result<String, gmeow_errors::Diag> {
    dir.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .ok_or_else(|| stage_err(&format!("directory {} has no UTF-8 name", dir.display())))
}

fn stage_err(message: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-conformance".to_string(),
        message: message.to_string(),
    })
}

// ── Stage impl ───────────────────────────────────────────────────────────────────

/// The `stage-conformance` Transform stage: grades the committed external corpus
/// (native frozen verdict vs published external verdict) and emits the divergences
/// as the in-memory `graph/conformance` N-Quads product
/// [`crate::stages::carrier`] folds into `gmeow.gts`. It consumes no upstream
/// product — it reads the committed corpus directly.
pub struct ConformanceStage;

impl Stage for ConformanceStage {
    fn id(&self) -> &str {
        "stage-conformance"
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    /// The named graphs this stage attaches to the carrier (its delta), from the
    /// single Rust-side attach table; mirrored by the slice module.ttl gmeow:attachesGraph
    /// declarations and verified against the run-time delta by the scheduler.
    fn attaches_graphs(&self) -> &[String] {
        crate::stages::attach::graphs(self.id())
    }
    /// The blob-representation lanes this stage attaches (its delta), from the single
    /// Rust-side attach table; mirrored by gmeow:attachesBlobRep and run-time-verified.
    fn attaches_blob_reps(&self) -> &[String] {
        crate::stages::attach::blob_reps(self.id())
    }
    fn impl_version(&self) -> &str {
        "conformance.v1"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        // The committed external corpus is a raw source read: every case's
        // published source (W3C `source/manifest.ttl`, TPTP `source/problem.p`, or
        // OntoUML `source/model.ttl`) plus its frozen native verdict busts this
        // stage's cache (a corpus edit re-grades + re-folds the bundle).
        let external = root.join(EXTERNAL_ROOT);
        let mut files: Vec<PathBuf> = Vec::new();
        if external.is_dir() {
            for corpus_dir in sorted_dirs(&external)? {
                for case_dir in sorted_dirs(&corpus_dir)? {
                    let manifest = case_dir.join("source").join("manifest.ttl");
                    let szs = case_dir.join("source").join("problem.p");
                    let model = case_dir.join("source").join("model.ttl");
                    let verdicts = case_dir.join("expected").join("verdicts.json");
                    if manifest.is_file() {
                        files.push(manifest);
                    } else if szs.is_file() {
                        files.push(szs);
                    } else if model.is_file() {
                        // An OntoUML foundation-discipline case grades `source/model.ttl`
                        // against its documented anti-pattern. The model, its frozen
                        // `expected/materialized.nq` golden (absent for a source-only
                        // divergence case), and the `profile.json` provenance all bust
                        // the cache — without them a case edit would leave a stale fold.
                        files.push(model);
                        let materialized = case_dir.join("expected").join("materialized.nq");
                        if materialized.is_file() {
                            files.push(materialized);
                        }
                        let profile = case_dir.join("profile.json");
                        if profile.is_file() {
                            files.push(profile);
                        }
                        continue;
                    } else {
                        continue;
                    }
                    if verdicts.is_file() {
                        files.push(verdicts);
                    }
                }
            }
        }
        files.sort();
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // Grade the committed corpus ONCE; both projections read this single result
        // (PIPELINE_SPINE §3.2/§8 — no re-walk, no re-grade).
        let by_corpus = grade_external_corpora(input.root)?;
        let nq = divergence_nq_from_corpora(&by_corpus);
        let tallies = agreement_tallies_json(input.root, &by_corpus)?;
        // Attach the conformance graph (divergence findings + reified comparison and
        // tally individuals) as the carrier's `graph/conformance` named graph so the
        // presenter reads it as a pure keyed fold (PIPELINE_SPINE §4), never re-parses the
        // byte artifact. Every graded comparison now folds, so the graph is non-empty
        // whenever the committed corpus grades anything. The byte lane is kept for readers.
        let dataset = crate::stages::carrier::parse_into_graph(
            &nq,
            "application/n-quads",
            crate::stages::carrier::GRAPH_CONFORMANCE,
        )?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(CONFORMANCE_NQ_PATH.to_string(), nq);
        // The single graded result, attached for `stage-export-agreement` to project
        // into the benchmark dashboard — never written to disk (`pipeline/` prefix).
        artifacts.insert(AGREEMENT_TALLIES_PATH.to_string(), tallies);
        Ok(StageOutput::new(StageProduct::from_artifacts_over(
            self.id(),
            dataset,
            artifacts,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn agreement_tallies_are_deterministic_sorted_and_cover_lane_a() {
        // The single grade feeds a deterministic, sorted tally JSON. Every committed
        // Lane-A corpus must appear with cases == agree + corpus_only + dl_gap.
        let root = repo_root();
        let by_corpus = grade_external_corpora(&root).expect("grade");
        let a = agreement_tallies_json(&root, &by_corpus).expect("tallies a");
        let b = agreement_tallies_json(&root, &by_corpus).expect("tallies b");
        assert_eq!(a, b, "agreement tallies must be deterministic");

        let records: BTreeMap<String, TallyRecord> =
            serde_json::from_slice(&a).expect("tally JSON parses");
        assert!(
            !records.is_empty(),
            "the committed corpus must grade something"
        );
        for (corpus, r) in &records {
            assert_eq!(
                r.cases,
                r.agree + r.corpus_only + r.dl_gap,
                "corpus {corpus}: cases must partition into agree/corpus-only/dl-gap"
            );
            assert!(
                matches!(r.lane.as_str(), "a" | "b" | "divergence"),
                "corpus {corpus}: lane must be a recognized token, got {:?}",
                r.lane
            );
        }
        // Sorted keys: the serialized order must equal the BTreeMap key order.
        let keys: Vec<&String> = records.keys().collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "tally JSON keys must be sorted");
    }

    #[test]
    fn grades_committed_corpus_deterministically() {
        let root = repo_root();
        let a = build_conformance_divergence(&root).expect("grade run a");
        let b = build_conformance_divergence(&root).expect("grade run b");
        assert_eq!(a, b, "divergence fold must be deterministic");
        // Every emitted quad lands in the conformance graph (never elsewhere).
        let text = String::from_utf8(a).expect("utf-8");
        for line in text.lines() {
            assert!(
                line.ends_with(&format!(
                    "<{}> .",
                    gmeow_conformance::divergence::CONFORMANCE_GRAPH
                )),
                "line not in the conformance graph: {line}"
            );
        }
    }

    #[test]
    fn tptp_problem_divergence_folds_into_a_conformance_finding() {
        // Proves the `source/problem.p` (SZS) grading path AND the divergence fold
        // end-to-end: a TPTP case whose published SZS ground truth disagrees with the
        // frozen native verdict surfaces as a `gmeow:Finding` in the conformance graph
        // — never silently agreed away. The committed `tptp-mini` cases agree by
        // construction, so this synthetic case is the only always-on exercise of the
        // problem.p dispatch's divergence branch.
        let tmp = tempfile::tempdir().expect("tempdir");
        let ext = tmp.path().join("external");
        let case = ext.join("tptp-fold-probe").join("case1");
        std::fs::create_dir_all(case.join("source")).unwrap();
        std::fs::create_dir_all(case.join("expected")).unwrap();
        // Published SZS ground truth: Unsatisfiable → the runner's `inconsistent` bucket.
        std::fs::write(
            case.join("source").join("problem.p"),
            "% SZS status Unsatisfiable for fold-probe\nfof(a, axiom, p(x)).\n",
        )
        .unwrap();
        // Frozen native verdict: the EL/DL fragment could not decide it (an honest gap).
        let world = "https://gmeow.example/tptp-fold-probe/case1/w";
        std::fs::write(
            case.join("expected").join("verdicts.json"),
            format!("{{ \"{world}\": {{ \"status\": \"incomplete\" }} }}"),
        )
        .unwrap();

        // The problem.p dispatch grades the case (published from SZS, native from the
        // frozen verdict) — it is NOT skipped.
        let graded = grade_external_cases(&ext).expect("grade");
        let [g] = graded.as_slice() else {
            panic!(
                "expected exactly one graded TPTP case, got {}",
                graded.len()
            );
        };
        assert_eq!(
            g.comparison.published, "inconsistent",
            "SZS Unsatisfiable projects to the inconsistent bucket"
        );
        assert_eq!(
            g.comparison.native, "incomplete",
            "the frozen native verdict is threaded through the problem.p path"
        );

        // The divergence folds into a gmeow:Finding, and every quad rides the
        // conformance graph.
        let nq = emit_divergence_nq(&g.corpus, std::slice::from_ref(&g.comparison));
        assert!(
            !nq.trim().is_empty(),
            "a native↔published divergence must emit at least one quad"
        );
        assert!(
            nq.contains("gmeow"),
            "the fold must emit a gmeow:Finding, got: {nq}"
        );
        for line in nq.lines() {
            assert!(
                line.ends_with(&format!(
                    "<{}> .",
                    gmeow_conformance::divergence::CONFORMANCE_GRAPH
                )),
                "divergence quad not in the conformance graph: {line}"
            );
        }
    }

    #[test]
    fn ontouml_model_divergence_folds_into_a_conformance_finding() {
        // Proves the `source/model.ttl` (OntoUML foundation-discipline) grading path
        // AND the divergence fold end-to-end: a case whose documented anti-pattern the
        // frozen native disciplines did NOT reproduce surfaces as a `gmeow:Finding` in
        // the conformance graph — never silently agreed away. The committed
        // `ontouml-mini` cases agree by construction, so this synthetic case is the only
        // always-on exercise of the model.ttl dispatch's divergence branch.
        let tmp = tempfile::tempdir().expect("tempdir");
        let ext = tmp.path().join("external");
        let case = ext.join("ontouml-fold-probe").join("case1");
        std::fs::create_dir_all(case.join("source")).unwrap();
        std::fs::create_dir_all(case.join("expected")).unwrap();
        // Marks the case as an OntoUML case (content is not re-parsed by the fold).
        std::fs::write(case.join("source").join("model.ttl"), "# probe\n").unwrap();
        // Documented anti-pattern the disciplines were expected to reproduce.
        std::fs::write(
            case.join("profile.json"),
            "{ \"documented_antipattern\": \"RelComp\" }",
        )
        .unwrap();
        // Frozen native materialization fires a DIFFERENT discipline (FreeRole), so the
        // documented RelComp was not reproduced — a genuine divergence.
        let world = "https://gmeow.example/ontouml-fold-probe/case1/w";
        std::fs::write(
            case.join("expected").join("materialized.nq"),
            format!(
                "<{world}#C> <https://blackcatinformatics.ca/logic/violation> \
                 <https://blackcatinformatics.ca/logic/FreeRole> <{world}> .\n"
            ),
        )
        .unwrap();
        std::fs::write(
            case.join("expected").join("verdicts.json"),
            format!("{{ \"{world}\": {{ \"status\": \"consistent\" }} }}"),
        )
        .unwrap();

        let graded = grade_external_cases(&ext).expect("grade");
        let [g] = graded.as_slice() else {
            panic!(
                "expected exactly one graded OntoUML case, got {}",
                graded.len()
            );
        };
        assert_eq!(
            g.comparison.published, "RelComp",
            "the documented anti-pattern is the published verdict"
        );
        assert_eq!(
            g.comparison.native, "FreeRole",
            "the fired discipline set (not containing RelComp) is the native verdict"
        );

        let nq = emit_divergence_nq(&g.corpus, std::slice::from_ref(&g.comparison));
        assert!(
            !nq.trim().is_empty(),
            "a documented↔fired divergence must emit at least one quad"
        );
        assert!(
            nq.contains("gmeow"),
            "the fold must emit a gmeow:Finding, got: {nq}"
        );
        for line in nq.lines() {
            assert!(
                line.ends_with(&format!(
                    "<{}> .",
                    gmeow_conformance::divergence::CONFORMANCE_GRAPH
                )),
                "divergence quad not in the conformance graph: {line}"
            );
        }
    }

    #[test]
    fn input_files_busts_cache_on_ontouml_case_files() {
        // An OntoUML case carries neither `source/manifest.ttl` nor `source/problem.p`,
        // so it must be caught by the `source/model.ttl` branch — otherwise the case is
        // dropped from the cache key and a `model.ttl` / `materialized.nq` / `profile.json`
        // edit would leave a stale `gmeow.gts` fold that the semantic drift gate cannot
        // see (both sides agree on the stale value). Regression guard for that omission.
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let case = root
            .join(EXTERNAL_ROOT)
            .join("ontouml-mini")
            .join("some-case");
        std::fs::create_dir_all(case.join("source")).unwrap();
        std::fs::create_dir_all(case.join("expected")).unwrap();
        let model = case.join("source").join("model.ttl");
        let materialized = case.join("expected").join("materialized.nq");
        let profile = case.join("profile.json");
        std::fs::write(&model, "# model\n").unwrap();
        std::fs::write(&materialized, "# golden\n").unwrap();
        std::fs::write(&profile, "{}\n").unwrap();

        let files = ConformanceStage.input_files(root).expect("input_files");
        for want in [&model, &materialized, &profile] {
            assert!(
                files.contains(want),
                "cache key must include {} (else an OntoUML case edit leaves a stale fold), got {files:?}",
                want.display()
            );
        }
    }
}
