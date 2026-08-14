// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Declarative coverage registry for the `validate` gate, and the resolver that
//! wires the committed **central-DSL** SHACL inputs onto the live entrypoint.
//!
//! Two things live here, kept deliberately together so they cannot drift:
//!
//! * [`VALIDATE_PHASE_COVERAGE`] — the single declarative source of truth for
//!   *which* validation phase runs *where*: live in `gmeow-dev validate` (the
//!   `make validate` gate) or in a named Rust test on `make check`. The
//!   `validate` help string, the CLI wiring, and the help⟺registry /
//!   liveness tests all read this table; `docs/DSL-VALIDATION-COVERAGE.md` is
//!   its prose companion.
//! * [`authored_dsl_shacl_inputs`] — resolves the three committed central-DSL
//!   trees (`dsl/{mappings,statements,tests}`) and their committed shape files
//!   (`shapes/{mapping,statement,test}-dsl-shapes.ttl`) for the live gate,
//!   **hard-failing** (never silently skipping) when a selected surface's inputs
//!   are absent, empty, or unreadable. This is what makes a "dark DSL phase"
//!   impossible on the real repository: a missing input is a loud error, not a
//!   no-inputs pass.
//!
//! Scope: DSL SHACL is a property of the **authored working tree**. The
//! `gmeow-dev validate --gts <bundle>` path validates a folded bundle that is
//! not the authored tree and therefore carries no DSL surface by design — the
//! resolver is used only on the authored-source entry.

use std::path::Path;

use gmeow_errors::Diag;

/// Where a validate phase actually executes on `make check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseHome {
    /// Runs live inside `gmeow-dev validate` — i.e. the `make validate` gate.
    OnValidate,
    /// Runs in a Rust test on `make check`, owned by the named test/crate
    /// (e.g. `"example_sweep"`, `"slicetest"`). Deliberately NOT re-run in the
    /// `validate` gate — doing so would duplicate a whole-corpus computation
    /// (`docs/GATE-AND-PIPELINE.md` P2/P3).
    OnRustTest(&'static str),
}

/// One row of the validate coverage registry: a phase, the surface it validates,
/// and where it runs.
#[derive(Debug, Clone, Copy)]
pub struct PhaseCoverage {
    /// Stable phase label (matches the `Timing.phase` the orchestration emits).
    pub phase: &'static str,
    /// The input surface the phase validates (repo-relative glob).
    pub surface: &'static str,
    /// Where the phase executes.
    pub home: PhaseHome,
}

/// The declarative coverage map for the "decide per phase 9-13" record.
///
/// This is NOT coupled to `validate_all`'s phase dispatch (that rewrite is out of
/// scope); it is the declared truth the mechanical help⟺registry check and the
/// liveness test compare against, so the `validate` help string can never quietly
/// drift from what actually runs.
pub const VALIDATE_PHASE_COVERAGE: &[PhaseCoverage] = &[
    PhaseCoverage {
        phase: "example-coverage",
        surface: "slices/*/*/examples/",
        home: PhaseHome::OnValidate,
    },
    PhaseCoverage {
        phase: "per-example-shacl",
        surface: "slices/*/*/examples/*.ttl",
        home: PhaseHome::OnRustTest("example_sweep"),
    },
    PhaseCoverage {
        phase: "mapping-dsl-shacl",
        surface: "dsl/mappings/",
        home: PhaseHome::OnValidate,
    },
    PhaseCoverage {
        phase: "statement-dsl-shacl",
        surface: "dsl/statements/",
        home: PhaseHome::OnValidate,
    },
    PhaseCoverage {
        phase: "test-dsl-shacl",
        surface: "dsl/tests/",
        home: PhaseHome::OnValidate,
    },
    PhaseCoverage {
        phase: "test-dsl-shacl-slice-local",
        surface: "slices/*/*/tests/*.ttl",
        home: PhaseHome::OnRustTest("slicetest"),
    },
];

