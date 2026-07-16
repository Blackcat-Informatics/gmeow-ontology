// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `ingest-external` — the external-corpus ingestion CLI.
//!
//! It ingests a W3C `manifest.ttl` or a TPTP SZS/FOF problem and produces a runner
//! verdict, and is the reproducible refresh entry point the corpus vendoring and
//! Lane-B grading procedures follow.
//!
//! Usage:
//!
//! ```text
//! ingest-external --szs <problem.p> [--world <iri> --quads <n>]
//!     Parse a TPTP SZS problem and print the runner verdict. With --world/--quads,
//!     print the full world-indexed verdicts.json value; otherwise print the bare
//!     status (consistent | inconsistent | incomplete).
//!
//! ingest-external --manifest <manifest.ttl>
//!     Parse a W3C entailment manifest and print one `<name>\t<status>` line per
//!     mf:PositiveEntailment / mf:NegativeEntailment entry.
//!
//! ingest-external --vendor-el <input.rdf> <out-dir>
//!     Vendor the W3C OWL 2 EL conformance suite (ConsistencyTest /
//!     InconsistencyTest only). Agreeing deciders land in <out-dir> (the Lane-A
//!     corpus); every case the native reasoner cannot soundly decide (honest
//!     DlGap), or where it decides but disagrees with W3C (CorpusOnly), lands in
//!     the sibling <out-dir>-divergence corpus as committed data carrying BOTH the
//!     frozen native verdict and the W3C published verdict — never silently dropped.
//!
//! ingest-external --grade-suite <input.rdf> <corpus-name> <out.nq>
//!     Grade EVERY ConsistencyTest / InconsistencyTest in <input.rdf> gap-tolerantly
//!     against the native reasoner, record divergences (DlGap / CorpusOnly) as a
//!     gmeow:Finding N-Quads graph written to <out.nq>, and print a summary line.
//!     Entailment tests are counted but not graded (they need conclusion-negation).
//!
//! ingest-external --grade-ore <ontology-dir> <corpus-name> <out.nq>
//!     Grade every `*.owl` ontology under <ontology-dir> against the native DL
//!     consistency path for SOUNDNESS. The ORE 2015 corpus is curated-consistent and
//!     ships NO per-ontology reference verdict, so every ontology's published expected
//!     is `consistent`: a native `inconsistent` is a soundness flag (CorpusOnly →
//!     hard-fail), and any ontology the native path cannot parse (ORE ships OWL 2
//!     Functional Syntax, which the native RDF codecs do not read) or cannot decide
//!     (gaps non-empty) becomes an honest DlGap Finding — never a silent skip. The
//!     divergences are written as a gmeow:Finding N-Quads graph to <out.nq>.
//!
//! ingest-external --vendor-ontouml <corpus-dir>
//!     Regenerate the derived anatomy of a self-authored OntoUML Lane-A corpus from
//!     its `<slug>/source/model.ttl` files: lower each model onto the world-scoped
//!     `logic:` stereotype ABox, run the native foundation disciplines, and (re)write
//!     `profile.json`, the `input.logic.ttl` stub, the generated `input.nq`, and the
//!     blessed `expected/{materialized.nq,verdicts.json}`. Lane-A is agreeing-by-
//!     construction: a documented anti-pattern the disciplines do NOT reproduce, or a
//!     clean control that fires anything, belongs in the sibling -divergence corpus
//!     (a hard error), never Lane-A.
//!
//! ingest-external --grade-ontouml <catalog-dir> <corpus-name> <out.nq>
//!     Grade every `ontology.ttl` / `model.ttl` under <catalog-dir> against the native
//!     foundation disciplines gap-tolerantly. The live FAIR catalog ships no per-model
//!     documented anti-pattern, so the null hypothesis is `clean`: any fired discipline
//!     is surfaced as a CorpusOnly finding for review, any un-lowerable model is an
//!     honest DlGap capability gap, and each model's sibling `metadata.ttl` license is
//!     audited + disclosed (never a skip). Divergences are written as a gmeow:Finding
//!     N-Quads graph to <out.nq>.
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_conformance::external::lower::premise_ds_to_world_nquads;
use gmeow_conformance::external::tptp::{TptpError, lower_and_decide, parse_tptp};
use gmeow_conformance::external::{
    DisciplineVerdict, ExternalOutcome, ManifestTestKind, OntologyDoc, OntoumlError, compare,
    fired_disciplines, lower_and_evaluate, native_verdict_string, outcome_from_szs,
    parse_ontouml_model, parse_szs_status, parse_test_manifest, parse_test_manifest_rdfxml,
    runner_verdict_json,
};
use gmeow_conformance::run::RunnerQuad;
use gmeow_conformance::serialize::{
    VerdictStatus, build_verdicts, count_worlds, materialized_to_nquads,
};
use gmeow_license::{LicensePolicy, policy_for_license};
use gmeow_logic::foundation::{AntiRigidityPolicy, FoundationQuad};
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm, TermRef};

const USAGE: &str = "\
usage:
  ingest-external --szs <problem.p> [--world <iri> --quads <n>]
  ingest-external --manifest <manifest.ttl>
  ingest-external --vendor-el <input.rdf> <out-dir>
  ingest-external --vendor-full <input.rdf> <out-dir>
  ingest-external --vendor-tptp <corpus-dir>
  ingest-external --vendor-ontouml <corpus-dir>
  ingest-external --grade-suite <input.rdf> <corpus-name> <out.nq>
  ingest-external --grade-ore <ontology-dir> <corpus-name> <out.nq>
  ingest-external --grade-tptp <problem-dir> <corpus-name> <out.nq>
  ingest-external --grade-ontouml <catalog-dir> <corpus-name> <out.nq>";

/// Wrap a bin-local error message as a typed diagnostic on the substrate.
fn ce(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(gmeow_conformance::error::Vendor { detail })
}

fn main() -> gmeow_errors::Result<()> {
    let mut szs: Option<PathBuf> = None;
    let mut manifest: Option<PathBuf> = None;
    let mut vendor_el: Option<(PathBuf, PathBuf)> = None;
    let mut vendor_full: Option<(PathBuf, PathBuf)> = None;
    let mut vendor_entailment: Option<(PathBuf, PathBuf)> = None;
    let mut vendor_tptp: Option<PathBuf> = None;
    let mut vendor_ontouml: Option<PathBuf> = None;
    let mut grade_suite: Option<(PathBuf, String, PathBuf)> = None;
    let mut grade_ore: Option<(PathBuf, String, PathBuf)> = None;
    let mut grade_tptp: Option<(PathBuf, String, PathBuf)> = None;
    let mut grade_ontouml: Option<(PathBuf, String, PathBuf)> = None;
    let mut world: Option<String> = None;
    let mut quads: Option<u64> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--szs" => szs = Some(PathBuf::from(next(&mut args, "--szs")?)),
            "--manifest" => manifest = Some(PathBuf::from(next(&mut args, "--manifest")?)),
            "--vendor-el" => {
                let input = PathBuf::from(next(&mut args, "--vendor-el")?);
                let out = PathBuf::from(next(&mut args, "--vendor-el <out-dir>")?);
                vendor_el = Some((input, out));
            }
            "--vendor-full" => {
                let input = PathBuf::from(next(&mut args, "--vendor-full")?);
                let out = PathBuf::from(next(&mut args, "--vendor-full <out-dir>")?);
                vendor_full = Some((input, out));
            }
            "--vendor-entailment" => {
                let input = PathBuf::from(next(&mut args, "--vendor-entailment")?);
                let out = PathBuf::from(next(&mut args, "--vendor-entailment <out-dir>")?);
                vendor_entailment = Some((input, out));
            }
            "--vendor-tptp" => {
                vendor_tptp = Some(PathBuf::from(next(
                    &mut args,
                    "--vendor-tptp <corpus-dir>",
                )?));
            }
            "--vendor-ontouml" => {
                vendor_ontouml = Some(PathBuf::from(next(
                    &mut args,
                    "--vendor-ontouml <corpus-dir>",
                )?));
            }
            "--grade-suite" => {
                let input = PathBuf::from(next(&mut args, "--grade-suite")?);
                let corpus_name = next(&mut args, "--grade-suite <corpus-name>")?;
                let out_nq = PathBuf::from(next(&mut args, "--grade-suite <out.nq>")?);
                grade_suite = Some((input, corpus_name, out_nq));
            }
            "--grade-ore" => {
                let dir = PathBuf::from(next(&mut args, "--grade-ore")?);
                let corpus_name = next(&mut args, "--grade-ore <corpus-name>")?;
                let out_nq = PathBuf::from(next(&mut args, "--grade-ore <out.nq>")?);
                grade_ore = Some((dir, corpus_name, out_nq));
            }
            "--grade-tptp" => {
                let dir = PathBuf::from(next(&mut args, "--grade-tptp")?);
                let corpus_name = next(&mut args, "--grade-tptp <corpus-name>")?;
                let out_nq = PathBuf::from(next(&mut args, "--grade-tptp <out.nq>")?);
                grade_tptp = Some((dir, corpus_name, out_nq));
            }
            "--grade-ontouml" => {
                let dir = PathBuf::from(next(&mut args, "--grade-ontouml")?);
                let corpus_name = next(&mut args, "--grade-ontouml <corpus-name>")?;
                let out_nq = PathBuf::from(next(&mut args, "--grade-ontouml <out.nq>")?);
                grade_ontouml = Some((dir, corpus_name, out_nq));
            }
            "--world" => world = Some(next(&mut args, "--world")?),
            "--quads" => {
                quads = Some(
                    next(&mut args, "--quads")?
                        .parse()
                        .map_err(|e| ce(format!("--quads must be a non-negative integer: {e}")))?,
                )
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(ce(format!("unknown argument: {other}\n{USAGE}"))),
        }
    }

    let mode_count = szs.is_some() as u8
        + manifest.is_some() as u8
        + vendor_el.is_some() as u8
        + vendor_full.is_some() as u8
        + vendor_entailment.is_some() as u8
        + vendor_tptp.is_some() as u8
        + vendor_ontouml.is_some() as u8
        + grade_suite.is_some() as u8
        + grade_ore.is_some() as u8
        + grade_tptp.is_some() as u8
        + grade_ontouml.is_some() as u8;
    if mode_count > 1 {
        return Err(ce(format!(
            "--szs, --manifest, --vendor-el, --vendor-full, --vendor-tptp, --vendor-ontouml, \
             --grade-suite, --grade-ore, --grade-tptp, and --grade-ontouml are mutually \
             exclusive\n{USAGE}"
        )));
    }

    // `--vendor-full` shares the Lane-A vendoring path with `--vendor-el` but binds
    // the full W3C suite manifest; dispatched here so the tuple match below stays
    // the pre-existing arity.
    if let Some((input, out)) = vendor_full {
        return vendor_full_corpus(&input, &out);
    }

    // `--vendor-entailment` shares the same Lane-A vendoring path, bound to the
    // self-authored inline entailment-mini manifest and its self-authored metadata.
    if let Some((input, out)) = vendor_entailment {
        return vendor_entailment_corpus(&input, &out);
    }

    match (
        szs,
        manifest,
        vendor_el,
        vendor_tptp,
        vendor_ontouml,
        grade_suite,
        grade_ore,
        grade_tptp,
        grade_ontouml,
    ) {
        (Some(path), None, None, None, None, None, None, None, None) => {
            ingest_szs(&path, world.as_deref(), quads)
        }
        (None, Some(path), None, None, None, None, None, None, None) => {
            // `--world`/`--quads` shape an SZS single-world verdict; they have no
            // meaning for a manifest (one line per entry). Reject loudly rather than
            // parse-and-drop them (no-optionality / no silent misuse).
            if world.is_some() || quads.is_some() {
                return Err(ce(format!(
                    "--world / --quads apply only to --szs, not --manifest\n{USAGE}"
                )));
            }
            ingest_manifest(&path)
        }
        (None, None, Some((input, out)), None, None, None, None, None, None) => {
            vendor_el_corpus(&input, &out)
        }
        (None, None, None, Some(corpus_dir), None, None, None, None, None) => {
            vendor_tptp_corpus(&corpus_dir)
        }
        (None, None, None, None, Some(corpus_dir), None, None, None, None) => {
            vendor_ontouml_corpus(&corpus_dir)
        }
        (None, None, None, None, None, Some((input, corpus_name, out_nq)), None, None, None) => {
            grade_suite_corpus(&input, &corpus_name, &out_nq)
        }
        (None, None, None, None, None, None, Some((dir, corpus_name, out_nq)), None, None) => {
            grade_ore_corpus(&dir, &corpus_name, &out_nq)
        }
        (None, None, None, None, None, None, None, Some((dir, corpus_name, out_nq)), None) => {
            grade_tptp_corpus(&dir, &corpus_name, &out_nq)
        }
        (None, None, None, None, None, None, None, None, Some((dir, corpus_name, out_nq))) => {
            grade_ontouml_corpus(&dir, &corpus_name, &out_nq)
        }
        _ => Err(ce(format!(
            "one of --szs / --manifest / --vendor-el / --vendor-tptp / --vendor-ontouml / \
             --grade-suite / --grade-ore / --grade-tptp / --grade-ontouml is required\n{USAGE}"
        ))),
    }
}

/// The SPDX header prepended to every generated TPTP-corpus stub/derived file
/// (self-authored, CC-BY-4.0 — the same license the corpus.json declares).
const TPTP_SPDX_HEADER: &str = "# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>\n\
     # SPDX-License-Identifier: CC-BY-4.0\n";

/// The SPDX header prepended to every generated OntoUML-corpus stub/derived file
/// (self-authored, CC-BY-4.0 — the same license the corpus.json declares).
const ONTOUML_SPDX_HEADER: &str = "# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>\n\
     # SPDX-License-Identifier: CC-BY-4.0\n";

/// Regenerate the derived anatomy of a self-authored TPTP Lane-A corpus from its
/// source `.p` files.
///
/// For each `<corpus-dir>/<slug>/source/problem.p`: parse the TPTP body, apply the
/// FOL-negation reduction, lower the EL/DL fragment to a world-scoped EDB, decide
/// it natively, and (re)write `profile.json` (carrying the raw `szs_status`
/// provenance), the `input.logic.ttl` stub, the generated `input.nq`, and the
/// blessed `expected/verdicts.json`.
///
/// Hard-fail (Lane-A is agreeing-by-construction): a problem the native engine
/// cannot decide (parser/lowering capability gap) or decides in *disagreement*
/// with its `% SZS status` belongs in the sibling `-divergence` corpus, not here —
/// so it is a loud error, never silently vendored.
fn vendor_tptp_corpus(corpus_dir: &Path) -> gmeow_errors::Result<()> {
    let corpus_name = corpus_dir
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| {
            ce(format!(
                "cannot derive corpus name from {}",
                corpus_dir.display()
            ))
        })?;

    let mut slugs: Vec<PathBuf> = std::fs::read_dir(corpus_dir)
        .map_err(|e| ce(format!("cannot read {}: {e}", corpus_dir.display())))?
        .map(|e| e.map(|e| e.path()).map_err(|e| ce(e.to_string())))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|p| p.is_dir())
        .collect();
    slugs.sort();

    let mut vendored = 0usize;
    for case_dir in slugs {
        let slug = case_dir
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ce(format!("bad case dir {}", case_dir.display())))?
            .to_string();
        let problem = case_dir.join("source").join("problem.p");
        if !problem.is_file() {
            return Err(ce(format!(
                "{}: TPTP case has no source/problem.p",
                case_dir.display()
            )));
        }
        let text = std::fs::read_to_string(&problem)
            .map_err(|e| ce(format!("cannot read {}: {e}", problem.display())))?;

        // The declared SZS ground truth: raw token (provenance) + 3-bucket outcome.
        let raw_token =
            parse_szs_status(&text).map_err(|e| ce(format!("{}: {e}", problem.display())))?;
        let declared =
            outcome_from_szs(&text).map_err(|e| ce(format!("{}: {e}", problem.display())))?;

        // Parse + FOL-negation reduction + native decision.
        let formulas = parse_tptp(&text).map_err(|e| {
            ce(match e {
                TptpError::Syntax(m) => format!("{}: malformed TPTP: {m}", problem.display()),
                TptpError::Unsupported(m) => format!(
                    "{}: out-of-fragment construct ({m}) — this problem belongs in the \
                 sibling -divergence corpus, not Lane-A",
                    problem.display()
                ),
            })
        })?;
        let world_iri = format!("https://gmeow.example/{corpus_name}/{slug}/w");
        let (native, lowered) = lower_and_decide(&formulas, &world_iri).map_err(|g| {
            ce(format!(
                "{}: native engine cannot decide this problem ({}) — it belongs in the \
                 sibling -divergence corpus (an honest DlGap), not Lane-A",
                problem.display(),
                g.reason
            ))
        })?;

        // Lane-A is agreeing-by-construction: native MUST match the SZS ground truth.
        if native != declared {
            return Err(ce(format!(
                "{}: native decided {:?} but the SZS status declares {:?} — a divergence \
                 belongs in the sibling -divergence corpus, never Lane-A",
                problem.display(),
                native.verdict_status().as_str(),
                declared.verdict_status().as_str()
            )));
        }

        write_tptp_case(
            &case_dir,
            &world_iri,
            &lowered.input_nq,
            lowered.quad_count,
            native,
            &raw_token,
        )?;
        println!(
            "VENDOR {slug}: {} (SZS {raw_token})",
            native.verdict_status().as_str()
        );
        vendored += 1;
    }

    if vendored == 0 {
        return Err(ce(format!(
            "{}: no TPTP cases found (expected <slug>/source/problem.p dirs)",
            corpus_dir.display()
        )));
    }
    println!(
        "vendored {vendored} TPTP case(s) into {}",
        corpus_dir.display()
    );
    Ok(())
}

