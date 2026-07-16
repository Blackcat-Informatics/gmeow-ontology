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

/// Canonical module data visible to one slice's example-conformance cells.
///
/// Ordinary slices remain strictly slice-local. The three grounding slices are
/// different by contract: `logic:`, `lang:`, and `math:` form one interlocked,
/// co-foundational kernel (`docs/GROUNDING.md`), so a grounding constraint may
/// legitimately need the asserted type of a peer-owned target. Give those
/// cells the three grounding modules as data while [`crate::exec`] still scopes
/// the enforcing shapes to the tested slice's authority.
pub fn conformance_module_files(slice_dir: &Path) -> Vec<PathBuf> {
    let grounding = slices_root().join("grounding");
    // datatest-stable passes repository-relative paths, while unit callers often
    // use absolute paths. Normalize the existing slice directory before testing
    // membership so both routes receive the same grounding-kernel scope.
    let canonical_slice = slice_dir
        .canonicalize()
        .unwrap_or_else(|_| slice_dir.to_path_buf());
    let is_grounding_kernel_member = canonical_slice.parent() == Some(grounding.as_path())
        && matches!(
            canonical_slice.file_name().and_then(|name| name.to_str()),
            Some("lang" | "logic" | "math")
        );
    if !is_grounding_kernel_member {
        return vec![module_file(slice_dir)];
    }

    ["lang", "logic", "math"]
        .into_iter()
        .map(|name| grounding.join(name).join("module.ttl"))
        .collect()
}

/// The SHACL surfaces enforcing one slice. Canonical generated validation and
/// constraint projections always participate; a residual local `<slice>/shapes.ttl`
/// is added while equivalence-proven migration is incomplete. This makes partial
/// migration compositional: deleting one proved-equivalent local shape cannot hide
/// its generated replacement merely because another ValidationOnly residue remains.
pub fn shapes_files(slice_dir: &Path) -> Vec<PathBuf> {
    let mut files = vec![
        repo_root().join("generated/shapes/validation-shapes.ttl"),
        repo_root().join("generated/shapes/constraint-shapes.ttl"),
        repo_root().join("generated/shapes/procedural-constraints.ttl"),
    ];
    let local = slice_dir.join("shapes.ttl");
    if local.is_file() {
        files.push(local);
    }
    files
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
    fn conformance_data_is_slice_local_except_for_the_grounding_kernel() {
        let ordinary = slices_root().join("core/epistemics");
        assert_eq!(
            conformance_module_files(&ordinary),
            vec![module_file(&ordinary)]
        );

        let grounding = slices_root().join("grounding");
        let expected = ["lang", "logic", "math"]
            .into_iter()
            .map(|name| grounding.join(name).join("module.ttl"))
            .collect::<Vec<_>>();
        for name in ["lang", "logic", "math"] {
            assert_eq!(conformance_module_files(&grounding.join(name)), expected);
        }
    }

    #[test]
    fn partially_migrated_slice_adds_local_residue_after_generated_authorities() {
        let slice = repo_root().join("slices/grounding/lang");
        assert!(slice.join("shapes.ttl").is_file());
        assert_eq!(
            shapes_files(&slice),
            vec![
                repo_root().join("generated/shapes/validation-shapes.ttl"),
                repo_root().join("generated/shapes/constraint-shapes.ttl"),
                repo_root().join("generated/shapes/procedural-constraints.ttl"),
                slice.join("shapes.ttl"),
            ]
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
