// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The executable acceptance surface for the external-corpora dogfooding work.
//!
//! Six falsifiable gates, each reading the SHIPPED artifact where specified rather
//! than the DSL:
//!
//! 1. **NO-DRIFT** (both Rust surfaces) — the shipped `logic:SzsStatus` table in
//!    `gmeow.gts` and the shipped `gmeow-conformance-corpus.sssom.tsv` rows must
//!    reproduce, respectively, `status.rs::outcome_for_szs` and
//!    `manifest.rs::ManifestTestKind::outcome`, exactly (and over the whole 9-token
//!    SZS domain). Binds BOTH Rust tables to their shipped projections.
//! 2. **GRAPH-ISOLATION** — no production graph in `gmeow.gts` `owl:imports` the
//!    conformance graph, and every folded conformance individual lives in the
//!    `graph/conformance` named graph (never the default/authored graph).
//! 3. **NON-EMPTY** (per corpus) — every graded corpus under
//!    `conformance/logic/cases/external/<corpus>/` contributes ≥1 reified
//!    `gmeow:ConformanceComparison` and ≥1 `gmeow:CorpusAgreementTally` in the
//!    shipped conformance graph.
//! 4. **CORROBORATION** — an all-agree ledger folds ONLY non-blocking
//!    `FindingCategory::Corroboration` findings (never `IncompleteCheck`), and that
//!    category is `Blocking::Coherent` (non-gating).
//! 5. **LATTICE-DERIVATION** — each reified comparison's `comparisonLatticeRelation`
//!    is the RDF image of its `DivergenceKind`: `VerdictEquivalent`⟺Agree,
//!    `VerdictWeaker`⟺DlGap, `VerdictIncomparable`⟺CorpusOnly.
//! 6. **FATAL-REGRESSION** — `CorpusOnly` / `DlGap` still grade to a
//!    BLOCKING category (`gate()` == Fatal) while `Agree` does not, so
//!    Task 4's every-comparison fold did not weaken the soundness gate.
//!
//! Gates 4/5/6 drive the real Rust emitter (`divergence_findings` /
//! `emit_divergence_nq`) and the real gate morphism (`gmeow_errors::grade::gate`),
//! so they PASS with no regeneration. Gates 1/2/3 read the SHIPPED generated
//! artifacts (`gmeow.gts`, `functions.fno.ttl`, `gmeow-conformance-corpus.sssom.tsv`);
//! until `make check` re-mints those artifacts with the Task 1–4 individuals,
//! they FAIL with a CLEAN drift / empty report (never a panic or parse crash) and
//! pass post-regenerate.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_conformance::divergence::{CONFORMANCE_GRAPH, emit_divergence_nq};
use gmeow_conformance::external::{ManifestTestKind, outcome_for_szs};
use gmeow_conformance::paths::repo_root;
use gmeow_conformance::serialize::VerdictStatus;
use gmeow_errors::grade::{Blocking, GateVerdict, Grade, gate};
use gmeow_logic::reason::{
    ExternalComparison, build_ledger, compare_external_corpus, divergence_findings, dl_gap_rows,
};
use purrdf::{RdfDataset, TermRef};

// ── shared vocabulary constants ────────────────────────────────────────────────

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const OWL_IMPORTS: &str = "http://www.w3.org/2002/07/owl#imports";

/// The `logic:rawStatusToken` predicate (SZS individuals AND reified comparisons).
const RAW_STATUS_TOKEN: &str = "https://blackcatinformatics.ca/logic/rawStatusToken";
/// The `logic:projectsToVerdict` predicate — carried ONLY by the 9 `logic:SzsStatus`
/// individuals, so `(rawStatusToken, projectsToVerdict)` uniquely selects the table.
const PROJECTS_TO_VERDICT: &str = "https://blackcatinformatics.ca/logic/projectsToVerdict";
const COMPARISON_CORPUS: &str = "https://blackcatinformatics.ca/gmeow/comparisonCorpus";
const COMPARISON_NATIVE_VERDICT: &str =
    "https://blackcatinformatics.ca/gmeow/comparisonNativeVerdict";
const COMPARISON_LATTICE_RELATION: &str =
    "https://blackcatinformatics.ca/gmeow/comparisonLatticeRelation";
