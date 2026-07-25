// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native ontology-surface authoring gates — the whole-corpus structural
//! invariants that recreate the retired pytest cluster (`test_shapes.py`,
//! `test_vocabulary_surface.py`, `test_norms.py`, `test_slices.py`).
//!
//! Every gate is a **detector** returning `Vec<Finding>` (mirroring
//! [`crate::slice_ownership::ownership_findings`]) so the *sink* is a per-family
//! policy knob, not an architecture fork: the live-loader-hole families are folded
//! into the [`crate::validate_all`] `ValidationRun` (so `make validate` HARD-FAILS
//! on the real corpus), and each detector is additionally exercised by a
//! synthetic-negative unit test (proving it fires) and a whole-corpus integration
//! test (asserting the committed corpus is clean, with non-vacuity guards).
//!
//! The gates *read* the corpus; they never author a `sh:NodeShape` — no second
//! source of truth is introduced.
//!
//! Each detector splits into a corpus-loading `pub fn` and a pure inner function
//! over already-parsed [`Dataset`]s, so the synthetic negatives can drive the
//! detection logic without a full repository layout.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use gmeow_errors::{Diag, Finding, Location, Result, Severity};
use purrdf::slice::rdf_query::{Dataset, Object, Subject};
use regex::Regex;

use crate::codes;

/// The tool / SARIF-rule namespace for every authoring-integrity finding.
const TOOL: &str = "authoring-integrity";

const SH_NODE_SHAPE: &str = "http://www.w3.org/ns/shacl#NodeShape";
const SLICE_CLASS: &str = "https://blackcatinformatics.ca/gmeow/Slice";
const SLICE_TIER: &str = "https://blackcatinformatics.ca/gmeow/sliceTier";
/// `gmeow:GroundingSlice` — a slice typed as one of the three co-foundational
/// grounding layers (`lang:`/`math:`/`logic:`). Duplicated from
/// [`crate::slice_peerage`]'s private const of the same value (both readers
/// keep their own copy rather than exposing it, matching this crate's existing
/// posture of per-module governance-vocabulary constants).
const GMEOW_GROUNDING_SLICE: &str = "https://blackcatinformatics.ca/gmeow/GroundingSlice";
/// `gmeow:sliceCoFoundationalWith` — the symmetric grounding-peerage relation.
const GMEOW_CO_FOUNDATIONAL_WITH: &str =
    "https://blackcatinformatics.ca/gmeow/sliceCoFoundationalWith";
/// The `slices/` subdirectory every grounding slice's manifest must live under.
const GROUNDING_GROUP_PREFIX: &str = "grounding/";

/// The core `rights` module, parsed in isolation for the graft-isolation gate.
const CORE_RIGHTS_MODULE: &str = "slices/core/rights/module.ttl";

/// The norms-extension IRIs the core `rights` module must never reference (in any
/// triple position) — the graft is asserted on the extension side only, matching
/// the retired `test_graft_axioms_live_extension_side_only`. Matched by **exact
/// term identity**, never substring (`normIssuer` must not match `normIssuerRole`).
const NORMS_EXTENSION_TERMS: &[&str] = &[
    "https://blackcatinformatics.ca/gmeow/Norm",
    "https://blackcatinformatics.ca/gmeow/deonticModality",
    "https://blackcatinformatics.ca/gmeow/normIssuer",
    "https://blackcatinformatics.ca/gmeow/normBearer",
];

// ── docs-term extraction regexes ─────────────────────────────────────────────
//
// Each pattern is a compile-time-constant literal; the `.expect` fires only if
// that exact literal is malformed, which is a programming error a unit test
// (forcing the `LazyLock` to compile) catches in CI — never a data-dependent
// runtime panic on the library path. Compiled once per process instead of once
// per markdown file.

/// Backticked inline term reference, e.g. `` `gmeow:Foo` ``.
static GMEOW_INLINE_TERM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`gmeow:([A-Za-z][A-Za-z0-9_]*)`").expect("valid static regex"));
/// Bare (non-backticked) `gmeow:Foo` reference, matched inside fenced code.
static GMEOW_BARE_TERM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bgmeow:([A-Za-z][A-Za-z0-9_]*)\b").expect("valid static regex"));
/// Fenced ```turtle ... ``` code block.
static TURTLE_FENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```turtle\n(.*?)\n```").expect("valid static regex"));

// ── shared helpers ───────────────────────────────────────────────────────────

/// Build one authoring-integrity finding, hanging the offending IRI off a logical
/// location for SARIF grouping (as [`crate::slice_ownership`] does).
fn finding(severity: Severity, code: &str, message: String, logical: Option<String>) -> Finding {
    let mut f = Finding::new(severity, code, message).with_tool(TOOL);
    if let Some(iri) = logical {
        f.add_location(Location::new(None, None, None, Some(iri)));
    }
    f
}

fn io_err(path: &Path, e: &std::io::Error) -> Diag {
    Diag::of_kind(crate::error::Io {
        detail: format!("{}: {e}", path.display()),
    })
}

fn parse_err(path: &Path, e: &str) -> Diag {
    Diag::of_kind(crate::error::Parse {
        detail: format!("{}: {e}", path.display()),
    })
}

/// Parse a Turtle file into a native [`Dataset`], hard-failing on read/parse error
/// (no-optionality: a committed corpus file that cannot be read or parsed is a
/// HARD FAIL, never a silently skipped gate input).
fn parse_ttl(path: &Path) -> Result<Dataset> {
    let bytes = std::fs::read(path).map_err(|e| io_err(path, &e))?;
    Dataset::parse_turtle(&bytes, &path.display().to_string())
        .map_err(|e| parse_err(path, &e.to_string()))
}

/// Render a path relative to the repo root for a stable, location-independent
/// message (falls back to the full path when it is not under the root).
fn rel(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Every `manifest.ttl` under `slices_dir` — the discovery surface the retired
/// `discover_slices` walked (all manifests, so a duplicate IRI or a tierless
/// manifest anywhere is caught). Deterministically sorted.
fn all_manifests(slices_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![slices_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        match std::fs::read_dir(&dir) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(|e| io_err(&dir, &e))?;
                    let path = entry.path();
                    if path.is_dir() && !path.is_symlink() {
                        stack.push(path);
                    } else if path.file_name().is_some_and(|n| n == "manifest.ttl") {
                        out.push(path);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(&dir, &e)),
        }
    }
    out.sort();
    Ok(out)
}

// ── the live aggregator ──────────────────────────────────────────────────────

/// Run every ontology-surface authoring gate over the committed corpus and return
/// the union of their findings. This is the SINGLE function `validate_all` folds
/// onto the run ledger, so the live `make validate` path and the integration test
/// exercise exactly the same production aggregation — no re-implementation.
///
/// `project_root` is the repository root (the shape/profile/catalog/graft/term
/// gates scan it); `slices_dir` is the slice tree (the slice-discipline gate scans
/// it). Any read/parse failure is a HARD FAIL (propagated), never a silently
/// skipped gate.
///
/// Before folding any detector, the corpus is checked against the SAME
/// non-vacuity floors the whole-corpus integration tests assert independently
/// (`crates/validate/tests/authoring_integrity.rs`): a genuinely populated merged
/// shape-file set, declared-term set, and catalog `<uri>` set. Those tests only
/// guard the test binary; without this floor here, an environmental fault that
/// silently shrank the live corpus to empty (e.g. a subtree that failed to read
/// down to zero files, or a symlink loop that skipped every file) would still
/// report zero findings — a VACUOUS PASS on the real `make validate` path.
fn require_non_vacuous_corpus(project_root: &Path) -> Result<()> {
    let shape_files = purrdf::shapes::shape_union::shape_files(project_root)
        .map_err(|e| Diag::of_kind(crate::error::Io { detail: e }))?;
    if shape_files.is_empty() {
        return Err(Diag::of_kind(crate::error::Io {
            detail: "authoring-integrity: merged shape-file corpus floor 1 not met (got 0 \
                      files) — corpus read is vacuous, refusing to pass"
                .to_string(),
        }));
    }

    let declared = declared_ontology_terms(project_root)?;
    if declared.len() <= 50 {
        return Err(Diag::of_kind(crate::error::Io {
            detail: format!(
                "authoring-integrity: declared ontology terms floor 50 not met (got {}) — \
                 corpus read is vacuous, refusing to pass",
                declared.len()
            ),
        }));
    }

    let catalog_names = catalog_uri_names(project_root)?;
    if catalog_names.len() <= 1 {
        return Err(Diag::of_kind(crate::error::Io {
            detail: format!(
                "authoring-integrity: catalog <uri> entries floor 1 not met (got {}) — corpus \
                 read is vacuous, refusing to pass",
                catalog_names.len()
            ),
        }));
    }

    Ok(())
}

pub fn authoring_integrity_findings(
    project_root: &Path,
    slices_dir: &Path,
) -> Result<Vec<Finding>> {
    require_non_vacuous_corpus(project_root)?;
    let declared = declared_ontology_terms(project_root)?;
    let mut findings = shape_iri_collision_findings(project_root)?;
    findings.extend(graft_isolation_findings(project_root)?);
    findings.extend(slice_discipline_findings(slices_dir)?);
    findings.extend(peerage_discipline_findings(slices_dir)?);
    findings.extend(profile_closure_findings(project_root)?);
    findings.extend(catalog_closure_findings(project_root)?);
    findings.extend(module_iri_findings(project_root)?);
    findings.extend(example_undeclared_term_findings(project_root, &declared)?);
    findings.extend(slice_source_untagged_findings(project_root)?);
    findings.extend(nonslice_authored_untagged_findings(project_root)?);
    findings.extend(seam_registry_drift_findings(project_root, slices_dir)?);
    Ok(findings)
}

// ── R1: shape-IRI ownership collision ────────────────────────────────────────

/// Every `sh:NodeShape` IRI must be declared in exactly one shape file: merged
/// into one graph, two files declaring the same IRI fuse into a shape whose
/// meaning depends on parse order (the retired
/// `test_no_nodeshape_iri_collision_across_shape_files`).
///
/// Uses [`purrdf::shapes::shape_union::shape_files`] — byte-identical to the merged
/// corpus the live validator sees (base `shapes/*.ttl` minus the DSL lints, the
/// fail-closed `generated/shapes/*.ttl`, and every `slices/*/*/shapes.ttl`).
pub fn shape_iri_collision_findings(repo_root: &Path) -> Result<Vec<Finding>> {
    let files = purrdf::shapes::shape_union::shape_files(repo_root)
        .map_err(|e| Diag::of_kind(crate::error::Io { detail: e }))?;
    let mut loaded: Vec<(PathBuf, Dataset)> = Vec::with_capacity(files.len());
    for f in files {
        let ds = parse_ttl(&f)?;
        loaded.push((f, ds));
    }
    detect_shape_collisions(&loaded, repo_root)
}

