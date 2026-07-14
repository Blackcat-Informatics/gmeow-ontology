// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-test duration budget gate for the Rust test suite.
//!
//! The always-on policy: **every test on the default/ci nextest profile must
//! complete under 25 s of real wall time.** Tests that are irreducibly heavier are
//! carved out of those profiles via `default-filter` in `.config/nextest.toml` and
//! run on the `maint-heavy` profile instead, so a JUnit produced by the default/ci
//! profile should contain NO over-budget test.
//!
//! This binary parses the JUnit report nextest emits (`target/nextest/<profile>/
//! junit.xml`) and reports any `<testcase>` whose `time` exceeds the budget. A
//! leaked off-gate test that is genuinely >25 s therefore trips the gate here too —
//! no separate leak guard is needed: presence over budget IS the finding.
//!
//! **Enforcement is environment-aware.** Wall-clock is only a meaningful signal on
//! a dedicated, uncontended runner; on the shared developer box (many concurrent
//! worktrees, load far above core count) every heavy test inflates and the budget
//! reports false positives that block nothing real. So:
//!   - In CI (the authoritative timing environment) the gate **HARD-FAILS (exit 1)**
//!     on any over-budget test.
//!   - Locally it is **ADVISORY**: the offenders are still printed (as a warning, so
//!     the signal is never lost) but the gate returns success (exit 0).
//!
//! This is not a degraded fallback — it is measuring the right thing in the right
//! place. CI enforcement fails safe: it is keyed on the platform-guaranteed `CI`
//! variable (and made explicit by the CI step setting `GMEOW_TEST_BUDGET_ENFORCE`),
//! so the hard gate is on whenever the measurement is trustworthy.
//!
//! Usage:
//!   gmeow-test-budget <JUNIT_PATH>
//! Env:
//!   GMEOW_TEST_BUDGET_SECS     override the 25.0 s budget (e.g. CI variance headroom).
//!   GMEOW_TEST_BUDGET_ENFORCE  explicit enforcement override (`1`/`true`/`on` → hard
//!                              fail, `0`/`false`/`off` → advisory). Wins over the `CI`
//!                              autodetect in both directions. Unset → enforce iff `CI`
//!                              is set (non-empty).
//!
//! Rust-first (`.goals`): no Python, no XML crate — nextest's JUnit is small and
//! regular, so a std-only attribute scan is sufficient.

use std::env::VarError;
use std::io::IsTerminal;
use std::process::ExitCode;

use gmeow_cli_core::{ConsoleMode, Reporter, report_diag};
use gmeow_errors::{Diag, FindingCategory, Grade, Severity, Standpoint};

mod error;

/// The always-on per-test budget, in seconds.
const DEFAULT_BUDGET_SECS: f64 = 25.0;

/// Explicit enforcement override. When set to a bool it wins over the `CI`
/// autodetect in both directions; unset, enforcement follows `CI`.
const ENFORCE_VAR: &str = "GMEOW_TEST_BUDGET_ENFORCE";

/// The default JUnit location for the `ci` nextest profile.
const DEFAULT_JUNIT: &str = "target/nextest/ci/junit.xml";

/// The emitting tool name every diagnostic here is stamped with.
const TOOL: &str = "test-budget";

/// A boxed reporter for this bin. stdout carries the OK product line, so
/// diagnostics default to the HUMAN stderr surface; an agent opts into the
/// machine surface with `GMEOW_CONSOLE=jsonl`.
fn reporter() -> Box<dyn Reporter> {
    let mode = ConsoleMode::resolve_stderr_default(
        None,
        std::env::var("GMEOW_CONSOLE").ok().as_deref(),
        std::io::stderr().is_terminal(),
    );
    gmeow_cli_core::reporter_for(mode)
}

/// Surface an Error-grade diagnostic (the gate's hard failure) on the console
/// sink — the substrate replacement for a bare error stderr write. The `code` is
/// interned once (idempotently); the message carries the specifics.
fn emit_error(reporter: &dyn Reporter, code: &str, message: impl Into<String>) {
    let diag = Diag::new(
        gmeow_errors::code::register_code(code),
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        ),
        message,
    );
    reporter.report(&report_diag(diag, TOOL));
}

