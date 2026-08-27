// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `archive-blobs` stage: the SINGLE producer of the bundle's by-reference TAR
//! archive blobs (eleven of them — see [`ARCHIVE_REPS`]).
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

/// The archive representation ids, from the crate that OWNS them: `gmeow-bundle-view`
/// addresses these blobs on the read side, and a second definition here would be a
/// second source of truth for a string the two sides have to agree on exactly.
pub(crate) use gmeow_bundle_view::bundle_blobs::{
    REP_AXIOMS, REP_CELLS, REP_MAPPINGS, REP_QUERIES, REP_SCHEMAS, REP_SHAPES, REP_TESTS,
};

/// The stage id — matches the `gmeow:stage-archive-blobs` individual.
pub const STAGE_ID: &str = "stage-archive-blobs";

/// tar of the generated Pydantic model package, member = package-relative path
/// (`gmeow_models/...`). Re-exported from the reader-side definition in
/// [`crate::bundle_blobs`] so producer and reader share ONE constant (a drifted
/// label would silently fold/read an empty package).
pub(crate) use crate::bundle_blobs::REP_MODELS_PYTHON;
/// tar of the WHOLE `lang:` deliverable family — everything under
/// [`LANG_PROJECTION_DIR`](crate::stages::lang_projection::LANG_PROJECTION_DIR) plus
/// the two non-RDF terminology surfaces in [`LANG_GLOSSARY_MEMBERS`] — member =
/// repo-relative path. [`is_lang_projection_member`] is the ONE authority on
/// membership.
///
/// # Why these have their OWN rep instead of riding `generated-opaque-archive`
///
/// A `gmeow:CompressionDictionary` primes a REP: the rep is the unit the medium
/// registry assigns a dictionary to, so a payload family that shares a rep with
/// unrelated bytes cannot be primed separately from them. The grammar / CoNLL-U /
/// TEI / GMN1 bytes and the ~18 MB TBX termbase + glossary table are one distinct
/// external-format family a consumer extracts on its own, so they get their own rep
/// rather than being welded to the general `generated/` archive's medium assignment.
///
/// # Why the glossary surfaces belong to THIS family, not the general one
///
/// `generated/projections/glossary.tbx` (ISO-30042 TBX) and
/// `generated/catalog/glossary.md` are projections of the SAME reviewed `.po` fold
/// the rest of this family comes from, produced by the SAME `stage-export-glossary`
/// leaf that emits `glossary.vartrans.ttl`, and their byte profile is the
/// natural-language term inventory — the exact vocabulary a linguistic dictionary is
/// trained on. Leaving them on `generated-opaque-archive` primed them with the
/// core-tier dictionary instead, and left `gmeow-lang-ast-v1` measuring a population
/// two orders of magnitude smaller than the family it names.
///
/// `glossary.vartrans.ttl` is deliberately NOT here: it is RDF and rides the
/// `graph/fanout/projections/glossary.vartrans.ttl` named graph, which is where a
/// canonical fold belongs. De-folding a named graph into bytes to widen a
/// dictionary's population would trade queryable structure for compression, which is
/// never a legal trade.
///
/// The split costs ZERO ontological use, which is exactly why it is legal here and
/// was NOT the answer for the mathematical content: every member is ALREADY an
/// opaque byte projection (a standalone external-format artifact a consumer reads as
/// a file; none reconstructs from a canonical named-graph fold), and the queryable
/// `lang:ProjectionEmission` / `graph/lang-glossary-corpus` semantics keep riding
/// their named graphs independently. Nothing that was a graph becomes bytes.
///
/// The members must therefore be carried by THIS rep and no longer by
/// `REP_GENERATED`: `carrier::opaque_already_carried` refuses every member so the two
/// archives cannot double-carry a path, and the superset reverse sweep would catch it
/// if they did.
pub(crate) const REP_LANG_PROJECTIONS: &str = "lang-projections-archive";
/// tar of the RDF 1.2 statement layer's two committed byte projections — the OWL
/// downcast and the RDF-1.2 lead — member = repo-relative path.
///
/// # Why these have their OWN rep
///
/// Both files are BYTE-DECORATED RDF: their committed form carries generated banners
/// and section markers that are not graph data, so they cannot reconstruct from a
/// canonical named-graph fold and have always travelled as opaque bytes (the
/// `byte_decorated_rdf_paths_fall_through_to_blob_members` gate in
/// [`crate::stages::superset`] pins that). Riding `REP_GENERATED` welded them to the
/// core-tier dictionary's medium assignment, which left the claim dictionary
/// with only the ~9 KB `yaml-ld-archive` to prime. Giving the statement layer its own
/// rep puts the claim vocabulary — reifier IRIs, annotation-coat predicates,
/// standpoint qualifiers — on a frame set a claim dictionary can actually be measured
/// over.
///
/// Nothing that was a graph becomes bytes: the queryable statement semantics keep
/// riding the `graph/statements` named graph, exactly as before. Only the medium
/// assignment of bytes that were ALREADY bytes changes.
pub(crate) const REP_STATEMENTS: &str = "statements-archive";
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
/// this archive. It rides the dictionary-less medium together with
/// [`REP_STATEMENTS`]: on its own this is ONE ~9 KB frame, far too small a population
/// to pay for any dictionary's in-band bytes. Measured over the claim corpus's WHOLE
/// frame set — this archive plus the statement layer's two byte projections — the claim
/// dictionary still did not pay, so it was retired and both reps ride unprimed.
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