/// The pure collision logic over already-parsed shape files: a `sh:NodeShape` IRI
/// (named subjects only — `subjects_of_type` excludes blank nodes) appearing in
/// more than one file is an ownership collision.
fn detect_shape_collisions(files: &[(PathBuf, Dataset)], root: &Path) -> Result<Vec<Finding>> {
    let mut iri_to_files: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for (path, ds) in files {
        let shapes = ds
            .subjects_of_type(SH_NODE_SHAPE)
            .map_err(|e| parse_err(path, &e.to_string()))?;
        for iri in shapes {
            iri_to_files.entry(iri).or_default().push(path.clone());
        }
    }
    let mut findings = Vec::new();
    for (iri, mut owners) in iri_to_files {
        if owners.len() > 1 {
            owners.sort();
            let list = owners
                .iter()
                .map(|p| rel(p, root))
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(finding(
                Severity::Error,
                codes::AUTHORING_SHAPE_IRI_COLLISION,
                format!(
                    "sh:NodeShape {iri} is declared in {n} shape files ({list}) — a merged \
                     shape graph must define each shape IRI exactly once",
                    n = owners.len(),
                ),
                Some(iri),
            ));
        }
    }
    findings.sort_by(|a, b| a.message.cmp(&b.message));
    Ok(findings)
}

// ── R4: graft isolation ──────────────────────────────────────────────────────

/// The core `rights` module must reference zero norms-extension IRIs (the retired
/// `test_graft_axioms_live_extension_side_only`): the graft lives on the extension
/// side only, with zero core churn.
pub fn graft_isolation_findings(repo_root: &Path) -> Result<Vec<Finding>> {
    let path = repo_root.join(CORE_RIGHTS_MODULE);
    let ds = parse_ttl(&path)?;
    Ok(detect_graft_leaks(&ds, &rel(&path, repo_root)))
}

/// The pure graft-leak logic: any norms-extension IRI appearing in subject,
/// predicate, or object position by **exact term identity**.
fn detect_graft_leaks(ds: &Dataset, source_label: &str) -> Vec<Finding> {
    // Ordered set of leaked terms → the positions they were found in.
    let mut leaked: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
    let mut note = |term: &str, position: &'static str| {
        if let Some(canon) = NORMS_EXTENSION_TERMS.iter().find(|t| **t == term) {
            let positions = leaked.entry(*canon).or_default();
            if !positions.contains(&position) {
                positions.push(position);
            }
        }
    };
    ds.for_each_quad(|s, p, o, _g| {
        if let Subject::Named(iri) = &s {
            note(iri, "subject");
        }
        note(p, "predicate");
        if let Object::Named(iri) = &o {
            note(iri, "object");
        }
    });
    leaked
        .into_iter()
        .map(|(term, positions)| {
            finding(
                Severity::Error,
                codes::AUTHORING_GRAFT_LEAK,
                format!(
                    "core module {source_label} references norms-extension IRI {term} (in {pos}) \
                     — the norms graft must live on the extension side only",
                    pos = positions.join("/"),
                ),
                Some(term.to_string()),
            )
        })
        .collect()
}

// ── R6: slice discipline (duplicate IRI + mandatory tier) ────────────────────

/// Slice discipline: a slice IRI is manifest-only identity and must be unique, and
/// every `gmeow:Slice` manifest must carry a `gmeow:sliceTier` (the retired
/// `test_duplicate_iri_rejected` / `test_missing_tier_rejected`). Closes the
/// purrdf loader hole — `SliceCatalog::discover` keeps duplicate IRIs and
/// `ManifestView.tier` is silently `None`.
pub fn slice_discipline_findings(slices_dir: &Path) -> Result<Vec<Finding>> {
    let manifests = all_manifests(slices_dir)?;
    let mut loaded: Vec<(PathBuf, Dataset)> = Vec::with_capacity(manifests.len());
    for m in manifests {
        let ds = parse_ttl(&m)?;
        loaded.push((m, ds));
    }
    detect_slice_discipline(&loaded, slices_dir)
}

/// The pure slice-discipline logic over already-parsed manifests.
fn detect_slice_discipline(manifests: &[(PathBuf, Dataset)], root: &Path) -> Result<Vec<Finding>> {
    let mut iri_to_manifests: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let mut findings = Vec::new();
    for (path, ds) in manifests {
        let slices = ds
            .subjects_of_type(SLICE_CLASS)
            .map_err(|e| parse_err(path, &e.to_string()))?;
        for iri in slices {
            let tiers = ds
                .object_iris(&iri, SLICE_TIER)
                .map_err(|e| parse_err(path, &e.to_string()))?;
            if tiers.is_empty() {
                findings.push(finding(
                    Severity::Error,
                    codes::SLICE_DISCIPLINE_MISSING_TIER,
                    format!(
                        "gmeow:Slice {iri} in {file} declares no gmeow:sliceTier — tier is \
                         mandatory (tierCore / tierExtension / tierProfile)",
                        file = rel(path, root),
                    ),
                    Some(iri.clone()),
                ));
            }
            iri_to_manifests.entry(iri).or_default().push(path.clone());
        }
    }
    for (iri, mut manifests) in iri_to_manifests {
        if manifests.len() > 1 {
            manifests.sort();
            let list = manifests
                .iter()
                .map(|p| rel(p, root))
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(finding(
                Severity::Error,
                codes::SLICE_DISCIPLINE_DUPLICATE_IRI,
                format!(
                    "slice IRI {iri} is declared by {n} manifests ({list}) — slice identity is \
                     manifest-only and must be unique",
                    n = manifests.len(),
                ),
                Some(iri),
            ));
        }
    }
    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    Ok(findings)
}

// ── R8: grounding-peerage discipline ─────────────────────────────────────────

/// Grounding-peerage discipline: three independent gates over the manifest
/// corpus, none of which the loader or the R6 slice-discipline gate above
/// checks:
///
/// * **non-grounding peerage** — a manifest declares
///   `gmeow:sliceCoFoundationalWith` but its own slice node is not typed
///   `gmeow:GroundingSlice`; the peerage grant (Principle 19) is reserved to
///   the three co-foundational grounding layers.
/// * **asymmetric peerage** — `gmeow:sliceCoFoundationalWith` is a symmetric
///   relation; slice A declaring peerage with B requires B to declare it back.
/// * **grounding-marker drift** — a slice's `gmeow:GroundingSlice` typing must
///   agree with its physical location under `slices/grounding/*` in BOTH
///   directions (typed-but-elsewhere, or under `grounding/`-but-untyped).
pub fn peerage_discipline_findings(slices_dir: &Path) -> Result<Vec<Finding>> {
    let manifests = all_manifests(slices_dir)?;
    let mut loaded: Vec<(PathBuf, Dataset)> = Vec::with_capacity(manifests.len());
    for m in manifests {
        let ds = parse_ttl(&m)?;
        loaded.push((m, ds));
    }
    detect_peerage_discipline(&loaded, slices_dir)
}

/// The pure peerage-discipline logic over already-parsed manifests.
fn detect_peerage_discipline(
    manifests: &[(PathBuf, Dataset)],
    root: &Path,
) -> Result<Vec<Finding>> {
    use std::collections::BTreeSet;

    let mut findings = Vec::new();
    // (declaring_iri -> to_iri) pairs, for the symmetry check, plus the
    // declaring manifest path (for a stable, deterministic message).
    let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();

    for (path, ds) in manifests {
        let group = rel(path, root);
        let under_grounding = group.starts_with(GROUNDING_GROUP_PREFIX);
        let slices = ds
            .subjects_of_type(SLICE_CLASS)
            .map_err(|e| parse_err(path, &e.to_string()))?;
        for iri in slices {
            let is_grounding = ds
                .has_type(&iri, GMEOW_GROUNDING_SLICE)
                .map_err(|e| parse_err(path, &e.to_string()))?;
            let peers = ds
                .object_iris(&iri, GMEOW_CO_FOUNDATIONAL_WITH)
                .map_err(|e| parse_err(path, &e.to_string()))?;

            if !peers.is_empty() && !is_grounding {
                findings.push(finding(
                    Severity::Error,
                    codes::SLICE_DISCIPLINE_NON_GROUNDING_PEERAGE,
                    format!(
                        "slice {iri} in {file} declares gmeow:sliceCoFoundationalWith but is not \
                         typed gmeow:GroundingSlice — the peerage grant (Principle 19) is reserved \
                         to the three co-foundational grounding layers",
                        file = rel(path, root),
                    ),
                    Some(iri.clone()),
                ));
            }

            if under_grounding && !is_grounding {
                findings.push(finding(
                    Severity::Error,
                    codes::SLICE_DISCIPLINE_GROUNDING_MARKER_DRIFT,
                    format!(
                        "slice {iri} lives under {file}, under slices/grounding/, but is not typed \
                         gmeow:GroundingSlice",
                        file = rel(path, root),
                    ),
                    Some(iri.clone()),
                ));
            } else if !under_grounding && is_grounding {
                findings.push(finding(
                    Severity::Error,
                    codes::SLICE_DISCIPLINE_GROUNDING_MARKER_DRIFT,
                    format!(
                        "slice {iri} in {file} is typed gmeow:GroundingSlice but does not live \
                         under slices/grounding/",
                        file = rel(path, root),
                    ),
                    Some(iri.clone()),
                ));
            }

            for peer in peers {
                pairs.insert((iri.clone(), peer));
            }
        }
    }

    for (from, to) in &pairs {
        if !pairs.contains(&(to.clone(), from.clone())) {
            findings.push(finding(
                Severity::Error,
                codes::SLICE_DISCIPLINE_ASYMMETRIC_PEERAGE,
                format!(
                    "slice {from} declares gmeow:sliceCoFoundationalWith {to}, but {to} does not \
                     declare the relation back — gmeow:sliceCoFoundationalWith is symmetric and \
                     must be authored on both manifests"
                ),
                Some(from.clone()),
            ));
        }
    }

    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    Ok(findings)
}