/// Surface an advisory (non-blocking) Warning-grade diagnostic — the local,
/// contention-tolerant form of the budget report. Same offender list as the hard
/// failure, but a `Warning`/`Advisory` grade so it informs without gating.
fn emit_warning(reporter: &dyn Reporter, code: &str, message: impl Into<String>) {
    let diag = Diag::new(
        gmeow_errors::code::register_code(code),
        Grade::new(
            Severity::Warning,
            FindingCategory::PolicyWarning,
            Standpoint::Advisory,
        ),
        message,
    );
    reporter.report(&report_diag(diag, TOOL));
}

/// Resolve whether the gate enforces (hard-fails) or is advisory.
///
/// Precedence: an explicit `GMEOW_TEST_BUDGET_ENFORCE` bool wins in both
/// directions; otherwise enforcement follows the presence of a non-empty `CI`.
///
/// - explicit `Ok(bool)` → that value.
/// - explicit `Ok(unparsable)` → hard error (never silently pick a mode).
/// - explicit `Err(NotUnicode)` → hard error.
/// - explicit `Err(NotPresent)` → `ci` is non-empty.
///
/// `ci` is treated as present only when `Ok(non-empty)` — an empty string counts
/// as unset, matching shell semantics.
fn resolve_enforcement(
    explicit: Result<String, VarError>,
    ci: Result<String, VarError>,
) -> gmeow_errors::Result<bool> {
    match explicit {
        Ok(s) => parse_enforce_bool(&s),
        Err(VarError::NotUnicode(raw)) => Err(Diag::of_kind(error::InvalidBudgetVar {
            reason: format!("{ENFORCE_VAR} is set but contains non-UTF-8 bytes: {raw:?}"),
        })),
        Err(VarError::NotPresent) => Ok(matches!(ci, Ok(v) if !v.is_empty())),
    }
}

/// Parse an enforcement bool: `1`/`true`/`yes`/`on` → true; `0`/`false`/`no`/`off`
/// → false (case-insensitive). Anything else is a hard error, so a typo in the
/// override cannot silently flip the gate to the wrong mode.
fn parse_enforce_bool(s: &str) -> gmeow_errors::Result<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(Diag::of_kind(error::InvalidBudgetVar {
            reason: format!(
                "{ENFORCE_VAR}={other:?} is not a recognised boolean — \
                 expected one of 1/true/yes/on or 0/false/no/off"
            ),
        })),
    }
}

/// Resolve the test-budget from the environment variable result.
///
/// - `Err(NotPresent)` → legitimate unset, return `DEFAULT_BUDGET_SECS`.
/// - `Err(NotUnicode)` → set but not valid UTF-8 → hard error.
/// - `Ok(s)` where `s` parses to a finite f64 > 0 → use it.
/// - `Ok(s)` where `s` is unparsable, <= 0, or non-finite → hard error.
///
/// Returns `Ok(budget)` on success, `Err(message)` on hard failure.
fn resolve_budget(var: Result<String, VarError>) -> gmeow_errors::Result<f64> {
    match var {
        Err(VarError::NotPresent) => Ok(DEFAULT_BUDGET_SECS),
        Err(VarError::NotUnicode(raw)) => Err(Diag::of_kind(error::InvalidBudgetVar {
            reason: format!("GMEOW_TEST_BUDGET_SECS is set but contains non-UTF-8 bytes: {raw:?}"),
        })),
        Ok(s) => {
            let v: f64 = s.parse().map_err(|_| {
                Diag::of_kind(error::InvalidBudgetVar {
                    reason: format!(
                        "GMEOW_TEST_BUDGET_SECS={s:?} is not a valid number — \
                         expected a positive finite f64 (e.g. \"30.0\")"
                    ),
                })
            })?;
            if !v.is_finite() {
                return Err(Diag::of_kind(error::InvalidBudgetVar {
                    reason: format!(
                        "GMEOW_TEST_BUDGET_SECS={s:?} is non-finite (NaN or infinity) — \
                         expected a positive finite f64"
                    ),
                }));
            }
            if v <= 0.0 {
                return Err(Diag::of_kind(error::InvalidBudgetVar {
                    reason: format!(
                        "GMEOW_TEST_BUDGET_SECS={s:?} is <= 0 — budget must be a positive number"
                    ),
                }));
            }
            Ok(v)
        }
    }
}

