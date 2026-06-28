// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Golden regeneration ("bless") mode.
//!
//! Activated by `GMEOW_CONFORMANCE_BLESS=1`, [`write_expected`] writes a case's
//! `expected/` goldens from a fresh [`CaseOutputs`] instead of asserting against
//! them. This is the native regeneration path for the conformance corpus.
//!
//! Comparison is canonical-JSON / graph-isomorphism, so bless writes a clean,
//! deterministic canonical form (sorted-key pretty JSON; the compiler's
//! byte-stable projection text); a subsequent assert run is self-consistent.
//!
//! `explanation/*.md` goldens (content-hash-named, prose-bearing) follow the same
//! refresh-or-seed rule as every other golden: an existing file is refreshed from the
//! fresh outputs, and a brand-new one is written only in seed/init mode. The gate
//! itself compares only the cited-IRI skeleton (not the prose), so prose drift is
//! pinned separately by the renderer's unit/snapshot tests rather than the corpus.
//!
//! The one artifact kind genuinely never written by bless is `witnesses.json` — a
//! bless-only side file the diff never consumed; authoring it remains manual.
//!
//! ## Curated corpus + idempotency
//!
//! Each case commits a *curated* subset of goldens (a query case may pin only its
//! `answers/`; a projection case pins the projection set). Bless therefore only
//! **refreshes goldens that already exist** — it never spawns a golden the case
//! deliberately omits. Combined with the deterministic front-end (RDFC-1.0 blank
//! labels), this makes `bless → git diff` clean: a second bless on a fresh
//! checkout is a no-op. To **seed a brand-new case**, set
//! `GMEOW_CONFORMANCE_BLESS_INIT=1`, which writes the full produced golden set; the
//! author then curates (deletes) the targets that case should not gate.

use std::path::Path;

use crate::run::CaseOutputs;

/// Whether bless is in seed/init mode (`GMEOW_CONFORMANCE_BLESS_INIT=1`) — write the
/// full produced golden set rather than only refreshing existing goldens.
fn init_mode() -> bool {
    std::env::var_os("GMEOW_CONFORMANCE_BLESS_INIT").is_some_and(|v| v == "1")
}

/// Write the reproducible `expected/` goldens for `case_dir` from `out`.
///
/// In the default (refresh) mode only goldens that already exist on disk are
/// rewritten, so the curated corpus stays curated and bless is idempotent. In
/// init mode (`GMEOW_CONFORMANCE_BLESS_INIT=1`) the full produced set is written.
///
/// # Errors
/// Returns an error string on any filesystem or serialization failure.
pub fn write_expected(case_dir: &Path, out: &CaseOutputs) -> Result<(), String> {
    let init = init_mode();
    let expected = case_dir.join("expected");
    let proj = expected.join("projections");

    // Projections are (re)written only when the case already commits a
    // `projections/` golden dir (curated corpus), or in init mode (seed a new case).
    if init || proj.is_dir() {
        mkdirs(&proj)?;

        // Projection RDF + report + ledger.
        for (target, filename) in [
            ("owl-dl", "owl-dl.ttl"),
            ("owl-el", "owl-el.ttl"),
            ("gufo", "gufo.ttl"),
            ("canonical-rdf12", "canonical-rdf12.ttl"),
        ] {
            if let Some(content) = out.projections.rdf.get(target) {
                write_if(init, &proj.join(filename), |p| write_text(p, content))?;
            }
        }
        write_if(init, &proj.join("projection-report.ttl"), |p| {
            write_text(p, &out.projections.report_turtle)
        })?;
        write_if(init, &proj.join("preservation-ledger.json"), |p| {
            write_json(p, &out.projections.ledger)
        })?;

        // Plain-text projections (now deterministic; gated like the RDF set).
        for (target, filename) in [
            ("datalog", "datalog.dl"),
            ("n3", "n3.n3"),
            ("nemo", "nemo.rls"),
        ] {
            if let Some(content) = out.projections.text.get(target) {
                write_if(init, &proj.join(filename), |p| write_text(p, content))?;
            }
        }
    }

    // Verdicts.
    write_if(init, &expected.join("verdicts.json"), |p| {
        write_json(p, &out.verdicts)
    })?;

    // Certification — write when the case opts in or a golden already exists.
    let profile_val = crate::compare::read_profile_value(case_dir);
    let cert_opt_in = profile_val
        .get("certify")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let cert_path = expected.join("certification.json");
    if init || cert_opt_in || cert_path.exists() {
        write_json(&cert_path, &out.certification)?;
    }

    // Budget — write when the case declares budget_params or a golden exists.
    let declares_budget = profile_val.get("budget_params").is_some();
    let budget_path = expected.join("budget.json");
    if init || declares_budget || budget_path.exists() {
        let actual_budget = serde_json::json!({
            "budget_status": out.budget_status,
            "incomplete": out.incomplete,
        });
        write_json(&budget_path, &actual_budget)?;
    }

    // Materialized N-Quads — refresh-only: a case whose gated artifact is its
    // `answers/` (goal / probabilistic cases) curates `materialized.nq` out even
    // though materialization is non-empty, so writing on non-empty alone would spawn
    // an uncommitted golden and break idempotency. Seed a new case with init.
    let mat_path = expected.join("materialized.nq");
    if init || mat_path.exists() {
        write_text(&mat_path, &out.materialized_nquads)?;
    }

    // Runtime preservation judgment (opt-in, #773): refresh an existing golden or
    // seed in init mode — a case opts in by committing `runtime-preservation.json`.
    write_if(init, &expected.join("runtime-preservation.json"), |p| {
        write_json(p, &out.preservation)
    })?;

    // Answers (#504).
    if !out.answers.is_empty() {
        let answers_dir = expected.join("answers");
        mkdirs(&answers_dir)?;
        for (stem, value) in &out.answers {
            write_json(&answers_dir.join(format!("{stem}.json")), value)?;
        }
    }

    // Explanation .md files — refresh existing goldens; seed new ones in init mode.
    if !out.explanations.is_empty() {
        let expl_dir = expected.join("explanation");
        if init {
            mkdirs(&expl_dir)?;
        }
        for expl in &out.explanations {
            let hash = expl
                .target_quad_reifier
                .rsplit('/')
                .next()
                .unwrap_or(&expl.target_quad_reifier);
            let path = expl_dir.join(format!("{hash}.md"));
            write_if(init, &path, |p| write_text(p, &expl.markdown))?;
        }
    }

    Ok(())
}

