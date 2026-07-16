// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! CLI-core diagnostic kinds.
//!
//! Resolving a [`DiagnosticsConfig`](crate::DiagnosticsConfig) from flags, env,
//! and defaults is a HARD failure surface: an unrecognized console mode or
//! artifact token has no silent fallback (no-optionality), so a typo cannot
//! degrade output. Each defect is a [`gmeow_errors::DiagKind`] minted by
//! [`gmeow_errors::define_diag_kind!`], so a raised diagnostic carries a stable
//! registered [`Code`], a [`Grade`], and stays downcastable to its typed value
//! off the [`Diag`](gmeow_errors::Diag) source. There is no hand-rolled error
//! `enum`: the substrate is the single content-bound carrier.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// An unrecognized `--diagnostics-console` / `GMEOW_DIAGNOSTICS_CONSOLE`
    /// token. The diagnostics policy has no silent fallback, so an unknown mode
    /// is a hard fail rather than a degrade to the default.
    pub struct UnknownConsoleMode { value: String }
    code = "cli-core.diagnostics.unknown-console-mode";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "unknown diagnostics console mode: {:?}", value;
}

define_diag_kind! {
    /// One or more entries in a `--diagnostics-artifacts` selector are not known
    /// kinds. `unknown` is the comma-joined offending tokens; `expected` is the
    /// comma-joined canonical kinds the selector may name.
    pub struct UnknownArtifactKind { unknown: String, expected: String }
    code = "cli-core.diagnostics.unknown-artifact-kind";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "unknown diagnostics artifact kind(s): {} (expected a subset of {}, or 'none'/'all')", unknown, expected;
}

define_diag_kind! {
    /// The artifact selector parsed to an empty set (e.g. a bare `,`). `raw` is
    /// the original selector string, echoed for the operator.
    pub struct EmptyArtifactSelection { raw: String }
    code = "cli-core.diagnostics.empty-artifact-selection";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "empty diagnostics artifact selection: {:?}", raw;
}

define_diag_kind! {
    /// A documentation projection could not be reconciled safely to disk.
    pub struct DocsProjectionFailed { detail: String }
    code = "gmeow-cli-core.docs-export.io";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

/// The complete cli-core diagnostic-code catalog, in registration order. Every
/// [`DiagKind`](gmeow_errors::DiagKind) minted in the crate appears here exactly
/// once — [`register_all`] seeds them and the collision test proves the code
/// strings are distinct.
pub const CLI_CORE_DIAG_CODES: &[&str] = &[
    UnknownConsoleMode::CODE,
    UnknownArtifactKind::CODE,
    EmptyArtifactSelection::CODE,
    DocsProjectionFailed::CODE,
];

/// Eagerly intern every cli-core diagnostic code, seeding the process-wide code
/// registry before any `intern` against it. Idempotent (each `register()` is a
/// `LazyLock`), and interning is the single enumeration authority — a duplicate
/// code literal would collapse two kinds onto one handle, which the collision
/// test forbids.
pub fn register_all() -> Vec<Code> {
    vec![
        UnknownConsoleMode::register(),
        UnknownArtifactKind::register(),
        EmptyArtifactSelection::register(),
        DocsProjectionFailed::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_cli_core_code_interns_with_no_collision() {
        let handles = register_all();
        // register_all() and the catalog enumerate the same kinds in the same order.
        assert_eq!(
            handles.len(),
            CLI_CORE_DIAG_CODES.len(),
            "register_all() and CLI_CORE_DIAG_CODES must enumerate the same kinds"
        );

        // Every catalogued code interns (register_all seeded the registry).
        for code in CLI_CORE_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "cli-core code `{code}` did not intern after register_all()"
            );
        }

        // No two kinds may share a code literal: distinct strings AND distinct
        // interned handles. A duplicate `code = "..."` would fail loudly here.
        let distinct_strings: HashSet<&&str> = CLI_CORE_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            CLI_CORE_DIAG_CODES.len(),
            "duplicate cli-core diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(
            distinct_handles.len(),
            handles.len(),
            "two cli-core diagnostic kinds interned to the same code handle"
        );
    }
}