/// (Re)write the derived anatomy of one TPTP Lane-A case: `profile.json` (with the
/// raw `szs_status` provenance), the `input.logic.ttl` stub, the generated
/// `input.nq`, and the blessed `expected/verdicts.json`. The authored
/// `source/problem.p` is left untouched.
fn write_tptp_case(
    case_dir: &Path,
    world_iri: &str,
    input_nq: &str,
    quad_count: usize,
    outcome: ExternalOutcome,
    szs_status: &str,
) -> gmeow_errors::Result<()> {
    let expected_dir = case_dir.join("expected");
    std::fs::create_dir_all(&expected_dir)
        .map_err(|e| ce(format!("cannot create {}: {e}", expected_dir.display())))?;

    // profile.json — consistency mode, native engine, raw SZS token as provenance.
    let mut profile = BTreeMap::new();
    profile.insert("verdict_mode", serde_json::json!("consistency"));
    profile.insert("mode", serde_json::json!("native"));
    profile.insert("szs_status", serde_json::json!(szs_status));
    let profile_json = serde_json::to_string_pretty(&profile)
        .map_err(|e| ce(format!("serialize profile.json: {e}")))?
        + "\n";
    std::fs::write(case_dir.join("profile.json"), profile_json)
        .map_err(|e| ce(format!("cannot write profile.json: {e}")))?;

    // input.logic.ttl — stub required by the per-case anatomy (not compiled in
    // consistency mode; the native DL path reads input.nq only).
    let stub_ttl = format!(
        "{TPTP_SPDX_HEADER}#\n\
         # verdict_mode=consistency TPTP case. The OWL EDB is the world-scoped N-Quads\n\
         # in input.nq, GENERATED from source/problem.p by `ingest-external --vendor-tptp`\n\
         # (parse → FOL-negation reduction → EL/DL lowering), decided by the native DL\n\
         # consistency path. This stub only satisfies the per-case anatomy.\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n"
    );
    std::fs::write(case_dir.join("input.logic.ttl"), stub_ttl)
        .map_err(|e| ce(format!("cannot write input.logic.ttl: {e}")))?;

    // input.nq — the generated, world-scoped, sorted+deduped EDB.
    std::fs::write(case_dir.join("input.nq"), input_nq)
        .map_err(|e| ce(format!("cannot write input.nq: {e}")))?;

    // expected/verdicts.json — the native verdict the harness re-asserts.
    let mut world_entry = BTreeMap::new();
    world_entry.insert("quads", serde_json::json!(quad_count as u64));
    world_entry.insert(
        "status",
        serde_json::json!(outcome.verdict_status().as_str()),
    );
    let mut verdicts_obj = BTreeMap::new();
    verdicts_obj.insert(world_iri.to_owned(), world_entry);
    let verdicts_json = serde_json::to_string_pretty(&verdicts_obj)
        .map_err(|e| ce(format!("serialize verdicts.json: {e}")))?
        + "\n";
    std::fs::write(expected_dir.join("verdicts.json"), &verdicts_json)
        .map_err(|e| ce(format!("cannot write expected/verdicts.json: {e}")))?;

    Ok(())
}

/// Read the DOCUMENTED anti-pattern label of an OntoUML case from its authored
/// `profile.json` (the `documented_antipattern` string key). Returns `None` when the
/// file is absent or the key is missing (a clean-control case); a present-but-non-
/// string value is a hard error. The corpus author sets this label; this tool
/// regenerates the rest of the anatomy and never invents it.
fn read_documented_antipattern(profile_path: &Path) -> gmeow_errors::Result<Option<String>> {
    if !profile_path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(profile_path)
        .map_err(|e| ce(format!("cannot read {}: {e}", profile_path.display())))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| ce(format!("cannot parse {}: {e}", profile_path.display())))?;
    match value.get("documented_antipattern") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(ce(format!(
            "{}: documented_antipattern must be a string, got {other}",
            profile_path.display()
        ))),
    }
}

/// Regenerate the derived anatomy of a self-authored OntoUML Lane-A corpus from its
/// source `model.ttl` files.
///
/// For each `<corpus-dir>/<slug>/source/model.ttl`: read the documented anti-pattern
/// label from the case's authored `profile.json` (absent for a clean control), parse
/// the model, lower it onto the world-scoped `logic:` stereotype ABox, run the native
/// foundation disciplines, and (re)write `profile.json`, the `input.logic.ttl` stub,
/// the generated `input.nq`, and the blessed `expected/{materialized.nq,verdicts.json}`.
///
/// Hard-fail (Lane-A is agreeing-by-construction): a documented anti-pattern the
/// disciplines do NOT reproduce (`CorpusOnly`) belongs in the sibling `-divergence`
/// corpus (an honest gap), a clean control that fires anything (`EngineOnly`) is a
/// soundness false positive, and a well-formed but out-of-fragment construct
/// (`Unsupported`) belongs in `-divergence` too — each a loud error, never vendored.
fn vendor_ontouml_corpus(corpus_dir: &Path) -> gmeow_errors::Result<()> {
    let corpus_name = corpus_dir
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| {
            ce(format!(
                "cannot derive corpus name from {}",
                corpus_dir.display()
            ))
        })?;

    let mut slugs: Vec<PathBuf> = std::fs::read_dir(corpus_dir)
        .map_err(|e| ce(format!("cannot read {}: {e}", corpus_dir.display())))?
        .map(|e| e.map(|e| e.path()).map_err(|e| ce(e.to_string())))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|p| p.is_dir())
        .collect();
    slugs.sort();

    let mut vendored = 0usize;
    for case_dir in slugs {
        let slug = case_dir
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ce(format!("bad case dir {}", case_dir.display())))?
            .to_string();
        let model_path = case_dir.join("source").join("model.ttl");
        if !model_path.is_file() {
            return Err(ce(format!(
                "{}: OntoUML case has no source/model.ttl",
                case_dir.display()
            )));
        }
        let text = std::fs::read_to_string(&model_path)
            .map_err(|e| ce(format!("cannot read {}: {e}", model_path.display())))?;

        // The documented anti-pattern is authored provenance carried in the case's
        // profile.json (absent for a clean control); we regenerate the rest here.
        let documented = read_documented_antipattern(&case_dir.join("profile.json"))?;

        // Build the base IRI from an ABSOLUTE path (mirrors `ingest_manifest`).
        let abs = std::path::absolute(&model_path)
            .map_err(|e| ce(format!("cannot resolve {}: {e}", model_path.display())))?;
        let base = format!("file://{}", abs.display());
        let model = parse_ontouml_model(&text, Some(&base)).map_err(|e| {
            ce(match e {
                OntoumlError::Syntax(m) => {
                    format!("{}: malformed OntoUML: {m}", model_path.display())
                }
                OntoumlError::Unsupported(m) => format!(
                    "{}: out-of-fragment construct ({m}) — this model belongs in the \
                 sibling -divergence corpus, not Lane-A",
                    model_path.display()
                ),
            })
        })?;

        let world_iri = format!("https://gmeow.example/{corpus_name}/{slug}/w");
        let (fq, input_nq, _count) =
            lower_and_evaluate(&model, &world_iri, AntiRigidityPolicy::WitnessObligation).map_err(
                |e| {
                    ce(match e {
                        OntoumlError::Syntax(m) => format!(
                            "{}: OntoUML lowering/evaluation failed: {m}",
                            model_path.display()
                        ),
                        OntoumlError::Unsupported(m) => format!(
                            "{}: out-of-fragment construct ({m}) — this model belongs in the \
                             sibling -divergence corpus (an honest gap), not Lane-A",
                            model_path.display()
                        ),
                    })
                },
            )?;

        let fired = fired_disciplines(&fq);

        // Lane-A is agreeing-by-construction: the documented anti-pattern MUST fire,
        // and a clean control MUST fire nothing.
        match compare(documented.as_deref(), &fired) {
            DisciplineVerdict::Agree => {}
            DisciplineVerdict::CorpusOnly => {
                return Err(ce(format!(
                    "{}: documented anti-pattern {} was NOT reproduced by the native \
                     disciplines (fired: {fired:?}) — this model belongs in the sibling \
                     -divergence corpus (an honest gap), never Lane-A",
                    model_path.display(),
                    documented.as_deref().unwrap_or("<none>")
                )));
            }
            DisciplineVerdict::EngineOnly => {
                return Err(ce(format!(
                    "{}: clean-control case fired disciplines {fired:?} — a soundness FALSE \
                     POSITIVE; Lane-A clean controls must fire nothing",
                    model_path.display()
                )));
            }
            DisciplineVerdict::DlGap => {
                return Err(ce(format!(
                    "{}: discipline comparison yielded an unexpected DlGap — a lowering gap \
                     belongs in the sibling -divergence corpus, not Lane-A",
                    model_path.display()
                )));
            }
        }

        write_ontouml_case(&case_dir, &input_nq, &fq, documented.as_deref())?;
        println!("VENDOR {slug}: fired={fired:?} documented={documented:?}");
        vendored += 1;
    }

    if vendored == 0 {
        return Err(ce(format!(
            "{}: no OntoUML cases found (expected <slug>/source/model.ttl dirs)",
            corpus_dir.display()
        )));
    }
    println!(
        "vendored {vendored} OntoUML case(s) into {}",
        corpus_dir.display()
    );
    Ok(())
}

/// (Re)write the derived anatomy of one OntoUML Lane-A case: `profile.json` (native
/// foundation-lowering materialization, carrying the documented anti-pattern label
/// only when present), the `input.logic.ttl` stub, the generated `input.nq`, and the
/// blessed `expected/{materialized.nq,verdicts.json}`. The authored `source/model.ttl`
/// is left untouched.
fn write_ontouml_case(
    case_dir: &Path,
    input_nq: &str,
    fq: &[FoundationQuad],
    documented: Option<&str>,
) -> gmeow_errors::Result<()> {
    let expected_dir = case_dir.join("expected");
    std::fs::create_dir_all(&expected_dir)
        .map_err(|e| ce(format!("cannot create {}: {e}", expected_dir.display())))?;

    // profile.json — native foundation-lowering materialization (not certified). The
    // documented anti-pattern label is carried ONLY when present (a clean control
    // omits the key entirely, matching the `Option<String>` None semantics).
    let mut profile = BTreeMap::new();
    profile.insert(
        "reasoning_contract",
        serde_json::json!({ "preset": "StratifiedNAFProfile" }),
    );
    profile.insert("mode", serde_json::json!("native"));
    profile.insert("foundation_lowering", serde_json::json!(true));
    profile.insert("certify", serde_json::json!(false));
    if let Some(label) = documented {
        profile.insert("documented_antipattern", serde_json::json!(label));
    }
    let profile_json = serde_json::to_string_pretty(&profile)
        .map_err(|e| ce(format!("serialize profile.json: {e}")))?
        + "\n";
    std::fs::write(case_dir.join("profile.json"), profile_json)
        .map_err(|e| ce(format!("cannot write profile.json: {e}")))?;

    // input.logic.ttl — the seed logic: stereotype ABox as a default-graph program
    // (the harness compiles this file, so it must carry the same seed facts input.nq
    // world-scopes; the foundation materialization itself reads input.nq). Derived
    // mechanically by dropping each N-Quad's world-graph term — every term is an IRI
    // (`<…>`), so the trailing graph token is unambiguous.
    let program_lines: Vec<String> = input_nq
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let body = line.trim_end().trim_end_matches('.').trim_end();
            // `<s> <p> <o> <g>` → drop the last (graph) token.
            let triple = body.rsplit_once(' ').map(|(t, _g)| t).unwrap_or(body);
            format!("{triple} .")
        })
        .collect();
    let program_ttl = format!(
        "{ONTOUML_SPDX_HEADER}#\n\
         # foundation-lowering native case. This default-graph logic: stereotype ABox\n\
         # is GENERATED from source/model.ttl by `ingest-external --vendor-ontouml`\n\
         # (OntoUML metamodel → logic: stereotype lowering); input.nq world-scopes the\n\
         # same facts for the native foundation evaluator.\n{}\n",
        program_lines.join("\n")
    );
    std::fs::write(case_dir.join("input.logic.ttl"), program_ttl)
        .map_err(|e| ce(format!("cannot write input.logic.ttl: {e}")))?;

    // input.nq — the generated, world-scoped, sorted+deduped stereotype ABox.
    std::fs::write(case_dir.join("input.nq"), input_nq)
        .map_err(|e| ce(format!("cannot write input.nq: {e}")))?;

    // Map FoundationQuads → RunnerQuads field-for-field (no filtering) — byte-
    // identical to what `materialize_foundation` produces in the harness.
    let runner_quads: Vec<RunnerQuad> = fq
        .iter()
        .map(|q| RunnerQuad {
            graph: q.graph.clone(),
            subject: q.subject.clone(),
            predicate: q.predicate.clone(),
            obj: q.object.clone(),
            derivation_id: q.derivation_id.clone(),
            rule_iri: q.rule_iri.clone(),
            source_quad_ids: q.source_quad_ids.clone(),
            // The foundation chase runs to completion (no governor) ⇒ every quad `ok`.
            budget_status: gmeow_logic::seam::BudgetStatus::Ok.as_str().to_string(),
        })
        .collect();

    // expected/materialized.nq — the derived foundation quads the harness re-asserts.
    std::fs::write(
        expected_dir.join("materialized.nq"),
        materialized_to_nquads(&runner_quads),
    )
    .map_err(|e| ce(format!("cannot write expected/materialized.nq: {e}")))?;

    // expected/verdicts.json — foundation materialization is always consistent (a
    // discipline violation is a materialized diagnostic, not an inconsistency).
    let counts = count_worlds(&runner_quads);
    let verdicts = build_verdicts(&counts, |_| VerdictStatus::Consistent);
    let verdicts_json = serde_json::to_string_pretty(&verdicts)
        .map_err(|e| ce(format!("serialize verdicts.json: {e}")))?
        + "\n";
    std::fs::write(expected_dir.join("verdicts.json"), &verdicts_json)
        .map_err(|e| ce(format!("cannot write expected/verdicts.json: {e}")))?;

    Ok(())
}

/// Ingest a TPTP SZS problem → runner verdict.
fn ingest_szs(
    path: &std::path::Path,
    world: Option<&str>,
    quads: Option<u64>,
) -> gmeow_errors::Result<()> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| ce(format!("cannot read {}: {e}", path.display())))?;
    let outcome = outcome_from_szs(&text).map_err(|e| ce(e.to_string()))?;
    match (world, quads) {
        (Some(world), Some(quads)) => {
            let verdict = runner_verdict_json(world, quads, outcome);
            println!(
                "{}",
                serde_json::to_string_pretty(&verdict)
                    .map_err(|e| ce(format!("serialize verdict: {e}")))?
            );
        }
        (None, None) => println!("{}", outcome.verdict_status().as_str()),
        _ => return Err(ce("--world and --quads must be given together".to_string())),
    }
    Ok(())
}

/// Ingest a W3C entailment manifest → one `<name>\t<status>` line per entry.
fn ingest_manifest(path: &std::path::Path) -> gmeow_errors::Result<()> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| ce(format!("cannot read {}: {e}", path.display())))?;
    // Build the base IRI from an ABSOLUTE path so a relative `--manifest foo/x.ttl`
    // yields `file:///abs/foo/x.ttl` (empty authority) rather than the malformed
    // `file://foo/x.ttl` (where `foo` would be read as the authority). `absolute` is
    // lexical (no filesystem access) — enough for a Linux-only ingest path without
    // pulling in a `url` crate dependency edge.
    let abs = std::path::absolute(path)
        .map_err(|e| ce(format!("cannot resolve {}: {e}", path.display())))?;
    let base = format!("file://{}", abs.display());
    let entries = parse_test_manifest(&text, Some(&base)).map_err(|e| ce(e.to_string()))?;
    if entries.is_empty() {
        return Err(ce(format!("no entailment entries in {}", path.display())));
    }
    for entry in entries {
        println!(
            "{}\t{}",
            entry.name,
            entry.outcome().verdict_status().as_str()
        );
    }
    Ok(())
}

/// Convert an entry name to a deterministic slug: lowercase, runs of
/// non-[a-z0-9] replaced by a single `-`, trimmed of leading/trailing `-`.
fn to_slug(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut result = String::with_capacity(lower.len());
    let mut in_sep = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            in_sep = false;
            result.push(ch);
        } else if !in_sep {
            in_sep = true;
            result.push('-');
        }
    }
    result.trim_matches('-').to_string()
}

/// The result of lowering one manifest entry into a world-scoped dataset.
struct LoweredEntry {
    slug: String,
    world_iri: String,
    /// The world-scoped N-Quads text (sorted, deduped, trailing newline).
    input_nq: String,
    /// The quad count (after dedup).
    quad_count: usize,
}

