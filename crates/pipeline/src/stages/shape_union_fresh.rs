// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The FRESH shape-union loader: the registry union
//! (`purrdf::shapes::shape_union::load_shapes`) with every produced
//! `generated/shapes/*.ttl` member sourced from THIS run's consumed stage products
//! instead of disk.
//!
//! The registry loader reads every union member off disk. That is correct for the
//! AUTHORED sources (`shapes/*.ttl`, `slices/*/*/shapes.ttl`) but wrong inside the
//! pipeline for the generated members: each is a produced projection whose committed
//! file the fanout rewrites only AFTER phase 1 returns, so a disk read hands the
//! consumer the PREVIOUS run's bytes forever (the stale-disk-fold class). The carrier
//! already enforces the freshness law for its REP_SHAPES fold
//! (`carrier.rs::build_archive_blobs`: "The `generated/shapes/*.ttl` members are NEVER
//! read off disk"); this module extends the SAME law to the shape-union consumers —
//! `stage-export-json-schema`, `stage-export-pydantic`, and `stage-validate` — through
//! ONE fresh-union implementation, so a shape-source edit reaches every derived
//! surface in a single `make check`.
//!
//! The merge semantics replicate `purrdf::shapes::shape_union::load_shapes` EXACTLY:
//! the ordered `shape_files` file list, per-file Turtle parse via
//! [`parse_dataset`], per-file blank-label scoping via [`RdfDataset::union`], document
//! `@prefix` recovery via [`extract_prefixes`] with last-declaration-wins over the
//! sorted file order, and the final [`from_dataset_with_prefixes`] typing. Only the
//! BYTE SOURCE of the generated members differs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use purrdf::shapes::shape_union::EXCLUDED;
use purrdf::shapes::shapes::{Shapes, from_dataset_with_prefixes};
use purrdf::shapes::text_ingest::extract_prefixes;
use purrdf::{RdfDataset, RdfDatasetBuilder, parse_dataset};

use crate::node::StageProduct;

/// The repo-relative prefix of the produced shape-union members.
const GENERATED_SHAPES_PREFIX: &str = "generated/shapes/";

/// The producer stages a fresh-union consumer must `consumes()` — the stages whose
/// in-memory products carry THIS run's `generated/shapes/*.ttl` bytes (see
/// [`fresh_generated_shape_members`]). Sorted; the slice DAG's
/// `gmeow:dataflowConsumes` mirrors these edges and `run.rs::full_spec` repeats them.
pub const GENERATED_SHAPE_PRODUCERS: &[&str] = &[
    "stage-compile-logic",
    "stage-export-constraint-shapes",
    "stage-export-frame-shapes",
    "stage-export-result-shapes",
];

/// [`GENERATED_SHAPE_PRODUCERS`] as the owned `consumes()` list a fresh-union
/// export leaf holds.
pub fn producer_consumes() -> Vec<String> {
    GENERATED_SHAPE_PRODUCERS
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

fn stage_err(stage: &str, message: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: stage.to_owned(),
        message,
    })
}

/// Assemble the fresh `{repo-relative path → bytes}` map of every produced
/// `generated/shapes/*.ttl` union member from a consuming stage's upstream products:
///
///   * `validation-shapes.ttl` + `procedural-constraints.ttl` ← `stage-compile-logic`
///     (OPT axis + OWL-restriction derivation; procedural `logic:Constraint` surface)
///   * `result-shapes.ttl` ← `stage-export-result-shapes`
///   * `frame-shapes.ttl` ← `stage-export-frame-shapes` (P11 frame relativity)
///   * `constraint-shapes.ttl` ← `stage-export-constraint-shapes`
///
/// Each MUST exist in its product (no-optionality, fail-closed): a missing artifact
/// HARD-fails and is never papered over with a stale on-disk read — the committed
/// file is the PREVIOUS run's projection, so falling back would freeze the
/// last-committed bytes forever (the stale-disk-fold class). `stage` names the
/// consuming stage for error attribution.
pub fn fresh_generated_shape_members(
    stage: &str,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let sources: [(&str, &str); 5] = [
        (
            "stage-compile-logic",
            crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH,
        ),
        (
            "stage-compile-logic",
            crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
        ),
        (
            "stage-export-constraint-shapes",
            crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH,
        ),
        (
            "stage-export-frame-shapes",
            crate::stages::frame_shapes::FRAME_SHAPES_PATH,
        ),
        (
            "stage-export-result-shapes",
            crate::stages::result_shapes::RESULT_SHAPES_PATH,
        ),
    ];
    let mut fresh: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (producer, rel) in sources {
        let bytes = upstream
            .get(producer)
            .and_then(|p| p.artifact(rel))
            .ok_or_else(|| {
                stage_err(
                    stage,
                    format!(
                        "{producer} produced no {rel} product; refusing to fall back to a \
                         stale on-disk read (the stale-disk-fold class, fail-closed)"
                    ),
                )
            })?;
        fresh.insert(rel.to_string(), bytes.to_vec());
    }
    Ok(fresh)
}