/// The committed central-DSL SHACL inputs the live `validate` gate wires: one
/// (directory, shapes-text) pair per surface, all mandatory.
#[derive(Debug, Clone)]
pub struct AuthoredDslShacl {
    pub mapping_dir: String,
    pub statement_dir: String,
    pub test_dir: String,
    pub mapping_shapes: String,
    pub statement_shapes: String,
    pub test_shapes: String,
}

/// Repo-relative locations of every central-DSL surface's directory and its
/// committed shapes file — the single place these paths are named.
const MAPPING_DIR: &str = "dsl/mappings";
const STATEMENT_DIR: &str = "dsl/statements";
const TEST_DIR: &str = "dsl/tests";
const MAPPING_SHAPES: &str = "shapes/mapping-dsl-shapes.ttl";
const STATEMENT_SHAPES: &str = "shapes/statement-dsl-shapes.ttl";
const TEST_SHAPES: &str = "shapes/test-dsl-shapes.ttl";

/// Resolve the committed central-DSL SHACL inputs for the `make validate` gate,
/// rooted at `root` (the repository root).
///
/// Every declared surface is mandatory. A missing directory, a directory that
/// holds zero `.ttl` files, or an unreadable/empty shapes file is a **hard fail**
/// with a distinct diagnostic — never a silent skip and never a "no-inputs" pass.
/// That is the no-optionality guarantee for the live gate, and the thing the
/// liveness test in `crates/validate/tests/dsl_shacl_live.rs` proves cannot
/// regress.
///
/// # Errors
///
/// Returns `Err` naming the specific failure (absent dir / empty dir / unreadable
/// shapes / empty shapes) for the first surface that cannot be resolved.
pub fn authored_dsl_shacl_inputs(root: &Path) -> gmeow_errors::Result<AuthoredDslShacl> {
    Ok(AuthoredDslShacl {
        mapping_dir: require_dsl_dir(root, MAPPING_DIR)?,
        statement_dir: require_dsl_dir(root, STATEMENT_DIR)?,
        test_dir: require_dsl_dir(root, TEST_DIR)?,
        mapping_shapes: require_shapes(root, MAPPING_SHAPES)?,
        statement_shapes: require_shapes(root, STATEMENT_SHAPES)?,
        test_shapes: require_shapes(root, TEST_SHAPES)?,
    })
}

/// Resolve one central-DSL directory to an absolute path string, hard-failing
/// with a distinct message for "does not exist" vs "exists but holds no `.ttl`".
fn require_dsl_dir(root: &Path, rel: &str) -> gmeow_errors::Result<String> {
    let dir = root.join(rel);
    if !dir.is_dir() {
        return Err(io_err(format!(
            "DSL SHACL input directory `{rel}` does not exist under {} — the `validate` gate \
             requires it (declared OnValidate in VALIDATE_PHASE_COVERAGE); its absence is a hard \
             fail, never a silent skip",
            root.display()
        )));
    }
    // Enumerate through the SAME walk the DSL phases use, so this count and the
    // gate's validated set are one authority.
    let ttl = crate::validate_all::collect_ttl_paths(&dir.to_string_lossy())?;
    if ttl.is_empty() {
        return Err(io_err(format!(
            "DSL SHACL input directory {} contains no `.ttl` file — a selected DSL surface with \
             zero inputs is a hard fail, never a no-inputs pass",
            dir.display()
        )));
    }
    Ok(dir.to_string_lossy().into_owned())
}

/// Read one committed DSL shapes file, hard-failing on unreadable or empty.
fn require_shapes(root: &Path, rel: &str) -> gmeow_errors::Result<String> {
    let path = root.join(rel);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        io_err(format!(
            "cannot read committed DSL SHACL shapes {}: {e} — the `validate` gate requires it; \
             its absence is a hard fail, never a silent skip",
            path.display()
        ))
    })?;
    if text.trim().is_empty() {
        return Err(io_err(format!(
            "committed DSL SHACL shapes {} is empty — a selected DSL surface with empty shapes \
             is a hard fail",
            path.display()
        )));
    }
    Ok(text)
}

fn io_err(detail: String) -> Diag {
    Diag::of_kind(crate::error::Io { detail })
}