/// Lower one manifest entry's inline premise into a world-scoped N-Quads dataset.
///
/// Returns `Ok(Some(lowered))` on success, `Ok(None)` when the entry must be skipped
/// (with the reason already printed to stdout), or `Err` on a hard failure that should
/// stop the caller.
///
/// The `world_iri_prefix` is prepended to the slug to form the world IRI:
/// `{world_iri_prefix}{slug}/w`.
fn lower_entry(
    entry: &gmeow_conformance::external::ManifestEntry,
    world_iri_prefix: &str,
) -> Option<LoweredEntry> {
    let slug = to_slug(&entry.name);
    let world_iri = format!("{world_iri_prefix}{slug}/w");

    // ── Extract the inline premise RDF/XML ────────────────────────────────
    let premise_xml = match &entry.action {
        Some(OntologyDoc::InlineRdfXml(xml)) => xml.clone(),
        Some(OntologyDoc::Reference(_)) => {
            println!("SKIP {slug}: premise is an IRI reference, not inline RDF/XML (Lane-B)");
            return None;
        }
        None => {
            println!(
                "SKIP {slug}: no recognized premise document (e.g. fsPremiseOntology only) — Lane-B"
            );
            return None;
        }
    };

    // ── Parse the premise RDF/XML ─────────────────────────────────────────
    let premise_ds = match purrdf::parse_dataset(
        premise_xml.as_bytes(),
        "application/rdf+xml",
        Some("http://example.org/"),
    ) {
        Ok(ds) => ds,
        Err(e) => {
            println!("SKIP {slug}: premise unparsable: {e}");
            return None;
        }
    };
    if premise_ds.quad_refs().count() == 0 {
        println!("SKIP {slug}: premise parsed to zero quads (vacuous pass not permitted)");
        return None;
    }

    // ── Build world-scoped N-Quads ────────────────────────────────────────
    let (input_nq, quad_count) = match premise_ds_to_world_nquads(premise_ds.as_ref(), &world_iri) {
        Ok(r) => r,
        Err(e) => {
            println!("SKIP {slug}: premise N-Quads build failed: {e}");
            return None;
        }
    };

    if input_nq.trim().is_empty() {
        println!("SKIP {slug}: premise yields zero valid N-Quads (vacuous pass not permitted)");
        return None;
    }

    Some(LoweredEntry {
        slug,
        world_iri,
        input_nq,
        quad_count,
    })
}

/// The outcome of lowering one entailment manifest entry (`A ⊨ C`) for vendoring.
enum EntailmentLowering {
    /// The native reasoner decided the entailment: the reduced EDB (`premise ∪ ¬C`),
    /// world-scoped, plus the native consistency verdict on it (the verdict the
    /// conformance harness reproduces by re-running `dl_consistency` on `input.nq`).
    Decided {
        slug: String,
        world_iri: String,
        input_nq: String,
        quad_count: usize,
        native_status: &'static str,
        premise_xml: String,
    },
    /// The native reasoner cannot soundly grade this entailment as one consistency
    /// case: a structured gap (a multi-goal conjunction, a role/existential/malformed
    /// conclusion, or a native coverage gap). Carries the premise-only world-scoped
    /// EDB for the divergence bucket, plus the structured `gmeow:gapShape` token.
    Gap {
        slug: String,
        world_iri: String,
        input_nq: String,
        quad_count: usize,
        gap_shape: &'static str,
        premise_xml: String,
    },
    /// The entry is not lowerable (unparsable, or a non-inline premise/conclusion).
    Skip,
}

/// Build the reduced default-graph dataset `premise ∪ negation` (the negation triples
/// are all IRIs) programmatically via [`purrdf::RdfDatasetBuilder`] — no premise
/// serialize→reparse roundtrip, purrdf still owns every term's encoding. Mirrors the
/// shipped runtime reduction in `gmeow_logic::entail::build_world_edb`, minus the
/// world-scoping (this reduction lives in the default graph).
fn build_reduced_dataset(
    premise: &purrdf::RdfDataset,
    negation: &[(String, String, String)],
) -> gmeow_errors::Result<std::sync::Arc<purrdf::RdfDataset>> {
    let mut builder = RdfDatasetBuilder::new();
    for q in premise.quads() {
        // Default graph only — the reduced EDB is the premise's default-graph triples
        // (the prior path serialized SerializeGraph::DefaultGraph) unioned with the
        // negation. Named-graph quads, if any, are not part of the reduction.
        if q.g.is_some() {
            continue;
        }
        let TermRef::Iri(pred) = premise.resolve(q.p) else {
            // A non-IRI predicate is not well-formed RDF; skip it defensively.
            continue;
        };
        let pred = pred.to_owned();
        let subject = premise.to_owned_term(q.s);
        let object = premise.to_owned_term(q.o);
        builder.push_owned_quad(&RdfQuad::new(subject, pred, object));
    }
    for (s, p, o) in negation {
        builder.push_owned_quad(&RdfQuad::new(
            RdfTerm::iri(s.clone()),
            p.clone(),
            RdfTerm::iri(o.clone()),
        ));
    }
    builder
        .freeze()
        .map_err(|e| ce(format!("reduced EDB failed to build: {e}")))
}

/// Lower one entailment manifest entry into either a decided reduced-EDB consistency
/// case (single-triple conclusion the native path grades) or a structured gap.
fn lower_entailment_entry(
    entry: &gmeow_conformance::external::ManifestEntry,
    world_iri_prefix: &str,
) -> EntailmentLowering {
    let slug = to_slug(&entry.name);
    let world_iri = format!("{world_iri_prefix}{slug}/w");

    let premise_xml = match &entry.action {
        Some(OntologyDoc::InlineRdfXml(xml)) => xml.clone(),
        _ => {
            println!("SKIP {slug}: entailment premise is not inline RDF/XML (Lane-B)");
            return EntailmentLowering::Skip;
        }
    };
    let conclusion_xml = match &entry.result {
        Some(OntologyDoc::InlineRdfXml(xml)) => xml.clone(),
        _ => {
            println!("SKIP {slug}: entailment conclusion is not inline RDF/XML (Lane-B)");
            return EntailmentLowering::Skip;
        }
    };

    let premise_ds = match purrdf::parse_dataset(
        premise_xml.as_bytes(),
        "application/rdf+xml",
        Some("http://example.org/"),
    ) {
        Ok(ds) => ds,
        Err(e) => {
            println!("SKIP {slug}: entailment premise unparsable: {e}");
            return EntailmentLowering::Skip;
        }
    };
    let conclusion_ds = match purrdf::parse_dataset(
        conclusion_xml.as_bytes(),
        "application/rdf+xml",
        Some("http://example.org/"),
    ) {
        Ok(ds) => ds,
        Err(e) => {
            println!("SKIP {slug}: entailment conclusion unparsable: {e}");
            return EntailmentLowering::Skip;
        }
    };

    // Premise-only world-scoped EDB — the divergence-bucket input.nq for a gap case.
    let (premise_nq, premise_qc) = match premise_ds_to_world_nquads(premise_ds.as_ref(), &world_iri)
    {
        Ok(r) => r,
        Err(e) => {
            println!("SKIP {slug}: entailment premise N-Quads build failed: {e}");
            return EntailmentLowering::Skip;
        }
    };

    match gmeow_logic::entail::reduce_for_vendoring(premise_ds.as_ref(), conclusion_ds.as_ref()) {
        Err(e) => {
            // The reserved-namespace soundness guard fired — refuse, never guess.
            println!("SKIP {slug}: entailment reduction hard-failed: {e}");
            EntailmentLowering::Skip
        }
        Ok(gmeow_logic::entail::VendorReduction::Gap(gap)) => EntailmentLowering::Gap {
            slug,
            world_iri,
            input_nq: premise_nq,
            quad_count: premise_qc,
            gap_shape: gap.shape.as_token(),
            premise_xml,
        },
        Ok(gmeow_logic::entail::VendorReduction::MultiGoal) => EntailmentLowering::Gap {
            slug,
            world_iri,
            input_nq: premise_nq,
            quad_count: premise_qc,
            gap_shape: gmeow_logic::entail::CapabilityGapShape::VendoringMultiGoal.as_token(),
            premise_xml,
        },
        Ok(gmeow_logic::entail::VendorReduction::Single(negation)) => {
            let reduced_ds = match build_reduced_dataset(premise_ds.as_ref(), &negation) {
                Ok(d) => d,
                Err(e) => {
                    println!("SKIP {slug}: reduced EDB build failed: {e}");
                    return EntailmentLowering::Skip;
                }
            };
            let (input_nq, quad_count) =
                match premise_ds_to_world_nquads(reduced_ds.as_ref(), &world_iri) {
                    Ok(r) => r,
                    Err(e) => {
                        println!("SKIP {slug}: reduced EDB N-Quads build failed: {e}");
                        return EntailmentLowering::Skip;
                    }
                };
            let world_ds = match purrdf::dataset_from_bytes(
                input_nq.as_bytes(),
                purrdf::NativeRdfFormat::NQuads,
            ) {
                Ok(d) => d,
                Err(e) => {
                    println!("SKIP {slug}: reduced EDB round-trip failed: {e}");
                    return EntailmentLowering::Skip;
                }
            };
            let verdict = match gmeow_logic::reason::dl_consistency(world_ds.as_ref()) {
                Ok(v) => v,
                Err(e) => {
                    println!("SKIP {slug}: reduced EDB dl_consistency failed: {e}");
                    return EntailmentLowering::Skip;
                }
            };
            if !verdict.gaps.is_empty() {
                // The native path admits a coverage gap on the reduced EDB.
                return EntailmentLowering::Gap {
                    slug,
                    world_iri,
                    input_nq: premise_nq,
                    quad_count: premise_qc,
                    gap_shape: "native-coverage",
                    premise_xml,
                };
            }
            let native_status = if verdict.consistent {
                "consistent"
            } else {
                "inconsistent"
            };
            EntailmentLowering::Decided {
                slug,
                world_iri,
                input_nq,
                quad_count,
                native_status,
                premise_xml,
            }
        }
    }
}

/// The W3C SPDX header prepended to every vendored source/stub file.
const W3C_SPDX_HEADER: &str = "# SPDX-FileCopyrightText: 2009 W3C (Massachusetts Institute of Technology, ERCIM, Keio, Beihang)\n# SPDX-License-Identifier: W3C\n";

/// Frozen-verdict provenance for one vendored case: the native verdict the
/// committed golden re-asserts, and the W3C published expected verdict.
struct CaseVerdicts<'a> {
    /// The status the committed `expected/verdicts.json` carries (the verdict the
    /// conformance harness re-asserts the native engine produces).
    committed_status: &'a str,
    /// The W3C published expected verdict, carried verbatim as provenance.
    published_status: &'a str,
    /// The native token (`consistent` / `inconsistent` / `incomplete`), recorded
    /// in `profile.json` as the frozen native decision for divergence cases.
    native_token: &'a str,
    /// For an entailment case the native reasoner could not soundly grade, the
    /// structured `gmeow:gapShape` token (from [`gmeow_logic::entail::GapShape`]) —
    /// recorded in `profile.json` so the pipeline reifier can project it as first-class
    /// `gmeow:CapabilityGap` data. `None` for a decided (consistency or entailment) case.
    gap_shape: Option<&'a str>,
}

/// The honest-DlGap divergence bucket sibling of the Lane-A `out_dir`:
/// `<parent>/<name>-divergence`. A name-less path falls back to the literal
/// `w3c-owl2-el-divergence` under the same parent.
fn sibling_divergence_dir(out_dir: &Path) -> PathBuf {
    let parent = out_dir.parent().unwrap_or_else(|| Path::new("."));
    let name = out_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("w3c-owl2-el");
    parent.join(format!("{name}-divergence"))
}

/// Write one vendored external case directory mirroring the committed layout
/// (`profile.json`, `input.logic.ttl`, `input.nq`, `expected/verdicts.json`,
/// `source/manifest.ttl`, `source/premise.rdf`).
///
/// `corpus_name` keys the world IRI prefix and the manifest `ex:` namespace.
/// `verdicts` carries the committed status the harness re-asserts plus the W3C
/// published verdict (recorded as provenance so a divergence case is queryable
/// data, never a silent drop).
#[allow(clippy::too_many_arguments)]
fn write_case(
    out_dir: &Path,
    corpus_name: &str,
    slug: &str,
    world_iri: &str,
    input_nq: &str,
    quad_count: u64,
    otest_type: &str,
    premise_xml: Option<&str>,
    verdicts: &CaseVerdicts<'_>,
) -> gmeow_errors::Result<()> {
    let case_dir = out_dir.join(slug);
    let source_dir = case_dir.join("source");
    let expected_dir = case_dir.join("expected");
    std::fs::create_dir_all(&source_dir)
        .map_err(|e| ce(format!("cannot create {}: {e}", source_dir.display())))?;
    std::fs::create_dir_all(&expected_dir)
        .map_err(|e| ce(format!("cannot create {}: {e}", expected_dir.display())))?;

    // profile.json — consistency mode, native engine. For a divergence case the
    // native decision and the W3C published verdict are frozen here as provenance.
    let mut profile = BTreeMap::new();
    profile.insert("verdict_mode", serde_json::json!("consistency"));
    profile.insert("mode", serde_json::json!("native"));
    profile.insert("native_verdict", serde_json::json!(verdicts.native_token));
    profile.insert(
        "w3c_published_verdict",
        serde_json::json!(verdicts.published_status),
    );
    if let Some(shape) = verdicts.gap_shape {
        profile.insert("gap_shape", serde_json::json!(shape));
    }
    let profile_json = serde_json::to_string_pretty(&profile)
        .map_err(|e| ce(format!("serialize profile.json for {slug}: {e}")))?
        + "\n";
    std::fs::write(case_dir.join("profile.json"), profile_json)
        .map_err(|e| ce(format!("cannot write profile.json for {slug}: {e}")))?;

    // input.logic.ttl — stub required by the per-case anatomy; not compiled in
    // consistency mode (the native DL consistency path reads input.nq only).
    let stub_ttl = format!(
        "{W3C_SPDX_HEADER}#\n\
         # verdict_mode=consistency external case. The OWL EDB is the world-scoped\n\
         # N-Quads in input.nq, decided by the native DL consistency path. This file\n\
         # exists only to satisfy the per-case anatomy; it is NOT compiled in consistency mode.\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n"
    );
    std::fs::write(case_dir.join("input.logic.ttl"), stub_ttl)
        .map_err(|e| ce(format!("cannot write input.logic.ttl for {slug}: {e}")))?;

    // input.nq — already sorted + deduped by the caller.
    std::fs::write(case_dir.join("input.nq"), input_nq)
        .map_err(|e| ce(format!("cannot write input.nq for {slug}: {e}")))?;

    // expected/verdicts.json — the verdict the harness re-asserts.
    let mut world_entry = BTreeMap::new();
    world_entry.insert("quads", serde_json::json!(quad_count));
    world_entry.insert("status", serde_json::json!(verdicts.committed_status));
    let mut verdicts_obj = BTreeMap::new();
    verdicts_obj.insert(world_iri.to_owned(), world_entry);
    let verdicts_json = serde_json::to_string_pretty(&verdicts_obj)
        .map_err(|e| ce(format!("serialize verdicts.json for {slug}: {e}")))?
        + "\n";
    std::fs::write(expected_dir.join("verdicts.json"), &verdicts_json).map_err(|e| {
        ce(format!(
            "cannot write expected/verdicts.json for {slug}: {e}"
        ))
    })?;

    // source/manifest.ttl — carries the W3C otest type and published verdict.
    let manifest_ttl = format!(
        "{W3C_SPDX_HEADER}@prefix otest: <http://www.w3.org/2007/OWL/testOntology#> .\n@prefix mf: <http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#> .\n@prefix ex: <https://gmeow.example/{corpus_name}/{slug}/> .\n\nex:{slug} a otest:{otest_type} ;\n    otest:identifier \"{slug}\" ;\n    mf:action ex:premise.rdf .\n"
    );
    std::fs::write(source_dir.join("manifest.ttl"), &manifest_ttl)
        .map_err(|e| ce(format!("cannot write source/manifest.ttl for {slug}: {e}")))?;

    // source/premise.rdf — verbatim inline RDF/XML for provenance.
    if let Some(xml) = premise_xml {
        std::fs::write(source_dir.join("premise.rdf"), xml)
            .map_err(|e| ce(format!("cannot write source/premise.rdf for {slug}: {e}")))?;
    }

    Ok(())
}

