// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Path resolution for the slice-test harness.
//!
//! The harness is anchored at this crate's manifest directory
//! (`crates/slicetest`) via `CARGO_MANIFEST_DIR`, so it never relies on the
//! process working directory. From there it derives the repository root and the
//! `slices/` tree, and resolves the two path conventions the test-DSL fixes:
//!
//! * `gmeow:cqQueryFile` is **repo-root-relative** (so a shared
//!   `queries/competency/<name>.rq` and a slice-local
//!   `slices/<group>/<name>/queries/competency/<name>.rq` are addressed the same
//!   way) — resolved by [`query_file`].
//! * `gmeow:exampleFile` is **slice-relative** (resolved against the owning
//!   slice directory, NOT the repo root) — resolved by [`example_file`].

use std::path::{Path, PathBuf};

/// The repository root, derived from this crate's manifest directory at compile
/// time (`crates/slicetest/../..`).
///
/// # Panics
///
/// Panics if the canonical repo root does not exist, which can only happen if
/// the crate is built outside the repository tree — an impossible state for the
/// harness.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root (crates/slicetest/../..) must exist")
}

/// The `slices/` tree under the repository root.
pub fn slices_root() -> PathBuf {
    repo_root().join("slices")
}

/// The owning slice directory for a discovered `tests/<file>.ttl` spec file.
///
/// A spec lives at `.../slices/<group>/<name>/tests/<file>.ttl`, so the slice
/// directory is the spec file's grandparent (`tests/` -> `<name>/`).
pub fn slice_dir(test_file: &Path) -> PathBuf {
    let dir = test_file
        .parent() // .../<name>/tests
        .and_then(Path::parent); // .../<name>
    // The datatest-stable harness only ever feeds paths matching
    // `.../tests/<file>.ttl`, so a missing grandparent means a caller bug.
    debug_assert!(
        dir.is_some(),
        "slice_dir expects .../<name>/tests/<file>.ttl, got {}",
        test_file.display()
    );
    dir.unwrap_or(test_file).to_path_buf()
}

/// The slice's canonical module graph (`<slice>/module.ttl`).
pub fn module_file(slice_dir: &Path) -> PathBuf {
    slice_dir.join("module.ttl")
}

/// The SHACL surfaces enforcing one slice. A slice mid-migration keeps a local
/// `<slice>/shapes.ttl` AND is enforced by the canonical generated projections of its
/// `logic:`/OWL-authored gates: both surfaces are loaded together so a newly
/// `logic:`-authored gate is enforced *before* the local file is retired
/// (equivalence-before-deletion — the committed local shapes are the golden oracle the
/// projector reproduces, per LOGIC-VALIDATION.md). After the local file is deleted the
/// generated projections are the sole authority.
pub fn shapes_files(slice_dir: &Path) -> Vec<PathBuf> {
    let generated = vec![
        repo_root().join("generated/shapes/validation-shapes.ttl"),
        repo_root().join("generated/shapes/constraint-shapes.ttl"),
        repo_root().join("generated/shapes/procedural-constraints.ttl"),
    ];
    let local = slice_dir.join("shapes.ttl");
    if local.is_file() {
        let mut both = vec![local];
        both.extend(generated);
        both
    } else {
        generated
    }
}

/// The slice's `examples/` directory.
pub fn examples_dir(slice_dir: &Path) -> PathBuf {
    slice_dir.join("examples")
}

/// Resolve a repo-root-relative `gmeow:cqQueryFile` to an absolute path.
pub fn query_file(rel: &str) -> PathBuf {
    repo_root().join(rel)
}

/// Resolve a slice-relative `gmeow:exampleFile` against its owning slice dir.
pub fn example_file(slice_dir: &Path, rel: &str) -> PathBuf {
    slice_dir.join(rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_dir_is_the_spec_grandparent() {
        let spec = Path::new("/repo/slices/core/epistemics/tests/competency.ttl");
        assert_eq!(slice_dir(spec), Path::new("/repo/slices/core/epistemics"));
    }

    #[test]
    fn module_and_migrated_shapes_resolve_to_their_authorities() {
        let slice = Path::new("/repo/slices/core/epistemics");
        assert_eq!(
            module_file(slice),
            Path::new("/repo/slices/core/epistemics/module.ttl")
        );
        assert_eq!(
            shapes_files(slice),
            vec![
                repo_root().join("generated/shapes/validation-shapes.ttl"),
                repo_root().join("generated/shapes/constraint-shapes.ttl"),
                repo_root().join("generated/shapes/procedural-constraints.ttl"),
            ]
        );
        assert_eq!(
            examples_dir(slice),
            Path::new("/repo/slices/core/epistemics/examples")
        );
    }

    #[test]
    fn example_file_is_slice_relative_query_file_is_repo_relative() {
        let slice = Path::new("/repo/slices/core/epistemics");
        // exampleFile resolves against the slice, never the repo root.
        assert_eq!(
            example_file(slice, "tests/counter-examples/x.ttl"),
            Path::new("/repo/slices/core/epistemics/tests/counter-examples/x.ttl")
        );
        // The real repo root is used for cqQueryFile; just assert the suffix so
        // the test is independent of where the checkout lives.
        assert!(
            query_file("queries/competency/agents.rq").ends_with("queries/competency/agents.rq")
        );
    }
}