// ── R2: imports / profile / catalog closure + module-IRI ─────────────────────

const OWL_ONTOLOGY: &str = "http://www.w3.org/2002/07/owl#Ontology";
const OWL_IMPORTS: &str = "http://www.w3.org/2002/07/owl#imports";
const ONTOLOGY_IRI: &str = "https://blackcatinformatics.ca/gmeow";
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const TIER_CORE: &str = "https://blackcatinformatics.ca/gmeow/tierCore";
const TIER_EXTENSION: &str = "https://blackcatinformatics.ca/gmeow/tierExtension";
const TIER_PROFILE: &str = "https://blackcatinformatics.ca/gmeow/tierProfile";
const FULL_PROFILE: &str = "generated/profiles/full.ttl";
const CLAIMS_PROFILE: &str = "generated/profiles/claims.ttl";
const CATALOG_FILE: &str = "catalog-v001.xml";

/// The tier of a slice, classified from its `gmeow:sliceTier` IRI.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tier {
    Core,
    Extension,
    Profile,
}

fn classify_tier(iri: &str) -> Option<Tier> {
    match iri {
        TIER_CORE => Some(Tier::Core),
        TIER_EXTENSION => Some(Tier::Extension),
        TIER_PROFILE => Some(Tier::Profile),
        _ => None,
    }
}

/// One discovered slice: its IRI, the parsed tier (`None` = tierless, owned by the
/// slice-discipline gate), and the raw tier IRIs (to detect an unrecognized value).
struct SliceRec {
    iri: String,
    tier: Option<Tier>,
    raw_tiers: Vec<String>,
}

/// Every slice declared by a `manifest.ttl` under `slices_dir`, with its tier.
fn read_slices(slices_dir: &Path) -> Result<Vec<SliceRec>> {
    let mut out = Vec::new();
    for manifest in all_manifests(slices_dir)? {
        let ds = parse_ttl(&manifest)?;
        for iri in ds
            .subjects_of_type(SLICE_CLASS)
            .map_err(|e| parse_err(&manifest, &e.to_string()))?
        {
            let raw_tiers = ds
                .object_iris(&iri, SLICE_TIER)
                .map_err(|e| parse_err(&manifest, &e.to_string()))?;
            let tier = if raw_tiers.len() == 1 {
                classify_tier(&raw_tiers[0])
            } else {
                None
            };
            out.push(SliceRec {
                iri,
                tier,
                raw_tiers,
            });
        }
    }
    out.sort_by(|a, b| a.iri.cmp(&b.iri));
    Ok(out)
}

/// Every `slices/*/*/module.ttl` — the minting slice modules the catalog and
/// module-IRI gates key on. Deterministically sorted.
fn slice_module_files(slices_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![slices_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        match std::fs::read_dir(&dir) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(|e| io_err(&dir, &e))?;
                    let path = entry.path();
                    if path.is_dir() && !path.is_symlink() {
                        stack.push(path);
                    } else if path.file_name().is_some_and(|n| n == "module.ttl") {
                        out.push(path);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(&dir, &e)),
        }
    }
    out.sort();
    Ok(out)
}

/// The `owl:Ontology` subject IRI of a module (the first, matching the retired
/// Python which took `[0]`); `None` when the module declares no ontology.
fn module_ontology_iri(ds: &Dataset, path: &Path) -> Result<Option<String>> {
    let mut subjects = ds
        .subjects_of_type(OWL_ONTOLOGY)
        .map_err(|e| parse_err(path, &e.to_string()))?;
    subjects.sort();
    Ok(subjects.into_iter().next())
}

/// A profile document's `owl:imports` IRI set (union across its `owl:Ontology`
/// subjects, named-node objects only).
fn profile_imports(repo_root: &Path, rel_path: &str) -> Result<std::collections::BTreeSet<String>> {
    let path = repo_root.join(rel_path);
    let ds = parse_ttl(&path)?;
    let mut out = std::collections::BTreeSet::new();
    for subject in ds
        .subjects_of_type(OWL_ONTOLOGY)
        .map_err(|e| parse_err(&path, &e.to_string()))?
    {
        for iri in ds
            .object_iris(&subject, OWL_IMPORTS)
            .map_err(|e| parse_err(&path, &e.to_string()))?
        {
            out.insert(iri);
        }
    }
    Ok(out)
}

/// Profile & partition closure: `full` imports the root plus every extension,
/// `claims` is a strict subset of core, and every slice carries exactly one
/// recognized tier (the retired `test_full_profile_imports_every_slice`,
/// `test_claims_profile_is_genuinely_sub_core`). A tierless slice is owned by the
/// slice-discipline gate and NOT re-reported here.
pub fn profile_closure_findings(repo_root: &Path) -> Result<Vec<Finding>> {
    let slices = read_slices(&repo_root.join("slices"))?;
    let full_imports = profile_imports(repo_root, FULL_PROFILE)?;
    let claims_imports = profile_imports(repo_root, CLAIMS_PROFILE)?;
    Ok(detect_profile_closure(
        &slices,
        &full_imports,
        &claims_imports,
    ))
}

fn detect_profile_closure(
    slices: &[SliceRec],
    full_imports: &std::collections::BTreeSet<String>,
    claims_imports: &std::collections::BTreeSet<String>,
) -> Vec<Finding> {
    use std::collections::BTreeSet;
    let core: BTreeSet<&str> = slices
        .iter()
        .filter(|s| s.tier == Some(Tier::Core))
        .map(|s| s.iri.as_str())
        .collect();
    let extensions: BTreeSet<&str> = slices
        .iter()
        .filter(|s| s.tier == Some(Tier::Extension))
        .map(|s| s.iri.as_str())
        .collect();

    let mut findings = Vec::new();

    // A slice that has a sliceTier value but it is not one of the three recognized
    // tiers is a partition break (a tierless slice is the discipline gate's job).
    for s in slices {
        if s.tier.is_none() && !s.raw_tiers.is_empty() {
            findings.push(finding(
                Severity::Error,
                codes::AUTHORING_PROFILE_CLOSURE,
                format!(
                    "slice {iri} has unrecognized gmeow:sliceTier {tiers:?} — a slice must be \
                     exactly one of tierCore / tierExtension / tierProfile",
                    iri = s.iri,
                    tiers = s.raw_tiers,
                ),
                Some(s.iri.clone()),
            ));
        }
    }

    // full.ttl imports == {ontology IRI} ∪ extensions.
    let mut expected_full: BTreeSet<&str> = extensions.clone();
    expected_full.insert(ONTOLOGY_IRI);
    let full_set: BTreeSet<&str> = full_imports.iter().map(String::as_str).collect();
    if full_set != expected_full {
        let extra: Vec<&&str> = full_set.difference(&expected_full).collect();
        let missing: Vec<&&str> = expected_full.difference(&full_set).collect();
        findings.push(finding(
            Severity::Error,
            codes::AUTHORING_PROFILE_CLOSURE,
            format!(
                "generated/profiles/full.ttl owl:imports must equal the root plus every extension. \
                 extra: {extra:?}; missing: {missing:?}"
            ),
            None,
        ));
    }

    // claims.ttl imports ⊊ core (strict subset).
    let claims_set: BTreeSet<&str> = claims_imports.iter().map(String::as_str).collect();
    if !(claims_set.is_subset(&core) && claims_set != core) {
        let extra: Vec<&&str> = claims_set.difference(&core).collect();
        findings.push(finding(
            Severity::Error,
            codes::AUTHORING_PROFILE_CLOSURE,
            format!(
                "generated/profiles/claims.ttl owl:imports must be a STRICT subset of core. \
                 not-in-core: {extra:?}"
            ),
            None,
        ));
    }

    findings.sort_by(|a, b| a.message.cmp(&b.message));
    findings
}

/// Catalog closure: every slice module's `owl:Ontology` IRI is mapped in the
/// generated OASIS catalog (the retired `test_all_modules_are_in_catalog`).
pub fn catalog_closure_findings(repo_root: &Path) -> Result<Vec<Finding>> {
    let catalog_names = catalog_uri_names(repo_root)?;
    let mut module_iris: Vec<(String, PathBuf)> = Vec::new();
    for module in slice_module_files(&repo_root.join("slices"))? {
        let ds = parse_ttl(&module)?;
        if let Some(iri) = module_ontology_iri(&ds, &module)? {
            module_iris.push((iri, module));
        }
    }
    let mut findings = Vec::new();
    for (iri, module) in module_iris {
        if !catalog_names.contains(&iri) {
            findings.push(finding(
                Severity::Error,
                codes::AUTHORING_CATALOG_MISSING_MODULE,
                format!(
                    "module {file} declares owl:Ontology {iri} which is absent from {CATALOG_FILE}",
                    file = rel(&module, repo_root),
                ),
                Some(iri),
            ));
        }
    }
    findings.sort_by(|a, b| a.message.cmp(&b.message));
    Ok(findings)
}

/// The `name` attribute of every `<uri>` element in the OASIS catalog, parsed with
/// a real read-only XML DOM (comments/CDATA/entities handled — never a substring
/// scan). Matched by local element name, so the catalog's default namespace does
/// not hide the entries.
fn catalog_uri_names(repo_root: &Path) -> Result<std::collections::BTreeSet<String>> {
    let path = repo_root.join(CATALOG_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| io_err(&path, &e))?;
    parse_catalog_names(&text, &path)
}

/// Parse the OASIS catalog XML text into the set of `<uri>` `name` attributes.
/// Matched by local element name so the catalog's default namespace does not hide
/// the entries; a real DOM parse handles comments/CDATA/entities.
fn parse_catalog_names(text: &str, path: &Path) -> Result<std::collections::BTreeSet<String>> {
    let doc = roxmltree::Document::parse(text)
        .map_err(|e| parse_err(path, &format!("catalog XML: {e}")))?;
    let mut names = std::collections::BTreeSet::new();
    for node in doc.descendants() {
        if node.is_element()
            && node.tag_name().name() == "uri"
            && let Some(name) = node.attribute("name")
        {
            names.insert(name.to_string());
        }
    }
    Ok(names)
}