/// Strip a DOCTYPE declaration with internal entity definitions from RDF/XML and
/// expand the defined entities inline, so the result is parseable by a non-validating
/// XML parser that does not load external DTDs.
///
/// This handles the W3C OWL 2 test suite's use of `<!ENTITY name 'value'>` in an
/// internal DTD subset: the entities are extracted, then both the DOCTYPE block and
/// all `&name;` references in the remaining text are replaced.
fn expand_xml_entities(src: &str) -> String {
    // Extract entity definitions from an internal DOCTYPE subset (if present).
    let mut entities: Vec<(String, String)> = Vec::new();

    if let Some(dt_start) = src.find("<!DOCTYPE")
        && let Some(bracket_open) = src[dt_start..].find('[')
    {
        let bracket_open_abs = dt_start + bracket_open;
        if let Some(bracket_close) = src[bracket_open_abs..].find(']') {
            let bracket_close_abs = bracket_open_abs + bracket_close;
            let inner = &src[bracket_open_abs + 1..bracket_close_abs];
            // Extract `<!ENTITY name 'value'>` and `<!ENTITY name "value">`.
            let mut rest = inner;
            while let Some(start) = rest.find("<!ENTITY") {
                rest = &rest[start + 8..];
                rest = rest.trim_start();
                // Read entity name (up to first whitespace).
                let name_end = rest
                    .find(|c: char| c.is_ascii_whitespace())
                    .unwrap_or(rest.len());
                let name = rest[..name_end].to_string();
                rest = rest[name_end..].trim_start();
                // Read entity value (quoted).
                let value = if rest.starts_with('\'') {
                    let end = rest[1..].find('\'').map(|i| i + 1);
                    if let Some(e) = end {
                        let v = rest[1..e].to_string();
                        rest = &rest[e + 1..];
                        v
                    } else {
                        break;
                    }
                } else if rest.starts_with('"') {
                    let end = rest[1..].find('"').map(|i| i + 1);
                    if let Some(e) = end {
                        let v = rest[1..e].to_string();
                        rest = &rest[e + 1..];
                        v
                    } else {
                        break;
                    }
                } else {
                    break;
                };
                if !name.is_empty() && !value.is_empty() {
                    entities.push((name, value));
                }
            }
        }
    }

    // Remove the entire DOCTYPE block (from `<!DOCTYPE` up to the closing `>`
    // after the `]>`), as the parser does not need it once entities are expanded.
    let src_no_doctype = if let Some(dt_start) = src.find("<!DOCTYPE") {
        // Find the end: `]>` closes the internal subset; bare `>` closes a DOCTYPE
        // with no internal subset.
        if let Some(bracket_close) = src[dt_start..].find(']') {
            let abs = dt_start + bracket_close;
            // Consume past the `]>`.
            if src[abs..].starts_with(']') {
                let after = abs + 1;
                if let Some(gt) = src[after..].find('>') {
                    format!("{}{}", &src[..dt_start], &src[after + gt + 1..])
                } else {
                    src[..dt_start].to_string() + &src[abs + 1..]
                }
            } else {
                src.to_string()
            }
        } else if let Some(gt) = src[dt_start..].find('>') {
            format!("{}{}", &src[..dt_start], &src[dt_start + gt + 1..])
        } else {
            src.to_string()
        }
    } else {
        src.to_string()
    };

    // Expand all `&name;` entity references.
    let mut result = src_no_doctype;
    for (name, value) in &entities {
        let entity_ref = format!("&{name};");
        result = result.replace(&entity_ref, value);
    }
    result
}

/// Parse and expand an RDF/XML manifest file, returning consistency/inconsistency
/// entries and the entailment entries (Lane-B, out of consistency-lane scope).
///
/// Both `vendor_el_corpus` and `grade_suite_corpus` share this parsing step.
/// The entailment entries are returned separately so `grade_suite_corpus` can
/// emit DlGap Findings for them rather than silently counting and discarding.
fn parse_consistency_entries(
    input_rdf: &Path,
) -> gmeow_errors::Result<(
    Vec<gmeow_conformance::external::ManifestEntry>,
    Vec<gmeow_conformance::external::ManifestEntry>,
)> {
    let raw_src = std::fs::read_to_string(input_rdf)
        .map_err(|e| ce(format!("cannot read {}: {e}", input_rdf.display())))?;
    // Expand XML entities (the W3C EL suite uses a DOCTYPE internal subset with
    // entity references; expand them before handing to the parser).
    let src = expand_xml_entities(&raw_src);
    let abs = std::path::absolute(input_rdf)
        .map_err(|e| ce(format!("cannot resolve {}: {e}", input_rdf.display())))?;
    let base = format!("file://{}", abs.display());
    let entries = parse_test_manifest_rdfxml(&src, Some(&base)).map_err(|e| ce(e.to_string()))?;

    let mut entailment_entries = Vec::new();
    let mut consistency_entries = Vec::new();
    for entry in entries {
        match entry.kind {
            ManifestTestKind::Consistency | ManifestTestKind::Inconsistency => {
                consistency_entries.push(entry);
            }
            ManifestTestKind::PositiveEntailment | ManifestTestKind::NegativeEntailment => {
                entailment_entries.push(entry);
            }
        }
    }
    println!(
        "INFO: {} consistency + {} entailment entries parsed (entailment graded by refutation)",
        consistency_entries.len(),
        entailment_entries.len()
    );
    Ok((consistency_entries, entailment_entries))
}

/// Vendor a curated Lane-A subset of the W3C OWL 2 EL conformance suite.
///
/// Reads `<input_rdf>` (RDF/XML), keeps only ConsistencyTest / InconsistencyTest
/// entries, runs the native DL consistency path on each, and emits the ones the
/// native path decides AND agrees with the W3C declared outcome.
#[allow(clippy::too_many_arguments)]
fn vendor_lane_a_from_manifest(
    input_rdf: &Path,
    out_dir: &Path,
    corpus_name: &str,
    source_url: &str,
    refresh_command: &str,
    spdx_license: &str,
    version_or_commit: &str,
) -> gmeow_errors::Result<()> {
    let divergence_name = format!("{corpus_name}-divergence");
    let base_iri = format!("https://gmeow.example/{corpus_name}/");

    let (consistency_entries, entailment_entries) = parse_consistency_entries(input_rdf)?;

    // The honest-DlGap divergence bucket is a SIBLING of the Lane-A out_dir: every
    // case the native path cannot soundly decide (`gaps` non-empty → `incomplete`)
    // is vendored here as committed data — its frozen native verdict AND the W3C
    // published verdict — rather than silently dropped. The Lane-A corpus carries
    // agreeing deciders; this corpus carries the named divergence set.
    let divergence_dir = sibling_divergence_dir(out_dir);

    // ── Run native reasoner on each, emit Lane-A cases ────────────────────────
    let mut vendored: usize = 0;
    let mut divergence_vendored: usize = 0;
    let mut skipped_disagree: usize = 0;
    let mut skipped_unparsable: usize = 0;

    // Ensure output directories exist.
    std::fs::create_dir_all(out_dir)
        .map_err(|e| ce(format!("cannot create out-dir {}: {e}", out_dir.display())))?;
    std::fs::create_dir_all(&divergence_dir).map_err(|e| {
        ce(format!(
            "cannot create divergence dir {}: {e}",
            divergence_dir.display()
        ))
    })?;

    // We need a stable slug→entry list for deterministic output; entries are
    // already sorted by IRI from the manifest parser. Collect slugs in order.
    for entry in &consistency_entries {
        let lowered = match lower_entry(entry, &base_iri) {
            Some(l) => l,
            None => {
                skipped_unparsable += 1;
                continue;
            }
        };
        let LoweredEntry {
            slug,
            world_iri,
            input_nq,
            quad_count,
        } = lowered;

        // ── Run the native DL consistency path ────────────────────────────────
        let world_ds = match purrdf::dataset_from_bytes(
            input_nq.as_bytes(),
            purrdf::NativeRdfFormat::NQuads,
        ) {
            Ok(ds) => ds,
            Err(e) => {
                println!("SKIP {slug}: world N-Quads round-trip failed: {e}");
                skipped_unparsable += 1;
                continue;
            }
        };

        let verdict = match gmeow_logic::reason::dl_consistency(world_ds.as_ref()) {
            Ok(v) => v,
            Err(e) => {
                println!("SKIP {slug}: native DL consistency run failed: {e}");
                skipped_unparsable += 1;
                continue;
            }
        };

        let otest_type = match entry.kind {
            ManifestTestKind::Consistency => "ConsistencyTest",
            ManifestTestKind::Inconsistency => "InconsistencyTest",
            _ => unreachable!("only Consistency/Inconsistency reach this branch"),
        };
        let premise_xml = match &entry.action {
            Some(OntologyDoc::InlineRdfXml(xml)) => Some(xml.as_str()),
            _ => None,
        };
        let declared_status = entry.outcome().verdict_status().as_str();

        // ── Honest-DlGap divergence: native cannot decide → vendor as data ────
        // `gaps` non-empty means the native path admits it cannot decide this
        // case (e.g. owl:Thing oneOf — the DL/Full-divergent singleton-universe
        // case). It is NOT dropped: it is committed to the divergence bucket with
        // its frozen native `incomplete` verdict and the W3C published verdict.
        if !verdict.gaps.is_empty() {
            write_case(
                &divergence_dir,
                &divergence_name,
                &slug,
                &world_iri,
                &input_nq,
                quad_count as u64,
                otest_type,
                premise_xml,
                &CaseVerdicts {
                    committed_status: "incomplete",
                    published_status: declared_status,
                    native_token: "incomplete",
                    gap_shape: None,
                },
            )?;
            divergence_vendored += 1;
            println!("DIVERGENCE {slug}: native incomplete, W3C declares {declared_status}");
            continue;
        }

        // Determine declared vs native verdict.
        let native_status = if verdict.consistent {
            "consistent"
        } else {
            "inconsistent"
        };

        // A decided native verdict that disagrees with W3C would be a soundness
        // defect (CorpusOnly). The grade gate pins corpus_only=0; if one ever
        // reappears here we record it in the divergence bucket so it is durable,
        // queryable data rather than a silent drop.
        if native_status != declared_status {
            write_case(
                &divergence_dir,
                &divergence_name,
                &slug,
                &world_iri,
                &input_nq,
                quad_count as u64,
                otest_type,
                premise_xml,
                &CaseVerdicts {
                    committed_status: native_status,
                    published_status: declared_status,
                    native_token: native_status,
                    gap_shape: None,
                },
            )?;
            divergence_vendored += 1;
            skipped_disagree += 1;
            println!(
                "DIVERGENCE {slug}: native decided {native_status}, W3C declares {declared_status} (CorpusOnly)"
            );
            continue;
        }

        // ── EMIT agreeing Lane-A case ─────────────────────────────────────────
        // All agreeing deciders are vendored; the soundness-budget and CI gates
        // bound the corpus size in practice (the W3C EL suite is small and the
        // per-case cost is sub-second). Divergence cases are always vendored in
        // full regardless of count.
        write_case(
            out_dir,
            corpus_name,
            &slug,
            &world_iri,
            &input_nq,
            quad_count as u64,
            otest_type,
            premise_xml,
            &CaseVerdicts {
                committed_status: declared_status,
                published_status: declared_status,
                native_token: native_status,
                gap_shape: None,
            },
        )?;
        vendored += 1;
        println!("EMIT {slug}: {declared_status}");
    }

    // ── Grade the ENTAILMENT tests (`A ⊨ C`) by refutation ────────────────────
    // Each entailment entry is reduced to the consistency of `premise ∪ ¬conclusion`
    // (the native `dl_entails` reduction). A single-triple conclusion is frozen as one
    // reduced-EDB consistency case the harness re-decides; a multi-goal / role /
    // existential / malformed / native-coverage conclusion is an honest structured gap
    // vendored to the divergence bucket with its `gmeow:gapShape` token.
    let mut entailment_vendored: usize = 0;
    let mut entailment_gap: usize = 0;
    for entry in &entailment_entries {
        let (otest_type, declared_status) = match entry.kind {
            ManifestTestKind::PositiveEntailment => ("PositiveEntailmentTest", "inconsistent"),
            ManifestTestKind::NegativeEntailment => ("NegativeEntailmentTest", "consistent"),
            _ => continue,
        };
        match lower_entailment_entry(entry, &base_iri) {
            EntailmentLowering::Skip => {
                skipped_unparsable += 1;
            }
            EntailmentLowering::Gap {
                slug,
                world_iri,
                input_nq,
                quad_count,
                gap_shape,
                premise_xml,
            } => {
                write_case(
                    &divergence_dir,
                    &divergence_name,
                    &slug,
                    &world_iri,
                    &input_nq,
                    quad_count as u64,
                    otest_type,
                    Some(&premise_xml),
                    &CaseVerdicts {
                        committed_status: "incomplete",
                        published_status: declared_status,
                        native_token: "incomplete",
                        gap_shape: Some(gap_shape),
                    },
                )?;
                divergence_vendored += 1;
                entailment_gap += 1;
                println!("ENTAIL-GAP {slug}: {gap_shape} (W3C declares {declared_status})");
            }
            EntailmentLowering::Decided {
                slug,
                world_iri,
                input_nq,
                quad_count,
                native_status,
                premise_xml,
            } => {
                if native_status != declared_status {
                    write_case(
                        &divergence_dir,
                        &divergence_name,
                        &slug,
                        &world_iri,
                        &input_nq,
                        quad_count as u64,
                        otest_type,
                        Some(&premise_xml),
                        &CaseVerdicts {
                            committed_status: native_status,
                            published_status: declared_status,
                            native_token: native_status,
                            gap_shape: None,
                        },
                    )?;
                    divergence_vendored += 1;
                    skipped_disagree += 1;
                    println!(
                        "ENTAIL-DIVERGENCE {slug}: native {native_status}, W3C declares {declared_status} (CorpusOnly)"
                    );
                } else {
                    write_case(
                        out_dir,
                        corpus_name,
                        &slug,
                        &world_iri,
                        &input_nq,
                        quad_count as u64,
                        otest_type,
                        Some(&premise_xml),
                        &CaseVerdicts {
                            committed_status: declared_status,
                            published_status: declared_status,
                            native_token: native_status,
                            gap_shape: None,
                        },
                    )?;
                    entailment_vendored += 1;
                    vendored += 1;
                    println!("ENTAIL-EMIT {slug}: {declared_status}");
                }
            }
        }
    }

    // ── Write corpus.json for both buckets ────────────────────────────────────
    let corpus_json = format!(
        "{{\n  \
        \"name\": \"{corpus_name}\",\n  \
        \"spdx_license\": \"{spdx_license}\",\n  \
        \"source_url\": \"{source_url}\",\n  \
        \"version_or_commit\": \"{version_or_commit}\",\n  \
        \"refresh_command\": \"{refresh_command}\",\n  \
        \"lane\": \"a\"\n}}\n"
    );
    std::fs::write(out_dir.join("corpus.json"), corpus_json)
        .map_err(|e| ce(format!("cannot write corpus.json: {e}")))?;

    // The divergence bucket's lane is `divergence`: native and W3C disagree there
    // by construction (honest DlGap), so the soundness gate that asserts
    // committed==declared must EXCLUDE this lane; the dedicated divergence gate
    // pins it instead.
    let divergence_corpus_json = format!(
        "{{\n  \
        \"name\": \"{divergence_name}\",\n  \
        \"spdx_license\": \"{spdx_license}\",\n  \
        \"source_url\": \"{source_url}\",\n  \
        \"version_or_commit\": \"{version_or_commit}\",\n  \
        \"refresh_command\": \"{refresh_command}\",\n  \
        \"lane\": \"divergence\"\n}}\n"
    );
    std::fs::write(divergence_dir.join("corpus.json"), divergence_corpus_json)
        .map_err(|e| ce(format!("cannot write divergence corpus.json: {e}")))?;

    // ── Print final summary ───────────────────────────────────────────────────
    // COVERAGE SPIKE: `entailment_vendored` is the pinned non-gap floor count — the
    // number of previously-ungradeable entailment tests now graded with a real
    // (non-gap) native verdict agreeing with W3C. `entailment_gap` is the honest
    // capability boundary (multi-goal / role / existential / native-coverage).
    println!(
        "vendored={vendored} divergence_vendored={divergence_vendored} skipped_disagree={skipped_disagree} skipped_unparsable={skipped_unparsable} entailment_vendored={entailment_vendored} entailment_gap={entailment_gap}"
    );

    Ok(())
}

/// Vendor the W3C OWL 2 **EL** profile suite as a Lane-A corpus (plus its
/// honest-DlGap divergence sibling).  A thin binding of
/// [`vendor_lane_a_from_manifest`] to the EL profile manifest constants.
fn vendor_el_corpus(input_rdf: &Path, out_dir: &Path) -> gmeow_errors::Result<()> {
    vendor_lane_a_from_manifest(
        input_rdf,
        out_dir,
        "w3c-owl2-el",
        "https://www.w3.org/2009/11/owl-test/profile-EL.rdf",
        "curl -sSL https://www.w3.org/2009/11/owl-test/profile-EL.rdf -o .tmp/w3c-owl2/profile-EL.rdf && cargo run -p gmeow-conformance --bin ingest-external -- --vendor-el .tmp/w3c-owl2/profile-EL.rdf conformance/logic/cases/external/w3c-owl2-el",
        "W3C",
        "w3c-2009-11-archive",
    )
}

/// Vendor the self-authored, license-clean **entailment-mini** corpus: a small set of
/// W3C-`otest:`-style entailment tests carrying BOTH an inline premise and an inline
/// conclusion, so the native `dl_entails` reduction grades them end-to-end. Unlike the
/// upstream OWL 2 profile suites (whose entailment premises/conclusions are reference
/// documents the inline path cannot grade), this authored source is fully inline and
/// gives the entailment lane a non-vacuous, drift-free coverage floor.
fn vendor_entailment_corpus(input_rdf: &Path, out_dir: &Path) -> gmeow_errors::Result<()> {
    vendor_lane_a_from_manifest(
        input_rdf,
        out_dir,
        "entailment-mini",
        "gmeow-self-authored",
        "cargo run -p gmeow-conformance --bin ingest-external -- --vendor-entailment conformance/logic/cases/external/entailment-mini/_source.rdf conformance/logic/cases/external/entailment-mini",
        "CC-BY-4.0",
        "gmeow-self-authored-1",
    )
}