/// The two NON-RDF terminology surfaces `stage-export-glossary` emits that join the
/// `lang:` deliverable family on [`REP_LANG_PROJECTIONS`] — the ISO-30042 TBX termbase
/// and the human-readable glossary table.
///
/// `generated/projections/glossary.vartrans.ttl`, the third surface that leaf emits, is
/// deliberately ABSENT: it is RDF and rides its RDF-fanout named graph, and a named
/// graph is never de-folded into bytes to widen a dictionary's population.
pub(crate) const LANG_GLOSSARY_MEMBERS: [&str; 2] = [
    crate::stages::lang_glossary::GLOSSARY_TABLE_PATH,
    crate::stages::lang_glossary::GLOSSARY_TBX_PATH,
];

/// The statement layer's two committed byte projections, folded as [`REP_STATEMENTS`]
/// off THIS run's `stage-statements` product. Order is canonical for the fail-closed
/// scan; the archive re-sorts members by key for determinism.
pub(crate) const STATEMENT_FILES: [&str; 2] = [
    crate::stages::statements::OWL_PATH,
    crate::stages::statements::RDF12_PATH,
];

/// Whether a committed path is a member of the `lang:` deliverable family
/// [`REP_LANG_PROJECTIONS`] carries.
///
/// THE one authority, in both directions: [`lang_projection_members`] selects the
/// archive's members with it and `carrier::opaque_already_carried` refuses the same
/// paths from the generated-opaque archive with it, so the two can never disagree
/// about which archive owns a path — the double-carry the superset reverse sweep
/// exists to catch, and which would also hand the SAME bytes to two differently-primed
/// frames.
///
/// The prefix test requires the directory separator, so a sibling directory whose name
/// merely starts with `generated/projections/lang` cannot be swept in.
pub(crate) fn is_lang_projection_member(path: &str) -> bool {
    let dir = crate::stages::lang_projection::LANG_PROJECTION_DIR;
    (path.starts_with(dir) && path.as_bytes().get(dir.len()) == Some(&b'/'))
        || LANG_GLOSSARY_MEMBERS.contains(&path)
}

/// Whether a committed path is one of the statement-layer byte projections
/// [`REP_STATEMENTS`] carries. The same one-authority role
/// [`is_lang_projection_member`] plays for the `lang:` family.
pub(crate) fn is_statements_member(path: &str) -> bool {
    STATEMENT_FILES.contains(&path)
}