/// Module-IRI discipline: each slice module's `owl:Ontology` IRI equals its
/// location-derived IRI `…/gmeow/slices/<slice-dir-name>` — the parent directory
/// name, never the group segment (the retired `test_module_iri_matches_filename`).
pub fn module_iri_findings(repo_root: &Path) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    for module in slice_module_files(&repo_root.join("slices"))? {
        let ds = parse_ttl(&module)?;
        let slice_dir = module
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let expected = format!("{GMEOW_NS}slices/{slice_dir}");
        match module_ontology_iri(&ds, &module)? {
            Some(iri) if iri == expected => {}
            Some(iri) => findings.push(finding(
                Severity::Error,
                codes::AUTHORING_MODULE_IRI_MISMATCH,
                format!(
                    "module {file} declares owl:Ontology {iri} but its location requires {expected}",
                    file = rel(&module, repo_root),
                ),
                Some(iri),
            )),
            None => findings.push(finding(
                Severity::Error,
                codes::AUTHORING_MODULE_IRI_MISMATCH,
                format!(
                    "module {file} declares no owl:Ontology (expected {expected})",
                    file = rel(&module, repo_root),
                ),
                None,
            )),
        }
    }
    findings.sort_by(|a, b| a.message.cmp(&b.message));
    Ok(findings)
}

// ── R3: term-declaration + language-tag discipline ───────────────────────────

use std::collections::BTreeSet;

/// A minimal [`crate::lint::LintConfig`] for declared-term collection — only
/// `namespace` is read by `collect_typed_terms_dataset` (it filters GMEOW terms).
fn minimal_lint_cfg() -> crate::lint::LintConfig {
    crate::lint::LintConfig {
        namespace: GMEOW_NS.to_string(),
        ontology_iri: ONTOLOGY_IRI.to_string(),
        selector_tokens: BTreeSet::new(),
        core_slice_iris: std::collections::HashSet::new(),
        annotation_predicates: std::collections::HashSet::new(),
    }
}

/// The authored GMEOW vocabulary sources — the files that MINT terms: the root
/// ontology, every slice module, the slice-manifest vocabulary, the test-DSL and
/// mapping/statement DSL vocabularies, and the authored shapes. A term is
/// "declared" iff it is a typed subject in one of these (the same universe the
/// deleted gate's declared set + the DSL vocabularies covered).
fn vocabulary_source_files(repo_root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = vec![repo_root.join("ontology/gmeow.ttl")];
    files.extend(slice_module_files(&repo_root.join("slices"))?);
    for optional in [
        "slices/vocabulary.ttl",
        "dsl/tests/vocabulary.ttl",
        "shapes/gmeow-shapes.ttl",
    ] {
        let p = repo_root.join(optional);
        if p.is_file() {
            files.push(p);
        }
    }
    files.extend(ttl_recursive(&repo_root.join("dsl/mappings"))?);
    files.extend(ttl_recursive(&repo_root.join("dsl/statements"))?);
    files.sort();
    files.dedup();
    Ok(files)
}

/// The declared GMEOW vocabulary terms — the single authority
/// [`crate::lint::declared_terms_dataset`] over the merged vocabulary sources (NOT
/// a re-derivation). The typed-term set is the canonical "declared vocabulary"; an
/// undeclared predicate a fixture/example uses is the silent typo SHACL leaves
/// inert.
pub fn declared_ontology_terms(repo_root: &Path) -> Result<BTreeSet<String>> {
    use purrdf::slice::rdf_query::DatasetAccumulator;
    let mut acc = DatasetAccumulator::new();
    for source in vocabulary_source_files(repo_root)? {
        let bytes = std::fs::read(&source).map_err(|e| io_err(&source, &e))?;
        acc.add_turtle(&bytes, &source.display().to_string())
            .map_err(|e| parse_err(&source, &e.to_string()))?;
    }
    let ds = acc
        .freeze()
        .map_err(|e| parse_err(repo_root, &e.to_string()))?;
    Ok(
        crate::lint::declared_terms_dataset(ds.inner(), &minimal_lint_cfg())
            .into_iter()
            .collect(),
    )
}

/// Every GMEOW-namespace vocabulary term used in a dataset (any triple position),
/// excluding instance IRIs (the `…/examples/` and `…/example/` worked-example
/// namespaces — the latter is the Rust-referenced convention in
/// `logic-compile`/`docs`) and ontology-module IRIs (`…/modules/`).
fn gmeow_vocab_terms(ds: &Dataset) -> BTreeSet<String> {
    let examples = format!("{GMEOW_NS}examples/");
    let example = format!("{GMEOW_NS}example/");
    let modules = format!("{GMEOW_NS}modules/");
    let mut out = BTreeSet::new();
    let mut consider = |iri: &str| {
        if iri.starts_with(GMEOW_NS)
            && !iri.starts_with(&examples)
            && !iri.starts_with(&example)
            && !iri.starts_with(&modules)
        {
            out.insert(iri.to_string());
        }
    };
    ds.for_each_quad(|s, p, o, _g| {
        if let Subject::Named(iri) = &s {
            consider(iri);
        }
        consider(p);
        if let Object::Named(iri) = &o {
            consider(iri);
        }
    });
    out
}

/// The pure "uses only declared terms" logic over already-parsed files.
fn detect_undeclared_terms(
    declared: &BTreeSet<String>,
    files: &[(PathBuf, Dataset)],
    root: &Path,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (path, ds) in files {
        let undeclared: Vec<String> = gmeow_vocab_terms(ds)
            .into_iter()
            .filter(|t| !declared.contains(t))
            .collect();
        for term in undeclared {
            findings.push(finding(
                Severity::Error,
                codes::AUTHORING_UNDECLARED_TERM,
                format!(
                    "{file} references undeclared GMEOW term {term} — declare it or fix the typo",
                    file = rel(path, root),
                ),
                Some(term),
            ));
        }
    }
    findings.sort_by(|a, b| a.message.cmp(&b.message));
    findings
}

/// Load every `*.ttl` under a glob-like set of directories.
fn load_ttl_files(paths: &[PathBuf]) -> Result<Vec<(PathBuf, Dataset)>> {
    let mut out = Vec::with_capacity(paths.len());
    for p in paths {
        out.push((p.clone(), parse_ttl(p)?));
    }
    Ok(out)
}

/// `*.ttl` directly in a directory (non-recursive), sorted.
fn ttl_in_dir(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|e| io_err(dir, &e))?;
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "ttl") && path.is_file() {
                    out.push(path);
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io_err(dir, &e)),
    }
    out.sort();
    Ok(out)
}

/// `*.ttl` recursively under a directory, sorted.
fn ttl_recursive(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        match std::fs::read_dir(&d) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(|e| io_err(&d, &e))?;
                    let path = entry.path();
                    if path.is_dir() && !path.is_symlink() {
                        stack.push(path);
                    } else if path.extension().is_some_and(|e| e == "ttl") {
                        out.push(path);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(&d, &e)),
        }
    }
    out.sort();
    Ok(out)
}

/// Every `slices/*/*/examples/*.ttl`, discovered by MANIFEST (not module.ttl) so
/// a module-less pure-selection profile slice (which mints no module but does
/// ship worked examples) is covered too — a module-bearing slice always also
/// carries a manifest, so that behavior is unchanged. Sorted.
fn slice_example_files(slices_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for manifest in all_manifests(slices_dir)? {
        if let Some(slice_dir) = manifest.parent() {
            out.extend(ttl_in_dir(&slice_dir.join("examples"))?);
        }
    }
    out.sort();
    Ok(out)
}

/// R3b: slice worked examples reference only declared terms.
pub fn example_undeclared_term_findings(
    repo_root: &Path,
    declared: &BTreeSet<String>,
) -> Result<Vec<Finding>> {
    let files = load_ttl_files(&slice_example_files(&repo_root.join("slices"))?)?;
    Ok(detect_undeclared_terms(declared, &files, repo_root))
}

/// R3a: coverage fixtures reference only declared terms.
pub fn coverage_fixture_undeclared_findings(
    repo_root: &Path,
    declared: &BTreeSet<String>,
) -> Result<Vec<Finding>> {
    let files = load_ttl_files(&ttl_in_dir(&repo_root.join("tests/fixtures/coverage"))?)?;
    Ok(detect_undeclared_terms(declared, &files, repo_root))
}

/// The pure "localizable literals carry a language tag" logic. A literal object of
/// a localizable predicate with NO language tag is a distinct, untranslatable RDF
/// term (the retired `*_localizable_literals_are_language_tagged`).
fn detect_untagged_localizable(files: &[(PathBuf, Dataset)], root: &Path) -> Vec<Finding> {
    let localizable: BTreeSet<&str> = crate::localizable::LOCALIZABLE_PREDICATES
        .iter()
        .copied()
        .collect();
    let mut findings = Vec::new();
    for (path, ds) in files {
        let mut hits: Vec<String> = Vec::new();
        ds.for_each_quad(|_s, p, o, _g| {
            if localizable.contains(p)
                && let Object::Literal {
                    value, language, ..
                } = &o
                && language.is_none()
            {
                hits.push(format!("{p} \"{value}\""));
            }
        });
        hits.sort();
        hits.dedup();
        for hit in hits {
            findings.push(finding(
                Severity::Error,
                codes::AUTHORING_UNTAGGED_LOCALIZABLE_LITERAL,
                format!(
                    "{file}: localizable literal {hit} carries no language tag — add \
                     @x-gmeow-english (a plain literal is untranslatable)",
                    file = rel(path, root),
                ),
                None,
            ));
        }
    }
    findings.sort_by(|a, b| a.message.cmp(&b.message));
    findings
}

/// R3c: every localizable literal in slice source (`module.ttl`, `shapes.ttl`,
/// `mappings/*.ttl`, `examples/*.ttl`) carries a language tag. Discovered by
/// MANIFEST (not module.ttl) so a module-less pure-selection profile slice's
/// examples/shapes/mappings are covered too — `module.ttl` is pushed ONLY when
/// present, so a module-bearing slice's behavior is unchanged.
pub fn slice_source_untagged_findings(repo_root: &Path) -> Result<Vec<Finding>> {
    let slices = repo_root.join("slices");
    let mut paths = Vec::new();
    for manifest in all_manifests(&slices)? {
        if let Some(dir) = manifest.parent() {
            let module = dir.join("module.ttl");
            if module.is_file() {
                paths.push(module);
            }
            let shapes = dir.join("shapes.ttl");
            if shapes.is_file() {
                paths.push(shapes);
            }
            paths.extend(ttl_recursive(&dir.join("mappings"))?);
            paths.extend(ttl_in_dir(&dir.join("examples"))?);
        }
    }
    paths.sort();
    paths.dedup();
    let files = load_ttl_files(&paths)?;
    Ok(detect_untagged_localizable(&files, repo_root))
}