/// Vendor the **full** W3C OWL 2 test suite (`all.rdf`, DL/Full) as a Lane-A
/// corpus (plus its divergence sibling).  The native engine is EL/Horn, so the
/// agreeing subset it can soundly decide becomes graded Lane-A cases and every
/// DL/Full case it cannot decide (or that disagrees) is vendored to the
/// `w3c-owl2-full-divergence` sibling as honest, queryable divergence data.
fn vendor_full_corpus(input_rdf: &Path, out_dir: &Path) -> gmeow_errors::Result<()> {
    vendor_lane_a_from_manifest(
        input_rdf,
        out_dir,
        "w3c-owl2-full",
        "https://www.w3.org/2009/11/owl-test/all.rdf",
        "curl -sSL https://www.w3.org/2009/11/owl-test/all.rdf -o .tmp/w3c-owl2/all.rdf && cargo run -p gmeow-conformance --bin ingest-external -- --vendor-full .tmp/w3c-owl2/all.rdf conformance/logic/cases/external/w3c-owl2-full",
        "W3C",
        "w3c-2009-11-archive",
    )
}

/// Read the quarantine baseline — the set of case slugs in the committed
/// divergence corpus directory.  These are the accepted, honest DlGap cases
/// (currently the two `webont-thing-00{4,5}` EL gaps) whose divergence is
/// known and committed.  Every subdirectory of `quarantine_dir` that is itself
/// a directory is treated as one quarantined slug.
///
/// Returns the slug set, or an error if the directory cannot be read.
fn load_quarantine_slugs(quarantine_dir: &Path) -> gmeow_errors::Result<BTreeSet<String>> {
    let mut slugs = BTreeSet::new();
    let rd = std::fs::read_dir(quarantine_dir).map_err(|e| {
        ce(format!(
            "cannot read quarantine baseline dir {}: {e}",
            quarantine_dir.display()
        ))
    })?;
    for entry in rd {
        let entry = entry.map_err(|e| ce(format!("dir entry error in quarantine dir: {e}")))?;
        if entry.path().is_dir()
            && let Some(name) = entry.file_name().to_str()
        {
            slugs.insert(name.to_owned());
        }
    }
    Ok(slugs)
}

/// The soundness-scoped gate over one external-corpus grade run.
///
/// This is the invariant this whole grading lane protects:
///
/// * **`CorpusOnly` rows are always a soundness violation** — the native path
///   decided a verdict that disagrees with the published external ground truth.
///   Any `CorpusOnly` in the ledger is unexpected and causes a hard-fail.
///
/// * **`DlGap` rows split into three accepted classes and one unexpected class:**
///   - Entailment-test gaps: the test kind is outside the consistency lane
///     (needs conclusion-negation); their slugs arrive in `entailment_slugs`
///     and are always accepted.
///   - Quarantined consistency gaps: the native path honestly cannot decide a
///     DL/Full-divergent consistency case; their slugs are in `quarantine_slugs`
///     (the committed `w3c-owl2-el-divergence/` directory) and are accepted.
///   - All other `DlGap` rows are unexpected (a new gap appeared outside the
///     accepted scope) and cause a hard-fail.
///
/// Returns `Ok(())` when no unexpected divergences exist.  Returns `Err` with a
/// sorted, one-line-per-offender list when ANY unexpected divergence is found.
///
/// The function is small and side-effect-free so unit tests can drive it
/// directly without touching the filesystem or the reasoner.
pub fn soundness_gate(
    ledger: &gmeow_logic::reason::DivergenceLedger,
    entailment_slugs: &BTreeSet<String>,
    quarantine_slugs: &BTreeSet<String>,
) -> Result<(), Vec<String>> {
    let mut unexpected: Vec<String> = Vec::new();

    for row in &ledger.rows {
        match row.kind {
            gmeow_logic::reason::DivergenceKind::CorpusOnly => {
                // Always a soundness violation — the native path decided the WRONG
                // answer for a published external ground-truth case.
                unexpected.push(format!(
                    "CORPUS-ONLY case {:?} (world {:?}): {}",
                    row.subject, row.world, row.detail
                ));
            }
            gmeow_logic::reason::DivergenceKind::DlGap => {
                // A DlGap from an entailment test is out-of-scope for the
                // consistency lane — accepted regardless of quarantine.
                if entailment_slugs.contains(&row.subject) {
                    continue;
                }
                // A DlGap for a consistency case that is in the committed
                // quarantine baseline is an accepted, honest gap.
                if quarantine_slugs.contains(&row.subject) {
                    continue;
                }
                // Any other DlGap is unexpected: a new consistency gap appeared
                // outside the accepted scope.
                unexpected.push(format!(
                    "DL-GAP case {:?} (world {:?}) not in quarantine baseline: {}",
                    row.subject, row.world, row.detail
                ));
            }
            // Agree / NativeOnly / OracleOnly are not produced by
            // compare_external_corpus; ignore them here.
            _ => {}
        }
    }

    if unexpected.is_empty() {
        Ok(())
    } else {
        unexpected.sort();
        Err(unexpected)
    }
}

/// The default quarantine baseline directory for the W3C OWL 2 EL divergence
/// corpus, resolved at compile time relative to this crate's manifest.
fn default_quarantine_dir() -> PathBuf {
    gmeow_conformance::paths::cases_root()
        .join("external")
        .join("w3c-owl2-el-divergence")
}

/// The committed accepted-DlGap quarantine baseline directory for a graded corpus.
///
/// The EL-profile grade keeps the tight, exactly-pinned `w3c-owl2-el-divergence`
/// baseline (two honest `owl:Thing oneOf {singleton}` gaps; the on-gate
/// `el_divergence_gate` asserts its membership is EXACTLY those two). The full
/// OWL 2 grade — which runs a Horn/EL/DL-clash reasoner against the entire W3C
/// OWL 2 test suite, DL and Full profiles included — needs its own, larger
/// baseline: every DL/Full case whose (in)consistency turns on
/// reasoning-by-contradiction the native chase honestly withholds (a non-empty
/// `DlVerdict::gaps`), plus the premise-unlowerable cases. Keeping the two
/// baselines in separate directories lets each grade pin its own accepted set
/// without cross-contaminating the EL exactness gate.
fn quarantine_dir_for(corpus_name: &str) -> PathBuf {
    match corpus_name {
        "w3c-owl2-full" => gmeow_conformance::paths::cases_root()
            .join("external")
            .join("w3c-owl2-full-divergence"),
        _ => default_quarantine_dir(),
    }
}

/// The declared status token of a TPTP problem, tolerant of both the `% SZS status`
/// result-comment form (used by the vendored corpora) and the real-distribution
/// header field `% Status : <Token>`. Returns the raw SZS token.
fn tptp_declared_status(text: &str) -> gmeow_errors::Result<String> {
    if let Ok(token) = parse_szs_status(text) {
        return Ok(token);
    }
    // TPTP distribution header: `% Status   : Theorem`.
    for line in text.lines() {
        let Some(rest) = line.trim_start().strip_prefix('%') else {
            continue;
        };
        let rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("Status")
            && let Some(colon) = after.trim_start().strip_prefix(':')
            && let Some(token) = colon.split_whitespace().next()
        {
            return Ok(token.to_string());
        }
    }
    Err(ce(
        "no `% SZS status` or `% Status :` line found in the TPTP problem".to_string(),
    ))
}

/// Recursively collect `*.p` TPTP problem files under `dir`, sorted.
fn collect_tptp_problems(dir: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in
            std::fs::read_dir(&d).map_err(|e| ce(format!("read_dir {}: {e}", d.display())))?
        {
            let entry = entry.map_err(|e| ce(e.to_string()))?;
            // `file_type()` reuses the directory-walk's `stat` rather than issuing a
            // second one per entry, and does not traverse symlinks — so a circular
            // link cannot drive the walk into an infinite loop.
            let file_type = entry
                .file_type()
                .map_err(|e| ce(format!("file_type {}: {e}", entry.path().display())))?;
            let path = entry.path();
            if file_type.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|s| s.to_str()) == Some("p") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Lane-B: grade a live-fetched subset of the real TPTP distribution against the
/// native FOL path (parse → FOL-negation reduction → EL/DL lowering →
/// `dl_consistency`), gap-tolerantly, and write the divergences as a `gmeow:Finding`
/// N-Quads graph to `out_nq`.
///
/// Every problem's declared `% SZS status` (or distribution `% Status :`) is compared
/// to the native decision. A problem the native EL/DL fragment cannot decide (a
/// well-formed `Unsupported`/`LoweringGap` capability gap) is recorded with a native
/// `incomplete` token — which the ledger classifies as a **`DlGap`** row — and its
/// reason is disclosed to stderr, an honest "our engine cannot express this", never a
/// silent pass. A malformed source (`TptpError::Syntax`) is instead a HARD FAIL: a
/// corrupt/mis-fetched problem is a corpus defect, not a capability gap. This is the
/// non-required, network/Docker-allowed lane
/// (the real TPTP distribution has per-problem licenses and is never vendored); it is
/// the documented path from the tiny Lane-A `tptp-mini` corpus to the full set.
fn grade_tptp_corpus(
    problem_dir: &Path,
    corpus_name: &str,
    out_nq: &Path,
) -> gmeow_errors::Result<()> {
    let problems = collect_tptp_problems(problem_dir)?;
    if problems.is_empty() {
        return Err(ce(format!(
            "no *.p TPTP problems found under {}",
            problem_dir.display()
        )));
    }

    let mut comparisons: Vec<gmeow_logic::reason::ExternalComparison> = Vec::new();
    let mut capability_gaps = 0usize;
    for problem in &problems {
        let text = std::fs::read_to_string(problem)
            .map_err(|e| ce(format!("cannot read {}: {e}", problem.display())))?;
        let raw_token =
            tptp_declared_status(&text).map_err(|e| ce(format!("{}: {e}", problem.display())))?;
        let published = gmeow_conformance::external::outcome_for_szs(&raw_token)
            .map_err(|e| ce(e.to_string()))?
            .verdict_status()
            .as_str()
            .to_string();

        let slug = problem
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("problem")
            .to_string();
        let world = format!("https://gmeow.example/{corpus_name}/{slug}/w");

        // Gap-tolerant native decision. A well-formed but out-of-fragment problem
        // — `Unsupported` at parse, or a `LoweringGap` at decision — is an honest
        // capability gap → a native `incomplete` token → a DlGap ledger row, with
        // the reason disclosed to stderr (maximal information flow), never a silent
        // pass. A malformed source (`TptpError::Syntax`) is a different thing: a
        // corpus-authoring defect, not a capability gap — so it is a HARD FAIL,
        // consistent with the honest-gap gate and the Lane-A vendor path (a broken
        // download must never masquerade as a benign fragment boundary).
        let native = match parse_tptp(&text) {
            Ok(formulas) => match lower_and_decide(&formulas, &world) {
                Ok((outcome, _)) => outcome.verdict_status().as_str().to_string(),
                Err(gap) => {
                    println!("{}: capability gap: {}", problem.display(), gap.reason);
                    capability_gaps += 1;
                    "incomplete".to_string()
                }
            },
            Err(TptpError::Syntax(m)) => {
                return Err(ce(format!("{}: malformed TPTP: {m}", problem.display())));
            }
            Err(TptpError::Unsupported(m)) => {
                println!(
                    "{}: capability gap (out of fragment): {m}",
                    problem.display()
                );
                capability_gaps += 1;
                "incomplete".to_string()
            }
        };

        comparisons.push(gmeow_logic::reason::ExternalComparison {
            case: slug,
            world,
            native,
            published,
        });
    }

    let nq = gmeow_conformance::divergence::emit_divergence_nq(corpus_name, &comparisons);
    if let Some(parent) = out_nq.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ce(format!(
                "cannot create output dir {}: {e}",
                parent.display()
            ))
        })?;
    }
    std::fs::write(out_nq, &nq)
        .map_err(|e| ce(format!("cannot write {}: {e}", out_nq.display())))?;

    let rows = gmeow_logic::reason::compare_external_corpus(corpus_name, &comparisons);
    let ledger = gmeow_logic::reason::build_ledger(Vec::new(), Vec::new(), Vec::new(), rows);
    println!(
        "graded={} agree={} corpus_only={} dl_gap={} capability_gaps={capability_gaps}",
        comparisons.len(),
        ledger.agree,
        ledger.corpus_only,
        ledger.dl_gap
    );
    Ok(())
}

