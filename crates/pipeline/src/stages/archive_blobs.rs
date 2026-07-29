// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `archive-blobs` stage: the SINGLE producer of the bundle's by-reference TAR
//! archive blobs (ten of them — see [`ARCHIVE_REPS`]).
//!
//! The pre-pipeline generator folded five TAR archives into `gmeow.gts` —
//! `mappings-archive` / `cells-archive` / `queries-archive` / `tests-archive` /
//! `schemas-archive` — that the wheel-mode consumer loaders read back
//! (`bundled_sssom` / `bundled_cells` / `bundled_queries` / `bundled_tests`). The
//! pipeline cutover dropped the WRITER (only the reader survived, orphaned), so a
//! repo-free `gmeow.gts` lost its lift maps / cells / queries / test specs and every
//! wheel-mode consumer (up-projection, docs-from-bundle, export) broke. The writer
//! was restored as a dep-free, byte-deterministic USTAR codec (sorted members,
//! zeroed mtime/uid/gid, mode 0644) so the composed snapshot stays fold-stable.
//! Member-name conventions MIRROR the reader: mappings/queries use the bare
//! filename; cells/tests preserve the repo-relative path (so
//! `bundled_cells_under(prefix)` can route by directory).
//!
//! # Why this is a STAGE, not sink-inline work
//!
//! The fold used to run INSIDE the terminal `stage-gts-sink`, so the archives did
//! not exist as a product until the last node of the DAG — any stage wanting to read
//! them (e.g. a corpus selector over `cells-archive` / `shapes-archive`) would close
//! a cycle. Re-specifying such a selector over the archives' constituent INPUTS
//! instead would duplicate archive-membership logic and create a second source of
//! truth for "what is in `cells-archive`" (Principle 4). So the fold is a first-class
//! stage whose product carries each archive on the by-reference blob lane, keyed by
//! its `representation` label; the sink now READS that product instead of computing
//! the archives itself, and any other consumer reads the SAME product.
//!
//! Every member is sourced from an in-memory upstream PRODUCT wherever a producing
//! stage exists (schemas / axioms / mappings / queries / generated shapes / Pydantic
//! package / the claim corpus's JSON-LD-family surface), never from the committed
//! `generated/` files, which are not flushed until
//! the post-run reconcile returns — a disk read here would tar the STALE committed set
//! and a source edit could never reach the bundle in one pass. Only genuinely AUTHORED
//! source trees (`dsl/mappings/`, `slices/<g>/<n>/tests/`, `shapes/`,
//! `slices/<g>/<n>/shapes.ttl`) are read from the repo, and every one of them is
//! declared in [`ArchiveBlobsStage::input_files`] so the stage's cache key busts when
//! any of them changes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::gts_compose::BlobRow;
use purrdf::provenance::DatasetProvenance;
use purrdf::{ContentDigest, PipelineBundle};

use crate::bundle::{PipelineHandle, attach_rep_blob, bundle_from_artifacts};
use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::carrier::{
    archive_blob, list_files, members_relpath, slice_files, slice_named_files,
};

/// The stage id — matches the `gmeow:stage-archive-blobs` individual.
pub const STAGE_ID: &str = "stage-archive-blobs";

/// tar of `generated/mappings/*.sssom.tsv`, member = bare filename.
pub(crate) const REP_MAPPINGS: &str = "mappings-archive";
/// tar of the cell/projection TTL sources, member = repo-relative path.
pub(crate) const REP_CELLS: &str = "cells-archive";
/// tar of `generated/queries/*.rq`, member = bare filename.
pub(crate) const REP_QUERIES: &str = "queries-archive";
/// tar of the slice test-DSL specs, member = repo-relative path.
pub(crate) const REP_TESTS: &str = "tests-archive";
/// tar of the SHACL-derived JSON Schema + OpenAPI, member = bare filename.
pub(crate) const REP_SCHEMAS: &str = "schemas-archive";
/// tar of the generated Pydantic model package, member = package-relative path
/// (`gmeow_models/...`). Re-exported from the reader-side definition in
/// [`crate::bundle_blobs`] so producer and reader share ONE constant (a drifted
/// label would silently fold/read an empty package).
pub(crate) use crate::bundle_blobs::REP_MODELS_PYTHON;
/// tar of the FULL SHACL shape surface, member = repo-relative path:
/// every `shapes/*.ttl` (incl. the 4 DSL/manifest lints the consumer's DSL phases
/// need) + every `generated/shapes/*.ttl` (P11 frame shapes) + every per-slice
/// `slices/<g>/<n>/shapes.ttl`. The full surface — NOT the validator's filtered
/// union — so a repo-free `gmeow validate` can re-derive both the data-graph
/// union and the DSL phases. The Python reader (`bundle.bundled_shapes`) MUST use
/// this exact rep string.
pub(crate) const REP_SHAPES: &str = "shapes-archive";
/// tar of the compiled logic/DL projection surface, member = repo-relative
/// path: the small committed projections in [`AXIOM_FILES`]. NOT the big reasoning
/// OUTPUTS (inferred-closure / reasoning-explanations / dl-el-crosscheck-report),
/// which ride other channels. The Python reader (`bundle.bundled_axioms`) MUST use
/// this exact rep string.
pub(crate) const REP_AXIOMS: &str = "axioms-archive";
/// tar of the `lang:` projection deliverables under
/// [`LANG_PROJECTION_DIR`](crate::stages::lang_projection::LANG_PROJECTION_DIR),
/// member = repo-relative path.
///
/// # Why these have their OWN rep instead of riding `generated-opaque-archive`
///
/// A `gmeow:CompressionDictionary` primes a REP: the rep is the unit the medium
/// registry assigns a dictionary to, so a payload family that shares a rep with
/// unrelated bytes cannot be primed separately from them. These ~150 KB of grammar /
/// CoNLL-U / TEI / GMN1 bytes are a distinct external-format family a consumer
/// extracts on its own, so they get their own rep rather than being welded to the
/// general `generated/` archive's medium assignment.
///
/// The split costs ZERO ontological use, which is exactly why it is legal here and
/// was NOT the answer for the mathematical content: these files are ALREADY opaque
/// byte projections (standalone external-format artifacts a consumer reads as files;
/// none reconstructs from a canonical named-graph fold), and their queryable
/// `lang:ProjectionEmission` semantics keep riding the `graph/lang-projection-corpus`
/// named graph independently. Nothing that was a graph becomes bytes.
///
/// The members must therefore be carried by THIS rep and no longer by
/// `REP_GENERATED`: `carrier::opaque_already_carried` refuses the prefix so the two
/// archives cannot double-carry a path, and the superset reverse sweep would catch it
/// if they did.
pub(crate) const REP_LANG_PROJECTIONS: &str = "lang-projections-archive";
/// tar of the CLAIM CORPUS's JSON-LD-family surface — the JSON-LD-star + YAML-LD-star
/// projections of the RDF 1.2 statement layer, member = bare filename
/// ([`YAMLLD_JSONLD_MEMBER`] / [`YAMLLD_YAMLLD_MEMBER`]). Re-exported from the
/// reader-side definition in [`crate::bundle_blobs`] so producer and reader share ONE
/// constant (a drifted label would silently fold/read an empty archive).
///
/// # Why this frame exists, and why its members are the CLAIM corpus
///
/// `gmeow:payloadSchemaYamlLdArchive` is registered against this rep. The writer used
/// to be `#[cfg(test)]`, so the production sink authored no such frame at all and the
/// rep was a reader-side declaration with no live producer. The frame exists now
/// because a JSON-LD-family consumer reads the reified statement layer straight out of
/// this archive. A claim-specific dictionary for it was measured and RETIRED — one
/// ~9 KB frame is too small a population for any grid cell to pay for a dictionary's
/// own in-band bytes — so the frame is primed by `gmeow:dictGmeowCoreV1` and stays
/// dictionary-compressed.
///
/// The members are the claim corpus and nothing else — not the whole carrier. (The
/// whole-carrier JSON-LD-star document is a `make build` deliverable,
/// `dist/gmeow.jsonld`; at ~666 MB it is not a bundle frame, and it has an entirely
/// different byte profile.)
///
/// The bytes are rendered ONCE by `stage-statements` (the owner of the statement
/// layer's projections) and only TARRED here — the transform-once razor.
/// Its two member names are re-exported alongside it for the same reason.
pub(crate) use crate::bundle_blobs::{REP_YAMLLD, YAMLLD_JSONLD_MEMBER, YAMLLD_YAMLLD_MEMBER};

/// The compiled logic/DL projection files folded as [`REP_AXIOMS`]: the
/// small, committed, drift-gated projections a repo-free consumer needs. The
/// big reasoning outputs are deliberately excluded. Order is canonical for the
/// fail-closed scan; the archive re-sorts members by key for determinism.
pub(crate) const AXIOM_FILES: [&str; 4] = [
    "generated/owl/gmeow-dl.ttl",
    "generated/owl/gmeow-el.ttl",
    "generated/logic/gmeow.logic.rdf12.ttl",
    "generated/datalog/gmeow.dl",
];