/// R3d: every localizable literal in hand-authored non-slice source (`shapes/*.ttl`,
/// `governance/*.ttl`, `dsl/mappings/**/*.ttl`, `dsl/statements/**/*.ttl`) carries a
/// language tag.
pub fn nonslice_authored_untagged_findings(repo_root: &Path) -> Result<Vec<Finding>> {
    let mut paths = ttl_in_dir(&repo_root.join("shapes"))?;
    paths.extend(ttl_in_dir(&repo_root.join("governance"))?);
    paths.extend(ttl_recursive(&repo_root.join("dsl/mappings"))?);
    paths.extend(ttl_recursive(&repo_root.join("dsl/statements"))?);
    paths.sort();
    paths.dedup();
    let files = load_ttl_files(&paths)?;
    Ok(detect_untagged_localizable(&files, repo_root))
}

/// Retired terms permitted to appear in historical/migration docs prose.
const RETIRED_DOCS_TERMS: &[&str] = &["alternateName", "gender", "sex"];

/// Documentation-only GMEOW names permitted in docs prose/examples — intentionally
/// NOT declared terms, allowlisted exactly (the analogue of the retired
/// `_RETIRED_DOCS_TERMS`): a retired documentation-only shape recorded for
/// historical context, and a migration-guide illustrative `logic:Constraint` name.
const DOCS_DOCUMENTATION_ONLY_TERMS: &[&str] = &[
    // The standpoint module records this retired documentation-only shape in prose.
    "StandpointCoexistenceShape",
    // MIGRATING-SHAPES-TO-LOGIC.md's illustrative constraint, paired with the real
    // gmeow:ClaimNeedsEvidenceShape it would replace.
    "ClaimNeedsEvidenceConstraint",
];

/// Extract every `gmeow:LocalName` referenced in a markdown document — inside
/// fenced ```turtle blocks (backticked and bare) and inline `` `gmeow:Name` `` —
/// as full IRIs. Pure over the text, so a unit test can drive it.
fn extract_gmeow_terms_from_markdown(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut add = |name: &str| {
        out.insert(format!("{GMEOW_NS}{name}"));
    };
    // Fenced turtle blocks: both backticked and bare prefixed names.
    for block in TURTLE_FENCE.captures_iter(text) {
        let body = &block[1];
        for cap in GMEOW_INLINE_TERM.captures_iter(body) {
            add(&cap[1]);
        }
        for cap in GMEOW_BARE_TERM.captures_iter(body) {
            add(&cap[1]);
        }
    }
    // Inline backticked terms anywhere in the prose.
    for cap in GMEOW_INLINE_TERM.captures_iter(text) {
        add(&cap[1]);
    }
    out
}

/// Every GMEOW-namespace term appearing in ANY triple position across a set of TTL
/// files — the set of real term IRIs a doc example may legitimately name (a shape
/// named only as a `logic:formalizes` object is still a real term). A docs `gmeow:`
/// reference is a typo only when it appears NOWHERE in the authored/generated
/// ontology. Instance IRIs (`…/example(s)/`, `…/modules/`) are excluded.
fn gmeow_terms_any_position(files: &[PathBuf]) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for path in files {
        let ds = parse_ttl(path)?;
        out.extend(gmeow_vocab_terms(&ds));
    }
    Ok(out)
}

/// The `docs/*.md` files (top-level, non-recursive), sorted.
fn docs_md_files(repo_root: &Path) -> Result<Vec<PathBuf>> {
    let dir = repo_root.join("docs");
    let mut out = Vec::new();
    match std::fs::read_dir(&dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|e| io_err(&dir, &e))?;
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "md") && path.is_file() {
                    out.push(path);
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io_err(&dir, &e)),
    }
    out.sort();
    Ok(out)
}

/// Every `gmeow:` term referenced across the docs corpus — exposed so the gate's
/// non-vacuity guard can prove the fence/inline extractor is not silently
/// yielding an empty set (a broken regex would otherwise pass vacuously).
pub fn docs_gmeow_terms(repo_root: &Path) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for md in docs_md_files(repo_root)? {
        let text = std::fs::read_to_string(&md).map_err(|e| io_err(&md, &e))?;
        out.extend(extract_gmeow_terms_from_markdown(&text));
    }
    Ok(out)
}

/// The allowlist of `gmeow:` terms a doc example may legitimately name: every
/// GMEOW-namespace SUBJECT declared across the root ontology + slice modules
/// (which mint the classes, properties, and shape individuals), the authored
/// `gmeow-shapes.ttl`, the slice-manifest vocabulary, the test-DSL vocabulary, the
/// mapping/statement DSL sources, plus the retired-terms prose allowance. This
/// mirrors the retired `_docs_allowlist` exactly.
fn docs_allowlist(repo_root: &Path) -> Result<BTreeSet<String>> {
    let mut files = vec![repo_root.join("ontology/gmeow.ttl")];
    files.extend(slice_module_files(&repo_root.join("slices"))?);
    for optional in [
        "shapes/gmeow-shapes.ttl",
        "slices/vocabulary.ttl",
        "dsl/tests/vocabulary.ttl",
    ] {
        let p = repo_root.join(optional);
        if p.is_file() {
            files.push(p);
        }
    }
    files.extend(ttl_recursive(&repo_root.join("dsl/mappings"))?);
    files.extend(ttl_recursive(&repo_root.join("dsl/statements"))?);
    // Slice shape files declare shape IRIs a doc may name; the derived SHACL shapes
    // and the shape-grounding ledger carry the logic:formalizes shape names.
    for module in slice_module_files(&repo_root.join("slices"))? {
        if let Some(dir) = module.parent() {
            let shapes = dir.join("shapes.ttl");
            if shapes.is_file() {
                files.push(shapes);
            }
        }
    }
    files.extend(ttl_in_dir(&repo_root.join("generated/shapes"))?);
    files.extend(ttl_in_dir(&repo_root.join("generated/logic"))?);
    files.sort();
    files.dedup();
    let mut allow = gmeow_terms_any_position(&files)?;
    for name in RETIRED_DOCS_TERMS
        .iter()
        .chain(DOCS_DOCUMENTATION_ONLY_TERMS)
    {
        allow.insert(format!("{GMEOW_NS}{name}"));
    }
    Ok(allow)
}

/// R3e: user-copyable docs examples reference only allowlisted `gmeow:` terms.
pub fn docs_undeclared_findings(repo_root: &Path) -> Result<Vec<Finding>> {
    let allow = docs_allowlist(repo_root)?;
    let mut findings = Vec::new();
    for md in docs_md_files(repo_root)? {
        let text = std::fs::read_to_string(&md).map_err(|e| io_err(&md, &e))?;
        for term in extract_gmeow_terms_from_markdown(&text) {
            if !allow.contains(&term) {
                findings.push(finding(
                    Severity::Error,
                    codes::AUTHORING_UNDECLARED_TERM,
                    format!(
                        "docs example {file} references unallowlisted GMEOW term {term}",
                        file = rel(&md, repo_root),
                    ),
                    Some(term),
                ));
            }
        }
    }
    findings.sort_by(|a, b| a.message.cmp(&b.message));
    Ok(findings)
}

// ── R7: grounding seam-registry drift ────────────────────────────────────────
//
// The generated seam-registry page (`gmeow_docs::render::Page::SeamRegistry`,
// materialized at `ontology-docs/seams/index.md` by `make sync
// SYNC_OUTPUTS=docs`) is a pure projection of the `gmeow:Seam` individuals
// authored in the grounding slices' manifests (docs/GROUNDING.md, "The seam
// registry"). This gate is a SECOND, INDEPENDENT reader of that same
// governance data — `gmeow-validate` cannot depend on `gmeow-docs` (which
// itself depends on `gmeow-validate`), so drift is caught by comparing the
// canonical data straight off the manifests against the rendered page text,
// never by re-running the renderer. `ontology-docs/` is an ON-DEMAND `docs`
// output, not part of the `SYNC_OUTPUTS=generated` tree `make validate`
// requires, so an absent page is a cache miss (nothing rendered yet), never a
// hard fail — this mirrors every other tolerant-absence generated-artifact
// read in this file (e.g. `generated/shapes` in `docs_allowlist`).

/// The site-relative path of the generated seam-registry page.
const SEAM_REGISTRY_PAGE_PATH: &str = "ontology-docs/seams/index.md";

/// Backtick-wrapped CURIE in one of the four grounding term families
/// (`gmeow:`/`logic:`/`lang:`/`math:`) — generalizes [`GMEOW_INLINE_TERM`] to
/// every family a `gmeow:seamCarryingTerm` may name.
static SEAM_PAGE_CARRYING_TERM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"`(gmeow|logic|lang|math):([A-Za-z][A-Za-z0-9_]*)`").expect("valid static regex")
});
/// Backtick-wrapped `NAME.md` design-doc filename (a `gmeow:seamOwningDoc` value).
static SEAM_PAGE_OWNING_DOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`([A-Za-z0-9_-]+\.md)`").expect("valid static regex"));

// `SeamRecord` + `seam_records_of` — the single reader of the `gmeow:Seam`
// governance data — live in `crate::slice_peerage` (which the peerage-coverage
// engine also needs, extended with the directed `(from, to)` legs and raw
// carrying-term IRIs this text-comparison drift gate never needed). Sharing
// one reader here keeps this drift gate and the coverage engine from ever
// reading the same governance data two different ways.
use crate::slice_peerage::{SeamRecord, seam_records_of};

/// The seam-table region of the rendered page: from the table header through
/// (but excluding) the `## Definitions` heading, or the whole text when either
/// marker is absent (e.g. the zero-seams render, or a malformed page — the
/// drift check then degrades to scanning everything, never panicking). Scoping
/// the reverse (page → data) scan to this region keeps incidental backtick-CURIE
/// mentions elsewhere on the page (e.g. `gmeow:sliceCoFoundationalWith` in the
/// intro prose) from being misread as claimed carrying terms.
fn seam_table_region(page_text: &str) -> &str {
    let start = page_text
        .find("| Seam | Direction | Carrying terms | Owning doc |")
        .unwrap_or(0);
    let region = &page_text[start..];
    let end = region.find("## Definitions").unwrap_or(region.len());
    &region[..end]
}

