// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native compliance report (port of `gmeow_tools.compliance`).
//!
//! The constitution manifest knows which gates enforce which principles; this
//! module renders the per-principle gate evidence as an RDF Turtle proof object.
//! The report is a runtime artifact (it embeds run results and a UTC timestamp),
//! so it belongs under `dist/`, never the drift-gated `generated/` tree.
//!
//! # Gate runners
//!
//! [`run_constitution_gate`] is fully self-contained here (it reuses
//! [`crate::constitution::constitution_full_report`]). The remaining runnable
//! gates — `validate`, `lint-alignment`, `sync` — orchestrate the
//! whole-repo validation and the regeneration pipeline, which live above this
//! crate; the `gmeow-dev` binary supplies their [`GateRun`] outcomes. The report
//! rendering ([`build_report`]) is pure: gate outcomes in, Turtle out.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gmeow_errors::Severity;

use crate::constitution::{
    Enforcement, Principle, collect_enforcements, collect_principles, constitution_full_report,
};

/// The governance meta namespace.
pub const META: &str = "https://blackcatinformatics.ca/gmeow/meta#";

/// The runnable in-process gate names cited by the manifest's enforcements.
///
/// Mirrors the Python `RUNNERS` keys: an enforcement citing one of these (as a
/// make target or CLI command) has its status decided by that gate's outcome.
pub const RUNNER_NAMES: &[&str] = &["validate", "constitution-check", "lint-alignment", "sync"];

/// The outcome of one executed gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateRun {
    pub errors: usize,
    /// `None` when the warning count is unknown (e.g. prior-successful evidence).
    pub warnings: Option<usize>,
}

impl GateRun {
    pub fn new(errors: usize, warnings: Option<usize>) -> Self {
        Self { errors, warnings }
    }
}

/// Run the native constitution-as-code gate into a [`GateRun`].
///
/// Reuses [`constitution_full_report`]; the finding severities decide the error
/// and warning counts.
pub fn run_constitution_gate(
    manifest_path: &Path,
    constitution_path: &Path,
    root: &Path,
) -> GateRun {
    let findings = constitution_full_report(manifest_path, constitution_path, root);
    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    let warnings = findings
        .iter()
        .filter(|f| f.severity == Severity::Warning)
        .count();
    GateRun::new(errors, Some(warnings))
}

/// Pass evidence for gates already run by the surrounding workflow (all runnable
/// gates report zero errors and an unknown warning count).
pub fn assumed_passed_gate_runs(names: Option<&BTreeSet<String>>) -> BTreeMap<String, GateRun> {
    RUNNER_NAMES
        .iter()
        .filter(|name| names.is_none_or(|set| set.contains(**name)))
        .map(|name| ((*name).to_string(), GateRun::new(0, None)))
        .collect()
}

/// The `(status, errors, warnings)` for one enforcement's citations.
fn enforcement_status(
    citations: &[String],
    kind: &str,
    gate_runs: &BTreeMap<String, GateRun>,
) -> (&'static str, usize, Option<usize>) {
    // Dedupe preserving first-seen order: an enforcement may cite the same
    // runnable as both makeTarget and cliCommand; its run counts once.
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut ran: Vec<GateRun> = Vec::new();
    for citation in citations {
        if !seen.insert(citation.as_str()) {
            continue;
        }
        if let Some(run) = gate_runs.get(citation) {
            ran.push(*run);
        }
    }
    if !ran.is_empty() {
        let errors: usize = ran.iter().map(|r| r.errors).sum();
        let mut warnings_unknown = false;
        let mut warning_sum = 0usize;
        for run in &ran {
            match run.warnings {
                None => warnings_unknown = true,
                Some(w) => warning_sum += w,
            }
        }
        let warnings = if warnings_unknown {
            None
        } else {
            Some(warning_sum)
        };
        let status = if errors > 0 { "failed" } else { "passed" };
        return (status, errors, warnings);
    }
    if matches!(kind, "TestSuite" | "Gate" | "Shape" | "Lint") {
        ("gated-in-ci", 0, Some(0))
    } else {
        ("declared", 0, Some(0))
    }
}