fn parse_err(message: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Parse { message })
}

/// Every `*.ttl` directly under `dir`, sorted — a byte-faithful replica of purrdf's
/// private `ttl_files` (an absent directory yields an empty list, never a hard-fail).
/// This lets the AUTHORED sections be enumerated WITHOUT `purrdf::shape_files`, whose
/// `generated/shapes/*.ttl` fail-closed enforcement cannot serve a cold tree — the
/// generated members are THIS run's products, sourced from the carrier, never disk.
fn ttl_files_sorted(dir: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir)
        .map_err(|e| parse_err(format!("failed to read {}: {e}", dir.display())))?
    {
        let path = entry
            .map_err(|e| {
                parse_err(format!(
                    "failed to read dir entry in {}: {e}",
                    dir.display()
                ))
            })?
            .path();
        if path.extension().and_then(|e| e.to_str()) == Some("ttl") && path.is_file() {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// `shapes/*.ttl` minus the DSL/manifest lints ([`EXCLUDED`]), sorted — purrdf
/// `shape_files` section 1, replicated so it never triggers the `generated/shapes`
/// fail-closed read.
fn authored_base_shape_files(root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
    let mut base = ttl_files_sorted(&root.join("shapes"))?;
    base.retain(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| !EXCLUDED.contains(&n))
    });
    Ok(base)
}