/// Recursively collect OntoUML model files (named `ontology.ttl` or `model.ttl`)
/// under `dir`, sorted.
///
/// `dir` itself is the REQUIRED root catalog: if it does not exist this is a hard
/// failure (a missing required root must never silently degrade to "zero models"
/// per the no-optionality rule), so its `read_dir` is issued up front and any
/// `NotFound` propagates as an error. Subdirectories discovered during the
/// recursive walk are a different matter — one vanishing between being listed and
/// being read is a benign mid-walk race, so `NotFound` for those is tolerated as an
/// empty listing. Every other IO error still propagates.
///
/// Symlink-safe: `file_type()` reuses the directory-walk's `stat` and does not
/// traverse symlinks, so a circular link cannot drive the walk into an infinite loop
/// (mirrors `collect_tptp_problems`).
fn collect_ontouml_models(dir: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    // The root catalog is REQUIRED: read it eagerly so a missing root propagates as
    // an error before the tolerant walk loop below ever gets a chance to swallow it.
    let root_read = std::fs::read_dir(dir).map_err(|e| {
        ce(format!(
            "required OntoUML root catalog missing: read_dir {}: {e}",
            dir.display()
        ))
    })?;
    let mut root_reads = vec![root_read];
    let mut stack: Vec<PathBuf> = Vec::new();
    loop {
        let read = if let Some(read) = root_reads.pop() {
            read
        } else if let Some(d) = stack.pop() {
            // A subdirectory discovered mid-walk vanishing before it is read is a
            // benign race (unlike a missing root), so treat `NotFound` as empty.
            match std::fs::read_dir(&d) {
                Ok(read) => read,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(ce(format!("read_dir {}: {e}", d.display()))),
            }
        } else {
            break;
        };
        for entry in read {
            let entry = entry.map_err(|e| ce(e.to_string()))?;
            let file_type = entry
                .file_type()
                .map_err(|e| ce(format!("file_type {}: {e}", entry.path().display())))?;
            let path = entry.path();
            if file_type.is_dir() {
                stack.push(path);
            } else if matches!(
                path.file_name().and_then(|s| s.to_str()),
                Some("ontology.ttl" | "model.ttl")
            ) {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Read a declared license from a model's sibling `metadata.ttl` (if present),
/// returning the first `dcterms:` / `dcat:` / `schema:` license object as its bare
/// IRI or literal lexical form. Returns `None` when no metadata file or no license
/// triple is found. This is a provenance disclosure (never a gate): Lane-B grades
/// live and commits nothing.
fn read_ontouml_license(model_path: &Path) -> gmeow_errors::Result<Option<String>> {
    let Some(dir) = model_path.parent() else {
        return Ok(None);
    };
    let metadata = dir.join("metadata.ttl");
    if !metadata.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&metadata)
        .map_err(|e| ce(format!("cannot read {}: {e}", metadata.display())))?;
    // Resolve relative IRIs against the file's ABSOLUTE location (mirrors the vendor
    // path): a live catalog `metadata.ttl` commonly declares its license with a
    // document-relative IRI, which a `None` base would drop or mis-parse.
    let abs = std::path::absolute(&metadata)
        .map_err(|e| ce(format!("cannot resolve {}: {e}", metadata.display())))?;
    let base = format!("file://{}", abs.display());
    let ds = purrdf::parse_dataset(text.as_bytes(), "text/turtle", Some(&base))
        .map_err(|e| ce(format!("cannot parse {}: {e}", metadata.display())))?;
    const LICENSE_PREDS: [&str; 3] = [
        "http://purl.org/dc/terms/license",
        "http://www.w3.org/ns/dcat#license",
        "http://schema.org/license",
    ];
    for q in ds.quad_refs() {
        let TermRef::Iri(pred) = q.p else { continue };
        if !LICENSE_PREDS.contains(&pred) {
            continue;
        }
        match q.o {
            TermRef::Iri(iri) => return Ok(Some(iri.to_owned())),
            TermRef::Literal { lexical, .. } => return Ok(Some(lexical.to_owned())),
            _ => continue,
        }
    }
    Ok(None)
}

/// Lane-B: grade a live-fetched FAIR OntoUML/UFO catalog against the native
/// foundation disciplines (OntoUML metamodel → `logic:` stereotype lowering →
/// `gmeow_logic::foundation::evaluate`), gap-tolerantly, and write the divergences as
/// a `gmeow:Finding` N-Quads graph to `out_nq`.
///
/// The real catalog ships NO per-model documented anti-pattern, so the null
/// hypothesis is `clean`: every fired discipline surfaces as a `CorpusOnly` finding
/// for review (native != published `"clean"`), and every un-lowerable model — a
/// well-formed `Unsupported` fragment boundary at parse or lower time — is an honest
/// capability gap recorded with a native `incomplete` token (a `DlGap` row), its
/// reason disclosed to stderr, never a silent pass. A malformed source
/// (`OntoumlError::Syntax`) is instead a HARD FAIL: a corrupt/mis-fetched model is a
/// corpus defect, not a capability gap. Each model's sibling `metadata.ttl` license is
/// audited and disclosed (never a skip — the audit is provenance, not a gate; Lane-B
/// grades live and commits nothing). This is the non-required, network-allowed lane
/// (the real catalog has per-model licenses and is never vendored); it is the
/// documented path from the tiny Lane-A `ontouml-mini` corpus to the full set.
fn grade_ontouml_corpus(
    catalog_dir: &Path,
    corpus_name: &str,
    out_nq: &Path,
) -> gmeow_errors::Result<()> {
    let models = collect_ontouml_models(catalog_dir)?;
    if models.is_empty() {
        return Err(ce(format!(
            "no ontology.ttl / model.ttl OntoUML models found under {} — populate the \
             directory (or set the catalog subset URL) before grading",
            catalog_dir.display()
        )));
    }

    let mut comparisons: Vec<gmeow_logic::reason::ExternalComparison> = Vec::new();
    let mut capability_gaps = 0usize;
    for model_path in &models {
        // License audit + disclosure. NOT a skip: Lane-B grades live and commits
        // nothing, so a ReferenceOnly license is disclosed provenance, not a gate.
        match read_ontouml_license(model_path)? {
            Some(license) => {
                let policy = match policy_for_license(&license) {
                    LicensePolicy::ImportOk => "ImportOk",
                    LicensePolicy::ReferenceOnly => "ReferenceOnly",
                };
                println!("LICENSE {}: {license} → {policy}", model_path.display());
            }
            None => println!("LICENSE {}: unknown", model_path.display()),
        }

        let text = std::fs::read_to_string(model_path)
            .map_err(|e| ce(format!("cannot read {}: {e}", model_path.display())))?;

        // Slug = a sanitized catalog-relative path (deterministic, collision-free).
        let rel = model_path.strip_prefix(catalog_dir).unwrap_or(model_path);
        let slug = to_slug(&rel.to_string_lossy());
        let world = format!("https://gmeow.example/{corpus_name}/{slug}/w");

        // Resolve relative IRIs against the model's ABSOLUTE location (mirrors the
        // vendor path): real FAIR-catalog Turtle commonly uses document-relative IRIs,
        // which a `None` base would mis-parse into the wrong subjects.
        let abs = std::path::absolute(model_path)
            .map_err(|e| ce(format!("cannot resolve {}: {e}", model_path.display())))?;
        let base = format!("file://{}", abs.display());

        // Gap-tolerant native discipline verdict. A well-formed but out-of-fragment
        // model — `Unsupported` at parse or lower time — is an honest capability gap
        // → a native `incomplete` token → a DlGap ledger row, with the reason
        // disclosed to stderr, never a silent pass. A malformed source
        // (`OntoumlError::Syntax`) is a corpus-authoring defect → a HARD FAIL.
        let model = match parse_ontouml_model(&text, Some(&base)) {
            Ok(m) => m,
            Err(OntoumlError::Syntax(m)) => {
                return Err(ce(format!(
                    "{}: malformed OntoUML: {m}",
                    model_path.display()
                )));
            }
            Err(OntoumlError::Unsupported(reason)) => {
                println!(
                    "{}: capability gap (out of fragment): {reason}",
                    model_path.display()
                );
                capability_gaps += 1;
                comparisons.push(gmeow_logic::reason::ExternalComparison {
                    case: slug,
                    world,
                    native: "incomplete".to_string(),
                    published: "clean".to_string(),
                });
                continue;
            }
        };

        let native = match lower_and_evaluate(&model, &world, AntiRigidityPolicy::WitnessObligation)
        {
            Ok((fq, _nq, _count)) => {
                let fired = fired_disciplines(&fq);
                native_verdict_string(None, &fired)
            }
            Err(OntoumlError::Syntax(m)) => {
                return Err(ce(format!(
                    "{}: OntoUML lowering/evaluation failed: {m}",
                    model_path.display()
                )));
            }
            Err(OntoumlError::Unsupported(reason)) => {
                println!("{}: capability gap: {reason}", model_path.display());
                capability_gaps += 1;
                "incomplete".to_string()
            }
        };

        comparisons.push(gmeow_logic::reason::ExternalComparison {
            case: slug,
            world,
            native,
            published: "clean".to_string(),
        });
    }

    let nq = gmeow_conformance::divergence::emit_divergence_nq(corpus_name, &comparisons);
    if let Some(parent) = out_nq.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ce(format!(
                "cannot create output dir {}: {e}",
                parent.display()
            ))
        })?;
    }
    std::fs::write(out_nq, &nq)
        .map_err(|e| ce(format!("cannot write {}: {e}", out_nq.display())))?;

    let rows = gmeow_logic::reason::compare_external_corpus(corpus_name, &comparisons);
    let ledger = gmeow_logic::reason::build_ledger(Vec::new(), Vec::new(), Vec::new(), rows);
    println!(
        "graded={} agree={} corpus_only={} dl_gap={} capability_gaps={capability_gaps}",
        comparisons.len(),
        ledger.agree,
        ledger.corpus_only,
        ledger.dl_gap
    );
    Ok(())
}

/// Grade every ConsistencyTest / InconsistencyTest in `input_rdf` gap-tolerantly
/// against the native reasoner and write divergences as a `gmeow:Finding` N-Quads
/// graph to `out_nq`.
///
/// Unlike `vendor_el_corpus` (Lane-A, strict agree-only), this mode records EVERY
/// case outcome — including `DlGap` (native incomplete) and `CorpusOnly` (native
/// disagrees with published) — as the divergence grading signal.
fn grade_suite_corpus(
    input_rdf: &Path,
    corpus_name: &str,
    out_nq: &Path,
) -> gmeow_errors::Result<()> {
    let (consistency_entries, entailment_entries) = parse_consistency_entries(input_rdf)?;
    let entailment_skipped = entailment_entries.len();

    let mut comparisons: Vec<gmeow_logic::reason::ExternalComparison> = Vec::new();
    let mut unlowerable: usize = 0;

    let world_iri_prefix = format!("https://gmeow.example/{corpus_name}/");

    // Track which slugs came from entailment tests: these produce DlGap rows
    // that are accepted by the soundness gate (out of consistency-lane scope).
    let mut entailment_slugs: BTreeSet<String> = BTreeSet::new();

    // Emit a DlGap Finding for every entailment test: they are out of the
    // consistency-lane scope (need conclusion-negation), so we record them
    // as coverage gaps rather than silently dropping them.
    for entry in &entailment_entries {
        let slug = to_slug(&entry.name);
        let world_iri = format!("{world_iri_prefix}{slug}/w");
        let published = entry.outcome().verdict_status().as_str().to_string();
        entailment_slugs.insert(slug.clone());
        comparisons.push(gmeow_logic::reason::ExternalComparison {
            case: slug,
            world: world_iri,
            native: "incomplete".to_string(),
            published,
        });
    }

    for entry in &consistency_entries {
        let slug = to_slug(&entry.name);
        let world_iri = format!("{world_iri_prefix}{slug}/w");

        let lowered = match lower_entry(entry, &world_iri_prefix) {
            Some(l) => l,
            None => {
                // Record as DlGap rather than silently dropping: the native path
                // could not ingest the premise (IRI reference, vacuous, or unparsable).
                let published = entry.outcome().verdict_status().as_str().to_string();
                comparisons.push(gmeow_logic::reason::ExternalComparison {
                    case: slug.clone(),
                    world: world_iri,
                    native: "incomplete".to_string(),
                    published,
                });
                unlowerable += 1;
                continue;
            }
        };
        let LoweredEntry {
            slug,
            world_iri,
            input_nq,
            ..
        } = lowered;

        // Round-trip into a parsed dataset for dl_consistency.
        let world_ds = match purrdf::dataset_from_bytes(
            input_nq.as_bytes(),
            purrdf::NativeRdfFormat::NQuads,
        ) {
            Ok(ds) => ds,
            Err(e) => {
                println!("SKIP {slug}: world N-Quads round-trip failed: {e}");
                let published = entry.outcome().verdict_status().as_str().to_string();
                comparisons.push(gmeow_logic::reason::ExternalComparison {
                    case: slug.clone(),
                    world: world_iri,
                    native: "incomplete".to_string(),
                    published,
                });
                unlowerable += 1;
                continue;
            }
        };

        // Run the native DL consistency path.
        let verdict = match gmeow_logic::reason::dl_consistency(world_ds.as_ref()) {
            Ok(v) => v,
            Err(e) => {
                println!("SKIP {slug}: native DL consistency run failed: {e}");
                let published = entry.outcome().verdict_status().as_str().to_string();
                comparisons.push(gmeow_logic::reason::ExternalComparison {
                    case: slug.clone(),
                    world: world_iri,
                    native: "incomplete".to_string(),
                    published,
                });
                unlowerable += 1;
                continue;
            }
        };

        // Compute the native token gap-tolerantly: gaps → "incomplete".
        let native_token = if !verdict.gaps.is_empty() {
            "incomplete".to_string()
        } else if verdict.consistent {
            "consistent".to_string()
        } else {
            "inconsistent".to_string()
        };

        let published = entry.outcome().verdict_status().as_str().to_string();

        comparisons.push(gmeow_logic::reason::ExternalComparison {
            case: slug,
            world: world_iri,
            native: native_token,
            published,
        });
    }

    // Build the divergence graph.
    let nq = gmeow_conformance::divergence::emit_divergence_nq(corpus_name, &comparisons);

    // Write the output file (create parent dirs).
    if let Some(parent) = out_nq.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ce(format!(
                "cannot create output dir {}: {e}",
                parent.display()
            ))
        })?;
    }
    std::fs::write(out_nq, &nq)
        .map_err(|e| ce(format!("cannot write {}: {e}", out_nq.display())))?;

    // Derive counts from the ledger so they match the emitted graph exactly.
    let rows = gmeow_logic::reason::compare_external_corpus(corpus_name, &comparisons);
    let ledger = gmeow_logic::reason::build_ledger(Vec::new(), Vec::new(), Vec::new(), rows);

    let graded = comparisons.len();
    let agree = ledger.agree;
    let corpus_only = ledger.corpus_only;
    let dl_gap = ledger.dl_gap;

    println!(
        "graded={graded} agree={agree} corpus_only={corpus_only} dl_gap={dl_gap} entailment_skipped={entailment_skipped} unlowerable={unlowerable}"
    );

    // ── Invoke the strict enforce() authority and surface its reasons ─────────
    //
    // enforce() is the canonical gate for the classic native↔oracle cross-check;
    // it fails on ANY DlGap — including the accepted entailment and quarantined
    // consistency gaps.  We always call it so its structured reasons are printed
    // (surfaced, not bypassed), but the PASS/FAIL decision for this lane is made
    // by soundness_gate(), which applies the finer-grained soundness floor.
    let verdict = gmeow_logic::reason::enforce(&ledger);
    if !verdict.reasons.is_empty() {
        println!(
            "enforce() reasons ({}):",
            if verdict.passed { "PASS" } else { "FAIL" }
        );
        for reason in &verdict.reasons {
            println!("  - {reason}");
        }
    } else {
        println!("enforce(): PASS (no divergences)");
    }

    // ── Soundness gate: the invariant this grading lane protects ─────────────
    //
    // corpus_only == 0: the native path must NEVER decide a verdict that
    // disagrees with the published external ground truth.  A corpus_only > 0 is
    // a wrong decided answer and is always a hard-fail.
    //
    // DlGap rows are accepted in two cases:
    //   - Entailment-test DlGaps: out-of-scope for the consistency lane
    //     (need conclusion-negation; the test kind is not a consistency check).
    //   - Quarantined consistency DlGaps: honest gaps committed in the
    //     `w3c-owl2-el-divergence/` baseline (the native path cannot soundly
    //     decide a DL/Full-divergent case for this specific slug).
    // All other DlGap rows are unexpected and cause a hard-fail.
    let quarantine_dir = quarantine_dir_for(corpus_name);
    let quarantine_slugs = load_quarantine_slugs(&quarantine_dir)?;

    soundness_gate(&ledger, &entailment_slugs, &quarantine_slugs).map_err(|offenders| {
        let list = offenders.join("\n  ");
        ce(format!(
            "soundness gate FAILED: {n} unexpected divergence(s):\n  {list}",
            n = offenders.len()
        ))
    })
}

/// Detect whether ontology bytes are OWL 2 Functional Syntax.
///
/// The ORE 2015 corpus ships every ontology in OWL 2 Functional Syntax
/// (`Prefix(...)` / `Ontology(...)` headers). The native RDF codecs (Turtle / TriG /
/// N-Triples / N-Quads / RDF/XML) cannot read that surface, so a Functional-Syntax
/// ontology is an honest format gap (recorded as a DlGap Finding), never silently
/// skipped. This is a lexical sniff over the leading non-blank/non-comment line:
/// a Functional-Syntax document opens with `Prefix(` or `Ontology(` (possibly after
/// `# comment` lines), whereas an RDF/XML document opens with `<?xml` or `<rdf:RDF`.
fn is_owl_functional_syntax(bytes: &[u8]) -> bool {
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        // Non-UTF-8 bytes are not Functional Syntax; let the RDF/XML parser report
        // the real error so it lands as a parse-failure DlGap with a real message.
        Err(_) => return false,
    };
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        return trimmed.starts_with("Prefix(") || trimmed.starts_with("Ontology(");
    }
    false
}

/// One ORE-ontology grade outcome: the comparison row plus the divergence class it
/// fell into (so the summary can tally without re-deriving from the ledger).
enum OreOutcome {
    /// Native agreed with the curated-consistent expected (`consistent`).
    Agree,
    /// Native could not parse or could not decide → honest coverage gap.
    DlGap,
    /// Native decided `inconsistent` on a curated-consistent ontology → soundness flag.
    CorpusOnly,
}

/// Grade one ORE ontology file against the native DL consistency path.
///
/// Returns the divergence comparison (native vs. the curated-consistent `published`)
/// and the outcome class. ORE ships OWL 2 Functional Syntax (unreadable by the native
/// codecs) and provides no per-ontology reference verdict, so:
///   * Functional-Syntax / unparsable / undecidable → native `"incomplete"` (DlGap);
///   * native decided `consistent` → Agree;
///   * native decided `inconsistent` → `"inconsistent"` vs. published `"consistent"`
///     (CorpusOnly — a soundness flag, the only hard-fail condition for ORE).
fn grade_ore_ontology(
    path: &Path,
    slug: &str,
    world_iri: &str,
) -> (gmeow_logic::reason::ExternalComparison, OreOutcome) {
    // ORE real-world ontologies are curated-consistent; the distribution ships no
    // per-ontology reference verdict, so the published expected is `consistent`.
    let published = "consistent".to_string();
    let mut comparison = gmeow_logic::reason::ExternalComparison {
        case: slug.to_string(),
        world: world_iri.to_string(),
        native: "incomplete".to_string(),
        published: published.clone(),
    };

    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            println!("DL-GAP {slug}: cannot read ontology file: {e}");
            return (comparison, OreOutcome::DlGap);
        }
    };

    // Format gap: OWL 2 Functional Syntax is not in the native codec surface.
    // Recorded as a coverage DlGap with the named format, never a silent skip.
    if is_owl_functional_syntax(&bytes) {
        println!("DL-GAP {slug}: OWL 2 Functional Syntax (native RDF codecs cannot parse)");
        return (comparison, OreOutcome::DlGap);
    }

    // Parse as RDF/XML (the only OWL-bearing surface the native codecs read).
    let ds = match purrdf::parse_dataset(&bytes, "application/rdf+xml", Some("http://example.org/"))
    {
        Ok(ds) => ds,
        Err(e) => {
            println!("DL-GAP {slug}: ontology unparsable as RDF/XML: {e}");
            return (comparison, OreOutcome::DlGap);
        }
    };
    if ds.quad_refs().count() == 0 {
        println!("DL-GAP {slug}: ontology parsed to zero quads (vacuous, not graded)");
        return (comparison, OreOutcome::DlGap);
    }

    let verdict = match gmeow_logic::reason::dl_consistency(ds.as_ref()) {
        Ok(v) => v,
        Err(e) => {
            println!("DL-GAP {slug}: native DL consistency run failed: {e}");
            return (comparison, OreOutcome::DlGap);
        }
    };

    if !verdict.gaps.is_empty() {
        println!("DL-GAP {slug}: native could not decide (coverage gap)");
        return (comparison, OreOutcome::DlGap);
    }

    if verdict.consistent {
        comparison.native = "consistent".to_string();
        println!("AGREE {slug}: native consistent");
        (comparison, OreOutcome::Agree)
    } else {
        comparison.native = "inconsistent".to_string();
        println!(
            "CORPUS-ONLY {slug}: native inconsistent, ORE curated-consistent (soundness flag)"
        );
        (comparison, OreOutcome::CorpusOnly)
    }
}