/// The archive representations this stage attaches, in the CANONICAL order
/// [`build_archive_blobs`] returns them. [`archive_blobs_from_product`] reads them
/// back in exactly this order, so the row sequence a consumer sees is identical to
/// the sequence the fold produced (order-stable regardless of the blob lane's own
/// record order, which a cache round-trip is free to renormalize).
const ARCHIVE_REPS: [&str; 10] = [
    REP_MAPPINGS,
    REP_CELLS,
    REP_QUERIES,
    REP_TESTS,
    REP_SCHEMAS,
    REP_SHAPES,
    REP_AXIOMS,
    REP_MODELS_PYTHON,
    REP_LANG_PROJECTIONS,
    REP_YAMLLD,
];

/// THIS run's three generated SHACL shape surfaces, folded into REP_SHAPES from the
/// producing export leaves' products (never a stale disk read). Grouped into named
/// fields so the three same-typed `&[u8]` cannot be transposed at the call site.
pub(crate) struct ShapeSurfaces<'a> {
    pub(crate) result: &'a [u8],
    pub(crate) frame: &'a [u8],
    pub(crate) constraint: &'a [u8],
}

/// The four JSON Schema surfaces folded into REP_SCHEMAS, all sourced from THIS
/// run's `stage-export-json-schema` product. Grouped into named fields (like
/// [`ShapeSurfaces`]) so the same-typed `&[u8]` cannot be transposed at the call
/// site: the two SHACL-derived documents (`schema` = `gmeow.schema.json`, `openapi`
/// = `gmeow.openapi.json`) and the two hand-authored self-describing schemas (`card`
/// = `card.schema.json`, `finding` = `validate-finding.schema.json`).
pub(crate) struct SchemaSurfaces<'a> {
    pub(crate) schema: &'a [u8],
    pub(crate) openapi: &'a [u8],
    pub(crate) card: &'a [u8],
    pub(crate) finding: &'a [u8],
}

/// The claim corpus's two JSON-LD-family surfaces folded into [`REP_YAMLLD`], both
/// sourced from THIS run's `stage-statements` product. Grouped into named fields (like
/// [`ShapeSurfaces`]) so the two same-typed `&[u8]` cannot be transposed at the call
/// site — a transposition would put YAML bytes under the `.jsonld` member name and ship
/// a frame no JSON-LD consumer can read.
pub(crate) struct ClaimSerializations<'a> {
    /// The JSON-LD-star projection of the RDF 1.2 statement layer.
    pub(crate) jsonld: &'a [u8],
    /// The YAML-LD-star projection of the RDF 1.2 statement layer.
    pub(crate) yamlld: &'a [u8],
}

/// Build the bundle archive blobs from the repo tree: mappings, cells, queries,
/// tests, schemas, the SHACL shape surface, and the compiled logic/DL axiom
/// surface. The SHACL-derived JSON Schema + OpenAPI bytes are passed in from
/// THIS run's `stage-export-json-schema` product (not re-read from disk) so a single
/// regenerate folds the fresh schema — the committed `generated/schemas/*.json` are
/// not flushed until the post-run reconcile returns.
pub(crate) fn build_archive_blobs(
    root: &Path,
    schema_surfaces: &SchemaSurfaces<'_>,
    axiom_artifacts: &BTreeMap<String, Vec<u8>>,
    mappings_artifacts: &BTreeMap<String, Vec<u8>>,
    shape_surfaces: &ShapeSurfaces<'_>,
    models_python_artifacts: &BTreeMap<String, Vec<u8>>,
    claim_serializations: &ClaimSerializations<'_>,
) -> Result<Vec<BlobRow>, gmeow_errors::Diag> {
    // mappings: member = bare filename, sourced from THIS run's stage-mappings product
    // (not re-read from disk) so a mapping-source edit folds into the bundle in one
    // regenerate — the committed generated/mappings/*.sssom.tsv are not written until
    // the post-run reconcile returns, so a disk read here would tar the stale committed set.
    let mappings =
        members_basename_from_artifacts(mappings_artifacts, "generated/mappings/", ".sssom.tsv");
    // Fail closed, mirroring the axioms guard below: an empty match means the
    // stage-mappings product keyed its SSSOM under an unexpected prefix (or emitted
    // none), which would silently fold an EMPTY mappings archive into the bundle. A
    // missing required surface is a hard error, never a degraded fallback.
    if mappings.is_empty() {
        return Err(stage_err(
            "no generated/mappings/*.sssom.tsv artifacts in the stage-mappings product — \
             the mappings archive would fold empty (fail-closed)",
        ));
    }
    // queries: member = bare filename, sourced from THIS run's stage-mappings product
    // (not re-read from disk) so a generated-query edit folds into the bundle in one
    // regenerate — the committed generated/queries/*.rq are not written until the
    // post-run reconcile returns, so a disk read here would tar the stale committed set
    // (the same stale-disk-fold trap the mappings archive above avoids).
    let queries = members_basename_from_artifacts(mappings_artifacts, "generated/queries/", ".rq");
    // Fail closed, mirroring the mappings guard above: an empty match means the
    // stage-mappings product keyed its `.rq` under an unexpected prefix (or emitted
    // none), which would silently fold an EMPTY queries archive. A missing required
    // surface is a hard error, never a degraded fallback.
    if queries.is_empty() {
        return Err(stage_err(
            "no generated/queries/*.rq artifacts in the stage-mappings product — \
             the queries archive would fold empty (fail-closed)",
        ));
    }
    // schemas: the SHACL-derived JSON Schema + OpenAPI, member = bare
    // filename, taken from the in-memory stage product so the bundle never lags the
    // committed files by a regenerate. Bare-filename member names
    // (`gmeow.schema.json` / `gmeow.openapi.json`), so the fold is stable.
    let schemas = vec![
        (
            "gmeow.schema.json".to_string(),
            schema_surfaces.schema.to_vec(),
        ),
        (
            "gmeow.openapi.json".to_string(),
            schema_surfaces.openapi.to_vec(),
        ),
        (
            "card.schema.json".to_string(),
            schema_surfaces.card.to_vec(),
        ),
        (
            "validate-finding.schema.json".to_string(),
            schema_surfaces.finding.to_vec(),
        ),
    ];
    // cells: equivalences + projections + slice mappings, member = repo-relative path.
    let mut cells: Vec<(String, Vec<u8>)> = Vec::new();
    cells.extend(members_relpath(
        root,
        &list_files(&root.join("dsl/mappings/equivalences"), "ttl")?,
    )?);
    cells.extend(members_relpath(
        root,
        &list_files(&root.join("dsl/mappings/projections"), "ttl")?,
    )?);
    cells.extend(members_relpath(root, &slice_files(root, "mappings")?)?);
    cells.sort_by(|a, b| a.0.cmp(&b.0));
    // tests: slices/*/*/tests/*.ttl (non-recursive), member = repo-relative path.
    let mut tests = members_relpath(root, &slice_files(root, "tests")?)?;
    tests.sort_by(|a, b| a.0.cmp(&b.0));
    // shapes: the FULL SHACL surface, member = repo-relative path —
    // shapes/*.ttl (authored source) + the four generated/shapes/*.ttl members
    // (product-sourced below, P11 fail-closed) + slices/<g>/<n>/shapes.ttl. Carried
    // whole so a repo-free `gmeow validate` can reassemble both the data-graph union
    // and the DSL phases. The `generated/shapes/*.ttl` members are NEVER read off disk:
    // every one is a produced projection whose committed file the fanout rewrites from
    // the bundle, so a disk read would freeze the last-committed bytes forever (the
    // stale-disk-fold class). They are folded from THIS run's consumed products instead.
    let mut shapes: Vec<(String, Vec<u8>)> =
        members_relpath(root, &list_files(&root.join("shapes"), "ttl")?)?;
    shapes.extend(members_relpath(
        root,
        &slice_named_files(root, "shapes.ttl")?,
    )?);
    // The four generated/shapes/*.ttl members, each product-sourced (no disk enumeration):
    //   validation-shapes.ttl ← stage-compile-logic (OPT axis + OWL-restriction derivation)
    //   result-shapes.ttl     ← stage-export-result-shapes (ResultShape SHACL projection)
    //   frame-shapes.ttl      ← stage-export-frame-shapes (P11 frame relativity)
    //   constraint-shapes.ttl ← stage-export-constraint-shapes (logic: FOL-axiom projection)
    // Each MUST exist in its product (no-optionality, fail-closed): validation-shapes is
    // pulled from `axiom_artifacts` with a hard error on absence; result/frame/constraint
    // arrive as `shape_surfaces` fields already hard-failed at the call site in
    // [`ArchiveBlobsStage::run`]. constraint-shapes does not exist on disk on a first
    // run at all, so only the fresh product can carry it (H8) — the very reason a disk
    // enumeration was wrong. This replaces the P11 "fail-closed if none" disk guard.
    let validation_shapes = axiom_artifacts
        .get(crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH)
        .ok_or_else(|| {
            stage_err(
                "archive-blobs: stage-compile-logic produced no validation-shapes.ttl product; \
                 refusing to carry a stale on-disk read (P11 enforcement, fail-closed)",
            )
        })?;
    // The procedural-constraint SHACL surface (every logic:Constraint → sh:SPARQLConstraint) is
    // ALSO produced by stage-compile-logic, so it folds from the same product — header-only until
    // constraints are authored, and (like constraint-shapes.ttl) it does not exist on disk on a
    // first run at all, so only the fresh product can carry it (fail-closed, no stale disk read).
    let procedural_constraints = axiom_artifacts
        .get(crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH)
        .ok_or_else(|| {
            stage_err(
                "archive-blobs: stage-compile-logic produced no procedural-constraints.ttl \
                 product; refusing to carry a stale on-disk read (P11 enforcement, fail-closed)",
            )
        })?;
    for (rel, fresh_bytes) in [
        (
            crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH,
            validation_shapes.as_slice(),
        ),
        (
            crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
            procedural_constraints.as_slice(),
        ),
        (
            crate::stages::result_shapes::RESULT_SHAPES_PATH,
            shape_surfaces.result,
        ),
        (
            crate::stages::frame_shapes::FRAME_SHAPES_PATH,
            shape_surfaces.frame,
        ),
        (
            crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH,
            shape_surfaces.constraint,
        ),
    ] {
        if let Some(entry) = shapes.iter_mut().find(|(k, _)| k == rel) {
            entry.1 = fresh_bytes.to_vec();
        } else {
            shapes.push((rel.to_string(), fresh_bytes.to_vec()));
        }
    }
    shapes.sort_by(|a, b| a.0.cmp(&b.0));
    // axioms: the compiled logic/DL projection surface, member = repo-relative path.
    // Sourced from THIS run's `stage-compile-logic` product (not re-read from disk) so
    // a single regenerate folds the fresh projections — the committed files are not
    // flushed until the post-run reconcile returns. Each MUST exist (no-optionality,
    // fail-closed): a partial archive would silently break the consumer.
    let mut axioms: Vec<(String, Vec<u8>)> = Vec::with_capacity(AXIOM_FILES.len());
    for rel in AXIOM_FILES {
        let bytes = axiom_artifacts.get(rel).ok_or_else(|| {
            stage_err(&format!(
                "missing axiom artifact {rel} in the stage-compile-logic product (fail-closed)"
            ))
        })?;
        axioms.push((rel.to_string(), bytes.clone()));
    }
    axioms.sort_by(|a, b| a.0.cmp(&b.0));
    // models-python: the generated Pydantic package, member = package-relative path
    // (`gmeow_models/...`, the on-disk `packages/python/` prefix stripped). Sourced
    // from THIS run's stage-export-pydantic product so the bundle blob, the on-disk
    // wheel source tree, and `gmeow export-docs --format pydantic` are the SAME bytes
    // (four-way identity). Fail closed: an empty package would silently ship a bundle
    // without the documentation surface.
    let mut models_python: Vec<(String, Vec<u8>)> = models_python_artifacts
        .iter()
        .filter_map(|(path, bytes)| {
            path.strip_prefix(crate::stages::pydantic::PACKAGE_DISK_PREFIX)
                .map(|member| (member.to_string(), bytes.clone()))
        })
        .collect();
    models_python.sort_by(|a, b| a.0.cmp(&b.0));
    if models_python.is_empty() {
        return Err(stage_err(
            "no packages/python/gmeow_models/* artifacts in the stage-export-pydantic product — \
             the models-python archive would fold empty (fail-closed)",
        ));
    }
    // lang-projections: the `generated/projections/lang/**` external-format deliverables,
    // member = repo-relative path, sourced from THIS run's stage-mappings product for the
    // same stale-disk reason the mappings/queries archives are. See [`REP_LANG_PROJECTIONS`]
    // for why they are their OWN rep rather than members of the generated-opaque archive.
    let lang_projections = lang_projection_members(mappings_artifacts);
    // Fail closed, mirroring every other product-sourced archive above: an empty match
    // means stage-mappings keyed its projections under an unexpected prefix (or emitted
    // none), which would fold an EMPTY archive AND leave the lang-projections rep empty
    // — a silent capability degradation, not a fallback.
    if lang_projections.is_empty() {
        return Err(stage_err(
            "no generated/projections/lang/** artifacts in the stage-mappings product — the \
             lang-projections archive would fold empty (fail-closed)",
        ));
    }
    // yaml-ld: the claim corpus's JSON-LD-star + YAML-LD-star surface, member = bare
    // filename, rendered ONCE by stage-statements off the frozen statement dataset and
    // only TARRED here. See [`REP_YAMLLD`] for why the members are the claim corpus
    // rather than the whole carrier.
    let claim_serializations = vec![
        (
            YAMLLD_JSONLD_MEMBER.to_string(),
            claim_serializations.jsonld.to_vec(),
        ),
        (
            YAMLLD_YAMLLD_MEMBER.to_string(),
            claim_serializations.yamlld.to_vec(),
        ),
    ];
    // Fail closed, mirroring every other product-sourced archive above: an EMPTY
    // serialization means stage-statements rendered nothing, which would fold an archive
    // whose frame exists but carries no claims — leaving gmeow:dictGmeowClaimsV1 priming
    // an empty payload, which is the dead-weight state this rep was promoted to end. A
    // missing surface is a hard error, never a degraded fallback.
    if claim_serializations
        .iter()
        .any(|(_, bytes)| bytes.is_empty())
    {
        return Err(stage_err(
            "the stage-statements product carries an EMPTY JSON-LD-star / YAML-LD-star \
             projection of the statement layer — the yaml-ld archive would fold empty \
             (fail-closed)",
        ));
    }
    Ok(vec![
        archive_blob(REP_MAPPINGS, &mappings)?,
        archive_blob(REP_CELLS, &cells)?,
        archive_blob(REP_QUERIES, &queries)?,
        archive_blob(REP_TESTS, &tests)?,
        archive_blob(REP_SCHEMAS, &schemas)?,
        archive_blob(REP_SHAPES, &shapes)?,
        archive_blob(REP_AXIOMS, &axioms)?,
        archive_blob(REP_MODELS_PYTHON, &models_python)?,
        archive_blob(REP_LANG_PROJECTIONS, &lang_projections)?,
        archive_blob(REP_YAMLLD, &claim_serializations)?,
    ])
}