const TALLY_CORPUS: &str = "https://blackcatinformatics.ca/gmeow/tallyCorpus";
const CONFORMANCE_COMPARISON: &str = "https://blackcatinformatics.ca/gmeow/ConformanceComparison";
const CORPUS_AGREEMENT_TALLY: &str = "https://blackcatinformatics.ca/gmeow/CorpusAgreementTally";

/// The complete domain of `status.rs::outcome_for_szs` — the nine SZS tokens the
/// shipped `logic:SzsStatus` table MUST cover, no more and no fewer.
const SZS_DOMAIN: [&str; 9] = [
    "Theorem",
    "Unsatisfiable",
    "ContradictoryAxioms",
    "Satisfiable",
    "CounterSatisfiable",
    "Unknown",
    "GaveUp",
    "Timeout",
    "ResourceOut",
];

// ── shipped-artifact loaders ───────────────────────────────────────────────────

fn shipped_gts_path() -> PathBuf {
    repo_root().join("generated").join("dist").join("gmeow.gts")
}

fn functions_fno_path() -> PathBuf {
    repo_root()
        .join("generated")
        .join("projections")
        .join("functions.fno.ttl")
}

fn conformance_sssom_path() -> PathBuf {
    repo_root()
        .join("generated")
        .join("mappings")
        .join("gmeow-conformance-corpus.sssom.tsv")
}

fn external_corpora_root() -> PathBuf {
    repo_root()
        .join("conformance")
        .join("logic")
        .join("cases")
        .join("external")
}

/// Decode the SHIPPED `gmeow.gts` bundle into a named-graph-preserving dataset
/// (`dataset_from_gts_graph`, NOT the flattening variant — gates 2/3 read graph
/// names). A missing/corrupt bundle is a CLEAN, actionable panic, never an opaque
/// unwrap.
fn load_shipped_bundle() -> Arc<RdfDataset> {
    let path = shipped_gts_path();
    let bytes = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "SHIPPED bundle {} could not be read: {e} — run `make check`",
            path.display()
        )
    });
    let graph = purrdf::gts::read_all_segments(&bytes).unwrap_or_else(|e| {
        panic!(
            "gmeow.gts segment decode failed for {}: {e}",
            path.display()
        )
    });
    purrdf::gts::dataset_from_gts_graph(&graph).unwrap_or_else(|e| {
        panic!(
            "gmeow.gts named-graph fold failed for {}: {e}",
            path.display()
        )
    })
}

/// The bare local name of an IRI (after the last `#`, `/`, or `:`).
fn local_name(iri: &str) -> &str {
    iri.rsplit(['#', '/', ':']).next().unwrap_or(iri)
}

/// Map a shipped `logic:Conf*` verdict local name onto the runner
/// [`VerdictStatus`]. Returns `None` for an unrecognized local (drift).
fn conf_local_to_verdict_status(local: &str) -> Option<VerdictStatus> {
    match local {
        "ConfInconsistent" => Some(VerdictStatus::Inconsistent),
        "ConfConsistent" => Some(VerdictStatus::Consistent),
        "ConfIncomplete" => Some(VerdictStatus::Incomplete),
        _ => None,
    }
}