/// Render the compliance report as Turtle (pure; testable with fake gate runs).
///
/// `principles` and `enforcements` are the manifest projection
/// ([`collect_principles`] / [`collect_enforcements`]); `gate_runs` is keyed by
/// runnable-gate name.
pub fn build_report(
    principles: &[Principle],
    enforcements: &BTreeMap<String, Enforcement>,
    gate_runs: &BTreeMap<String, GateRun>,
    generated_at: &str,
    source_commit: &str,
    toolchain_version: &str,
    evidence_mode: &str,
) -> String {
    let assessed = principles
        .iter()
        .map(|p| format!("meta:Principle{}", p.number))
        .collect::<Vec<_>>()
        .join(", ");

    let mut lines: Vec<String> = vec![
        "# GMEOW compliance report — per-principle gate evidence.".to_string(),
        "# A runtime proof object, regenerated by `gmeow compliance-report`.".to_string(),
        "@prefix meta: <https://blackcatinformatics.ca/gmeow/meta#> .".to_string(),
        "@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .".to_string(),
        String::new(),
        "meta:report a meta:ComplianceReport ;".to_string(),
        format!("    meta:generatedAt \"{generated_at}\"^^xsd:dateTime ;"),
        format!("    meta:sourceCommit \"{source_commit}\" ;"),
        format!("    meta:toolchainVersion \"{toolchain_version}\" ;"),
        format!("    meta:evidenceMode \"{evidence_mode}\" ;"),
        format!("    meta:assesses {assessed} ."),
        String::new(),
    ];

    for principle in principles {
        let mut statuses: Vec<&str> = Vec::new();
        let mut body: Vec<String> = Vec::new();

        // Supersession / extends edges flow through to the report (maximal
        // information flow); prepend so the enforcement results stay terminal.
        let mut relations: Vec<String> = Vec::new();
        if !principle.superseded_in_part_by.is_empty() {
            let items = principle
                .superseded_in_part_by
                .iter()
                .map(|n| format!("meta:Principle{n}"))
                .collect::<Vec<_>>()
                .join(", ");
            relations.push(format!("    meta:supersededInPartBy {items}"));
        }
        if !principle.extends.is_empty() {
            let items = principle
                .extends
                .iter()
                .map(|n| format!("meta:Principle{n}"))
                .collect::<Vec<_>>()
                .join(", ");
            relations.push(format!("    meta:extends {items}"));
        }

        for iri in &principle.enforced_by {
            let Some(enforcement) = enforcements.get(iri) else {
                continue;
            };
            let name = iri.strip_prefix(META).unwrap_or(iri);
            let mut citations: Vec<String> = enforcement.make_targets.clone();
            citations.extend(enforcement.cli_commands.clone());
            let (status, errors, warnings) =
                enforcement_status(&citations, &enforcement.kind, gate_runs);
            statuses.push(status);
            let warning_count = match warnings {
                None => String::new(),
                Some(w) => format!(" ; meta:warningCount {w}"),
            };
            body.push(format!(
                "    meta:enforcementResult [ meta:enforcement meta:{name} ; \
                 meta:status \"{status}\" ; meta:errorCount {errors}{warning_count} ]"
            ));
        }

        let mut block: Vec<String> = relations;
        block.extend(body);
        let overall = if statuses.contains(&"failed") {
            "failed"
        } else {
            "passed"
        };

        lines.push(format!("meta:Principle{}Result", principle.number));
        lines.push("    a meta:PrincipleResult ;".to_string());
        lines.push(format!(
            "    meta:principle meta:Principle{} ;",
            principle.number
        ));
        lines.push(format!("    meta:status \"{overall}\" ;"));
        if block.is_empty() {
            // No body: terminate the status line instead.
            let last = lines.len() - 1;
            let trimmed = lines[last].trim_end_matches(';').trim_end().to_string();
            lines[last] = format!("{trimmed} .");
        } else {
            for b in &block[..block.len() - 1] {
                lines.push(format!("{b} ;"));
            }
            lines.push(format!("{} .", block[block.len() - 1]));
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

/// Run the self-contained gates and render the full compliance report.
///
/// The `validate` / `lint-alignment` / `sync` outcomes must be
/// supplied by the caller (they orchestrate crates above this one); the
/// constitution gate is run here. `generated_at` is the current UTC instant and
/// `source_commit` is the repo `HEAD`.
///
/// # Errors
///
/// Fails if the manifest cannot be read or parsed.
pub fn compliance_report(
    manifest_path: &Path,
    constitution_path: &Path,
    root: &Path,
    supplied_gate_runs: &BTreeMap<String, GateRun>,
    toolchain_version: &str,
    evidence_mode: &str,
) -> gmeow_errors::Result<String> {
    let ttl = std::fs::read(manifest_path).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            detail: format!("{}: cannot read: {e}", manifest_path.display()),
        })
    })?;
    let dataset = purrdf::parse_dataset(&ttl, "text/turtle", None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: format!("{}: does not parse: {e}", manifest_path.display()),
        })
    })?;
    let principles = collect_principles(&dataset);
    let enforcements = collect_enforcements(&dataset);

    // Fill in the constitution gate here; keep any caller-supplied gate runs.
    let mut gate_runs = supplied_gate_runs.clone();
    gate_runs
        .entry("constitution-check".to_string())
        .or_insert_with(|| run_constitution_gate(manifest_path, constitution_path, root));

    Ok(build_report(
        &principles,
        &enforcements,
        &gate_runs,
        &crate::time_util::utc_iso_seconds(),
        &git_head(root),
        toolchain_version,
        evidence_mode,
    ))
}