/// The [`REP_LANG_PROJECTIONS`] members of a `stage-mappings` product: every artifact
/// under [`LANG_PROJECTION_DIR`](crate::stages::lang_projection::LANG_PROJECTION_DIR),
/// keyed by its repo-relative committed path (so
/// [`committed_path_for_archive_member`](crate::stages::carrier::committed_path_for_archive_member)
/// is the identity for this rep) and sorted.
///
/// Split out as its own function because the carrier's `opaque_already_carried` guard
/// has to refuse the SAME prefix this selects — one authority for "which paths ride the
/// lang-projections archive", checked against that guard by
/// `carrier::lang_projection_rep_tests::the_lang_projection_prefix_is_the_same_set_the_archive_selects`.
pub(crate) fn lang_projection_members(
    artifacts: &BTreeMap<String, Vec<u8>>,
) -> Vec<(String, Vec<u8>)> {
    let prefix = format!("{}/", crate::stages::lang_projection::LANG_PROJECTION_DIR);
    let mut out: Vec<(String, Vec<u8>)> = artifacts
        .iter()
        .filter(|(path, _)| path.starts_with(&prefix))
        .map(|(path, bytes)| (path.clone(), bytes.clone()))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// `(filename, bytes)` archive members sourced from a STAGE PRODUCT's in-memory
/// artifacts (not disk): every artifact whose path is under `dir` and ends with
/// `suffix`, keyed by bare filename, sorted. Used for the mappings/queries archives so
/// the bundle carries THIS run's freshly-compiled surfaces rather than the stale
/// committed files (which are not flushed to disk until the post-run reconcile returns).
fn members_basename_from_artifacts(
    artifacts: &BTreeMap<String, Vec<u8>>,
    dir: &str,
    suffix: &str,
) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = artifacts
        .iter()
        .filter(|(path, _)| path.starts_with(dir) && path.ends_with(suffix))
        .map(|(path, bytes)| {
            let name = path.rsplit('/').next().unwrap_or(path).to_string();
            (name, bytes.clone())
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The archive rows the stage folded, read back off its product's by-reference blob
/// lane in the canonical [`ARCHIVE_REPS`] order — byte-identical to what
/// [`build_archive_blobs`] returned (the content store is content-addressed, so the
/// bytes survive the per-stage cache round-trip verbatim).
///
/// A missing product or a missing/incomplete blob record is a HARD FAIL: it means the
/// consumer forgot the `stage-archive-blobs` consumes edge, or the product was
/// truncated — never permission to ship a bundle without an archive surface.
pub(crate) fn archive_blobs_from_product(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<Vec<BlobRow>, gmeow_errors::Diag> {
    let bundle = upstream
        .get(STAGE_ID)
        .ok_or_else(|| {
            stage_err(&format!(
                "missing {STAGE_ID} product for the by-reference archive blobs"
            ))
        })?
        .bundle();
    ARCHIVE_REPS
        .iter()
        .map(|rep| archive_row(bundle, rep))
        .collect()
}

/// One archive row reconstructed from the product's blob lane: the record's declared
/// media type + representation and the content-store bytes its digest resolves to.
fn archive_row(
    bundle: &PipelineBundle<PipelineHandle>,
    rep: &str,
) -> Result<BlobRow, gmeow_errors::Diag> {
    let record = bundle
        .lookaside()
        .blobs
        .iter()
        .find(|r| r.representation.as_deref() == Some(rep))
        .ok_or_else(|| {
            stage_err(&format!(
                "the {STAGE_ID} product carries no `{rep}` blob record (fail-closed)"
            ))
        })?;
    let digest = ContentDigest::from_hex(&record.digest).ok_or_else(|| {
        stage_err(&format!(
            "the {STAGE_ID} product's `{rep}` blob record carries a malformed content digest {:?}",
            record.digest
        ))
    })?;
    let data = bundle
        .blobs()
        .get(&digest)
        .ok_or_else(|| {
            stage_err(&format!(
                "the {STAGE_ID} product's `{rep}` blob digest resolves to no content-store entry"
            ))
        })?
        .clone();
    let media_type = record.media_type.clone().ok_or_else(|| {
        stage_err(&format!(
            "the {STAGE_ID} product's `{rep}` blob record declares no media type"
        ))
    })?;
    Ok(BlobRow {
        data,
        media_type,
        rep: rep.to_string(),
    })
}

/// Every AUTHORED source file the fold reads straight off the repo tree — the cells /
/// tests / authored-shapes surfaces that no producing stage owns. Declared as the
/// stage's `input_files` so a change to any of them busts this stage's cache key
/// (cache soundness: the stage consumes no `stage-source-load`/`stage-snapshot`
/// product, so nothing else covers these reads).
fn authored_archive_sources(root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
    let mut files = list_files(&root.join("dsl/mappings/equivalences"), "ttl")?;
    files.extend(list_files(&root.join("dsl/mappings/projections"), "ttl")?);
    files.extend(slice_files(root, "mappings")?);
    files.extend(slice_files(root, "tests")?);
    files.extend(list_files(&root.join("shapes"), "ttl")?);
    files.extend(slice_named_files(root, "shapes.ttl")?);
    files.sort();
    files.dedup();
    Ok(files)
}

fn stage_err(message: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: STAGE_ID.to_string(),
        message: message.to_string(),
    })
}

// ── Stage impl ───────────────────────────────────────────────────────────────────

/// The `archive-blobs` pipeline stage: folds the ten by-reference TAR archives and
/// attaches each to its product's blob lane under its `representation` label.
pub struct ArchiveBlobsStage {
    consumes: Vec<String>,
}

impl ArchiveBlobsStage {
    /// Construct the stage. It consumes exactly the producers whose in-memory products
    /// supply archive members: `stage-compile-logic` (the axiom surface + the
    /// validation/procedural shape surfaces), `stage-mappings` (SSSOM + generated
    /// queries), `stage-export-json-schema` (the four JSON Schema documents),
    /// `stage-export-pydantic` (the model package), the three generated-shape export
    /// leaves, and `stage-statements` (the claim corpus's JSON-LD-star / YAML-LD-star
    /// surface). The edge set is declared identically here, in
    /// [`crate::run::full_spec`], and in `slices/core/pipeline/module.ttl`.
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-compile-logic".to_string(),
                "stage-export-constraint-shapes".to_string(),
                "stage-export-frame-shapes".to_string(),
                "stage-export-json-schema".to_string(),
                "stage-export-pydantic".to_string(),
                "stage-export-result-shapes".to_string(),
                "stage-mappings".to_string(),
                "stage-statements".to_string(),
            ],
        }
    }
}