/// Refresh-or-seed gate: run the writer `f` only when seeding (`init`) or when a
/// golden already exists at `path`. This is what keeps the curated corpus curated
/// and bless idempotent — a target the case omits is never spawned in refresh mode.
fn write_if(
    init: bool,
    path: &Path,
    f: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    if init || path.exists() {
        f(path)
    } else {
        Ok(())
    }
}

/// Create `dir` and all parents.
fn mkdirs(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))
}

/// Write `content` to `path` verbatim.
fn write_text(path: &Path, content: &str) -> Result<(), String> {
    std::fs::write(path, content).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Write `value` as sorted-key, 2-space-indented JSON with a trailing newline.
fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|e| format!("cannot serialize {}: {e}", path.display()))?;
    text.push('\n');
    write_text(path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recursively copy `src` into `dst` (used to bless a scratch copy of a case).
    fn copy_dir(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).expect("mkdir");
        for entry in std::fs::read_dir(src).expect("read_dir") {
            let entry = entry.expect("entry");
            let from = entry.path();
            let to = dst.join(entry.file_name());
            if from.is_dir() {
                copy_dir(&from, &to);
            } else {
                std::fs::copy(&from, &to).expect("copy");
            }
        }
    }

    /// Bless self-consistency: regenerating a case's goldens and re-running the diff
    /// yields no mismatches. Comparison is canonical/graph-iso, so the freshly
    /// blessed (canonical) goldens are accepted by the gate. (Explanation `.md` is
    /// refreshed like every other existing golden, but the renderer is deterministic,
    /// so it regenerates to byte-identical content and the cited-IRI skeleton still
    /// matches.)
    #[test]
    fn bless_is_self_consistent() {
        let src = crate::paths::cases_root()
            .join("foundation")
            .join("free-role");
        let tmp = std::env::temp_dir().join(format!("gmeow-bless-{}", std::process::id()));
        // Preserve the <category>/<case> tail so the derived case_id is stable.
        let dst = tmp.join("foundation").join("free-role");
        copy_dir(&src, &dst);

        let out = crate::run::run_case(&dst).expect("run_case ok");
        write_expected(&dst, &out).expect("bless ok");

        let out2 = crate::run::run_case(&dst).expect("run_case (post-bless) ok");
        let diffs = crate::compare::diff_case(&dst, &out2);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(diffs.is_empty(), "bless not self-consistent: {diffs:?}");
    }
}