fn main() -> ExitCode {
    // Seed the diagnostic-code registry before any intern (idempotent).
    error::register_all();
    let reporter = reporter();
    let mut args = std::env::args().skip(1);
    let junit_path = args.next().unwrap_or_else(|| DEFAULT_JUNIT.to_owned());

    let budget = match resolve_budget(std::env::var("GMEOW_TEST_BUDGET_SECS")) {
        Ok(b) => b,
        Err(diag) => {
            reporter.report(&report_diag(diag, TOOL));
            return ExitCode::FAILURE;
        }
    };

    // Enforce (hard-fail) in CI; advisory locally. A malformed explicit override
    // is a hard error — the gate never silently picks a mode.
    let enforce = match resolve_enforcement(std::env::var(ENFORCE_VAR), std::env::var("CI")) {
        Ok(e) => e,
        Err(diag) => {
            reporter.report(&report_diag(diag, TOOL));
            return ExitCode::FAILURE;
        }
    };

    let xml = match std::fs::read_to_string(&junit_path) {
        Ok(s) => s,
        Err(e) => {
            // Hard fail (no-optionality): the gate cannot run without the report.
            emit_error(
                reporter.as_ref(),
                "gmeow-test-budget.read",
                format!(
                    "cannot read JUnit report at {junit_path}: {e}\n\
                     run `cargo nextest run --profile ci` (or maint-heavy) first to produce it."
                ),
            );
            return ExitCode::FAILURE;
        }
    };

    let cases = match parse_testcases(&xml) {
        Ok(c) => c,
        Err(diag) => {
            reporter.report(&report_diag(diag, TOOL));
            return ExitCode::FAILURE;
        }
    };
    if cases.is_empty() {
        emit_error(
            reporter.as_ref(),
            "gmeow-test-budget.vacuous",
            format!(
                "no <testcase> elements found in {junit_path} — refusing to pass a vacuous gate."
            ),
        );
        return ExitCode::FAILURE;
    }

    let mut over: Vec<&TestCase> = cases.iter().filter(|c| c.time > budget).collect();
    over.sort_by(|a, b| {
        b.time
            .partial_cmp(&a.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let slowest = cases.iter().map(|c| c.time).fold(0.0_f64, f64::max);

    if over.is_empty() {
        println!(
            "test-budget: OK — {} tests, all under the {budget:.0}s budget (slowest {slowest:.1}s) [{junit_path}]",
            cases.len()
        );
        return ExitCode::SUCCESS;
    }

    let headline = if enforce { "FAIL" } else { "ADVISORY" };
    let mut message = format!(
        "{headline} — {} test(s) exceed the {budget:.0}s always-on budget [{junit_path}]:",
        over.len()
    );
    for c in &over {
        message.push_str(&format!("\n  {:7.1}s  {}::{}", c.time, c.classname, c.name));
    }
    message.push_str(
        "\n\nEither make the test(s) faster, or — if irreducibly heavy — add them to the\n\
         `default-filter` off-gate allowlist in .config/nextest.toml (and AGENTS.md) so\n\
         they run on the `maint-heavy` profile instead of the per-commit gate.",
    );

    if enforce {
        emit_error(reporter.as_ref(), "gmeow-test-budget.over-budget", message);
        ExitCode::FAILURE
    } else {
        // Local/advisory: wall-clock under shared-runner contention is not a
        // trustworthy signal, so surface the offenders as a warning but do not
        // gate. CI (where `CI`/`GMEOW_TEST_BUDGET_ENFORCE` is set) hard-fails.
        message.push_str(
            "\n\nAdvisory only on this environment (the 25s budget HARD-FAILS in CI). \
             If the same tests breach in CI, they must be fixed or off-gated.",
        );
        emit_warning(reporter.as_ref(), "gmeow-test-budget.over-budget", message);
        ExitCode::SUCCESS
    }
}

/// A single parsed JUnit test case.
#[derive(Debug)]
struct TestCase {
    classname: String,
    name: String,
    time: f64,
}

/// Scan every `<testcase ...>` opening tag and pull its `classname`, `name`, and
/// `time` attributes. nextest emits one regular `<testcase>` element per test with
/// these attributes on the opening tag, so a tag-level attribute scan is robust.
///
/// Returns `Err(message)` if any testcase element is missing a parseable `time`
/// attribute (hard-fail: a silent drop would weaken the gate).
fn parse_testcases(xml: &str) -> gmeow_errors::Result<Vec<TestCase>> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<testcase") {
        rest = &rest[start..];

        // Boundary guard: the char after "<testcase" must be whitespace, '>', or '/'
        // so that a hypothetical "<testcasex" element is not treated as a testcase.
        let boundary_char = rest.as_bytes().get("<testcase".len()).copied();
        let is_testcase = matches!(
            boundary_char,
            Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') | Some(b'>') | Some(b'/')
        );

        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..end];

        if is_testcase {
            let name = attr(tag, "name").unwrap_or_default();
            let classname = attr(tag, "classname").unwrap_or_default();
            match attr(tag, "time").and_then(|s| s.parse::<f64>().ok()) {
                Some(time) => {
                    out.push(TestCase {
                        classname,
                        name,
                        time,
                    });
                }
                None => {
                    return Err(Diag::of_kind(error::MalformedTestcase { classname, name }));
                }
            }
        }

        rest = &rest[end..];
    }
    Ok(out)
}