impl Default for ArchiveBlobsStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for ArchiveBlobsStage {
    fn id(&self) -> &str {
        STAGE_ID
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn attaches_blob_reps(&self) -> &[String] {
        crate::stages::attach::blob_reps(self.id())
    }
    fn impl_version(&self) -> &str {
        // v1: the by-reference archive fold, lifted verbatim out of the terminal sink so
        // the archives exist as a product mid-DAG (equivalence proven byte-for-byte by
        // `stage_product_is_byte_identical_to_the_pre_extraction_sink_fold`).
        // v2: `lang-projections-archive` joins the fold — the generated/projections/lang/**
        // deliverables move OFF the terminal's generated-opaque archive onto their own rep,
        // because a rep is the unit a dictionary primes and they are a distinct
        // external-format family.
        // v3: `yaml-ld-archive` joins the fold — the claim corpus's JSON-LD-star /
        // YAML-LD-star surface becomes a PRODUCTION frame (its writer was `#[cfg(test)]`),
        // so a JSON-LD-family consumer reads the statement layer out of the bundle.
        "archive-blobs.v3"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        authored_archive_sources(root)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let blobs = fold_archive_blobs(input.root, input.upstream)?;
        let mut bundle = bundle_from_artifacts(BTreeMap::new(), DatasetProvenance::new());
        for row in blobs {
            bundle = attach_rep_blob(bundle, &row.rep, &row.media_type, row.data)?;
        }
        Ok(StageOutput::new(StageProduct::from_bundle(
            self.id(),
            Arc::new(bundle),
        )))
    }
}

/// Gather the archive inputs off the consumed products and fold them. Split from
/// [`Stage::run`] so the equivalence gate can drive the SAME gathering the stage does
/// over a fixture upstream map.
///
/// Every read HARD-fails on absence (no-optionality): a missing producer artifact is a
/// broken DAG edge or a truncated product, never permission to fold a partial archive.
pub(crate) fn fold_archive_blobs(
    root: &Path,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<Vec<BlobRow>, gmeow_errors::Diag> {
    // THIS run's freshly-emitted JSON Schema + OpenAPI bytes (from the in-memory
    // product, not the on-disk files which are not written until the reconcile returns).
    let schema_json = upstream
        .get("stage-export-json-schema")
        .and_then(|p| p.artifact(crate::stages::json_schema::JSON_SCHEMA_PATH))
        .ok_or_else(|| stage_err("missing stage-export-json-schema gmeow.schema.json artifact"))?
        .to_vec();
    let openapi_json = upstream
        .get("stage-export-json-schema")
        .and_then(|p| p.artifact(crate::stages::json_schema::OPENAPI_PATH))
        .ok_or_else(|| stage_err("missing stage-export-json-schema gmeow.openapi.json artifact"))?
        .to_vec();
    // THIS run's two hand-authored self-describing schemas (the term `Card` shape +
    // the `validate_local` envelope shape), from the SAME product so they never lag a
    // regenerate — folded into REP_SCHEMAS alongside the SHACL-derived pair.
    let card_schema_json = upstream
        .get("stage-export-json-schema")
        .and_then(|p| p.artifact(crate::stages::json_schema::CARD_SCHEMA_PATH))
        .ok_or_else(|| stage_err("missing stage-export-json-schema card.schema.json artifact"))?
        .to_vec();
    let finding_schema_json = upstream
        .get("stage-export-json-schema")
        .and_then(|p| p.artifact(crate::stages::json_schema::FINDING_SCHEMA_PATH))
        .ok_or_else(|| {
            stage_err("missing stage-export-json-schema validate-finding.schema.json artifact")
        })?
        .to_vec();
    // THIS run's compiled axiom surface (REP_AXIOMS), from the stage-compile-logic
    // product so it never lags a regenerate.
    let compile_artifacts = upstream
        .get("stage-compile-logic")
        .ok_or_else(|| stage_err("missing stage-compile-logic product"))?
        .artifacts();
    // THIS run's compiled SSSOM surface (REP_MAPPINGS), from the stage-mappings product
    // so the archive never lags a mapping-source edit: the committed generated/mappings/
    // files are not flushed until the reconcile returns, so reading them from disk here
    // would tar the STALE committed set and a mapping edit could never reach the bundle
    // without a manual disk write. Sourced from the product exactly as schemas / axioms are.
    let mappings_artifacts = upstream
        .get("stage-mappings")
        .ok_or_else(|| stage_err("missing stage-mappings product"))?
        .artifacts();
    // THIS run's generated shape surfaces (REP_SHAPES members), from the producing
    // export leaves' products so the archive never lags a competency/frame edit:
    // the committed generated/shapes/*.ttl are projected back from the bundle by the
    // fanout, so a stale disk read here would freeze them forever (the exact trap the
    // validation-shapes.ttl override documents). Hard-fail if absent (no-optionality).
    let result_shapes_ttl = upstream
        .get("stage-export-result-shapes")
        .and_then(|p| p.artifact(crate::stages::result_shapes::RESULT_SHAPES_PATH))
        .ok_or_else(|| stage_err("missing stage-export-result-shapes result-shapes.ttl artifact"))?
        .to_vec();
    let frame_shapes_ttl = upstream
        .get("stage-export-frame-shapes")
        .and_then(|p| p.artifact(crate::stages::frame_shapes::FRAME_SHAPES_PATH))
        .ok_or_else(|| stage_err("missing stage-export-frame-shapes frame-shapes.ttl artifact"))?
        .to_vec();
    // THIS run's constraint-shapes surface (the SHACL projection of the logic: FOL axioms),
    // folded into REP_SHAPES from the fresh product for the SAME reason as result/frame
    // shapes: the committed generated/shapes/constraint-shapes.ttl is projected back from the
    // bundle by the fanout, and on a first run it does not exist on disk at all, so only the
    // fresh product can carry it (H8).
    let constraint_shapes_ttl = upstream
        .get("stage-export-constraint-shapes")
        .and_then(|p| p.artifact(crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH))
        .ok_or_else(|| {
            stage_err("missing stage-export-constraint-shapes constraint-shapes.ttl artifact")
        })?
        .to_vec();
    // THIS run's freshly-rendered Pydantic model package (REP_MODELS_PYTHON),
    // sourced from the stage-export-pydantic product so the bundle blob never lags a
    // regenerate: the committed packages/python/gmeow_models/* are not flushed until
    // the reconcile returns, so a disk read here would tar the stale committed tree.
    let models_python_artifacts = upstream
        .get("stage-export-pydantic")
        .ok_or_else(|| stage_err("missing stage-export-pydantic product"))?
        .artifacts();
    // THIS run's claim-corpus JSON-LD-family surface (REP_YAMLLD), sourced from the
    // stage-statements product for the same reason every surface above is: the render
    // ran ONCE in the producing stage, and these two artifacts ride the INTERNAL
    // `pipeline/statements/` lane, so they exist nowhere on disk to be stale-read.
    let claim_jsonld = upstream
        .get("stage-statements")
        .and_then(|p| p.artifact(crate::stages::statements::RDF12_JSONLD_PATH))
        .ok_or_else(|| {
            stage_err(
                "missing stage-statements JSON-LD-star projection of the statement layer \
                 (pipeline/statements/gmeow.rdf12.jsonld)",
            )
        })?
        .to_vec();
    let claim_yamlld = upstream
        .get("stage-statements")
        .and_then(|p| p.artifact(crate::stages::statements::RDF12_YAMLLD_PATH))
        .ok_or_else(|| {
            stage_err(
                "missing stage-statements YAML-LD-star projection of the statement layer \
                 (pipeline/statements/gmeow.rdf12.yamlld)",
            )
        })?
        .to_vec();
    build_archive_blobs(
        root,
        &SchemaSurfaces {
            schema: &schema_json,
            openapi: &openapi_json,
            card: &card_schema_json,
            finding: &finding_schema_json,
        },
        &compile_artifacts,
        &mappings_artifacts,
        &ShapeSurfaces {
            result: &result_shapes_ttl,
            frame: &frame_shapes_ttl,
            constraint: &constraint_shapes_ttl,
        },
        &models_python_artifacts,
        &ClaimSerializations {
            jsonld: &claim_jsonld,
            yamlld: &claim_yamlld,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::stages::carrier::ARCHIVE_MEDIA_TYPE;

    /// Decode `(name, bytes)` members from a USTAR archive via the shared codec.
    fn parse(raw: &[u8]) -> Vec<(String, Vec<u8>)> {
        purrdf::ustar::read_archive(raw).unwrap()
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    /// Empty schema surfaces for the blob-archive unit tests, which assert the
    /// REP_AXIOMS / mappings / queries / shapes channels and do not read the schema
    /// bytes (production sources them from the `stage-export-json-schema` product).
    fn empty_schemas() -> SchemaSurfaces<'static> {
        SchemaSurfaces {
            schema: b"",
            openapi: b"",
            card: b"",
            finding: b"",
        }
    }

    /// Minimal non-empty claim serializations for the blob-archive unit tests, so
    /// `build_archive_blobs` clears its [`REP_YAMLLD`] fail-closed guard. Production
    /// sources both from the `stage-statements` product's internal
    /// `pipeline/statements/` lane; the bytes are irrelevant to the channels these
    /// tests assert.
    fn sample_claim_serializations() -> ClaimSerializations<'static> {
        ClaimSerializations {
            jsonld: br#"{"@context":{},"@graph":[]}"#,
            yamlld: b"'@context': {}\n",
        }
    }

    /// The [`sample_claim_serializations`] bytes as a `stage-statements` product's
    /// internal artifact lane, for the fixtures that drive the fold through `upstream`.
    fn sample_claim_serialization_artifacts() -> BTreeMap<String, Vec<u8>> {
        let sample = sample_claim_serializations();
        BTreeMap::from([
            (
                crate::stages::statements::RDF12_JSONLD_PATH.to_string(),
                sample.jsonld.to_vec(),
            ),
            (
                crate::stages::statements::RDF12_YAMLLD_PATH.to_string(),
                sample.yamlld.to_vec(),
            ),
        ])
    }

    /// A minimal non-empty stage-export-pydantic product for the blob-archive unit
    /// tests: one package member under the on-disk prefix, so `build_archive_blobs`
    /// clears its models-python fail-closed guard.
    fn sample_models_python() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([(
            format!(
                "{}gmeow_models/__init__.py",
                crate::stages::pydantic::PACKAGE_DISK_PREFIX
            ),
            b"# gmeow_models\n".to_vec(),
        )])
    }

    /// Mirror the committed `generated/mappings/*.sssom.tsv`, `generated/queries/*.rq` AND
    /// `generated/projections/lang/**` into an artifact map keyed by repo-relative path —
    /// the stand-in for the stage-mappings product in blob-archive unit tests (production
    /// sources the SSSOM surface, the SPARQL query surface, and the `lang:` projection
    /// deliverables all from that one in-memory product).
    fn mappings_artifacts_from_disk(root: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        for p in list_files(&root.join("generated/mappings"), "sssom.tsv").unwrap_or_default() {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            out.insert(
                format!("generated/mappings/{name}"),
                std::fs::read(&p).unwrap_or_else(|_| panic!("read {}", p.display())),
            );
        }
        for p in list_files(&root.join("generated/queries"), "rq").unwrap_or_default() {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            out.insert(
                format!("generated/queries/{name}"),
                std::fs::read(&p).unwrap_or_else(|_| panic!("read {}", p.display())),
            );
        }
        // The lang-projection tree is NESTED (ebnf/, gmn1/v1/, conllu/, tei/, …) and
        // heterogeneous in extension, so it is walked rather than extension-filtered —
        // the same shape the production stage-mappings product keys it under.
        walk_into(
            &root.join(crate::stages::lang_projection::LANG_PROJECTION_DIR),
            root,
            &mut out,
        );
        assert!(
            out.keys()
                .any(|p| p.starts_with("generated/projections/lang/")),
            "the stage-mappings stand-in must carry the lang projections, or the \
             lang-projections archive's fail-closed guard is what these tests measure"
        );
        out
    }

    /// Every regular file under `dir`, inserted into `out` keyed by its path relative to
    /// `root`. Recursive; skips symlinks (a symlinked directory could cycle).
    fn walk_into(dir: &Path, root: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_symlink() {
                continue;
            }
            if path.is_dir() {
                walk_into(&path, root, out);
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.insert(
                    rel.to_string_lossy().into_owned(),
                    std::fs::read(&path).unwrap_or_else(|_| panic!("read {}", path.display())),
                );
            }
        }
    }

    /// The committed ResultShape SHACL projection — the stand-in for the
    /// stage-export-result-shapes product in blob-archive unit tests (production
    /// sources these from the in-memory product).
    fn fresh_result_shapes_from_disk(root: &Path) -> Vec<u8> {
        let rel = crate::stages::result_shapes::RESULT_SHAPES_PATH;
        std::fs::read(root.join(rel)).unwrap_or_else(|_| panic!("read {rel}"))
    }

    /// The committed P11 frame shapes — the stand-in for the
    /// stage-export-frame-shapes product in blob-archive unit tests.
    fn fresh_frame_shapes_from_disk(root: &Path) -> Vec<u8> {
        let rel = crate::stages::frame_shapes::FRAME_SHAPES_PATH;
        std::fs::read(root.join(rel)).unwrap_or_else(|_| panic!("read {rel}"))
    }

    /// The committed logic: FOL-axiom SHACL projection — the stand-in for the
    /// stage-export-constraint-shapes product in blob-archive unit tests.
    fn fresh_constraint_shapes_from_disk(root: &Path) -> Vec<u8> {
        let rel = crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH;
        std::fs::read(root.join(rel)).unwrap_or_else(|_| panic!("read {rel}"))
    }

    /// The four axiom projections + validation-shapes, mirrored off the committed tree — the
    /// stand-in for the `stage-compile-logic` product in blob-archive unit tests.
    fn axiom_artifacts_from_disk(root: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut axiom_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for rel in AXIOM_FILES {
            axiom_artifacts.insert(
                rel.to_string(),
                std::fs::read(root.join(rel)).unwrap_or_else(|_| panic!("read {rel}")),
            );
        }
        let vs_rel = crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH;
        axiom_artifacts.insert(
            vs_rel.to_string(),
            std::fs::read(root.join(vs_rel)).unwrap_or_else(|_| panic!("read {vs_rel}")),
        );
        // The procedural-constraints.ttl product is required (fail-closed) for the same reason:
        // mirror the committed header-only file, as the production stage-compile-logic emits it.
        let pc_rel = crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH;
        axiom_artifacts.insert(
            pc_rel.to_string(),
            std::fs::read(root.join(pc_rel)).unwrap_or_else(|_| panic!("read {pc_rel}")),
        );
        axiom_artifacts
    }

    // DOCUMENTED SWEEP — the four single-file generated edit kinds each reach the bundle
    // product-sourced in ONE fold, so a single update is a fixed point for
    // strict sync regardless of which one was edited:
    //   - generated query    → stage-mappings product          → REP_QUERIES  (members_basename_from_artifacts)
    //   - generated SSSOM map → stage-mappings product          → REP_MAPPINGS (members_basename_from_artifacts)
    //   - frame-shape source  → stage-export-frame-shapes prod  → REP_SHAPES   (ShapeSurfaces.frame)
    //   - competency test     → result-shapes projection        → stage-export-result-shapes prod → REP_SHAPES (ShapeSurfaces.result)
    // Shared invariant proven by the probes below: every archived `generated/` member is
    // sourced from an in-memory stage PRODUCT, never a disk read. Were any fold still a
    // `list_files(generated/…)` disk read (the stale-disk-fold bug), a product-only probe
    // could never reach the bundle and update/check would disagree forever.
    /// FIXED-POINT PROOF: a change to the `stage-mappings`
    /// product's generated SPARQL surface reaches the bundle in ONE fold. REP_QUERIES is
    /// product-sourced (`members_basename_from_artifacts`), not a disk read, so a query that
    /// exists ONLY in the in-memory product — never on disk — MUST appear in the archive. Were
    /// the fold still a `list_files(generated/queries)` disk read (the stale-disk-fold bug),
    /// the product-only probe could never reach the bundle and update/check
    /// would disagree forever. This encodes the "edit a generated query → one-pass fixed point"
    /// property directly at the fold, complementing the structural repo-static guard.
    #[test]
    fn a_query_present_only_in_the_mappings_product_reaches_the_bundle_in_one_fold() {
        let root = repo_root();
        let axiom_artifacts = axiom_artifacts_from_disk(&root);
        let shapes = ShapeSurfaces {
            result: &fresh_result_shapes_from_disk(&root),
            frame: &fresh_frame_shapes_from_disk(&root),
            constraint: &fresh_constraint_shapes_from_disk(&root),
        };

        // A probe query that exists ONLY in the product — it is NOT committed under
        // generated/queries/, so a disk read could never surface it.
        const PROBE_NAME: &str = "zzz-fixed-point-probe.rq";
        let probe_rel = format!("generated/queries/{PROBE_NAME}");
        assert!(
            !root.join(&probe_rel).exists(),
            "the probe must not exist on disk, or the test proves nothing"
        );
        let probe_bytes = b"# fixed-point probe: product-only generated query\n".to_vec();

        let mut mappings = mappings_artifacts_from_disk(&root);
        mappings.insert(probe_rel.clone(), probe_bytes.clone());

        let blobs = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts,
            &mappings,
            &shapes,
            &sample_models_python(),
            &sample_claim_serializations(),
        )
        .expect("archive blobs");
        let queries = blobs
            .iter()
            .find(|b| b.rep == REP_QUERIES)
            .expect("REP_QUERIES blob present");
        let members = parse(&queries.data);
        let probe = members
            .iter()
            .find(|(n, _)| n == PROBE_NAME)
            .expect("product-only probe query MUST reach REP_QUERIES (fold is product-sourced)");
        assert_eq!(
            probe.1, probe_bytes,
            "the folded probe bytes must be the product bytes, not a disk read"
        );

        // Fail-closed: an empty query surface in the product is a hard error, never a silent
        // fallback to a stale disk read.
        let mut no_queries = mappings_artifacts_from_disk(&root);
        no_queries.retain(|k, _| !k.starts_with("generated/queries/"));
        let err = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts,
            &no_queries,
            &shapes,
            &sample_models_python(),
            &sample_claim_serializations(),
        )
        .expect_err("empty queries product must fail closed");
        assert!(
            format!("{err:?}").contains("queries archive would fold empty"),
            "unexpected error: {err:?}"
        );
    }

    /// FIXED-POINT PROOF: a change to the `stage-mappings` product's generated SSSOM
    /// surface reaches the bundle in ONE fold. REP_MAPPINGS is product-sourced
    /// (`members_basename_from_artifacts`), an exact mirror of REP_QUERIES, so a mapping
    /// that exists ONLY in the in-memory product — never on disk — MUST appear in the
    /// archive. A stale disk read would leave the product-only probe stranded and make
    /// update/check disagree forever.
    #[test]
    fn a_mapping_present_only_in_the_stage_mappings_product_reaches_the_bundle_in_one_fold() {
        let root = repo_root();
        let axiom_artifacts = axiom_artifacts_from_disk(&root);
        let shapes = ShapeSurfaces {
            result: &fresh_result_shapes_from_disk(&root),
            frame: &fresh_frame_shapes_from_disk(&root),
            constraint: &fresh_constraint_shapes_from_disk(&root),
        };

        // A probe mapping that exists ONLY in the product — it is NOT committed under
        // generated/mappings/, so a disk read could never surface it.
        const PROBE_NAME: &str = "zzz-fixed-point-probe.sssom.tsv";
        let probe_rel = format!("generated/mappings/{PROBE_NAME}");
        assert!(
            !root.join(&probe_rel).exists(),
            "the probe must not exist on disk, or the test proves nothing"
        );
        let probe_bytes = b"# fixed-point probe: product-only SSSOM mapping\n".to_vec();

        let mut mappings = mappings_artifacts_from_disk(&root);
        mappings.insert(probe_rel.clone(), probe_bytes.clone());

        let blobs = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts,
            &mappings,
            &shapes,
            &sample_models_python(),
            &sample_claim_serializations(),
        )
        .expect("archive blobs");
        let archive = blobs
            .iter()
            .find(|b| b.rep == REP_MAPPINGS)
            .expect("REP_MAPPINGS blob present");
        let members = parse(&archive.data);
        let probe = members
            .iter()
            .find(|(n, _)| n == PROBE_NAME)
            .expect("product-only probe mapping MUST reach REP_MAPPINGS (fold is product-sourced)");
        assert_eq!(
            probe.1, probe_bytes,
            "the folded probe bytes must be the product bytes, not a disk read"
        );

        // Fail-closed: an empty mappings surface in the product is a hard error, never a
        // silent fallback to a stale disk read.
        let mut no_mappings = mappings_artifacts_from_disk(&root);
        no_mappings.retain(|k, _| !k.starts_with("generated/mappings/"));
        let err = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts,
            &no_mappings,
            &shapes,
            &sample_models_python(),
            &sample_claim_serializations(),
        )
        .expect_err("empty mappings product must fail closed");
        assert!(
            format!("{err:?}").contains("mappings archive would fold empty"),
            "unexpected error: {err:?}"
        );
    }

    /// FIXED-POINT PROOF: the frame-shape source, the competency test (which flows through
    /// the result-shapes ResultShape projection), and the constraint-shape source each reach
    /// the bundle in ONE fold. REP_SHAPES folds the `ShapeSurfaces { result, frame,
    /// constraint }` product BYTES — never a disk read — into members named by the full
    /// repo-relative projection paths, so a product-only surface that differs from the
    /// committed file MUST appear verbatim in the archive.
    #[test]
    fn product_only_shape_surfaces_reach_the_bundle_in_one_fold() {
        let root = repo_root();

        // Three distinct product-only surfaces, each differing from its committed file, so a
        // match in the archive proves the fold used the PRODUCT bytes, not a disk read.
        let result_probe = b"# fixed-point probe: product-only result-shapes surface\n".to_vec();
        let frame_probe = b"# fixed-point probe: product-only frame-shapes surface\n".to_vec();
        let constraint_probe =
            b"# fixed-point probe: product-only constraint-shapes surface\n".to_vec();
        assert_ne!(
            result_probe,
            fresh_result_shapes_from_disk(&root),
            "the result probe must differ from disk, or the test proves nothing"
        );
        assert_ne!(
            frame_probe,
            fresh_frame_shapes_from_disk(&root),
            "the frame probe must differ from disk, or the test proves nothing"
        );
        assert_ne!(
            constraint_probe,
            fresh_constraint_shapes_from_disk(&root),
            "the constraint probe must differ from disk, or the test proves nothing"
        );

        let blobs = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts_from_disk(&root),
            &mappings_artifacts_from_disk(&root),
            &ShapeSurfaces {
                result: &result_probe,
                frame: &frame_probe,
                constraint: &constraint_probe,
            },
            &sample_models_python(),
            &sample_claim_serializations(),
        )
        .expect("archive blobs");
        let archive = blobs
            .iter()
            .find(|b| b.rep == REP_SHAPES)
            .expect("REP_SHAPES blob present");
        let members = parse(&archive.data);

        for (path, probe) in [
            (
                crate::stages::result_shapes::RESULT_SHAPES_PATH,
                &result_probe,
            ),
            (crate::stages::frame_shapes::FRAME_SHAPES_PATH, &frame_probe),
            (
                crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH,
                &constraint_probe,
            ),
        ] {
            let member = members.iter().find(|(n, _)| n == path).unwrap_or_else(|| {
                panic!(
                    "a product-only shape surface MUST reach REP_SHAPES from the product, \
                     not a disk read: {path}"
                )
            });
            assert_eq!(
                &member.1, probe,
                "the folded {path} bytes must be the product bytes, not a disk read"
            );
        }
    }

    #[test]
    fn build_archive_blobs_folds_the_shapes_surface() {
        let root = repo_root();
        // schema/openapi bytes are irrelevant to the shapes blob; pass empty. The axiom
        // surface is irrelevant here too, but it must be present (fail-closed), so mirror
        // the committed projections into the artifact map.
        let axiom_artifacts = axiom_artifacts_from_disk(&root);
        let blobs = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts,
            &mappings_artifacts_from_disk(&root),
            &ShapeSurfaces {
                result: &fresh_result_shapes_from_disk(&root),
                frame: &fresh_frame_shapes_from_disk(&root),
                constraint: &fresh_constraint_shapes_from_disk(&root),
            },
            &sample_models_python(),
            &sample_claim_serializations(),
        )
        .expect("archive blobs");
        let blob = blobs
            .iter()
            .find(|b| b.rep == REP_SHAPES)
            .expect("REP_SHAPES blob present");
        assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);
        let members = parse(&blob.data);
        assert!(!members.is_empty(), "the shape surface must carry members");
        let names: Vec<&str> = members.iter().map(|(n, _)| n.as_str()).collect();

        // Base hand-authored shape + the generated frame shape (P11) + ≥1 per-slice.
        assert!(names.contains(&"shapes/gmeow-shapes.ttl"));
        assert!(names.contains(&"generated/shapes/frame-shapes.ttl"));
        assert!(
            names
                .iter()
                .any(|n| n.starts_with("slices/") && n.ends_with("/shapes.ttl")),
            "at least one per-slice shapes.ttl must be folded"
        );
        // The FULL surface carries the 4 DSL/manifest lints (the validator filters
        // them OUT of its data-graph union, but the consumer's DSL phases need them).
        for dsl in [
            "shapes/mapping-dsl-shapes.ttl",
            "shapes/statement-dsl-shapes.ttl",
            "shapes/test-dsl-shapes.ttl",
            "shapes/slice-manifest-shapes.ttl",
        ] {
            assert!(
                names.contains(&dsl),
                "DSL lint {dsl} must be in the FULL shape surface"
            );
        }
        // Member count == on-disk count (no silent drops).
        let on_disk = list_files(&root.join("shapes"), "ttl").unwrap().len()
            + list_files(&root.join("generated/shapes"), "ttl")
                .unwrap()
                .len()
            + slice_named_files(&root, "shapes.ttl").unwrap().len();
        assert_eq!(
            members.len(),
            on_disk,
            "every shape file must be folded exactly once"
        );
        // The slice-shape subset matches an independent on-disk enumeration — pins
        // `slice_named_files` against drift from the shacl crate's private walk.
        let folded_slices: std::collections::BTreeSet<&str> = names
            .iter()
            .copied()
            .filter(|n| n.starts_with("slices/") && n.ends_with("/shapes.ttl"))
            .collect();
        let disk_slices: std::collections::BTreeSet<String> =
            slice_named_files(&root, "shapes.ttl")
                .unwrap()
                .iter()
                .map(|p| {
                    p.strip_prefix(&root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/")
                })
                .collect();
        let disk_slices_ref: std::collections::BTreeSet<&str> =
            disk_slices.iter().map(String::as_str).collect();
        assert_eq!(folded_slices, disk_slices_ref);
        // Keys sorted (deterministic fold).
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "shape members must be sorted by key");
    }

    #[test]
    fn build_archive_blobs_folds_the_axiom_surface() {
        let root = repo_root();
        // The axiom surface is sourced from the stage-compile-logic product; mirror
        // that here by reading the committed projections into the artifact map.
        let axiom_artifacts = axiom_artifacts_from_disk(&root);
        let blobs = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts,
            &mappings_artifacts_from_disk(&root),
            &ShapeSurfaces {
                result: &fresh_result_shapes_from_disk(&root),
                frame: &fresh_frame_shapes_from_disk(&root),
                constraint: &fresh_constraint_shapes_from_disk(&root),
            },
            &sample_models_python(),
            &sample_claim_serializations(),
        )
        .expect("archive blobs");
        let blob = blobs
            .iter()
            .find(|b| b.rep == REP_AXIOMS)
            .expect("REP_AXIOMS blob present");
        assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);
        let members = parse(&blob.data);
        let names: std::collections::BTreeSet<&str> =
            members.iter().map(|(n, _)| n.as_str()).collect();
        // Exactly the compiled projections — no more, no less.
        let want: std::collections::BTreeSet<&str> = AXIOM_FILES.iter().copied().collect();
        assert_eq!(
            names, want,
            "REP_AXIOMS must carry exactly the projection files"
        );
        // The big reasoning OUTPUTS ride other channels — never in REP_AXIOMS.
        for big in [
            "generated/logic/inferred-closure.rdf12.ttl",
            "generated/logic/reasoning-explanations.rdf12.ttl",
            "generated/logic/dl-el-crosscheck-report.ttl",
        ] {
            assert!(!names.contains(big), "{big} must NOT be in REP_AXIOMS");
        }
        // Determinism: rebuild and assert byte-equality.
        let again = build_archive_blobs(
            &root,
            &empty_schemas(),
            &axiom_artifacts,
            &mappings_artifacts_from_disk(&root),
            &ShapeSurfaces {
                result: &fresh_result_shapes_from_disk(&root),
                frame: &fresh_frame_shapes_from_disk(&root),
                constraint: &fresh_constraint_shapes_from_disk(&root),
            },
            &sample_models_python(),
            &sample_claim_serializations(),
        )
        .expect("archive blobs");
        let blob2 = again.iter().find(|b| b.rep == REP_AXIOMS).unwrap();
        assert_eq!(
            blob.data, blob2.data,
            "REP_AXIOMS must be byte-deterministic"
        );
    }

    // ── The extraction equivalence gate ─────────────────────────────────────────

    /// A fixture upstream product map carrying EXACTLY the artifacts the archive fold
    /// reads, mirrored off the committed tree (production sources every one of them from
    /// the in-memory producer product; the bytes are irrelevant to the equivalence claim
    /// — only that BOTH paths see the same ones).
    fn fixture_upstream(root: &Path) -> BTreeMap<String, StageProduct> {
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-compile-logic".to_string(),
            StageProduct::from_artifacts("stage-compile-logic", axiom_artifacts_from_disk(root)),
        );
        upstream.insert(
            "stage-mappings".to_string(),
            StageProduct::from_artifacts("stage-mappings", mappings_artifacts_from_disk(root)),
        );
        let json_schema_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::from([
            (
                crate::stages::json_schema::JSON_SCHEMA_PATH.to_string(),
                b"{\"$id\":\"schema\"}".to_vec(),
            ),
            (
                crate::stages::json_schema::OPENAPI_PATH.to_string(),
                b"{\"openapi\":\"3.1.0\"}".to_vec(),
            ),
            (
                crate::stages::json_schema::CARD_SCHEMA_PATH.to_string(),
                b"{\"$id\":\"card\"}".to_vec(),
            ),
            (
                crate::stages::json_schema::FINDING_SCHEMA_PATH.to_string(),
                b"{\"$id\":\"finding\"}".to_vec(),
            ),
        ]);
        upstream.insert(
            "stage-export-json-schema".to_string(),
            StageProduct::from_artifacts("stage-export-json-schema", json_schema_artifacts),
        );
        upstream.insert(
            "stage-export-pydantic".to_string(),
            StageProduct::from_artifacts("stage-export-pydantic", sample_models_python()),
        );
        upstream.insert(
            "stage-statements".to_string(),
            StageProduct::from_artifacts(
                "stage-statements",
                sample_claim_serialization_artifacts(),
            ),
        );
        upstream.insert(
            "stage-export-result-shapes".to_string(),
            StageProduct::from_artifacts(
                "stage-export-result-shapes",
                BTreeMap::from([(
                    crate::stages::result_shapes::RESULT_SHAPES_PATH.to_string(),
                    fresh_result_shapes_from_disk(root),
                )]),
            ),
        );
        upstream.insert(
            "stage-export-frame-shapes".to_string(),
            StageProduct::from_artifacts(
                "stage-export-frame-shapes",
                BTreeMap::from([(
                    crate::stages::frame_shapes::FRAME_SHAPES_PATH.to_string(),
                    fresh_frame_shapes_from_disk(root),
                )]),
            ),
        );
        upstream.insert(
            "stage-export-constraint-shapes".to_string(),
            StageProduct::from_artifacts(
                "stage-export-constraint-shapes",
                BTreeMap::from([(
                    crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH.to_string(),
                    fresh_constraint_shapes_from_disk(root),
                )]),
            ),
        );
        upstream
    }

    /// The PRE-EXTRACTION sink fold, reproduced verbatim: the exact argument gathering
    /// `carrier::serialize_carrier_snapshot` performed inline before the archives moved
    /// into their own stage, feeding the same `build_archive_blobs`. Kept as an
    /// INDEPENDENT copy (not a call into the stage's own `fold_archive_blobs`) so the
    /// equivalence claim compares two paths, not one path against itself.
    fn legacy_sink_archive_blobs(
        root: &Path,
        upstream: &BTreeMap<String, StageProduct>,
    ) -> Vec<BlobRow> {
        let schema_json = upstream
            .get("stage-export-json-schema")
            .and_then(|p| p.artifact(crate::stages::json_schema::JSON_SCHEMA_PATH))
            .expect("gmeow.schema.json")
            .to_vec();
        let openapi_json = upstream
            .get("stage-export-json-schema")
            .and_then(|p| p.artifact(crate::stages::json_schema::OPENAPI_PATH))
            .expect("gmeow.openapi.json")
            .to_vec();
        let card_schema_json = upstream
            .get("stage-export-json-schema")
            .and_then(|p| p.artifact(crate::stages::json_schema::CARD_SCHEMA_PATH))
            .expect("card.schema.json")
            .to_vec();
        let finding_schema_json = upstream
            .get("stage-export-json-schema")
            .and_then(|p| p.artifact(crate::stages::json_schema::FINDING_SCHEMA_PATH))
            .expect("validate-finding.schema.json")
            .to_vec();
        let compile_artifacts = upstream
            .get("stage-compile-logic")
            .expect("stage-compile-logic product")
            .artifacts();
        let mappings_artifacts = upstream
            .get("stage-mappings")
            .expect("stage-mappings product")
            .artifacts();
        let result_shapes_ttl = upstream
            .get("stage-export-result-shapes")
            .and_then(|p| p.artifact(crate::stages::result_shapes::RESULT_SHAPES_PATH))
            .expect("result-shapes.ttl")
            .to_vec();
        let frame_shapes_ttl = upstream
            .get("stage-export-frame-shapes")
            .and_then(|p| p.artifact(crate::stages::frame_shapes::FRAME_SHAPES_PATH))
            .expect("frame-shapes.ttl")
            .to_vec();
        let constraint_shapes_ttl = upstream
            .get("stage-export-constraint-shapes")
            .and_then(|p| p.artifact(crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH))
            .expect("constraint-shapes.ttl")
            .to_vec();
        let models_python_artifacts = upstream
            .get("stage-export-pydantic")
            .expect("stage-export-pydantic product")
            .artifacts();
        let claim_jsonld = upstream
            .get("stage-statements")
            .and_then(|p| p.artifact(crate::stages::statements::RDF12_JSONLD_PATH))
            .expect("pipeline/statements/gmeow.rdf12.jsonld")
            .to_vec();
        let claim_yamlld = upstream
            .get("stage-statements")
            .and_then(|p| p.artifact(crate::stages::statements::RDF12_YAMLLD_PATH))
            .expect("pipeline/statements/gmeow.rdf12.yamlld")
            .to_vec();
        build_archive_blobs(
            root,
            &SchemaSurfaces {
                schema: &schema_json,
                openapi: &openapi_json,
                card: &card_schema_json,
                finding: &finding_schema_json,
            },
            &compile_artifacts,
            &mappings_artifacts,
            &ShapeSurfaces {
                result: &result_shapes_ttl,
                frame: &frame_shapes_ttl,
                constraint: &constraint_shapes_ttl,
            },
            &models_python_artifacts,
            &ClaimSerializations {
                jsonld: &claim_jsonld,
                yamlld: &claim_yamlld,
            },
        )
        .expect("the pre-extraction sink fold")
    }

    /// A comparable projection of one archive row: its rep, media type, whole payload
    /// bytes, AND the ORDERED `(member name, member bytes)` list the tar decodes to. The
    /// tar bytes alone already pin membership and order, but decoding them makes a
    /// failure name the offending member instead of printing two megabyte blobs.
    type RowShape = (String, String, Vec<u8>, Vec<(String, Vec<u8>)>);

    fn row_shape(row: &BlobRow) -> RowShape {
        (
            row.rep.clone(),
            row.media_type.clone(),
            row.data.clone(),
            parse(&row.data),
        )
    }

    fn shapes_of(rows: &[BlobRow]) -> Vec<RowShape> {
        rows.iter().map(row_shape).collect()
    }

    /// EQUIVALENCE BEFORE DELETION: the archive rows `stage-archive-blobs` publishes on
    /// its product are byte-identical — same rows, same order, same media types, same tar
    /// payloads, same ordered member lists — to what the terminal sink folded inline
    /// before the extraction.
    ///
    /// Both halves are driven from ONE fixture upstream map, so any divergence is the
    /// extraction's fault and nothing else: the legacy half reproduces the sink's former
    /// inline argument gathering, the new half runs the real `Stage::run` and then reads
    /// the rows back off the product's blob lane exactly as the sink now does.
    #[test]
    fn stage_product_is_byte_identical_to_the_pre_extraction_sink_fold() {
        let root = repo_root();
        let upstream = fixture_upstream(&root);

        let legacy = legacy_sink_archive_blobs(&root, &upstream);

        let stage = ArchiveBlobsStage::new();
        let output = stage
            .run(StageInput {
                root: &root,
                upstream: &upstream,
            })
            .expect("the archive-blobs stage runs");
        let mut with_product: BTreeMap<String, StageProduct> = upstream.clone();
        with_product.insert(STAGE_ID.to_string(), output.product);
        let extracted = archive_blobs_from_product(&with_product).expect("read the archive rows");

        // Same rows, in the same order (a dropped or reordered ARCHIVE surfaces here).
        let legacy_reps: Vec<&str> = legacy.iter().map(|r| r.rep.as_str()).collect();
        let extracted_reps: Vec<&str> = extracted.iter().map(|r| r.rep.as_str()).collect();
        assert_eq!(
            legacy_reps, extracted_reps,
            "the extracted archive rows must match the pre-extraction fold's rep sequence"
        );
        assert_eq!(
            legacy_reps.len(),
            ARCHIVE_REPS.len(),
            "the fold must publish every declared archive rep"
        );

        // Same bytes and same ordered members per row (a dropped or reordered MEMBER
        // inside any archive surfaces here).
        for (want, got) in shapes_of(&legacy).iter().zip(shapes_of(&extracted).iter()) {
            assert_eq!(
                want.0, got.0,
                "archive rep mismatch at the same ordinal position"
            );
            assert_eq!(want.1, got.1, "`{}` media type must be unchanged", want.0);
            assert_eq!(
                want.3, got.3,
                "`{}` must carry the identical ordered member list",
                want.0
            );
            assert_eq!(
                want.2, got.2,
                "`{}` must carry byte-identical archive payload",
                want.0
            );
        }

        // NON-VACUITY: the comparison actually discriminates. Drop ONE member from the
        // models-python package and ONE from the shapes surface (via a mutated fixture)
        // and the same comparison must FAIL — proving a real drop could not slip through
        // the assertions above.
        let mut thinned = upstream.clone();
        let mut extra_models = sample_models_python();
        extra_models.insert(
            format!(
                "{}gmeow_models/zzz_probe.py",
                crate::stages::pydantic::PACKAGE_DISK_PREFIX
            ),
            b"# probe member\n".to_vec(),
        );
        thinned.insert(
            "stage-export-pydantic".to_string(),
            StageProduct::from_artifacts("stage-export-pydantic", extra_models),
        );
        let perturbed = legacy_sink_archive_blobs(&root, &thinned);
        assert_ne!(
            shapes_of(&perturbed),
            shapes_of(&legacy),
            "adding one archive member MUST change the compared shape — otherwise the \
             equivalence assertions above are vacuous"
        );
    }

    /// The stage's DECLARED blob-rep attach set is exactly the reps it folds. The
    /// scheduler HARD-fails (`error::AttachDrift`) on any run-time divergence, and the
    /// loader HARD-fails if the RDF `gmeow:attachesBlobRep` declaration disagrees — this
    /// pins the third corner (the fold itself) so all three stay in lock-step.
    #[test]
    fn declared_blob_reps_are_exactly_the_folded_archive_reps() {
        let stage = ArchiveBlobsStage::new();
        let mut declared: Vec<String> = stage.attaches_blob_reps().to_vec();
        declared.sort();
        let mut folded: Vec<String> = ARCHIVE_REPS.iter().map(|r| (*r).to_string()).collect();
        folded.sort();
        assert_eq!(
            declared, folded,
            "gmeow:attachesBlobRep must declare exactly the archive reps the fold emits"
        );
    }

    /// The stage's declared raw `input_files` cover every AUTHORED tree the fold reads,
    /// so an edit to a cell / test / authored shape busts the cache key (the stage
    /// consumes no source-load product, so nothing else covers these reads).
    #[test]
    fn input_files_cover_every_authored_source_the_fold_reads() {
        let root = repo_root();
        let declared: std::collections::BTreeSet<PathBuf> = ArchiveBlobsStage::new()
            .input_files(&root)
            .expect("input files")
            .into_iter()
            .collect();
        assert!(
            !declared.is_empty(),
            "the fold reads authored trees; the declaration must not be empty"
        );
        for probe in [
            slice_files(&root, "tests").expect("slice tests"),
            slice_files(&root, "mappings").expect("slice mappings"),
            list_files(&root.join("shapes"), "ttl").expect("authored shapes"),
            slice_named_files(&root, "shapes.ttl").expect("slice shapes"),
        ] {
            for path in probe {
                assert!(
                    declared.contains(&path),
                    "input_files must declare the authored source {}",
                    path.display()
                );
            }
        }
    }
}