/// Classify a W3C manifest-kind IRI (full IRI or CURIE) into a [`ManifestTestKind`]
/// by its unambiguous local name. `InconsistencyTest` is tested before
/// `ConsistencyTest` (the former is not a case-sensitive suffix of the latter, but
/// the explicit order documents the intent).
fn manifest_kind_from_object(object: &str) -> Option<ManifestTestKind> {
    let local = local_name(object.trim_start_matches('<').trim_end_matches('>'));
    if local.ends_with("InconsistencyTest") {
        Some(ManifestTestKind::Inconsistency)
    } else if local.ends_with("ConsistencyTest") {
        Some(ManifestTestKind::Consistency)
    } else if local.ends_with("PositiveEntailment") {
        Some(ManifestTestKind::PositiveEntailment)
    } else if local.ends_with("NegativeEntailment") {
        Some(ManifestTestKind::NegativeEntailment)
    } else {
        None
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Gate 1 — NO-DRIFT (both Rust surfaces), reads the SHIPPED generated artifacts.
// ══════════════════════════════════════════════════════════════════════════════

/// Gate 1(a): the shipped `logic:SzsStatus` table in `gmeow.gts` must reproduce
/// `status.rs::outcome_for_szs` exactly, over exactly the nine-token domain — and
/// the shipped FnO catalog (`functions.fno.ttl`) must parse as a real generated
/// artifact. Binds `status.rs` to its shipped projection.
#[test]
fn no_drift_szs_table_matches_outcome_for_szs() {
    // The generated FnO catalog is a real shipped artifact: confirm it parses (the
    // SZS→verdict leg rides transforms.fno.ttl, merged into the same catalog; the
    // per-token verdict TABLE is authored as the projectsToVerdict edges checked
    // below, which is the load-bearing drift surface).
    let fno_path = functions_fno_path();
    let fno_text = std::fs::read_to_string(&fno_path).unwrap_or_else(|e| {
        panic!(
            "shipped FnO catalog {} could not be read: {e} — run `make check`",
            fno_path.display()
        )
    });
    let fno_ds =
        purrdf::parse_dataset(fno_text.as_bytes(), "text/turtle", None).unwrap_or_else(|e| {
            panic!(
                "shipped FnO catalog {} is not valid Turtle: {e}",
                fno_path.display()
            )
        });
    assert!(
        fno_ds.quad_refs().next().is_some(),
        "shipped FnO catalog {} parsed to zero triples",
        fno_path.display()
    );

    // Collect the shipped SZS table: subject → (rawStatusToken, projectsToVerdict-local).
    let ds = load_shipped_bundle();
    let mut raw_token: BTreeMap<String, String> = BTreeMap::new();
    let mut projects: BTreeMap<String, String> = BTreeMap::new();
    for q in ds.quad_refs() {
        let (TermRef::Iri(subj), TermRef::Iri(pred)) = (q.s, q.p) else {
            continue;
        };
        if pred == RAW_STATUS_TOKEN
            && let TermRef::Literal { lexical, .. } = q.o
        {
            raw_token.insert(subj.to_owned(), lexical.to_owned());
        } else if pred == PROJECTS_TO_VERDICT
            && let TermRef::Iri(v) = q.o
        {
            projects.insert(subj.to_owned(), local_name(v).to_owned());
        }
    }

    // The SZS individuals are exactly the subjects carrying BOTH edges.
    let mut shipped_tokens: BTreeSet<String> = BTreeSet::new();
    for (subj, verdict_local) in &projects {
        let token = raw_token.get(subj).unwrap_or_else(|| {
            panic!(
                "shipped SZS individual {subj} carries logic:projectsToVerdict but no \
                 logic:rawStatusToken — cannot bind it to outcome_for_szs (stale gmeow.gts? \
                 run `make check`)"
            )
        });
        let shipped = conf_local_to_verdict_status(verdict_local).unwrap_or_else(|| {
            panic!(
                "shipped SZS individual {subj} projects to unknown verdict {verdict_local:?} \
                 (expected one of ConfInconsistent/ConfConsistent/ConfIncomplete)"
            )
        });
        let native = outcome_for_szs(token)
            .unwrap_or_else(|e| {
                panic!("shipped SZS token {token:?} is outside outcome_for_szs: {e}")
            })
            .verdict_status();
        assert_eq!(
            shipped, native,
            "SZS table DRIFT for token {token:?}: gmeow.gts ships {shipped:?} but \
             status.rs::outcome_for_szs yields {native:?}"
        );
        assert!(
            shipped_tokens.insert(token.clone()),
            "shipped SZS token {token:?} appears twice in gmeow.gts"
        );
    }

    // Exact-domain: the shipped token set is precisely outcome_for_szs's 9-token domain.
    let expected: BTreeSet<String> = SZS_DOMAIN.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        shipped_tokens, expected,
        "SZS DOMAIN DRIFT: gmeow.gts ships {shipped_tokens:?} but outcome_for_szs's domain \
         is {expected:?} (stale gmeow.gts fails here until `make check`)"
    );
}

/// Gate 1(b): every row of the shipped `gmeow-conformance-corpus.sssom.tsv` must
/// reproduce `manifest.rs::ManifestTestKind::outcome()` — the row's object W3C IRI
/// parses to a kind whose outcome verdict equals the row's subject `logic:Conf*`
/// verdict. Binds `manifest.rs` to its shipped SSSOM projection.
#[test]
fn no_drift_conformance_sssom_matches_manifest_kind_outcome() {
    let path = conformance_sssom_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "shipped SSSOM {} could not be read: {e} — the eqConfCorpus cells project here; \
             run `make check`",
            path.display()
        )
    });

    // Parse the SSSOM TSV: `#`-comment preamble, then a tab-separated header, then rows.
    let mut lines = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty());
    let header = lines
        .next()
        .unwrap_or_else(|| panic!("shipped SSSOM {} has no header row", path.display()));
    let cols: Vec<&str> = header.split('\t').collect();
    let col = |name: &str| -> usize {
        cols.iter().position(|c| *c == name).unwrap_or_else(|| {
            panic!(
                "shipped SSSOM {} header {cols:?} lacks a {name:?} column",
                path.display()
            )
        })
    };
    let (subj_c, obj_c) = (col("subject_id"), col("object_id"));

    let mut kinds_seen: Vec<ManifestTestKind> = Vec::new();
    let mut rows = 0usize;
    for line in lines {
        let fields: Vec<&str> = line.split('\t').collect();
        let subject = fields.get(subj_c).copied().unwrap_or_else(|| {
            panic!(
                "shipped SSSOM {} row {line:?} has no subject_id field",
                path.display()
            )
        });
        let object = fields.get(obj_c).copied().unwrap_or_else(|| {
            panic!(
                "shipped SSSOM {} row {line:?} has no object_id field",
                path.display()
            )
        });

        let kind = manifest_kind_from_object(object).unwrap_or_else(|| {
            panic!(
                "shipped SSSOM {} row object {object:?} does not parse to a W3C manifest kind \
                 (PositiveEntailment/NegativeEntailment/InconsistencyTest/ConsistencyTest)",
                path.display()
            )
        });
        let subj_verdict = conf_local_to_verdict_status(local_name(subject)).unwrap_or_else(|| {
            panic!(
                "shipped SSSOM {} row subject {subject:?} is not a logic:Conf* verdict",
                path.display()
            )
        });
        assert_eq!(
            kind.outcome().verdict_status(),
            subj_verdict,
            "SSSOM DRIFT: row {subject} ↔ {object}: manifest kind {kind:?} outcomes to {:?} \
             but the row aligns it to {subj_verdict:?}",
            kind.outcome().verdict_status()
        );
        if !kinds_seen.contains(&kind) {
            kinds_seen.push(kind);
        }
        rows += 1;
    }

    assert!(
        rows >= 4,
        "shipped SSSOM {} has {rows} data rows; the four eqConfCorpus cells must all project \
         (stale/absent file fails here until `make check`)",
        path.display()
    );
    for kind in [
        ManifestTestKind::PositiveEntailment,
        ManifestTestKind::NegativeEntailment,
        ManifestTestKind::Inconsistency,
        ManifestTestKind::Consistency,
    ] {
        assert!(
            kinds_seen.contains(&kind),
            "shipped SSSOM {} is missing a row for manifest kind {kind:?}",
            path.display()
        );
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Gate 2 — GRAPH-ISOLATION, reads the SHIPPED bundle's graph structure.
// ══════════════════════════════════════════════════════════════════════════════

/// No production graph `owl:imports` the conformance graph, and every folded
/// conformance individual (`ConformanceComparison` / `CorpusAgreementTally`) lives
/// in the `graph/conformance` named graph — so folding agreements can never
/// contaminate the production `owl:imports` closure.
#[test]
fn graph_isolation_conformance_never_imported_and_lives_in_its_own_graph() {
    let ds = load_shipped_bundle();

    let mut comparison_or_tally_graphs: BTreeSet<String> = BTreeSet::new();
    let mut leaked: Vec<String> = Vec::new();
    let mut import_of_conformance = 0usize;

    for q in ds.quad_refs() {
        // (a) no owl:imports edge anywhere targets the conformance graph IRI.
        if let TermRef::Iri(pred) = q.p
            && pred == OWL_IMPORTS
            && let TermRef::Iri(obj) = q.o
            && obj == CONFORMANCE_GRAPH
        {
            import_of_conformance += 1;
        }

        // (b) every ConformanceComparison / CorpusAgreementTally type-quad rides the
        // conformance named graph, never the default/authored graph.
        if let TermRef::Iri(pred) = q.p
            && pred == RDF_TYPE
            && let TermRef::Iri(obj) = q.o
            && (obj == CONFORMANCE_COMPARISON || obj == CORPUS_AGREEMENT_TALLY)
        {
            match q.g {
                Some(TermRef::Iri(g)) => {
                    comparison_or_tally_graphs.insert(g.to_owned());
                    if g != CONFORMANCE_GRAPH {
                        leaked.push(format!("{} in graph {g}", local_name(obj)));
                    }
                }
                _ => leaked.push(format!("{} in the DEFAULT graph", local_name(obj))),
            }
        }
    }

    assert_eq!(
        import_of_conformance, 0,
        "GRAPH-ISOLATION VIOLATION: {import_of_conformance} owl:imports edge(s) target the \
         conformance graph <{CONFORMANCE_GRAPH}> — folded conformance evidence must never enter \
         the production import closure"
    );
    assert!(
        leaked.is_empty(),
        "GRAPH-ISOLATION VIOLATION: conformance individuals escaped the conformance graph: {leaked:?}"
    );
    assert!(
        comparison_or_tally_graphs.contains(CONFORMANCE_GRAPH),
        "no ConformanceComparison/CorpusAgreementTally individual found in the conformance graph \
         <{CONFORMANCE_GRAPH}> of the shipped bundle (stale gmeow.gts fails here until \
         `make check`; observed graphs: {comparison_or_tally_graphs:?})"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Gate 3 — NON-EMPTY per corpus, reads the SHIPPED conformance graph.
// ══════════════════════════════════════════════════════════════════════════════

/// How a committed external corpus directory grades (mirrors the pipeline's
/// `stage-conformance` dispatch minimally, to decide EXPECTED non-emptiness).
enum Gradeability {
    /// Verdict-token corpus (W3C manifest / TPTP SZS): its comparisons carry a
    /// `gmeow:comparisonNativeVerdict` in the `logic:Conf*` lattice.
    Verdict,
    /// OntoUML foundation-discipline corpus: comparisons carry a discipline-label
    /// `rawStatusToken`, NOT a `logic:Conf*` native verdict.
    Ontouml,
    /// Nothing to grade (source-only divergence corpus, empty dir, README) — the
    /// bundle legitimately carries no individuals for it; the gate logs the skip.
    None,
}

/// Classify a corpus directory by scanning its case tree once. OntoUML (a
/// `materialized.nq` golden) is checked FIRST because its cases also carry a
/// `verdicts.json`.
fn corpus_gradeability(corpus_dir: &Path) -> Gradeability {
    let mut any_materialized = false;
    let mut any_verdict = false;
    let mut any_published = false;
    let walk = walk_files(corpus_dir);
    for f in &walk {
        match f.file_name().and_then(|n| n.to_str()) {
            Some("materialized.nq") => any_materialized = true,
            Some("verdicts.json") => any_verdict = true,
            Some("manifest.ttl") | Some("problem.p") => any_published = true,
            _ => {}
        }
    }
    if any_materialized {
        Gradeability::Ontouml
    } else if any_verdict && any_published {
        Gradeability::Verdict
    } else {
        Gradeability::None
    }
}

/// Every regular file under `dir`, recursively (deterministic order not required —
/// the caller only tests presence).
///
/// A missing directory yields the empty set (an ungraded corpus legitimately has no
/// case tree); any OTHER I/O error — a permission fault, an unreadable entry — is a
/// HARD FAIL, never a silent empty. Swallowing it would let a corpus grade `None`
/// (skipped) because its tree was *inaccessible* rather than *absent*, inverting the
/// non-empty gate into "non-empty-or-unreadable".
fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return out,
        Err(err) => panic!("read_dir({}) failed: {err}", dir.display()),
    };
    for entry in entries {
        let entry = entry
            .unwrap_or_else(|err| panic!("read_dir entry under {} failed: {err}", dir.display()));
        let file_type = entry
            .file_type()
            .unwrap_or_else(|err| panic!("file_type({}) failed: {err}", entry.path().display()));
        if file_type.is_dir() {
            out.extend(walk_files(&entry.path()));
        } else {
            out.push(entry.path());
        }
    }
    out
}

#[test]
fn non_empty_conformance_graph_per_graded_corpus() {
    let ds = load_shipped_bundle();

    // Index the shipped conformance graph: reified comparisons and tallies, keyed by
    // their carried corpus name — and ONLY quads that ride the conformance graph.
    // subject → corpus (comparisons)
    let mut comparison_corpus: BTreeMap<String, String> = BTreeMap::new();
    // subject → true when the comparison carries a Conf* native verdict
    let mut comparison_has_conf_native: BTreeMap<String, bool> = BTreeMap::new();
    // subject → true when the comparison carries rawStatusToken
    let mut comparison_has_raw: BTreeMap<String, bool> = BTreeMap::new();
    // corpora carrying ≥1 CorpusAgreementTally
    let mut tally_corpora: BTreeSet<String> = BTreeSet::new();

    for q in ds.quad_refs() {
        // Restrict to the conformance graph so we never miscount a stray default-graph quad.
        if !matches!(q.g, Some(TermRef::Iri(g)) if g == CONFORMANCE_GRAPH) {
            continue;
        }
        let (TermRef::Iri(subj), TermRef::Iri(pred)) = (q.s, q.p) else {
            continue;
        };
        match pred {
            COMPARISON_CORPUS => {
                if let TermRef::Literal { lexical, .. } = q.o {
                    comparison_corpus.insert(subj.to_owned(), lexical.to_owned());
                }
            }
            COMPARISON_NATIVE_VERDICT => {
                if let TermRef::Iri(v) = q.o {
                    let is_conf = conf_local_to_verdict_status(local_name(v)).is_some();
                    comparison_has_conf_native.insert(subj.to_owned(), is_conf);
                }
            }
            RAW_STATUS_TOKEN => {
                comparison_has_raw.insert(subj.to_owned(), true);
            }
            TALLY_CORPUS => {
                if let TermRef::Literal { lexical, .. } = q.o {
                    tally_corpora.insert(lexical.to_owned());
                }
            }
            _ => {}
        }
    }

    // Aggregate per-corpus comparison evidence.
    let mut corpus_has_comparison: BTreeSet<String> = BTreeSet::new();
    let mut corpus_has_conf_comparison_with_raw: BTreeSet<String> = BTreeSet::new();
    for (subj, corpus) in &comparison_corpus {
        corpus_has_comparison.insert(corpus.clone());
        let conf = *comparison_has_conf_native.get(subj).unwrap_or(&false);
        let raw = *comparison_has_raw.get(subj).unwrap_or(&false);
        if conf && raw {
            corpus_has_conf_comparison_with_raw.insert(corpus.clone());
        }
    }

    // Walk the on-disk corpus directories and assert per-corpus non-emptiness.
    let root = external_corpora_root();
    let mut graded_any = false;
    let mut skipped: Vec<String> = Vec::new();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("external corpus root {} unreadable: {e}", root.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    for corpus_dir in dirs {
        let corpus = corpus_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| panic!("corpus dir {} has no UTF-8 name", corpus_dir.display()))
            .to_owned();
        match corpus_gradeability(&corpus_dir) {
            Gradeability::None => {
                // Log the skip — never silent (a source-only divergence corpus, an empty
                // dir, or README carry nothing to fold).
                skipped.push(corpus);
                continue;
            }
            Gradeability::Verdict => {
                graded_any = true;
                assert!(
                    corpus_has_conf_comparison_with_raw.contains(&corpus),
                    "NON-EMPTY: verdict corpus {corpus:?} has ≥1 gradeable case but the shipped \
                     conformance graph carries no ConformanceComparison for it with a Conf* \
                     comparisonNativeVerdict AND a rawStatusToken (stale gmeow.gts fails here \
                     until `make check`)"
                );
            }
            Gradeability::Ontouml => {
                graded_any = true;
                assert!(
                    corpus_has_comparison.contains(&corpus),
                    "NON-EMPTY: OntoUML corpus {corpus:?} has ≥1 gradeable case but the shipped \
                     conformance graph carries no ConformanceComparison for it (stale gmeow.gts \
                     fails here until `make check`)"
                );
            }
        }
        assert!(
            tally_corpora.contains(&corpus),
            "NON-EMPTY: corpus {corpus:?} grades cases but the shipped conformance graph carries \
             no CorpusAgreementTally for it (stale gmeow.gts fails here until `make check`)"
        );
    }

    assert!(
        graded_any,
        "no external corpus classified as gradeable — the committed corpus tree under {} is \
         missing or empty",
        root.display()
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Gate 4 — CORROBORATION (drives the real emitter over an all-agree ledger).
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn corroboration_agreements_fold_non_blocking_corroboration_findings() {
    // The category is Coherent (non-gating) by construction.
    assert_eq!(
        gmeow_errors::FindingCategory::Corroboration.blocking(),
        Blocking::Coherent,
        "FindingCategory::Corroboration must be Coherent (non-gating)"
    );

    // Drive the REAL emitter over an all-agree ledger — not a hand-built finding.
    let comparisons = [
        cmp("consistency/open", "w", "consistent", "consistent"),
        cmp("theorem/holds", "w", "inconsistent", "inconsistent"),
        cmp("beyond/decided", "w", "consistent", "consistent"),
    ];
    let rows = compare_external_corpus("w3c-owl2-el", &comparisons);
    let ledger = build_ledger(Vec::new(), Vec::new(), rows);
    let findings = divergence_findings(&ledger);

    assert_eq!(
        findings.len(),
        comparisons.len(),
        "every agreement folds as exactly one corroboration finding: {findings:?}"
    );
    for f in &findings {
        assert_eq!(
            f.category,
            Some(gmeow_errors::FindingCategory::Corroboration),
            "an all-agree row must fold as FindingCategory::Corroboration, got {:?}",
            f.category
        );
        assert_ne!(
            f.category,
            Some(gmeow_errors::FindingCategory::IncompleteCheck),
            "an agreement is corroboration, NOT an incomplete check"
        );
        assert_eq!(
            f.code, "reason.divergence.agreement",
            "the corroboration finding carries the agreement code"
        );
        // It can never gate: Coherent category ⇒ gate() == Collected whatever the axes.
        let grade = grade_of(f);
        assert_eq!(
            gate(grade),
            GateVerdict::Collected,
            "a corroboration finding must never gate: {grade:?}"
        );
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Gate 5 — LATTICE-DERIVATION (each comparison's relation ⟺ its DivergenceKind).
// ══════════════════════════════════════════════════════════════════════════════

/// Collect the object local names of `predicate` from an N-Quads document.
fn nq_object_locals(nquads: &str, predicate_iri: &str) -> Vec<String> {
    let pred_tok = format!("<{predicate_iri}>");
    let mut out = Vec::new();
    for line in nquads.lines() {
        let toks: Vec<&str> = line.split_whitespace().collect();
        // `<s> <p> <o> <g> .`
        if toks.len() >= 3 && toks[1] == pred_tok {
            let obj = toks[2].trim_start_matches('<').trim_end_matches('>');
            out.push(local_name(obj).to_owned());
        }
    }
    out
}

#[test]
fn lattice_relation_is_the_rdf_image_of_the_divergence_kind() {
    // Agree ⟺ VerdictEquivalent; the finding is the corroboration code.
    let agree = emit_divergence_nq("c", &[cmp("k", "w", "consistent", "consistent")]);
    assert_eq!(
        nq_object_locals(&agree, COMPARISON_LATTICE_RELATION),
        vec!["VerdictEquivalent".to_string()],
        "an Agree comparison must derive VerdictEquivalent: {agree}"
    );
    assert!(agree.contains("reason.divergence.agreement"));

    // DlGap (native incomplete) ⟺ VerdictWeaker.
    let gap = emit_divergence_nq("c", &[cmp("k", "w", "incomplete", "consistent")]);
    assert_eq!(
        nq_object_locals(&gap, COMPARISON_LATTICE_RELATION),
        vec!["VerdictWeaker".to_string()],
        "a DlGap comparison must derive VerdictWeaker: {gap}"
    );
    assert!(gap.contains("reason.divergence.dl-gap"));

    // CorpusOnly (decided-but-wrong) ⟺ VerdictIncomparable.
    let disagree = emit_divergence_nq("c", &[cmp("k", "w", "consistent", "inconsistent")]);
    assert_eq!(
        nq_object_locals(&disagree, COMPARISON_LATTICE_RELATION),
        vec!["VerdictIncomparable".to_string()],
        "a CorpusOnly comparison must derive VerdictIncomparable: {disagree}"
    );
    assert!(disagree.contains("reason.divergence.corpus-only"));

    // Over all three at once: exactly three comparison individuals, one relation each,
    // and the multiset of relations is precisely {Equivalent, Weaker, Incomparable}.
    let combined = emit_divergence_nq(
        "w3c-owl2-el",
        &[
            cmp("agree", "w", "consistent", "consistent"),
            cmp("gap", "w", "incomplete", "consistent"),
            cmp("disagree", "w", "consistent", "inconsistent"),
        ],
    );
    let comparison_types = combined
        .lines()
        .filter(|l| l.contains(&format!("<{CONFORMANCE_COMPARISON}>")))
        .count();
    assert_eq!(
        comparison_types, 3,
        "one reified comparison per input: {combined}"
    );
    let mut relations = nq_object_locals(&combined, COMPARISON_LATTICE_RELATION);
    relations.sort();
    assert_eq!(
        relations,
        vec![
            "VerdictEquivalent".to_string(),
            "VerdictIncomparable".to_string(),
            "VerdictWeaker".to_string(),
        ],
        "the three comparisons must derive one of each lattice relation: {combined}"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Gate 6 — FATAL-REGRESSION (Task 4 did not weaken the soundness gate).
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn fatal_regression_blocking_kinds_still_gate_and_agree_does_not() {
    // Build one ledger holding one row of every kind, then drive the REAL emitter +
    // the REAL gate() morphism over the resulting grades.
    let gaps = dl_gap_rows(&[gmeow_logic::reason::DlGap::new(
        "reason.dl-gap.union",
        "beyond EL",
    )]);
    let corpus = compare_external_corpus(
        "w3c-owl2-el",
        &[
            cmp("agree", "w", "consistent", "consistent"), // Agree
            cmp("wrong", "w", "consistent", "inconsistent"), // CorpusOnly
        ],
    );
    let ledger = build_ledger(Vec::new(), gaps, corpus);
    let findings = divergence_findings(&ledger);

    // Every kind's code must be present, so the gate is genuinely exercised.
    let by_code: BTreeMap<&str, GateVerdict> = findings
        .iter()
        .map(|f| (f.code.as_str(), gate(grade_of(f))))
        .collect();
    for code in [
        "reason.divergence.dl-gap",
        "reason.divergence.corpus-only",
        "reason.divergence.agreement",
    ] {
        assert!(
            by_code.contains_key(code),
            "the divergence emitter must produce a {code} finding for this ledger: {:?}",
            findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );
    }

    // The two soundness-failing kinds STILL gate Fatal (the Task-4 change is additive).
    for code in ["reason.divergence.dl-gap", "reason.divergence.corpus-only"] {
        assert_eq!(
            by_code[code],
            GateVerdict::Fatal,
            "FATAL-REGRESSION: {code} must still gate Fatal (blocking soundness kind)"
        );
    }
    // Agree is non-blocking and must NOT gate.
    assert_eq!(
        by_code["reason.divergence.agreement"],
        GateVerdict::Collected,
        "FATAL-REGRESSION: agreement must NOT gate (non-blocking corroboration)"
    );
}

// ── shared test helpers ────────────────────────────────────────────────────────

fn cmp(case: &str, world: &str, native: &str, published: &str) -> ExternalComparison {
    ExternalComparison {
        case: case.to_owned(),
        world: world.to_owned(),
        native: native.to_owned(),
        published: published.to_owned(),
    }
}

/// Reconstruct the diagnostic [`Grade`] a folded finding carries, so the real
/// `gate()` morphism can be driven over it. A folded divergence finding always
/// carries its category and standpoint (it flows through the DiagLedger).
fn grade_of(f: &gmeow_errors::Finding) -> Grade {
    Grade::new(
        f.severity,
        f.category
            .unwrap_or_else(|| panic!("folded divergence finding {} lacks a category", f.code)),
        f.standpoint
            .unwrap_or_else(|| panic!("folded divergence finding {} lacks a standpoint", f.code)),
    )
}