/// The archive representations this stage attaches, in the CANONICAL order
/// [`build_archive_blobs`] returns them. [`archive_blobs_from_product`] reads them
/// back in exactly this order, so the row sequence a consumer sees is identical to
/// the sequence the fold produced (order-stable regardless of the blob lane's own
/// record order, which a cache round-trip is free to renormalize).
const ARCHIVE_REPS: [&str; 11] = [
    REP_MAPPINGS,
    REP_CELLS,
    REP_QUERIES,
    REP_TESTS,
    REP_SCHEMAS,
    REP_SHAPES,
    REP_AXIOMS,
    REP_MODELS_PYTHON,
    REP_LANG_PROJECTIONS,
    REP_STATEMENTS,
    REP_YAMLLD,
];

/// The five PRODUCER PRODUCTS the fold reads archive members off, each an in-memory
/// artifact map keyed by logical path. Grouped into named fields (like
/// [`ShapeSurfaces`]) for a reason that is sharper here than anywhere else in this
/// module: all five have the SAME type, `&BTreeMap<String, Vec<u8>>`, so as positional
/// parameters a transposition would compile silently and fold one producer's output
/// into another's archive.
///
/// Every one of them is a PRODUCT read, never a disk read: the committed files these
/// members reconstruct are not flushed until the post-run reconcile returns, so a disk
/// read would tar the previous build's bytes.
pub(crate) struct ProductArtifacts<'a> {
    /// `stage-compile-logic` — the compiled logic/DL axiom surface plus the
    /// validation/procedural shape surfaces.
    pub(crate) axioms: &'a BTreeMap<String, Vec<u8>>,
    /// `stage-mappings` — the SSSOM lift maps, the generated SPARQL surface, and the
    /// `generated/projections/lang/**` deliverable tree.
    pub(crate) mappings: &'a BTreeMap<String, Vec<u8>>,
    /// `stage-export-glossary` — the two NON-RDF terminology surfaces that complete the
    /// `lang:` family (its `.vartrans.ttl` sibling is RDF and rides a named graph).
    pub(crate) glossary: &'a BTreeMap<String, Vec<u8>>,
    /// `stage-export-pydantic` — the generated Pydantic model package.
    pub(crate) models_python: &'a BTreeMap<String, Vec<u8>>,
    /// `stage-statements` — the statement layer's two committed byte projections.
    pub(crate) statements: &'a BTreeMap<String, Vec<u8>>,
}

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
    products: &ProductArtifacts<'_>,
    shape_surfaces: &ShapeSurfaces<'_>,
    claim_serializations: &ClaimSerializations<'_>,
) -> Result<Vec<BlobRow>, gmeow_errors::Diag> {
    let ProductArtifacts {
        axioms: axiom_artifacts,
        mappings: mappings_artifacts,
        glossary: glossary_artifacts,
        models_python: models_python_artifacts,
        statements: statement_artifacts,
    } = *products;
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
    // lang-projections: the WHOLE `lang:` deliverable family — the
    // `generated/projections/lang/**` external-format deliverables off THIS run's
    // stage-mappings product plus the two non-RDF terminology surfaces off THIS run's
    // stage-export-glossary product — member = repo-relative path, product-sourced for the
    // same stale-disk reason the mappings/queries archives are. See [`REP_LANG_PROJECTIONS`]
    // for why they are their OWN rep rather than members of the generated-opaque archive.
    let lang_projections = lang_projection_members(mappings_artifacts, glossary_artifacts);
    // Fail closed, mirroring every other product-sourced archive above: a missing member
    // means a producer keyed its output under an unexpected path (or emitted none), which
    // would fold a SHORT archive AND silently shrink the population gmeow-lang-ast-v1 is
    // measured over — a silent capability degradation, not a fallback. Checked as the two
    // separate obligations they are, so the diagnosis names which producer went quiet.
    if !lang_projections
        .iter()
        .any(|(path, _)| path.starts_with(crate::stages::lang_projection::LANG_PROJECTION_DIR))
    {
        return Err(stage_err(
            "no generated/projections/lang/** artifacts in the stage-mappings product — the \
             lang-projections archive would fold without its grammar/CoNLL-U/TEI/GMN1 family \
             (fail-closed)",
        ));
    }
    for member in LANG_GLOSSARY_MEMBERS {
        if !lang_projections.iter().any(|(path, _)| path == member) {
            return Err(stage_err(&format!(
                "missing {member} in the stage-export-glossary product — the lang-projections \
                 archive would fold without the terminology surface it primes (fail-closed)"
            )));
        }
    }
    // statements: the statement layer's two byte-decorated RDF projections, member =
    // repo-relative path, sourced from THIS run's stage-statements product (the compile
    // ran ONCE, in that stage). Each MUST exist (no-optionality, fail-closed): a partial
    // archive would both break the superset gate's reconstruction of a committed path and
    // leave the rep carrying a truncated population.
    let mut statements: Vec<(String, Vec<u8>)> = Vec::with_capacity(STATEMENT_FILES.len());
    for rel in STATEMENT_FILES {
        let bytes = statement_artifacts.get(rel).ok_or_else(|| {
            stage_err(&format!(
                "missing statement artifact {rel} in the stage-statements product (fail-closed)"
            ))
        })?;
        statements.push((rel.to_string(), bytes.clone()));
    }
    statements.sort_by(|a, b| a.0.cmp(&b.0));
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
    // whose frame exists but carries no claims — an empty payload, which is the
    // dead-weight state this rep was promoted to end. A
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
        archive_blob(REP_STATEMENTS, &statements)?,
        archive_blob(REP_YAMLLD, &claim_serializations)?,
    ])
}