/// Grade every `*.owl` ontology under `ontology_dir` against the native DL
/// consistency path and write divergences as a `gmeow:Finding` N-Quads graph.
///
/// The ORE 2015 corpus (Zenodo DOI 10.5281/zenodo.18578) is curated-consistent and
/// ships OWL 2 Functional Syntax with NO per-ontology reference verdict. We grade for
/// SOUNDNESS: the published expected is `consistent` for every ontology, a native
/// `inconsistent` is a CorpusOnly soundness violation (hard-fail), and every ontology
/// the native path cannot parse (Functional Syntax) or decide is recorded as an honest
/// DlGap Finding — never a silent skip. ORE is fetched-not-vendored for benchmarking
/// use only (the corpus license forbids redistribution), so nothing here is committed.
fn grade_ore_corpus(
    ontology_dir: &Path,
    corpus_name: &str,
    out_nq: &Path,
) -> gmeow_errors::Result<()> {
    // Collect `*.owl` ontology files deterministically (sorted by file name).
    let mut owl_files: Vec<PathBuf> = Vec::new();
    let rd = std::fs::read_dir(ontology_dir).map_err(|e| {
        ce(format!(
            "cannot read ORE ontology dir {}: {e}",
            ontology_dir.display()
        ))
    })?;
    for entry in rd {
        let entry = entry.map_err(|e| {
            ce(format!(
                "dir entry error in {}: {e}",
                ontology_dir.display()
            ))
        })?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("owl") {
            owl_files.push(path);
        }
    }
    owl_files.sort();

    if owl_files.is_empty() {
        return Err(ce(format!(
            "no *.owl ontologies under {} — refusing a vacuous ORE grade (a broken extract \
             must hard-fail, not silently pass)",
            ontology_dir.display()
        )));
    }

    let world_iri_prefix = format!("https://gmeow.example/{corpus_name}/");

    let mut comparisons: Vec<gmeow_logic::reason::ExternalComparison> = Vec::new();
    // Every DlGap slug is an ACCEPTED coverage gap for ORE: the corpus is graded for
    // soundness only, and the Functional-Syntax / undecidable gaps are the expected,
    // honest output (recorded as data), not a regression. Only CorpusOnly hard-fails.
    let mut dl_gap_slugs: BTreeSet<String> = BTreeSet::new();
    let mut graded = 0usize;
    let mut agree = 0usize;
    let mut dl_gap = 0usize;
    let mut corpus_only = 0usize;

    for path in &owl_files {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ce(format!("non-UTF-8 ontology file name: {}", path.display())))?;
        let slug = to_slug(stem);
        let world_iri = format!("{world_iri_prefix}{slug}/w");

        let (comparison, outcome) = grade_ore_ontology(path, &slug, &world_iri);
        graded += 1;
        match outcome {
            OreOutcome::Agree => agree += 1,
            OreOutcome::DlGap => {
                dl_gap += 1;
                dl_gap_slugs.insert(slug.clone());
            }
            OreOutcome::CorpusOnly => corpus_only += 1,
        }
        comparisons.push(comparison);
    }

    // Build + write the divergence graph (DlGap + CorpusOnly rows become Findings).
    let nq = gmeow_conformance::divergence::emit_divergence_nq(corpus_name, &comparisons);
    if let Some(parent) = out_nq.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ce(format!(
                "cannot create output dir {}: {e}",
                parent.display()
            ))
        })?;
    }
    std::fs::write(out_nq, &nq)
        .map_err(|e| ce(format!("cannot write {}: {e}", out_nq.display())))?;

    println!("graded={graded} agree={agree} corpus_only={corpus_only} dl_gap={dl_gap}");

    // ── Soundness gate ────────────────────────────────────────────────────────
    //
    // ORE has no manifest entailment tests and no committed quarantine baseline, so
    // the accepted-DlGap set is the full set of coverage gaps observed this run: a
    // Functional-Syntax or undecidable gap is the EXPECTED honest output for ORE, not
    // a regression. The gate's teeth for ORE are CorpusOnly == 0 — a native
    // `inconsistent` verdict on a curated-consistent real-world ontology is a
    // soundness defect and the only hard-fail condition.
    let rows = gmeow_logic::reason::compare_external_corpus(corpus_name, &comparisons);
    let ledger = gmeow_logic::reason::build_ledger(Vec::new(), Vec::new(), Vec::new(), rows);
    soundness_gate(&ledger, &BTreeSet::new(), &dl_gap_slugs).map_err(|offenders| {
        let list = offenders.join("\n  ");
        ce(format!(
            "ORE soundness gate FAILED: {n} unexpected divergence(s):\n  {list}",
            n = offenders.len()
        ))
    })
}

/// Read the value following a flag, or error with the flag name.
fn next(args: &mut impl Iterator<Item = String>, flag: &str) -> gmeow_errors::Result<String> {
    args.next()
        .ok_or_else(|| ce(format!("{flag} requires a value")))
}