/// `slices/*/*/shapes.ttl` (exactly two directory levels), sorted — a byte-faithful
/// replica of purrdf's private `slice_shape_files` (section 3 of `shape_files`).
fn authored_slice_shape_files(root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
    let slices_dir = root.join("slices");
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    if !slices_dir.exists() {
        return Ok(out);
    }
    for group in std::fs::read_dir(&slices_dir)
        .map_err(|e| parse_err(format!("failed to read {}: {e}", slices_dir.display())))?
    {
        let group_path = group
            .map_err(|e| {
                parse_err(format!(
                    "dir entry error under {}: {e}",
                    slices_dir.display()
                ))
            })?
            .path();
        if !group_path.is_dir() {
            continue;
        }
        // A read error would silently drop an entire slice subtree from the union —
        // hard-fail (no-optionality), matching purrdf's sibling read_dir.
        for slice in std::fs::read_dir(&group_path)
            .map_err(|e| parse_err(format!("failed to read {}: {e}", group_path.display())))?
        {
            let slice_path = slice
                .map_err(|e| {
                    parse_err(format!(
                        "dir entry error under {}: {e}",
                        group_path.display()
                    ))
                })?
                .path();
            if !slice_path.is_dir() {
                continue;
            }
            let candidate = slice_path.join("shapes.ttl");
            if candidate.is_file() {
                out.push(candidate);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// The AUTHORED (disk-read) half of the shape union — `shapes/*.ttl` minus the DSL
/// lints plus `slices/*/*/shapes.ttl` — purrdf `shape_files` sections 1 and 3, WITHOUT
/// its `generated/shapes` fail-closed read (the generated members are this run's
/// products, sourced from the carrier). A fresh-union consumer declares exactly these
/// as its raw `input_files` (cache soundness for the authored sources); the generated
/// members' freshness is covered by the producer products' digests on its
/// `consumes()` edges, so they are deliberately NOT declared (a `generated/` path in
/// `input_files` is itself the stale-disk-fold bug class), and enumerating them off
/// disk would additionally cold-fail on a clean clone.
pub fn authored_shape_files(root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
    let mut files = authored_base_shape_files(root)?;
    files.extend(authored_slice_shape_files(root)?);
    Ok(files)
}

/// The EFFECTIVE shape-union member set as `(repo-relative path, bytes)` — exactly the
/// bytes [`load_shapes_fresh`] parses, in the same order: authored `shapes/*.ttl` and
/// `slices/*/*/shapes.ttl` read from disk, generated members taken from THIS run's
/// product bytes.
///
/// This is the shape half of `stage-validate`'s recorded input digest
/// ([`crate::stages::validate::shacl_input_digest`]). Because the generated members
/// come from the product and not from `generated/shapes/*.ttl` on disk, a consumer
/// that recomputes the digest over its own DISK view detects committed-shape drift —
/// the one capability the removed duplicate whole-corpus SHACL run uniquely had.
///
/// # Errors
/// If an authored member cannot be read, or a `fresh` key does not lie under
/// `generated/shapes/`.
pub fn effective_union_members(
    root: &Path,
    fresh: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<(String, Vec<u8>)>, gmeow_errors::Diag> {
    for key in fresh.keys() {
        if !key.starts_with(GENERATED_SHAPES_PREFIX) {
            return Err(parse_err(format!(
                "fresh shape-union member {key} does not lie under \
                 {GENERATED_SHAPES_PREFIX} — only produced generated members may be \
                 byte-overridden"
            )));
        }
    }
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    for path in authored_shape_files(root)? {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = std::fs::read(&path)
            .map_err(|e| parse_err(format!("failed to read shape file {}: {e}", path.display())))?;
        out.push((rel, bytes));
    }
    out.extend(
        fresh
            .iter()
            .map(|(rel, bytes)| (rel.clone(), bytes.clone())),
    );
    Ok(out)
}

/// Parse the full shape union into ONE frozen [`RdfDataset`] + typed [`Shapes`],
/// with every `generated/shapes/*.ttl` member's bytes taken from `fresh` (THIS
/// run's consumed products — see [`fresh_generated_shape_members`]) and every
/// authored member read from disk as the registry loader does.
///
/// Exact-semantics twin of `purrdf::shapes::shape_union::load_shapes`: same ordered
/// file list, same per-file parse, same per-file blank scoping ([`RdfDataset::union`]
/// standardizes each input's blanks apart), same document-prefix recovery
/// (last declaration wins over the merge order), same
/// [`from_dataset_with_prefixes`] typing. The generated section is exactly the sorted
/// `fresh` product keys — never a disk enumeration of `generated/shapes/*.ttl`, which
/// is absent on a clean clone and the previous run's bytes on a warm one. On a fixed
/// point the fresh keys equal the on-disk listing, so the union order is byte-identical
/// to `purrdf::shapes::shape_union::shape_files`.
///
/// # Errors
///
/// HARD-fails (no-optionality, never a stale-disk fallback) when:
///   * a `fresh` key does not lie under `generated/shapes/`;
///   * an authored member cannot be read, any member fails to parse as Turtle, or
///     the merged union carries an unsupported SHACL construct.
pub fn load_shapes_fresh(
    root: &Path,
    fresh: &BTreeMap<String, Vec<u8>>,
) -> Result<(Arc<RdfDataset>, Shapes), gmeow_errors::Diag> {
    let union_err = |message: String| gmeow_errors::Diag::of_kind(crate::error::Parse { message });
    for key in fresh.keys() {
        if !key.starts_with(GENERATED_SHAPES_PREFIX) {
            return Err(union_err(format!(
                "fresh shape-union member {key} does not lie under \
                 {GENERATED_SHAPES_PREFIX} — only produced generated members may be \
                 byte-overridden"
            )));
        }
    }

    // The authored sections (base `shapes/*.ttl` → slice `slices/*/*/shapes.ttl`,
    // each sorted), replicating purrdf's `shape_files` sections 1 and 3 WITHOUT its
    // `generated/shapes` fail-closed read.
    let base = authored_base_shape_files(root)?;
    let slices = authored_slice_shape_files(root)?;

    // The generated section is exactly the fresh product keys (sorted; `fresh` is a
    // BTreeMap). A stale on-disk `generated/shapes/*.ttl` this run does not produce is
    // intentionally ignored — the fanout prunes it as an orphan, and reading it here
    // would be the stale-disk-fold class. The fresh keys are the sole authority.
    let generated_members: BTreeSet<&str> = fresh.keys().map(String::as_str).collect();

    // One ordered (label, bytes) walk replicating load_shapes' per-file loop:
    // base (disk) → generated (fresh product bytes) → slices (disk).
    enum Member<'a> {
        Disk(std::path::PathBuf),
        Fresh(&'a str),
    }
    let mut ordered: Vec<Member<'_>> = Vec::new();
    ordered.extend(base.into_iter().map(Member::Disk));
    for rel in &generated_members {
        ordered.push(Member::Fresh(rel));
    }
    ordered.extend(slices.into_iter().map(Member::Disk));

    let mut prefix_map: BTreeMap<String, String> = BTreeMap::new();
    let mut per_file: Vec<Arc<RdfDataset>> = Vec::with_capacity(ordered.len());
    for member in &ordered {
        let (label, bytes): (String, std::borrow::Cow<'_, [u8]>) = match member {
            Member::Disk(path) => (
                path.display().to_string(),
                std::borrow::Cow::Owned(std::fs::read(path).map_err(|e| {
                    union_err(format!("failed to read shape file {}: {e}", path.display()))
                })?),
            ),
            Member::Fresh(rel) => {
                // `ordered`'s Fresh entries are exactly `fresh`'s keys, so this lookup
                // always resolves; the defensive error covers only an internal invariant break.
                let bytes = fresh.get(*rel).map(Vec::as_slice).ok_or_else(|| {
                    union_err(format!(
                        "internal invariant: generated shape-union member {rel} was ordered \
                         from the fresh product map but has no bytes"
                    ))
                })?;
                ((*rel).to_string(), std::borrow::Cow::Borrowed(bytes))
            }
        };
        let text = std::str::from_utf8(&bytes)
            .map_err(|e| union_err(format!("shape file {label} is not UTF-8: {e}")))?;
        // Parse via the native codecs. The native codec drops document prefixes once
        // it folds to the IR, so the per-file `@prefix` map is recovered by scanning
        // the source text (mirrors load_shapes).
        let dataset = parse_dataset(&bytes, "text/turtle", None)
            .map_err(|e| union_err(format!("failed to parse Turtle shape file {label}: {e}")))?;
        per_file.push(dataset);
        for (prefix, namespace) in extract_prefixes(text) {
            prefix_map.insert(prefix, namespace);
        }
    }

    // Union all per-file datasets into one, standardizing blanks apart per file.
    let merged = if per_file.is_empty() {
        RdfDatasetBuilder::new()
            .freeze()
            .map_err(|e| union_err(format!("failed to build empty shapes dataset: {e}")))?
    } else {
        let refs: Vec<&RdfDataset> = per_file.iter().map(AsRef::as_ref).collect();
        Arc::new(RdfDataset::union(&refs))
    };
    let doc_prefixes: Vec<(String, String)> = prefix_map.into_iter().collect();
    let shapes = from_dataset_with_prefixes(&merged, &doc_prefixes).map_err(union_err)?;
    Ok((merged, shapes))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).unwrap();
        std::fs::write(path, content).unwrap();
    }

    const PREFIXES: &str =
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n@prefix ex: <https://example.test/> .\n";

    /// The sorted `sh:targetClass` IRIs of a loaded union — the assertable identity
    /// of which member files actually joined it.
    fn target_classes(shapes: &Shapes) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for shape in &shapes.node_shapes {
            for target in &shape.targets {
                if let purrdf::shapes::shapes::Target::Class(c) = target {
                    out.push(c.as_str().to_string());
                }
            }
        }
        out.sort();
        out
    }

    /// A tiny repo-root fixture: one authored shape + one on-disk generated member.
    fn mock_repo(disk_generated: &str) -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        write(
            &repo.path().join("shapes/authored-shapes.ttl"),
            &format!("{PREFIXES}ex:AuthoredShape a sh:NodeShape ; sh:targetClass ex:Authored .\n"),
        );
        write(
            &repo.path().join("generated/shapes/validation-shapes.ttl"),
            disk_generated,
        );
        std::fs::create_dir_all(repo.path().join("slices")).unwrap();
        repo
    }

    /// The regression the stale-disk-fold fix pins: the on-disk generated member
    /// carries one (stale) shape, the fresh product byte carries a DIFFERENT one —
    /// the loaded union MUST reflect the FRESH bytes, never the disk bytes.
    #[test]
    fn fresh_bytes_win_over_stale_disk_bytes() {
        let repo = mock_repo(&format!(
            "{PREFIXES}ex:StaleShape a sh:NodeShape ; sh:targetClass ex:Stale .\n"
        ));
        let fresh = BTreeMap::from([(
            "generated/shapes/validation-shapes.ttl".to_string(),
            format!("{PREFIXES}ex:FreshShape a sh:NodeShape ; sh:targetClass ex:Fresh .\n")
                .into_bytes(),
        )]);
        let (_store, shapes) = load_shapes_fresh(repo.path(), &fresh).expect("fresh union loads");
        let classes = target_classes(&shapes);
        assert!(
            classes.contains(&"https://example.test/Fresh".to_string()),
            "the union must carry THIS run's fresh generated shape; got {classes:?}"
        );
        assert!(
            !classes.contains(&"https://example.test/Stale".to_string()),
            "the union must NOT carry the previous run's on-disk bytes (the \
             stale-disk-fold class); got {classes:?}"
        );
        assert!(
            classes.contains(&"https://example.test/Authored".to_string()),
            "the authored disk member still joins the union; got {classes:?}"
        );
    }

    /// A generated member on disk with NO fresh entry is IGNORED: the fresh product
    /// keys are the SOLE authority for the generated section, so a stale on-disk file
    /// this run does not produce never joins the union (the fanout prunes it as an
    /// orphan). Reading it would be the stale-disk-fold class; and the authored section
    /// is enumerated without purrdf's `generated/shapes` fail-closed read, so an empty
    /// fresh set on a cold-shaped tree loads cleanly rather than hard-failing.
    #[test]
    fn stale_disk_generated_member_without_fresh_entry_is_ignored() {
        let repo = mock_repo(&format!(
            "{PREFIXES}ex:StaleShape a sh:NodeShape ; sh:targetClass ex:Stale .\n"
        ));
        let fresh: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let (_store, shapes) = load_shapes_fresh(repo.path(), &fresh)
            .expect("union loads with no fresh generated members");
        let classes = target_classes(&shapes);
        assert!(
            !classes.contains(&"https://example.test/Stale".to_string()),
            "a stale on-disk generated member with no fresh entry must NOT join the union \
             (fresh keys are the sole generated authority); got {classes:?}"
        );
        assert!(
            classes.contains(&"https://example.test/Authored".to_string()),
            "the authored disk member still joins; got {classes:?}"
        );
    }

    /// A fresh member that does not exist on disk yet (a first run) still joins
    /// the union — the case a disk enumeration can never serve.
    #[test]
    fn fresh_only_member_joins_the_union() {
        let repo = mock_repo(&format!(
            "{PREFIXES}ex:DiskShape a sh:NodeShape ; sh:targetClass ex:Disk .\n"
        ));
        let fresh = BTreeMap::from([
            (
                "generated/shapes/validation-shapes.ttl".to_string(),
                format!("{PREFIXES}ex:DiskShape a sh:NodeShape ; sh:targetClass ex:Disk .\n")
                    .into_bytes(),
            ),
            (
                "generated/shapes/constraint-shapes.ttl".to_string(),
                format!(
                    "{PREFIXES}ex:FirstRunShape a sh:NodeShape ; sh:targetClass ex:FirstRun .\n"
                )
                .into_bytes(),
            ),
        ]);
        let (_store, shapes) = load_shapes_fresh(repo.path(), &fresh).expect("fresh union loads");
        let classes = target_classes(&shapes);
        assert!(
            classes.contains(&"https://example.test/FirstRun".to_string()),
            "a product-only generated member (absent on disk) must join the union; got {classes:?}"
        );
    }

    /// A fresh key outside `generated/shapes/` is a misuse — hard-fail.
    #[test]
    fn non_generated_fresh_key_hard_fails() {
        let repo = mock_repo("# generated\n");
        let fresh = BTreeMap::from([("shapes/authored-shapes.ttl".to_string(), Vec::new())]);
        let err = load_shapes_fresh(repo.path(), &fresh)
            .expect_err("an authored path may never be byte-overridden");
        assert!(format!("{err}").contains("generated/shapes/"));
    }
}