/// R7: the generated seam-registry page carries exactly the `gmeow:Seam`
/// data authored in the grounding slices' manifests — every seam name,
/// carrying term, and owning doc in the data appears on the page, and no
/// carrying-term CURIE or `.md` doc reference on the page is unbacked by the
/// data (drift in either direction).
pub fn seam_registry_drift_findings(
    project_root: &Path,
    slices_dir: &Path,
) -> Result<Vec<Finding>> {
    let page_path = project_root.join(SEAM_REGISTRY_PAGE_PATH);
    let page_text = match std::fs::read_to_string(&page_path) {
        Ok(text) => text,
        // Not yet synced (`make sync SYNC_OUTPUTS=docs` never ran) — a cache
        // miss, not a drift finding.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_err(&page_path, &e)),
    };

    let mut seams: Vec<SeamRecord> = Vec::new();
    for manifest in all_manifests(slices_dir)? {
        let ds = parse_ttl(&manifest)?;
        seams.extend(seam_records_of(&ds, &manifest)?);
    }
    Ok(detect_seam_registry_drift(&seams, &page_text))
}

/// The pure drift-detection logic over already-read seam data + page text.
fn detect_seam_registry_drift(seams: &[SeamRecord], page_text: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let table_region = seam_table_region(page_text);

    // data -> page: every seam name, carrying term, and owning doc must appear
    // on the page.
    let mut all_carrying_terms: BTreeSet<&str> = BTreeSet::new();
    let mut all_owning_docs: BTreeSet<&str> = BTreeSet::new();
    for seam in seams {
        if !table_region.contains(seam.name.as_str()) {
            findings.push(finding(
                Severity::Error,
                codes::AUTHORING_SEAM_REGISTRY_DRIFT,
                format!(
                    "seam-registry drift: seam \"{}\" is declared in the grounding manifests but \
                     does not appear on the generated seam-registry page ({SEAM_REGISTRY_PAGE_PATH})",
                    seam.name
                ),
                None,
            ));
        }
        for term in &seam.carrying_terms {
            all_carrying_terms.insert(term.as_str());
            if !table_region.contains(&format!("`{term}`")) {
                findings.push(finding(
                    Severity::Error,
                    codes::AUTHORING_SEAM_REGISTRY_DRIFT,
                    format!(
                        "seam-registry drift: seam \"{}\"'s carrying term {term} does not appear \
                         on the generated seam-registry page ({SEAM_REGISTRY_PAGE_PATH})",
                        seam.name
                    ),
                    Some(term.clone()),
                ));
            }
        }
        for doc in &seam.owning_docs {
            all_owning_docs.insert(doc.as_str());
            if !table_region.contains(&format!("`{doc}`")) {
                findings.push(finding(
                    Severity::Error,
                    codes::AUTHORING_SEAM_REGISTRY_DRIFT,
                    format!(
                        "seam-registry drift: seam \"{}\"'s owning doc {doc} does not appear on \
                         the generated seam-registry page ({SEAM_REGISTRY_PAGE_PATH})",
                        seam.name
                    ),
                    None,
                ));
            }
        }
    }

    // page -> data: every backtick-wrapped CURIE / `.md` filename in the table
    // region must be backed by the data (no stray/orphan entry on the page).
    for cap in SEAM_PAGE_CARRYING_TERM.captures_iter(table_region) {
        let curie = format!("{}:{}", &cap[1], &cap[2]);
        if !all_carrying_terms.contains(curie.as_str()) {
            findings.push(finding(
                Severity::Error,
                codes::AUTHORING_SEAM_REGISTRY_DRIFT,
                format!(
                    "seam-registry drift: the generated page ({SEAM_REGISTRY_PAGE_PATH}) \
                     references carrying term {curie}, which no gmeow:Seam individual declares"
                ),
                Some(curie),
            ));
        }
    }
    for cap in SEAM_PAGE_OWNING_DOC.captures_iter(table_region) {
        let doc = cap[1].to_string();
        if !all_owning_docs.contains(doc.as_str()) {
            findings.push(finding(
                Severity::Error,
                codes::AUTHORING_SEAM_REGISTRY_DRIFT,
                format!(
                    "seam-registry drift: the generated page ({SEAM_REGISTRY_PAGE_PATH}) \
                     references owning doc {doc}, which no gmeow:Seam individual declares"
                ),
                None,
            ));
        }
    }

    findings.sort_by(|a, b| a.message.cmp(&b.message));
    findings.dedup_by(|a, b| a.message == b.message);
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn ds(ttl: &str) -> Dataset {
        let prefixes = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             @prefix dcterms: <http://purl.org/dc/terms/> .\n\
             @prefix ex: <https://example.org/> .\n";
        Dataset::parse_turtle(format!("{prefixes}{ttl}").as_bytes(), "test").unwrap()
    }

    #[test]
    fn shape_iri_collision_fires_when_one_iri_owns_two_files() {
        let a = ds("ex:PersonShape a sh:NodeShape .");
        let b = ds("ex:PersonShape a sh:NodeShape .\nex:OtherShape a sh:NodeShape .");
        let files = vec![
            (PathBuf::from("shapes/a.ttl"), a),
            (PathBuf::from("shapes/b.ttl"), b),
        ];
        let findings = detect_shape_collisions(&files, Path::new("")).unwrap();
        assert_eq!(findings.len(), 1, "exactly the colliding IRI is reported");
        assert_eq!(findings[0].code, codes::AUTHORING_SHAPE_IRI_COLLISION);
        assert!(findings[0].message.contains("PersonShape"));
        // The non-colliding OtherShape is not flagged.
        assert!(!findings[0].message.contains("OtherShape"));
    }

    #[test]
    fn shape_iri_collision_clean_when_every_iri_is_unique() {
        let a = ds("ex:AShape a sh:NodeShape .");
        let b = ds("ex:BShape a sh:NodeShape .");
        let files = vec![(PathBuf::from("a.ttl"), a), (PathBuf::from("b.ttl"), b)];
        assert!(
            detect_shape_collisions(&files, Path::new(""))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn graft_leak_fires_on_a_norms_iri_in_any_position() {
        // gmeow:Norm as object, gmeow:normIssuer as predicate.
        let d = ds("ex:x a gmeow:Norm ; gmeow:normIssuer ex:issuer .");
        let findings = detect_graft_leaks(&d, "slices/core/rights/module.ttl");
        let codes_seen: Vec<&str> = findings.iter().map(|f| f.code.as_str()).collect();
        assert!(
            findings.len() >= 2,
            "both Norm and normIssuer are flagged: {findings:?}"
        );
        assert!(codes_seen.iter().all(|c| *c == codes::AUTHORING_GRAFT_LEAK));
        assert!(findings.iter().any(|f| f.message.contains("/Norm")));
        assert!(findings.iter().any(|f| f.message.contains("/normIssuer")));
    }

    #[test]
    fn graft_leak_exact_identity_not_substring() {
        // A distinct term whose IRI has a norms term as a prefix must NOT match.
        let d = ds("ex:x <https://blackcatinformatics.ca/gmeow/normIssuerRole> ex:r .");
        assert!(
            detect_graft_leaks(&d, "m.ttl").is_empty(),
            "normIssuerRole must not match normIssuer by substring"
        );
    }

    #[test]
    fn slice_discipline_flags_missing_tier() {
        let d = ds("ex:s a gmeow:Slice ; rdfs:label \"S\"@x-gmeow-english .");
        let findings = detect_slice_discipline(
            &[(PathBuf::from("slices/g/s/manifest.ttl"), d)],
            Path::new(""),
        )
        .unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, codes::SLICE_DISCIPLINE_MISSING_TIER);
    }

    #[test]
    fn slice_discipline_flags_duplicate_iri() {
        let a = ds("ex:dup a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore .");
        let b = ds("ex:dup a gmeow:Slice ; gmeow:sliceTier gmeow:tierExtension .");
        let findings = detect_slice_discipline(
            &[
                (PathBuf::from("slices/core/one/manifest.ttl"), a),
                (PathBuf::from("slices/extensions/two/manifest.ttl"), b),
            ],
            Path::new(""),
        )
        .unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, codes::SLICE_DISCIPLINE_DUPLICATE_IRI);
        assert!(findings[0].message.contains("dup"));
    }

    #[test]
    fn slice_discipline_clean_on_well_formed_unique_tiered_manifests() {
        let a = ds("ex:one a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore .");
        let b = ds("ex:two a gmeow:Slice ; gmeow:sliceTier gmeow:tierExtension .");
        assert!(
            detect_slice_discipline(
                &[
                    (PathBuf::from("slices/core/one/manifest.ttl"), a),
                    (PathBuf::from("slices/extensions/two/manifest.ttl"), b),
                ],
                Path::new(""),
            )
            .unwrap()
            .is_empty()
        );
    }

    // ── R8: grounding-peerage discipline ─────────────────────────────────────
    //
    // `detect_peerage_discipline` computes manifest paths relative to
    // `slices_dir` itself (the live call passes `slices_dir` as `root`), so
    // these tests pass manifest paths WITHOUT a leading `slices/` (unlike the
    // R6 tests above, which pass `root = ""` and full `slices/...` paths) —
    // `grounding/x/manifest.ttl`, matching the real `rel(path, slices_dir)`
    // shape the grounding-marker-drift check keys on.

    #[test]
    fn peerage_discipline_flags_non_grounding_slice_declaring_peerage() {
        let a = ds("ex:one a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore ; \
             gmeow:sliceCoFoundationalWith ex:two .");
        let b = ds("ex:two a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore ; \
             gmeow:sliceCoFoundationalWith ex:one .");
        let findings = detect_peerage_discipline(
            &[
                (PathBuf::from("core/one/manifest.ttl"), a),
                (PathBuf::from("core/two/manifest.ttl"), b),
            ],
            Path::new(""),
        )
        .unwrap();
        let non_grounding: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.code == codes::SLICE_DISCIPLINE_NON_GROUNDING_PEERAGE)
            .collect();
        // Neither `one` nor `two` is typed gmeow:GroundingSlice, and the
        // relation IS mutually symmetric — only the non-grounding-peerage gate
        // fires (twice, once per non-grounding declarer), never the asymmetry
        // gate.
        assert_eq!(
            non_grounding.len(),
            2,
            "both non-grounding peers flagged: {findings:?}"
        );
        assert!(
            findings
                .iter()
                .all(|f| f.code != codes::SLICE_DISCIPLINE_ASYMMETRIC_PEERAGE),
            "a mutually-declared pair must not ALSO fire asymmetric-peerage: {findings:?}"
        );
    }

    #[test]
    fn peerage_discipline_flags_asymmetric_peerage() {
        let a = ds(
            "ex:one a gmeow:Slice, gmeow:GroundingSlice ; gmeow:sliceTier gmeow:tierCore ; \
             gmeow:sliceCoFoundationalWith ex:two .",
        );
        // `two` never declares the relation back to `one`.
        let b = ds("ex:two a gmeow:Slice, gmeow:GroundingSlice ; gmeow:sliceTier gmeow:tierCore .");
        let findings = detect_peerage_discipline(
            &[
                (PathBuf::from("grounding/one/manifest.ttl"), a),
                (PathBuf::from("grounding/two/manifest.ttl"), b),
            ],
            Path::new(""),
        )
        .unwrap();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, codes::SLICE_DISCIPLINE_ASYMMETRIC_PEERAGE);
        assert!(findings[0].message.contains("one"));
        assert!(findings[0].message.contains("two"));
    }

    #[test]
    fn peerage_discipline_flags_grounding_marker_drift_both_directions() {
        // Under grounding/ but NOT typed gmeow:GroundingSlice.
        let untyped_under_grounding = ds("ex:one a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore .");
        // Typed gmeow:GroundingSlice but NOT under grounding/.
        let typed_elsewhere =
            ds("ex:two a gmeow:Slice, gmeow:GroundingSlice ; gmeow:sliceTier gmeow:tierCore .");
        let findings = detect_peerage_discipline(
            &[
                (
                    PathBuf::from("grounding/one/manifest.ttl"),
                    untyped_under_grounding,
                ),
                (PathBuf::from("core/two/manifest.ttl"), typed_elsewhere),
            ],
            Path::new(""),
        )
        .unwrap();
        let drift: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.code == codes::SLICE_DISCIPLINE_GROUNDING_MARKER_DRIFT)
            .collect();
        assert_eq!(drift.len(), 2, "{findings:?}");
        assert!(drift.iter().any(|f| f.message.contains("one")));
        assert!(drift.iter().any(|f| f.message.contains("two")));
    }

    #[test]
    fn peerage_discipline_clean_on_the_real_corpus_shape() {
        // Mirrors the real grounding trio: three GroundingSlice manifests under
        // grounding/, mutually peered.
        let logic = ds(
            "ex:logic a gmeow:Slice, gmeow:GroundingSlice ; gmeow:sliceTier gmeow:tierCore ; \
             gmeow:sliceCoFoundationalWith ex:lang, ex:math .",
        );
        let lang = ds(
            "ex:lang a gmeow:Slice, gmeow:GroundingSlice ; gmeow:sliceTier gmeow:tierCore ; \
             gmeow:sliceCoFoundationalWith ex:logic, ex:math .",
        );
        let math = ds(
            "ex:math a gmeow:Slice, gmeow:GroundingSlice ; gmeow:sliceTier gmeow:tierCore ; \
             gmeow:sliceCoFoundationalWith ex:logic, ex:lang .",
        );
        let core = ds("ex:core a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore .");
        assert!(
            detect_peerage_discipline(
                &[
                    (PathBuf::from("grounding/logic/manifest.ttl"), logic),
                    (PathBuf::from("grounding/lang/manifest.ttl"), lang),
                    (PathBuf::from("grounding/math/manifest.ttl"), math),
                    (PathBuf::from("core/core/manifest.ttl"), core),
                ],
                Path::new(""),
            )
            .unwrap()
            .is_empty()
        );
    }

    fn slice(iri: &str, tier: Option<Tier>) -> SliceRec {
        SliceRec {
            iri: iri.to_string(),
            tier,
            raw_tiers: match tier {
                Some(Tier::Core) => vec![TIER_CORE.to_string()],
                Some(Tier::Extension) => vec![TIER_EXTENSION.to_string()],
                Some(Tier::Profile) => vec![TIER_PROFILE.to_string()],
                None => Vec::new(),
            },
        }
    }

    fn iset(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn profile_closure_clean_on_a_well_formed_partition() {
        let slices = vec![
            slice("g/core-a", Some(Tier::Core)),
            slice("g/ext-a", Some(Tier::Extension)),
            slice("g/prof-a", Some(Tier::Profile)),
        ];
        // full = ontology ∪ {ext}, claims ⊊ core.
        let full = iset(&[ONTOLOGY_IRI, "g/ext-a"]);
        let claims = iset(&[]); // strict subset of {core-a}
        assert!(detect_profile_closure(&slices, &full, &claims).is_empty());
    }

    #[test]
    fn profile_closure_flags_full_missing_an_extension() {
        let slices = vec![
            slice("g/core-a", Some(Tier::Core)),
            slice("g/ext-a", Some(Tier::Extension)),
        ];
        let full = iset(&[ONTOLOGY_IRI]); // missing ext-a
        let claims = iset(&[]);
        let findings = detect_profile_closure(&slices, &full, &claims);
        assert!(findings.iter().any(|f| f.message.contains("full.ttl")));
    }

    #[test]
    fn profile_closure_flags_claims_not_strict_subset() {
        let slices = vec![slice("g/core-a", Some(Tier::Core))];
        let full = iset(&[ONTOLOGY_IRI]);
        // claims == core (not STRICT) → violation.
        let claims = iset(&["g/core-a"]);
        let findings = detect_profile_closure(&slices, &full, &claims);
        assert!(findings.iter().any(|f| f.message.contains("claims.ttl")));
    }

    #[test]
    fn profile_closure_flags_unrecognized_tier_but_not_a_tierless_slice() {
        // A tierless slice is the discipline gate's job — NOT re-reported here.
        // (A real core slice is present so the claims⊊core check is well-formed;
        // claims ⊊ {core-a} holds for the empty claims set.)
        let clean = vec![
            slice("g/core-a", Some(Tier::Core)),
            slice("g/tierless", None),
        ];
        let full = iset(&[ONTOLOGY_IRI]);
        assert!(detect_profile_closure(&clean, &full, &iset(&[])).is_empty());

        // A slice WITH a sliceTier value that is not one of the three IS flagged.
        let bogus = SliceRec {
            iri: "g/bogus".to_string(),
            tier: None,
            raw_tiers: vec!["https://blackcatinformatics.ca/gmeow/tierBogus".to_string()],
        };
        let findings = detect_profile_closure(
            &[slice("g/core-a", Some(Tier::Core)), bogus],
            &full,
            &iset(&[]),
        );
        assert!(findings.iter().any(|f| f.message.contains("unrecognized")));
    }

    #[test]
    fn catalog_names_parse_ignores_comments_and_default_namespace() {
        let xml = "<?xml version=\"1.0\"?>\n\
             <!-- a comment mentioning uri name= that must not be scanned -->\n\
             <catalog xmlns=\"urn:oasis:names:tc:entity:xmlns:xml:catalog\">\n\
               <uri name=\"https://blackcatinformatics.ca/gmeow/slices/temporal\" uri=\"a.ttl\"/>\n\
               <uri name=\"https://blackcatinformatics.ca/gmeow\" uri=\"b.ttl\"/>\n\
             </catalog>";
        let names = parse_catalog_names(xml, Path::new("catalog-v001.xml")).unwrap();
        assert_eq!(names.len(), 2);
        assert!(names.contains("https://blackcatinformatics.ca/gmeow/slices/temporal"));
        // The commented-out text is not harvested.
        assert!(!names.iter().any(|n| n.contains("must not be scanned")));
    }

    #[test]
    fn module_iri_expected_is_the_slice_dir_not_the_group() {
        // The expected IRI derives from the immediate parent dir (slice name),
        // never the grandparent group segment.
        let module = PathBuf::from("slices/core/temporal/module.ttl");
        let slice_dir = module.parent().unwrap().file_name().unwrap();
        assert_eq!(slice_dir, "temporal");
        let expected = format!("{GMEOW_NS}slices/{}", slice_dir.to_string_lossy());
        assert_eq!(
            expected,
            "https://blackcatinformatics.ca/gmeow/slices/temporal"
        );
    }

    #[test]
    fn undeclared_term_fires_on_a_term_absent_from_the_declared_set() {
        let declared: BTreeSet<String> =
            ["https://blackcatinformatics.ca/gmeow/Person".to_string()]
                .into_iter()
                .collect();
        // Uses gmeow:Person (declared) and gmeow:hasBogusProp (undeclared).
        let d = ds("ex:x a gmeow:Person ; gmeow:hasBogusProp ex:y .");
        let findings =
            detect_undeclared_terms(&declared, &[(PathBuf::from("f.ttl"), d)], Path::new(""));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, codes::AUTHORING_UNDECLARED_TERM);
        assert!(findings[0].message.contains("hasBogusProp"));
    }

    #[test]
    fn vocab_terms_exclude_examples_and_modules_iris() {
        let d = ds("gmeow:RealTerm a gmeow:Class .\n\
             <https://blackcatinformatics.ca/gmeow/examples/foo> a gmeow:RealTerm .\n\
             <https://blackcatinformatics.ca/gmeow/modules/bar> a gmeow:RealTerm .");
        let terms = gmeow_vocab_terms(&d);
        assert!(terms.contains("https://blackcatinformatics.ca/gmeow/RealTerm"));
        assert!(!terms.iter().any(|t| t.contains("/examples/")));
        assert!(!terms.iter().any(|t| t.contains("/modules/")));
    }

    #[test]
    fn untagged_localizable_literal_fires_and_tagged_is_clean() {
        // rdfs:label untagged → flagged; skos:definition tagged → clean.
        let bad = ds("ex:x rdfs:label \"plain\" ; skos:definition \"tagged\"@x-gmeow-english .");
        let findings = detect_untagged_localizable(&[(PathBuf::from("m.ttl"), bad)], Path::new(""));
        assert_eq!(findings.len(), 1, "only the untagged label is flagged");
        assert_eq!(
            findings[0].code,
            codes::AUTHORING_UNTAGGED_LOCALIZABLE_LITERAL
        );
        assert!(findings[0].message.contains("label"));
    }

    #[test]
    fn untagged_ignores_non_localizable_predicates() {
        // A plain literal on a NON-localizable predicate is not a translation concern.
        let d = ds("ex:x ex:count \"42\" .");
        assert!(
            detect_untagged_localizable(&[(PathBuf::from("m.ttl"), d)], Path::new("")).is_empty()
        );
    }

    #[test]
    fn docs_markdown_extraction_finds_fenced_and_inline_terms() {
        let md = "# Doc\n\nUse `gmeow:Person` inline.\n\n```turtle\n\
             ex:a a gmeow:Organization ; `gmeow:memberOf` ex:b .\n```\n";
        let terms = extract_gmeow_terms_from_markdown(md);
        assert!(terms.contains("https://blackcatinformatics.ca/gmeow/Person"));
        assert!(terms.contains("https://blackcatinformatics.ca/gmeow/Organization"));
        assert!(terms.contains("https://blackcatinformatics.ca/gmeow/memberOf"));
    }

    /// F3 regression: a MODULE-LESS slice (a `manifest.ttl` with NO `module.ttl` —
    /// the pure-selection profile-slice shape) whose `examples/*.ttl` uses an
    /// undeclared term must still be caught. Discovery keyed on `slice_module_files`
    /// alone (the pre-fix behavior) would silently skip this slice entirely,
    /// because it mints no module and so is invisible to a module-only walk —
    /// exactly the blind spot `slices/profile/agent-runtime` demonstrated live.
    /// Keying on `all_manifests` instead (every slice has a manifest) closes it.
    #[test]
    fn example_undeclared_term_fires_on_a_module_less_slice() {
        let tmp = tempfile::tempdir().expect("temp slices dir");
        let slice_dir = tmp.path().join("profile/no-module-slice");
        std::fs::create_dir_all(slice_dir.join("examples")).unwrap();

        // A manifest declaring gmeow:Slice + a tier — NO module.ttl anywhere in
        // this slice directory (the module-less pure-selection shape).
        std::fs::write(
            slice_dir.join("manifest.ttl"),
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <https://blackcatinformatics.ca/gmeow/slices/no-module-slice> a gmeow:Slice ;\n\
               gmeow:sliceTier gmeow:tierProfile ;\n\
               rdfs:label \"no-module-slice\"@x-gmeow-english .\n",
        )
        .unwrap();
        assert!(
            !slice_dir.join("module.ttl").exists(),
            "the fixture must genuinely be module-less"
        );

        // The example references an undeclared GMEOW term.
        std::fs::write(
            slice_dir.join("examples/bad.ttl"),
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix ex: <https://blackcatinformatics.ca/gmeow/examples/no-module-slice/> .\n\
             ex:thing gmeow:totallyBogusUndeclaredPredicateXYZ ex:other .\n",
        )
        .unwrap();

        let declared: BTreeSet<String> = BTreeSet::new();
        let files = load_ttl_files(&slice_example_files(tmp.path()).unwrap()).unwrap();
        let findings = detect_undeclared_terms(&declared, &files, tmp.path());
        assert_eq!(
            findings.len(),
            1,
            "the module-less slice's example must be discovered and its undeclared \
             term flagged: {findings:?}"
        );
        assert_eq!(findings[0].code, codes::AUTHORING_UNDECLARED_TERM);
        assert!(
            findings[0]
                .message
                .contains("totallyBogusUndeclaredPredicateXYZ")
        );
    }

    #[test]
    fn docs_term_absent_from_every_allowlist_source_is_flagged() {
        // The firing negative for R3e: a fenced turtle term in no allowlist source.
        let md = "```turtle\nex:a `gmeow:TotallyUndeclaredXyz` ex:b .\n```";
        let terms = extract_gmeow_terms_from_markdown(md);
        let allow: BTreeSet<String> = BTreeSet::new();
        let unallowed: Vec<&String> = terms.iter().filter(|t| !allow.contains(*t)).collect();
        assert!(
            unallowed
                .iter()
                .any(|t| t.ends_with("/TotallyUndeclaredXyz")),
            "an unallowlisted docs term must be flagged: {unallowed:?}"
        );
    }

    // ── R7: grounding seam-registry drift ────────────────────────────────────

    fn sample_seam_manifest() -> Dataset {
        ds(
            "<https://blackcatinformatics.ca/gmeow/slices/logic> a gmeow:Slice, gmeow:GroundingSlice .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
            <https://blackcatinformatics.ca/gmeow/seam/denotation>\n\
                a gmeow:Seam ;\n\
                rdfs:label \"Denotation seam\"@x-gmeow-english ;\n\
                gmeow:seamDirection [\n\
                    gmeow:seamFromSlice <https://blackcatinformatics.ca/gmeow/slices/lang> ;\n\
                    gmeow:seamToSlice <https://blackcatinformatics.ca/gmeow/slices/logic>\n\
                ] ;\n\
                gmeow:seamCarryingTerm lang:denotationTarget , lang:denotationKind ;\n\
                gmeow:seamOwningDoc \"LANG-MEANING.md\" .\n",
        )
    }

    #[test]
    fn seam_records_of_reads_label_terms_and_docs() {
        let d = sample_seam_manifest();
        let records = seam_records_of(&d, Path::new("manifest.ttl")).unwrap();
        assert_eq!(records.len(), 1);
        let seam = &records[0];
        assert_eq!(seam.name, "Denotation seam");
        assert_eq!(
            seam.carrying_terms,
            BTreeSet::from([
                "lang:denotationKind".to_string(),
                "lang:denotationTarget".to_string(),
            ])
        );
        assert_eq!(
            seam.owning_docs,
            BTreeSet::from(["LANG-MEANING.md".to_string()])
        );
    }

    #[test]
    fn seam_records_of_ignores_a_non_grounding_slice_manifest() {
        // A gmeow:Seam authored on a slice NOT typed gmeow:GroundingSlice must not
        // be picked up (mirrors gmeow_docs::model::is_grounding_slice's gate).
        let d = ds(
            "<https://blackcatinformatics.ca/gmeow/slices/plain> a gmeow:Slice .\n\
                    <https://blackcatinformatics.ca/gmeow/seam/rogue> a gmeow:Seam ; rdfs:label \"Rogue\"@x-gmeow-english .\n",
        );
        let records = seam_records_of(&d, Path::new("manifest.ttl")).unwrap();
        assert!(records.is_empty());
    }

    fn matching_page_text() -> String {
        "# Grounding seams\n\n\
         Some intro prose naming `gmeow:sliceCoFoundationalWith` (never a carrying term).\n\n\
         | Seam | Direction | Carrying terms | Owning doc |\n\
         | --- | --- | --- | --- |\n\
         | **Denotation seam** | lang → logic | `lang:denotationKind`, `lang:denotationTarget` | `LANG-MEANING.md` |\n\n\
         ## Definitions\n\n\
         ### Denotation seam\n\n\
         The lang -> logic seam.\n"
            .to_string()
    }

    #[test]
    fn detect_seam_registry_drift_is_clean_when_page_matches_data() {
        let d = sample_seam_manifest();
        let seams = seam_records_of(&d, Path::new("manifest.ttl")).unwrap();
        let findings = detect_seam_registry_drift(&seams, &matching_page_text());
        assert!(
            findings.is_empty(),
            "a page that carries every seam/term/doc must not drift: {findings:?}"
        );
    }

    #[test]
    fn detect_seam_registry_drift_fires_when_a_carrying_term_is_missing_from_the_page() {
        let d = sample_seam_manifest();
        let seams = seam_records_of(&d, Path::new("manifest.ttl")).unwrap();
        // Drop denotationTarget from the rendered page.
        let page = matching_page_text().replace("`lang:denotationTarget`", "");
        let findings = detect_seam_registry_drift(&seams, &page);
        assert!(
            findings
                .iter()
                .any(|f| f.code == codes::AUTHORING_SEAM_REGISTRY_DRIFT
                    && f.message.contains("lang:denotationTarget")),
            "a data-side carrying term missing from the page must be flagged: {findings:?}"
        );
    }

    #[test]
    fn detect_seam_registry_drift_fires_on_an_orphan_page_term() {
        let d = sample_seam_manifest();
        let seams = seam_records_of(&d, Path::new("manifest.ttl")).unwrap();
        // The page claims an extra carrying term the data never declares.
        let page = matching_page_text().replace(
            "`lang:denotationTarget`",
            "`lang:denotationTarget`, `logic:NotARealCarryingTerm`",
        );
        let findings = detect_seam_registry_drift(&seams, &page);
        assert!(
            findings
                .iter()
                .any(|f| f.code == codes::AUTHORING_SEAM_REGISTRY_DRIFT
                    && f.message.contains("logic:NotARealCarryingTerm")),
            "a page-side term unbacked by data must be flagged: {findings:?}"
        );
    }

    #[test]
    fn detect_seam_registry_drift_fires_when_an_owning_doc_is_missing() {
        let d = sample_seam_manifest();
        let seams = seam_records_of(&d, Path::new("manifest.ttl")).unwrap();
        let page = matching_page_text().replace("`LANG-MEANING.md`", "(no doc)");
        let findings = detect_seam_registry_drift(&seams, &page);
        assert!(
            findings
                .iter()
                .any(|f| f.code == codes::AUTHORING_SEAM_REGISTRY_DRIFT
                    && f.message.contains("LANG-MEANING.md")),
            "a data-side owning doc missing from the page must be flagged: {findings:?}"
        );
    }

    #[test]
    fn seam_registry_drift_findings_is_a_noop_when_the_page_is_absent() {
        // ontology-docs/ is an on-demand `SYNC_OUTPUTS=docs` output; when it has
        // never been synced this gate must not hard-fail (a cache miss, not drift).
        let tmp = tempfile::tempdir().expect("temp project root");
        let slices_dir = tmp.path().join("slices");
        let slice_dir = slices_dir.join("grounding/logic");
        std::fs::create_dir_all(&slice_dir).unwrap();
        std::fs::write(
            slice_dir.join("manifest.ttl"),
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <https://blackcatinformatics.ca/gmeow/slices/logic> a gmeow:Slice, gmeow:GroundingSlice ;\n\
               gmeow:sliceTier gmeow:tierCore ;\n\
               rdfs:label \"logic\"@x-gmeow-english .\n",
        )
        .unwrap();
        assert!(!tmp.path().join(SEAM_REGISTRY_PAGE_PATH).exists());

        let findings = seam_registry_drift_findings(tmp.path(), &slices_dir).unwrap();
        assert!(
            findings.is_empty(),
            "an unsynced docs projection must not fail make validate: {findings:?}"
        );
    }
}