/// Extract the value of `key="..."` from a single XML opening tag.
///
/// The match is only accepted when the character immediately preceding `key="`
/// is ASCII whitespace, preventing `name` from matching the tail of `classname`
/// regardless of attribute order in the tag.
fn attr(tag: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=\"");
    let bytes = tag.as_bytes();
    for (pos, _) in tag.match_indices(needle.as_str()) {
        // Accept only if the preceding byte is whitespace (attribute boundary).
        // pos == 0 would mean the tag starts with the key, which can't be a
        // valid attribute position (the tag starts with `<testcase`), but we
        // guard it anyway for correctness.
        if pos == 0 || bytes[pos - 1].is_ascii_whitespace() {
            let val_start = pos + needle.len();
            let val_end = tag[val_start..].find('"')? + val_start;
            return Some(unescape_xml(&tag[val_start..val_end]));
        }
    }
    None
}

/// Minimal XML attribute unescaping for the five predefined entities.
fn unescape_xml(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
<testsuites>
  <testsuite name="gmeow-rdf">
    <testcase name="fast_one" classname="gmeow-rdf::a" time="1.250"></testcase>
    <testcase name="slow_one" classname="gmeow-rdf::b" time="42.000">
      <system-out>noise</system-out>
    </testcase>
    <testcase name="amp_&amp;_name" classname="gmeow-rdf::c" time="3.0"/>
  </testsuite>
</testsuites>
"#;

    #[test]
    fn parses_all_testcases_with_times() {
        let cases = parse_testcases(SAMPLE).unwrap();
        assert_eq!(cases.len(), 3);
        assert_eq!(cases[0].name, "fast_one");
        assert_eq!(cases[1].time, 42.0);
        assert_eq!(cases[2].name, "amp_&_name");
    }

    #[test]
    fn detects_over_budget() {
        let cases = parse_testcases(SAMPLE).unwrap();
        let over: Vec<_> = cases.iter().filter(|c| c.time > 25.0).collect();
        assert_eq!(over.len(), 1);
        assert_eq!(over[0].classname, "gmeow-rdf::b");
    }

    #[test]
    fn attr_handles_missing_key() {
        assert_eq!(attr("<testcase name=\"x\">", "time"), None);
    }

    /// Regression test: `classname` appears BEFORE `name` in the tag.
    /// The old substring `find` would match `name="` inside `classname="..."`
    /// and return the classname value instead of the real name.
    #[test]
    fn attr_classname_before_name_does_not_shadow_name() {
        let xml = r#"<testsuites>
  <testsuite name="gmeow-rdf">
    <testcase classname="gmeow-rdf::pkg" name="real_name" time="2.0"/>
  </testsuite>
</testsuites>"#;
        let cases = parse_testcases(xml).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].name, "real_name");
        assert_eq!(cases[0].classname, "gmeow-rdf::pkg");
        assert_eq!(cases[0].time, 2.0);
    }

    /// A testcase element with no `time` attribute must hard-fail, not silently drop.
    #[test]
    fn rejects_testcase_without_time() {
        let xml = r#"<testsuites>
  <testsuite name="gmeow-rdf">
    <testcase name="no_time_test" classname="gmeow-rdf::x"></testcase>
  </testsuite>
</testsuites>"#;
        let result = parse_testcases(xml);
        assert!(result.is_err(), "expected Err for missing time, got Ok");
        let diag = result.unwrap_err();
        assert!(
            diag.message().contains("no_time_test"),
            "error message should name the offending testcase, got: {}",
            diag.message()
        );
    }

    /// `resolve_budget` with `NotPresent` (var unset) → default.
    #[test]
    fn resolve_budget_unset_gives_default() {
        let result = resolve_budget(Err(VarError::NotPresent));
        assert_eq!(result.unwrap(), DEFAULT_BUDGET_SECS);
    }

    /// `resolve_budget` with a valid positive value → that value.
    #[test]
    fn resolve_budget_valid_value() {
        let result = resolve_budget(Ok("30.0".to_owned()));
        assert_eq!(result.unwrap(), 30.0);
    }

    /// `resolve_budget` with an unparsable string → Err.
    #[test]
    fn resolve_budget_rejects_non_numeric() {
        let result = resolve_budget(Ok("foo".to_owned()));
        assert!(result.is_err(), "expected Err for non-parsable string");
    }

    /// `resolve_budget` with zero → Err.
    #[test]
    fn resolve_budget_rejects_zero() {
        let result = resolve_budget(Ok("0".to_owned()));
        assert!(result.is_err(), "expected Err for zero budget");
    }

    /// `resolve_budget` with a negative value → Err.
    #[test]
    fn resolve_budget_rejects_negative() {
        let result = resolve_budget(Ok("-5".to_owned()));
        assert!(result.is_err(), "expected Err for negative budget");
    }

    /// `resolve_budget` with NotUnicode → Err.
    #[test]
    fn resolve_budget_rejects_not_unicode() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        let bad = OsString::from_vec(vec![0xFF, 0xFE]);
        let result = resolve_budget(Err(VarError::NotUnicode(bad)));
        assert!(result.is_err(), "expected Err for non-UTF-8 var");
    }

    /// Explicit override unset + `CI` set (non-empty) → enforce (CI behaviour).
    #[test]
    fn enforcement_ci_set_enforces() {
        let e = resolve_enforcement(Err(VarError::NotPresent), Ok("true".to_owned()));
        assert!(e.unwrap(), "CI present should enforce");
    }

    /// Explicit override unset + `CI` unset → advisory (local behaviour).
    #[test]
    fn enforcement_no_ci_is_advisory() {
        let e = resolve_enforcement(Err(VarError::NotPresent), Err(VarError::NotPresent));
        assert!(!e.unwrap(), "no CI should be advisory");
    }

    /// An empty `CI=""` counts as unset → advisory.
    #[test]
    fn enforcement_empty_ci_is_advisory() {
        let e = resolve_enforcement(Err(VarError::NotPresent), Ok(String::new()));
        assert!(!e.unwrap(), "empty CI should be advisory");
    }

    /// Explicit `0` wins even when `CI` is set → advisory.
    #[test]
    fn enforcement_explicit_false_beats_ci() {
        let e = resolve_enforcement(Ok("0".to_owned()), Ok("true".to_owned()));
        assert!(!e.unwrap(), "explicit 0 must override CI to advisory");
    }

    /// Explicit `1` wins even when `CI` is unset → enforce.
    #[test]
    fn enforcement_explicit_true_beats_absent_ci() {
        let e = resolve_enforcement(Ok("on".to_owned()), Err(VarError::NotPresent));
        assert!(e.unwrap(), "explicit on must enforce without CI");
    }

    /// A malformed explicit override is a hard error (never silently pick a mode).
    #[test]
    fn enforcement_rejects_malformed_override() {
        let e = resolve_enforcement(Ok("maybe".to_owned()), Err(VarError::NotPresent));
        assert!(e.is_err(), "unrecognised override must be a hard error");
    }

    /// `parse_enforce_bool` accepts the documented spellings, case-insensitively.
    #[test]
    fn parse_enforce_bool_spellings() {
        for t in ["1", "true", "YES", "On"] {
            assert!(parse_enforce_bool(t).unwrap(), "{t} should be true");
        }
        for f in ["0", "false", "NO", "Off"] {
            assert!(!parse_enforce_bool(f).unwrap(), "{f} should be false");
        }
        assert!(parse_enforce_bool("nope").is_err());
    }
}