#[cfg(test)]
mod tests {
    /// A live catalog `metadata.ttl` commonly declares its license with a
    /// document-relative IRI. `read_ontouml_license` must resolve it against the
    /// file's absolute location (regression for the earlier `None` base URI, which
    /// left the license unresolved or mis-parsed).
    #[test]
    fn read_ontouml_license_resolves_relative_iri_against_file_base() {
        // Self-contained temp dir (tempfile is not a dep of this crate); the process
        // id keeps concurrent nextest cases from colliding.
        let dir =
            std::env::temp_dir().join(format!("gmeow-ontouml-license-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("metadata.ttl"),
            "@prefix dcterms: <http://purl.org/dc/terms/> .\n\
             <> dcterms:license <LICENSE> .\n",
        )
        .unwrap();
        // read_ontouml_license derives `metadata.ttl` from the model's parent dir.
        let model_path = dir.join("model.ttl");
        let result = super::read_ontouml_license(&model_path);
        let _ = std::fs::remove_dir_all(&dir);
        let license = result
            .expect("license read must not error")
            .expect("a declared license must be found");
        assert!(
            license.starts_with("file://") && license.ends_with("/LICENSE"),
            "relative license IRI must resolve against the absolute file:// base, got {license:?}"
        );
    }

    /// `premise_ds_to_world_nquads` is not `pub`, so we test the same logic via a
    /// local helper that mirrors the fixed conversion exactly.  This keeps the test
    /// small and fast (no RDF parser, no reasoner).
    fn nt_lines_to_nquads(nt_text: &str, world_iri: &str) -> gmeow_errors::Result<Vec<String>> {
        nt_text
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
            .map(|line| {
                let trimmed = line.trim_end();
                let without_dot = trimmed.strip_suffix('.').ok_or_else(|| {
                    super::ce(format!(
                        "malformed N-Triples line (no trailing '.'): {line}"
                    ))
                })?;
                let body = without_dot.trim_end();
                Ok(format!("{body} <{world_iri}> ."))
            })
            .collect::<gmeow_errors::Result<Vec<String>>>()
    }

    const WORLD: &str = "https://gmeow.example/test/w";

    /// A normal N-Triples line (space before dot) must produce a well-formed N-Quad:
    /// exactly one graph IRI and one trailing dot, no double terminator.
    #[test]
    fn normal_nt_line_produces_well_formed_quad() {
        let nt = "_:s <http://example.org/p> <http://example.org/o> .\n";
        let quads = nt_lines_to_nquads(nt, WORLD).expect("must succeed");
        assert_eq!(quads.len(), 1);
        let q = &quads[0];
        // Must end with exactly ` .`
        assert!(q.ends_with(" ."), "quad must end with ' .': {q:?}");
        // Must contain the world IRI
        assert!(q.contains(WORLD), "quad must contain world IRI: {q:?}");
        // Must NOT contain double terminator `. <`
        assert!(
            !q.contains(". <"),
            "quad must not contain double terminator '. <': {q:?}"
        );
    }

    /// A line ending with `. ` (dot + trailing space) — the previously buggy case —
    /// must also produce a well-formed N-Quad with no double terminator.
    #[test]
    fn trailing_space_after_dot_produces_well_formed_quad() {
        // Dot followed by a trailing space (the bug trigger).
        let nt = "_:s <http://example.org/p> <http://example.org/o> . \n";
        let quads = nt_lines_to_nquads(nt, WORLD).expect("must succeed");
        assert_eq!(quads.len(), 1);
        let q = &quads[0];
        assert!(q.ends_with(" ."), "quad must end with ' .': {q:?}");
        assert!(q.contains(WORLD), "quad must contain world IRI: {q:?}");
        assert!(
            !q.contains(". <"),
            "quad must not contain double terminator '. <': {q:?}"
        );
    }

    /// A mix of normal, trailing-space, blank, and comment lines: only the two
    /// data lines must appear in the output, both well-formed.
    #[test]
    fn mixed_input_skips_blanks_and_comments() {
        let nt = "\
_:a <http://example.org/p> <http://example.org/o1> .\n\
# a comment line\n\
\n\
_:b <http://example.org/p> <http://example.org/o2> . \n\
";
        let quads = nt_lines_to_nquads(nt, WORLD).expect("must succeed");
        assert_eq!(quads.len(), 2, "expected exactly 2 quads: {quads:?}");
        for q in &quads {
            assert!(q.ends_with(" ."), "quad must end with ' .': {q:?}");
            assert!(!q.contains(". <"), "double terminator in: {q:?}");
        }
    }

    /// A line with NO trailing dot at all must hard-fail with a descriptive error,
    /// not silently produce a malformed quad.
    #[test]
    fn missing_trailing_dot_returns_err() {
        let nt = "_:s <http://example.org/p> <http://example.org/o>\n";
        let result = nt_lines_to_nquads(nt, WORLD);
        assert!(result.is_err(), "expected Err for missing dot");
        let msg = result.unwrap_err();
        assert!(
            msg.message().contains("malformed N-Triples line"),
            "error message should describe the problem: {msg:?}"
        );
    }

    /// `grade_suite_corpus` must emit a DlGap Finding for un-lowerable cases
    /// (IRI reference premise or entailment test) rather than silently dropping them.
    ///
    /// This test uses a synthetic two-entry manifest: one consistency test whose
    /// premise is an IRI reference (un-lowerable, no reasoner invoked) and one
    /// entailment test (out of consistency-lane scope). Both must appear as
    /// `dl-gap` Findings in the emitted N-Quads graph.
    #[test]
    fn grade_suite_emits_dlgap_for_unlowerable_and_entailment() {
        // RDF/XML manifest with:
        //   - one ConsistencyTest whose mf:action is an IRI reference (Lane-B skip)
        //   - one PositiveEntailmentTest (always out-of-scope for the consistency lane)
        let manifest_xml = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:mf="http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#"
         xmlns:otest="http://www.w3.org/2007/OWL/testOntology#">
  <otest:ConsistencyTest rdf:about="http://example.org/test/ref-premise">
    <mf:name>ref-premise</mf:name>
    <rdf:type rdf:resource="http://www.w3.org/2007/OWL/testOntology#ConsistencyTest"/>
    <mf:action rdf:resource="http://example.org/ontologies/premise.owl"/>
    <otest:status rdf:resource="http://www.w3.org/2007/OWL/testOntology#Approved"/>
    <mf:result rdf:resource="http://www.w3.org/2007/OWL/testOntology#Consistent"/>
  </otest:ConsistencyTest>
  <otest:PositiveEntailmentTest rdf:about="http://example.org/test/entailment-case">
    <mf:name>entailment-case</mf:name>
    <rdf:type rdf:resource="http://www.w3.org/2007/OWL/testOntology#PositiveEntailmentTest"/>
    <mf:action rdf:resource="http://example.org/ontologies/premise2.owl"/>
    <otest:status rdf:resource="http://www.w3.org/2007/OWL/testOntology#Approved"/>
    <mf:result rdf:resource="http://www.w3.org/2007/OWL/testOntology#Consistent"/>
  </otest:PositiveEntailmentTest>
</rdf:RDF>"#;

        // Write manifest to a temp file.
        let dir = std::env::temp_dir();
        let manifest_path = dir.join(format!(
            "gmeow-test-grade-suite-dlgap-{}.rdf",
            std::process::id()
        ));
        let out_nq = dir.join(format!(
            "gmeow-test-grade-suite-dlgap-{}.nq",
            std::process::id()
        ));
        std::fs::write(&manifest_path, manifest_xml).expect("write manifest");

        // Run grade_suite_corpus.  The `ref-premise` consistency test has an
        // IRI-reference premise (un-lowerable) and is NOT in the quarantine
        // baseline, so the soundness gate now correctly hard-fails: the
        // un-lowerable consistency DlGap is unexpected.  The entailment case
        // is accepted (entailment_slugs), but the consistency IRI-ref gap is
        // not — the function returns Err.
        let result = super::grade_suite_corpus(&manifest_path, "test-corpus", &out_nq);
        assert!(
            result.is_err(),
            "grade_suite_corpus must fail the soundness gate for an un-quarantined \
             un-lowerable consistency DlGap: {result:?}"
        );
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.message().contains("soundness gate FAILED"),
            "error must name the soundness gate failure: {err_msg:?}"
        );
        assert!(
            err_msg.message().contains("ref-premise"),
            "error must name the offending case: {err_msg:?}"
        );

        // The N-Quads output file is written BEFORE the soundness gate check,
        // so it must exist and contain dl-gap Findings for both cases.
        let nq = std::fs::read_to_string(&out_nq).expect("read output nq");

        // Both un-lowerable and entailment cases must appear as dl-gap Findings.
        assert!(
            nq.contains("reason.divergence.dl-gap"),
            "output must contain dl-gap Findings for un-lowerable/entailment cases: {nq:?}"
        );

        // There must be at least 2 Finding nodes (one per unlowerable/entailment case).
        let finding_type_count = nq.lines().filter(|l| l.contains("/Finding>")).count();
        assert!(
            finding_type_count >= 2,
            "expected at least 2 dl-gap Findings (ref-premise + entailment-case), got {finding_type_count}: {nq:?}"
        );

        // Clean up.
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&out_nq);
    }

    // ── soundness_gate unit tests ─────────────────────────────────────────────
    //
    // These tests drive the gate helper directly without touching the filesystem
    // or the reasoner.  They are sub-second and deterministic.

    /// Build a minimal `DivergenceLedger` from a single external comparison.
    fn ledger_from_one(
        case: &str,
        native: &str,
        published: &str,
    ) -> gmeow_logic::reason::DivergenceLedger {
        let rows = gmeow_logic::reason::compare_external_corpus(
            "test-corpus",
            &[gmeow_logic::reason::ExternalComparison {
                case: case.to_owned(),
                world: format!("https://gmeow.example/test-corpus/{case}/w"),
                native: native.to_owned(),
                published: published.to_owned(),
            }],
        );
        gmeow_logic::reason::build_ledger(Vec::new(), Vec::new(), Vec::new(), rows)
    }

    /// A synthetic CorpusOnly row (native decided WRONG) must always cause a
    /// hard-fail regardless of entailment or quarantine sets.
    #[test]
    fn soundness_gate_fails_on_corpus_only() {
        // native decided "consistent" but the corpus published "inconsistent"
        // → CorpusOnly → always a soundness violation.
        let ledger = ledger_from_one("some-case", "consistent", "inconsistent");
        assert_eq!(ledger.corpus_only, 1, "must be a CorpusOnly row");

        let result = super::soundness_gate(
            &ledger,
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeSet::new(),
        );
        assert!(
            result.is_err(),
            "CorpusOnly must hard-fail the soundness gate"
        );
        let offenders = result.unwrap_err();
        assert_eq!(offenders.len(), 1);
        assert!(
            offenders[0].contains("CORPUS-ONLY"),
            "offender line must name CORPUS-ONLY: {:?}",
            offenders[0]
        );
    }

    /// A DlGap from an entailment test (slug in entailment_slugs) must be
    /// accepted: the gate returns Ok.
    #[test]
    fn soundness_gate_accepts_entailment_dl_gap() {
        // native "incomplete" for an entailment test → DlGap, but accepted.
        let ledger = ledger_from_one("an-entailment-case", "incomplete", "consistent");
        assert_eq!(ledger.dl_gap, 1);

        let mut entailment_slugs = std::collections::BTreeSet::new();
        entailment_slugs.insert("an-entailment-case".to_owned());

        let result = super::soundness_gate(
            &ledger,
            &entailment_slugs,
            &std::collections::BTreeSet::new(),
        );
        assert!(
            result.is_ok(),
            "entailment DlGap must be accepted: {result:?}"
        );
    }

    /// A DlGap for a consistency case in the committed quarantine baseline
    /// must be accepted: the gate returns Ok.
    #[test]
    fn soundness_gate_accepts_quarantined_dl_gap() {
        // native "incomplete" for a consistency case that is in the quarantine.
        let ledger = ledger_from_one("webont-thing-004", "incomplete", "consistent");
        assert_eq!(ledger.dl_gap, 1);

        let mut quarantine_slugs = std::collections::BTreeSet::new();
        quarantine_slugs.insert("webont-thing-004".to_owned());

        let result = super::soundness_gate(
            &ledger,
            &std::collections::BTreeSet::new(),
            &quarantine_slugs,
        );
        assert!(
            result.is_ok(),
            "quarantined DlGap must be accepted: {result:?}"
        );
    }

    /// A DlGap for a consistency case NOT in either accepted set (not an
    /// entailment test, not in the quarantine baseline) must hard-fail.
    #[test]
    fn soundness_gate_fails_on_unexpected_dl_gap() {
        let ledger = ledger_from_one("new-unknown-gap", "incomplete", "consistent");
        assert_eq!(ledger.dl_gap, 1);

        let result = super::soundness_gate(
            &ledger,
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeSet::new(),
        );
        assert!(
            result.is_err(),
            "unexpected DlGap must hard-fail the soundness gate"
        );
        let offenders = result.unwrap_err();
        assert_eq!(offenders.len(), 1);
        assert!(
            offenders[0].contains("DL-GAP"),
            "offender line must name DL-GAP: {:?}",
            offenders[0]
        );
    }

    // ── ORE grading-adapter unit tests ───────────────────────────────────────
    //
    // These are network-free: they synthesize a tiny ORE-shaped ontology directory
    // (one OWL 2 Functional-Syntax file = format-gap DlGap, one trivial consistent
    // RDF/XML ontology = Agree) and assert the grade output, the emitted Findings,
    // and the soundness-gate exit.

    /// `is_owl_functional_syntax` must flag Functional-Syntax headers (the ORE
    /// surface) and must NOT flag RDF/XML.
    #[test]
    fn detects_owl_functional_syntax() {
        let functional = b"Prefix(owl:=<http://www.w3.org/2002/07/owl#>)\nOntology(<http://x>\n)";
        assert!(
            super::is_owl_functional_syntax(functional),
            "Functional-Syntax header must be detected"
        );
        // Leading comment lines are skipped before the sniff.
        let with_comment = b"# a comment\nOntology(<http://x>)";
        assert!(super::is_owl_functional_syntax(with_comment));

        let rdfxml = br#"<?xml version="1.0"?><rdf:RDF/>"#;
        assert!(
            !super::is_owl_functional_syntax(rdfxml),
            "RDF/XML must NOT be flagged as Functional Syntax"
        );
    }

    /// `grade_ore_corpus` over a synthetic dir: one Functional-Syntax ontology (a
    /// format-gap DlGap) and one trivial consistent RDF/XML ontology (Agree). The
    /// DlGap is an ACCEPTED ORE coverage gap, so the soundness gate must pass, the
    /// DlGap must surface as a `dl-gap` Finding, and the consistent one must NOT.
    #[test]
    fn grade_ore_records_functional_syntax_gap_and_passes_soundness() {
        let base = std::env::temp_dir().join(format!("gmeow-ore-grade-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("create ORE dir");

        // ORE-shaped OWL 2 Functional-Syntax ontology (native codecs cannot read it).
        std::fs::write(
            base.join("ore_ont_0001.owl"),
            "Prefix(owl:=<http://www.w3.org/2002/07/owl#>)\n\
             Ontology(<http://owl.cs.man.ac.uk/ore/ont1>\n\
             Declaration(Class(<http://a.com/ont#A>))\n)",
        )
        .expect("write functional ontology");

        // A trivial, satisfiable RDF/XML ontology (one class declaration): the native
        // DL consistency path decides `consistent`, matching the curated expected.
        std::fs::write(
            base.join("ore_ont_0002.owl"),
            r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="http://a.com/ont#A"/>
</rdf:RDF>"#,
        )
        .expect("write rdfxml ontology");

        let out_nq = base.join("divergence.nq");
        let result = super::grade_ore_corpus(&base, "ore-test", &out_nq);
        assert!(
            result.is_ok(),
            "ORE soundness gate must pass (no CorpusOnly): {result:?}"
        );

        let nq = std::fs::read_to_string(&out_nq).expect("read divergence nq");
        // The Functional-Syntax ontology must appear as a dl-gap Finding.
        assert!(
            nq.contains("reason.divergence.dl-gap"),
            "format-gap ontology must surface as a dl-gap Finding: {nq:?}"
        );
        // Two Findings: the DlGap plus the consistent ontology's agreement, which now
        // folds as a NON-blocking corroboration finding rather than being dropped.
        let finding_count = nq.lines().filter(|l| l.contains("/Finding>")).count();
        assert_eq!(
            finding_count, 2,
            "expected a dl-gap Finding + the consistent ontology's corroboration Finding: {nq:?}"
        );
        assert!(
            nq.contains("reason.divergence.agreement"),
            "the consistent ontology folds as a corroboration Finding: {nq:?}"
        );
        assert!(
            !nq.contains("reason.divergence.corpus-only"),
            "no soundness (corpus-only) divergence expected for trivial consistent ontology: {nq:?}"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// An empty (or no-`.owl`) directory must hard-fail rather than silently pass a
    /// vacuous grade — a broken extract is a real error.
    #[test]
    fn grade_ore_hard_fails_on_empty_dir() {
        let base = std::env::temp_dir().join(format!("gmeow-ore-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("create empty ORE dir");

        let out_nq = base.join("divergence.nq");
        let result = super::grade_ore_corpus(&base, "ore-test", &out_nq);
        assert!(
            result.is_err(),
            "an empty ORE extract must hard-fail, not vacuously pass"
        );
        assert!(
            result
                .unwrap_err()
                .message()
                .contains("no *.owl ontologies"),
            "error must name the empty-extract condition"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    // ── OntoUML vendoring/grading-adapter unit tests ─────────────────────────
    //
    // These are network-free: they exercise `write_ontouml_case` into a tempdir and
    // the `grade_ontouml_corpus` empty-dir hard-fail, mirroring the ORE-adapter tests.

    /// A world-scoped `FoundationQuad` helper for the write-case round-trip test.
    fn fq(subject: &str, predicate: &str, object: &str, world: &str) -> super::FoundationQuad {
        super::FoundationQuad {
            graph: world.to_owned(),
            subject: subject.to_owned(),
            predicate: predicate.to_owned(),
            object: object.to_owned(),
            rule_iri: "https://blackcatinformatics.ca/logic/assert".to_owned(),
            source_quad_ids: Vec::new(),
            derivation_id: "d0".to_owned(),
        }
    }

    /// `write_ontouml_case` must lay down the full per-case anatomy: `profile.json`
    /// (carrying the documented anti-pattern label), the `input.logic.ttl` stub, the
    /// generated `input.nq`, and the blessed `expected/{materialized.nq,verdicts.json}`.
    #[test]
    fn write_ontouml_case_round_trips_documented_case() {
        let base =
            std::env::temp_dir().join(format!("gmeow-ontouml-vendor-{}-doc", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("create case dir");

        let world = "https://gmeow.example/ontouml-mini/free-role/w";
        let input_nq = format!(
            "<https://ex/Wanderer> <https://blackcatinformatics.ca/logic/subClassOf> <https://ex/Wanderer> <{world}> .\n"
        );
        let quads = vec![fq(
            "https://ex/Wanderer",
            "https://blackcatinformatics.ca/logic/violation",
            "<https://blackcatinformatics.ca/logic/FreeRole>",
            world,
        )];

        super::write_ontouml_case(&base, &input_nq, &quads, Some("FreeRole"))
            .expect("write_ontouml_case must succeed");

        // profile.json — native foundation-lowering, certify:false, documented label.
        let profile = std::fs::read_to_string(base.join("profile.json")).expect("profile.json");
        assert!(
            profile.contains("\"documented_antipattern\": \"FreeRole\""),
            "{profile}"
        );
        assert!(profile.contains("\"certify\": false"), "{profile}");
        assert!(
            profile.contains("\"foundation_lowering\": true"),
            "{profile}"
        );
        assert!(
            profile.contains("\"preset\": \"StratifiedNAFProfile\""),
            "{profile}"
        );
        assert!(profile.contains("\"mode\": \"native\""), "{profile}");
        assert!(!profile.contains("verdict_mode"), "{profile}");

        // input.logic.ttl — CC-BY header + the seed logic: ABox as a default-graph
        // program (the same facts input.nq world-scopes, with the world term dropped).
        let program =
            std::fs::read_to_string(base.join("input.logic.ttl")).expect("input.logic.ttl");
        assert!(
            program.contains("SPDX-License-Identifier: CC-BY-4.0"),
            "{program}"
        );
        assert!(
            program.contains(
                "<https://ex/Wanderer> <https://blackcatinformatics.ca/logic/subClassOf> \
                 <https://ex/Wanderer> ."
            ),
            "{program}"
        );
        assert!(program.contains("--vendor-ontouml"), "{program}");

        // input.nq — verbatim world-scoped lowering.
        let nq = std::fs::read_to_string(base.join("input.nq")).expect("input.nq");
        assert_eq!(nq, input_nq);

        // expected/materialized.nq — the mapped foundation quad, world-scoped.
        let mat =
            std::fs::read_to_string(base.join("expected").join("materialized.nq")).expect("mat.nq");
        assert_eq!(
            mat,
            format!(
                "<https://ex/Wanderer> <https://blackcatinformatics.ca/logic/violation> \
                 <https://blackcatinformatics.ca/logic/FreeRole> <{world}> .\n"
            )
        );

        // expected/verdicts.json — one world, always consistent, quad-count 1.
        let verdicts =
            std::fs::read_to_string(base.join("expected").join("verdicts.json")).expect("verdicts");
        assert!(verdicts.contains(world), "{verdicts}");
        assert!(
            verdicts.contains("\"status\": \"consistent\""),
            "{verdicts}"
        );
        assert!(verdicts.contains("\"quads\": 1"), "{verdicts}");

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A clean-control case (`documented == None`) must OMIT the
    /// `documented_antipattern` key entirely (matching the `Option<String>` None
    /// semantics), and its `materialized.nq` must be empty (nothing fired).
    #[test]
    fn write_ontouml_case_clean_control_omits_documented_key() {
        let base =
            std::env::temp_dir().join(format!("gmeow-ontouml-vendor-{}-clean", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("create case dir");

        let world = "https://gmeow.example/ontouml-mini/clean/w";
        let input_nq = format!(
            "<https://ex/Person> <https://blackcatinformatics.ca/logic/subClassOf> <https://ex/Person> <{world}> .\n"
        );

        super::write_ontouml_case(&base, &input_nq, &[], None)
            .expect("write_ontouml_case must succeed");

        let profile = std::fs::read_to_string(base.join("profile.json")).expect("profile.json");
        assert!(!profile.contains("documented_antipattern"), "{profile}");
        assert!(profile.contains("\"certify\": false"), "{profile}");

        // No fired quads → empty materialized.nq and empty (`{}`) verdicts.
        let mat =
            std::fs::read_to_string(base.join("expected").join("materialized.nq")).expect("mat.nq");
        assert_eq!(mat, "");

        let _ = std::fs::remove_dir_all(&base);
    }

    /// An empty (or no-model) directory must hard-fail rather than silently pass a
    /// vacuous grade — a broken extract / unpopulated catalog is a real error.
    #[test]
    fn grade_ontouml_hard_fails_on_empty_dir() {
        let base = std::env::temp_dir().join(format!("gmeow-ontouml-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("create empty catalog dir");

        let out_nq = base.join("divergence.nq");
        let result = super::grade_ontouml_corpus(&base, "ontouml-catalog", &out_nq);
        assert!(
            result.is_err(),
            "an empty OntoUML catalog must hard-fail, not vacuously pass"
        );
        assert!(
            result
                .unwrap_err()
                .message()
                .contains("no ontology.ttl / model.ttl"),
            "error must name the empty-catalog condition"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A missing ROOT catalog directory must hard-fail (Gap G3): it is a required
    /// input, not a benign "nothing here yet" — unlike a subdirectory vanishing
    /// mid-walk (a benign race), a missing root must never silently degrade to an
    /// empty model set.
    #[test]
    fn collect_ontouml_models_hard_fails_on_missing_root() {
        let base =
            std::env::temp_dir().join(format!("gmeow-ontouml-missing-root-{}", std::process::id()));
        // Ensure it genuinely does not exist.
        let _ = std::fs::remove_dir_all(&base);

        let result = super::collect_ontouml_models(&base);
        assert!(
            result.is_err(),
            "a missing root OntoUML catalog must hard-fail, not vacuously return zero models"
        );
        assert!(
            result
                .unwrap_err()
                .message()
                .contains(&base.display().to_string()),
            "error must name the missing root path"
        );
    }

    /// A present root catalog containing one populated model dir and one entirely
    /// blank (empty) subdir must still succeed and return only the models that were
    /// actually found — the ROOT-only hard-fail added for Gap G3 must not make the
    /// walk over-eager and start rejecting ordinary empty subdirectories too.
    #[test]
    fn collect_ontouml_models_tolerates_blank_subdir_but_finds_present_models() {
        let base = std::env::temp_dir().join(format!("gmeow-ontouml-blank-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("present")).expect("create present model dir");
        std::fs::write(base.join("present").join("model.ttl"), "").expect("write model.ttl");
        // A subdir with no model files in it at all.
        std::fs::create_dir_all(base.join("blank")).expect("create blank dir");

        let result = super::collect_ontouml_models(&base);
        let models = result.expect("a present root with a blank sibling subdir must still succeed");
        assert_eq!(
            models,
            vec![base.join("present").join("model.ttl")],
            "the present model must still be found despite the blank sibling subdir"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A subdirectory that is removed between being *listed* by its parent and being
    /// individually `read_dir`-ed (a genuine mid-walk removal race, the scenario
    /// `NotFound` tolerance exists for) must not fail the whole walk — the other,
    /// still-present model must still be returned. This drives the walk itself
    /// (not a synthetic call to `std::fs::read_dir`), so it exercises the exact
    /// `Err(e) if e.kind() == NotFound => continue` arm for a non-root `d` in
    /// `collect_ontouml_models`, proving that arm is reachable and does not abort.
    #[test]
    fn collect_ontouml_models_tolerates_subdir_removed_mid_walk() {
        let base =
            std::env::temp_dir().join(format!("gmeow-ontouml-midwalk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("present")).expect("create present model dir");
        std::fs::write(base.join("present").join("model.ttl"), "").expect("write model.ttl");

        let vanishing = base.join("vanishing");
        std::fs::create_dir_all(&vanishing).expect("create vanishing dir");

        // Race a background thread against the walk: it repeatedly tries to remove
        // `vanishing` for as long as the walk might still be running, so at least
        // one removal attempt lands after the root listing has already staged
        // `vanishing` onto the walk's stack but before the walk individually reads
        // it. `remove_dir_all` on an already-gone path is a harmless no-op.
        let vanishing_bg = vanishing.clone();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_bg = stop.clone();
        let racer = std::thread::spawn(move || {
            while !stop_bg.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = std::fs::remove_dir_all(&vanishing_bg);
            }
        });

        let result = super::collect_ontouml_models(&base);
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        racer.join().expect("racer thread must not panic");

        let models = result.expect(
            "a subdir removed mid-walk must be tolerated as an empty listing, not fail the walk",
        );
        assert_eq!(
            models,
            vec![base.join("present").join("model.ttl")],
            "the present model must still be found regardless of the vanished sibling subdir"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// `grade_ontouml_corpus` over a synthetic catalog: one clean model (fires
    /// nothing → `clean` → Agree → non-blocking corroboration Finding) and one FreeRole
    /// anti-pattern model (fires `FreeRole` → native != published `"clean"` → CorpusOnly
    /// Finding). The FreeRole divergence must surface as a `corpus-only` Finding.
    #[test]
    fn grade_ontouml_surfaces_fired_discipline_as_corpus_only() {
        let base = std::env::temp_dir().join(format!("gmeow-ontouml-grade-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("clean")).expect("create clean dir");
        std::fs::create_dir_all(base.join("freerole")).expect("create freerole dir");

        // A clean kind class fires nothing.
        std::fs::write(
            base.join("clean").join("model.ttl"),
            "@prefix ontouml: <https://w3id.org/ontouml#> .\n\
             @prefix ex: <https://example.org/onto/> .\n\
             ex:Person a ontouml:Class ; ontouml:stereotype ontouml:kind .\n",
        )
        .expect("write clean model");

        // A lone role class (no rigid ancestor) is the FreeRole anti-pattern.
        std::fs::write(
            base.join("freerole").join("ontology.ttl"),
            "@prefix ontouml: <https://w3id.org/ontouml#> .\n\
             @prefix ex: <https://example.org/onto/> .\n\
             ex:Wanderer a ontouml:Class ; ontouml:stereotype ontouml:role .\n",
        )
        .expect("write freerole model");

        let out_nq = base.join("divergence.nq");
        super::grade_ontouml_corpus(&base, "ontouml-catalog", &out_nq)
            .expect("grade must succeed (grading never gates)");

        let nq = std::fs::read_to_string(&out_nq).expect("read divergence nq");
        assert!(
            nq.contains("reason.divergence.corpus-only"),
            "fired FreeRole must surface as a corpus-only Finding: {nq}"
        );
        // Two Findings: the corpus-only FreeRole divergence plus the clean model's
        // agreement, which now folds as a NON-blocking corroboration Finding.
        let finding_count = nq.lines().filter(|l| l.contains("/Finding>")).count();
        assert_eq!(
            finding_count, 2,
            "expected a corpus-only Finding + the clean model's corroboration Finding: {nq}"
        );
        assert!(
            nq.contains("reason.divergence.agreement"),
            "the clean model folds as a corroboration Finding: {nq}"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// When a ledger has only agreeing rows, the gate passes regardless of what
    /// the entailment/quarantine sets contain.
    #[test]
    fn soundness_gate_passes_on_pure_agreement() {
        let ledger = ledger_from_one("consistent-case", "consistent", "consistent");
        assert_eq!(ledger.agree, 1);
        assert_eq!(ledger.corpus_only, 0);
        assert_eq!(ledger.dl_gap, 0);

        let result = super::soundness_gate(
            &ledger,
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeSet::new(),
        );
        assert!(result.is_ok(), "pure agreement must pass: {result:?}");
    }
}