/// The [`REP_LANG_PROJECTIONS`] members of the two products that supply them: every
/// artifact [`is_lang_projection_member`] claims, keyed by its repo-relative committed
/// path (so
/// [`committed_path_for_archive_member`](crate::stages::carrier::committed_path_for_archive_member)
/// is the identity for this rep) and sorted.
///
/// `mappings_artifacts` carries the `generated/projections/lang/**` tree,
/// `glossary_artifacts` the two non-RDF terminology surfaces; the predicate — not the
/// source product — decides membership, so a member cannot be selected here and
/// simultaneously admitted to the generated-opaque archive.
pub(crate) fn lang_projection_members(
    mappings_artifacts: &BTreeMap<String, Vec<u8>>,
    glossary_artifacts: &BTreeMap<String, Vec<u8>>,
) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = mappings_artifacts
        .iter()
        .chain(glossary_artifacts.iter())
        .filter(|(path, _)| is_lang_projection_member(path))
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

/// Borrow one archive payload directly from the producer's content store.
///
/// The dictionary-corpus stage needs only the one or two reps its current corpus
/// selects. Reconstructing all eleven owned [`BlobRow`] values first duplicated the
/// complete archive population while the snapshot carrier was also resident.
///
/// # Errors
/// The archive producer is missing/released, or the requested representation's
/// record/digest/content is incomplete.
pub(crate) fn archive_blob_bytes_from_product<'a>(
    upstream: &'a BTreeMap<String, StageProduct>,
    rep: &str,
) -> Result<&'a [u8], gmeow_errors::Diag> {
    let product = upstream.get(STAGE_ID).ok_or_else(|| {
        stage_err(&format!(
            "missing {STAGE_ID} product for the `{rep}` archive blob"
        ))
    })?;
    if product.carrier_released {
        return Err(stage_err(&format!(
            "the {STAGE_ID} carrier was released before `{rep}` was read; the consumer must \
             declare {STAGE_ID} in carrier_consumes()"
        )));
    }
    archive_bytes(product.bundle(), rep)
}