/// The repository `HEAD` commit, or `"unknown"` if git is unavailable.
pub fn git_head(root: &Path) -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .to_path_buf()
    }

    fn manifest_paths() -> (PathBuf, PathBuf, PathBuf) {
        let root = repo_root();
        (
            root.join("governance").join("constitution.ttl"),
            root.join("CONSTITUTION.md"),
            root,
        )
    }

    fn manifest_projection() -> (Vec<Principle>, BTreeMap<String, Enforcement>) {
        let (manifest, _md, _root) = manifest_paths();
        let ttl = std::fs::read(&manifest).expect("manifest readable");
        let dataset = purrdf::parse_dataset(&ttl, "text/turtle", None).expect("manifest parses");
        (collect_principles(&dataset), collect_enforcements(&dataset))
    }

    fn fake_runs() -> BTreeMap<String, GateRun> {
        [
            ("validate", GateRun::new(0, Some(3))),
            ("constitution-check", GateRun::new(0, Some(3))),
            ("lint-alignment", GateRun::new(0, Some(0))),
            ("sync", GateRun::new(0, Some(0))),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
    }

    fn render(gate_runs: &BTreeMap<String, GateRun>, evidence_mode: &str) -> String {
        let (principles, enforcements) = manifest_projection();
        build_report(
            &principles,
            &enforcements,
            gate_runs,
            "2026-06-12T00:00:00+00:00",
            "deadbeef",
            "0.1.0",
            evidence_mode,
        )
    }

    #[test]
    fn report_is_nonempty_valid_turtle_covering_every_principle() {
        let report = render(&fake_runs(), "in-process");
        assert!(!report.trim().is_empty());
        let dataset =
            purrdf::parse_dataset(report.as_bytes(), "text/turtle", None).expect("valid turtle");
        // One PrincipleResult per principle.
        let (principles, _enf) = manifest_projection();
        let count = count_typed(&dataset, &format!("{META}PrincipleResult"));
        assert_eq!(count, principles.len());
    }

    #[test]
    fn report_carries_provenance_and_evidence_mode() {
        let report = render(&fake_runs(), "in-process");
        assert!(report.contains("deadbeef"));
        assert!(report.contains("2026-06-12T00:00:00+00:00"));
        assert!(report.contains("meta:toolchainVersion \"0.1.0\""));
    }

    #[test]
    fn runnable_gates_pass_and_failures_propagate() {
        assert!(render(&fake_runs(), "in-process").contains("\"passed\""));
        assert!(!render(&fake_runs(), "in-process").contains("\"failed\""));

        let mut failing = fake_runs();
        failing.insert("validate".to_string(), GateRun::new(2, Some(0)));
        assert!(render(&failing, "in-process").contains("\"failed\""));
    }

    #[test]
    fn out_of_process_enforcement_is_gated_in_ci_never_silent() {
        let report = render(&fake_runs(), "in-process");
        assert!(report.contains("\"gated-in-ci\""));
        assert!(report.contains("\"declared\""));
    }

    #[test]
    fn assumed_passed_marks_runnable_gates_passed_with_no_warning_count() {
        let gate_runs = assumed_passed_gate_runs(None);
        let report = render(&gate_runs, "prior-successful-gates");
        assert!(report.contains("meta:evidenceMode \"prior-successful-gates\""));
        assert!(!report.contains("\"failed\""));
        // Unknown warning count → no meta:warningCount on runnable results.
        // (Out-of-process results still carry a concrete count.)
        let dataset =
            purrdf::parse_dataset(report.as_bytes(), "text/turtle", None).expect("valid turtle");
        assert!(count_typed(&dataset, &format!("{META}PrincipleResult")) > 0);
    }

    #[test]
    fn constitution_gate_runs_against_the_repo() {
        let (manifest, md, root) = manifest_paths();
        let run = run_constitution_gate(&manifest, &md, &root);
        // The committed manifest must be internally consistent (zero errors).
        assert_eq!(run.errors, 0, "constitution gate should pass on main");
        assert!(run.warnings.is_some());
    }

    #[test]
    fn compliance_report_smoke_reuses_constitution_gate() {
        let (manifest, md, root) = manifest_paths();
        let report = compliance_report(
            &manifest,
            &md,
            &root,
            &assumed_passed_gate_runs(None),
            "0.1.0",
            "prior-successful-gates",
        )
        .expect("report renders");
        assert!(!report.trim().is_empty());
        assert!(report.contains("meta:ComplianceReport"));
        purrdf::parse_dataset(report.as_bytes(), "text/turtle", None).expect("valid turtle");
    }

    fn count_typed(ds: &purrdf::RdfDataset, class_iri: &str) -> usize {
        use purrdf::{DatasetView, GraphMatch, TermValue};
        let (Some(type_id), Some(class_id)) = (
            ds.term_id_by_value(&TermValue::iri(
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            )),
            ds.term_id_by_value(&TermValue::iri(class_iri)),
        ) else {
            return 0;
        };
        ds.quads_for_pattern(None, Some(type_id), Some(class_id), GraphMatch::Any)
            .count()
    }
}