fn archive_bytes<'a>(
    bundle: &'a PipelineBundle<PipelineHandle>,
    rep: &str,
) -> Result<&'a [u8], gmeow_errors::Diag> {
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
    bundle
        .blobs()
        .get(&digest)
        .map(Vec::as_slice)
        .ok_or_else(|| {
            stage_err(&format!(
                "the {STAGE_ID} product's `{rep}` blob digest resolves to no content-store entry"
            ))
        })
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
    let data = archive_bytes(bundle, rep)?.to_vec();
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

/// The `archive-blobs` pipeline stage: folds the eleven by-reference TAR archives and
/// attaches each to its product's blob lane under its `representation` label.
pub struct ArchiveBlobsStage {
    consumes: Vec<String>,
}

impl ArchiveBlobsStage {
    /// Construct the stage. It consumes exactly the producers whose in-memory products
    /// supply archive members: `stage-compile-logic` (the axiom surface + the
    /// validation/procedural shape surfaces), `stage-mappings` (SSSOM + generated
    /// queries + the `lang:` projection tree), `stage-export-glossary` (the two non-RDF
    /// terminology surfaces that complete the `lang:` family),
    /// `stage-export-json-schema` (the four JSON Schema documents),
    /// `stage-export-pydantic` (the model package), the three generated-shape export
    /// leaves, and `stage-statements` (the statement layer's two byte projections and
    /// the claim corpus's JSON-LD-star / YAML-LD-star surface). The edge set is declared
    /// identically here, in [`crate::run::full_spec`], and in
    /// `slices/core/pipeline/module.ttl`.
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-compile-logic".to_string(),
                "stage-export-constraint-shapes".to_string(),
                "stage-export-frame-shapes".to_string(),
                "stage-export-glossary".to_string(),
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
        // v4: `statements-archive` joins the fold and the two non-RDF terminology surfaces
        // join `lang-projections-archive`, both off `generated-opaque-archive`. A rep is
        // the unit a dictionary primes, so this is what gave the claim dictionary and
        // gmeow-lang-ast-v1 the frame sets their names claim. The claim dictionary was
        // later retired for failing the two-part code; its reps ride unprimed.
        "archive-blobs.v4"
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
    // THIS run's terminology surfaces (the two non-RDF members of REP_LANG_PROJECTIONS),
    // from the stage-export-glossary product: the `.po` fold ran ONCE in that leaf, and the
    // committed generated/catalog/glossary.md and generated/projections/glossary.tbx are
    // not flushed until the reconcile returns, so a disk read here would tar the stale
    // committed pair.
    let glossary_artifacts = upstream
        .get("stage-export-glossary")
        .ok_or_else(|| stage_err("missing stage-export-glossary product"))?
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
    // THIS run's statement layer (REP_STATEMENTS and REP_YAMLLD alike), sourced from the
    // stage-statements product for the same reason every surface above is: the compile and
    // the render both ran ONCE in that stage. The two committed byte projections are not
    // flushed until the reconcile returns, and the two JSON-LD-family artifacts ride the
    // INTERNAL `pipeline/statements/` lane and exist nowhere on disk at all.
    let statement_artifacts = upstream
        .get("stage-statements")
        .ok_or_else(|| stage_err("missing stage-statements product"))?
        .artifacts();
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
        &ProductArtifacts {
            axioms: &compile_artifacts,
            mappings: &mappings_artifacts,
            glossary: &glossary_artifacts,
            models_python: &models_python_artifacts,
            statements: &statement_artifacts,
        },
        &ShapeSurfaces {
            result: &result_shapes_ttl,
            frame: &frame_shapes_ttl,
            constraint: &constraint_shapes_ttl,
        },
        &ClaimSerializations {
            jsonld: &claim_jsonld,
            yamlld: &claim_yamlld,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn declared_blob_reps_match_the_static_archive_contract() {
        let mut declared: Vec<String> = ArchiveBlobsStage::new().attaches_blob_reps().to_vec();
        declared.sort();
        let mut expected: Vec<String> = ARCHIVE_REPS.iter().map(|rep| (*rep).to_string()).collect();
        expected.sort();
        assert_eq!(declared, expected);
    }

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
