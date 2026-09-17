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

use std::collections::{BTreeMap, BTreeSet};
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
use crate::slice_peerage::{GMEOW_CO_FOUNDATIONAL_WITH, GMEOW_GROUNDING_SLICE};
/// The `slices/` subdirectory every grounding slice's manifest must live under.
const GROUNDING_GROUP_PREFIX: &str = "grounding/";

/// The core `rights` module, parsed in isolation for the graft-isolation gate.
const CORE_RIGHTS_MODULE: &str = "slices/core/rights/module.ttl";

/// The norms-slice IRIs the core `rights` module must never reference (in any
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
/// A retired `owl:` authoring prefix token — a prefixed name (`owl:Foo`) or an
/// `@prefix owl:` declaration — at a name/prefix boundary. The leading
/// `(?:^|[^0-9A-Za-z_-])` excludes a longer prefix (`powl:` / `owlish`), and a
/// full IRI's `owl#` form never matches because `owl` is not followed by a colon
/// there. Matched per source LINE, so `^` anchors each line's start.
static RETIRED_OWL_PREFIX_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[^0-9A-Za-z_-])owl:").expect("valid static regex"));

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
    Dataset::parse_turtle(&bytes, None, &path.display().to_string())
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
/// Before folding any detector, the production path checks non-vacuity floors for
/// the merged shape-file set, declared-term set, and catalog `<uri>` set. Keeping
/// the floor here makes the explicit validator producer fail closed if an
/// environmental fault silently shrinks its input corpus; tests exercise the
/// detectors with synthetic inputs and never rebuild the repository corpus.
fn require_non_vacuous_corpus(
    project_root: &Path,
    declared: &BTreeSet<String>,
    docs_terms: &BTreeSet<String>,
) -> Result<()> {
    let shape_files = purrdf::shapes::shape_union::shape_files(project_root)
        .map_err(|e| Diag::of_kind(crate::error::Io { detail: e }))?;
    if shape_files.is_empty() {
        return Err(Diag::of_kind(crate::error::Io {
            detail: "authoring-integrity: merged shape-file corpus floor 1 not met (got 0 \
                      files) — corpus read is vacuous, refusing to pass"
                .to_string(),
        }));
    }

    if declared.len() <= 50 {
        return Err(Diag::of_kind(crate::error::Io {
            detail: format!(
                "authoring-integrity: declared ontology terms floor 50 not met (got {}) — \
                 corpus read is vacuous, refusing to pass",
                declared.len()
            ),
        }));
    }

    if docs_terms.is_empty() {
        return Err(Diag::of_kind(crate::error::Io {
            detail: "authoring-integrity: docs gmeow: term-reference floor 1 not met (got 0) \
                      — the per-document authority comparison would be vacuous, refusing to pass"
                .to_string(),
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

    // The R7 seam-registry drift floor: a comparison against ZERO `gmeow:Seam`
    // individuals certifies nothing, so a corpus whose grounding manifests declare
    // no seam at all is refused rather than passed. Read off `project_root`'s own
    // slice tree (like every other floor here), independently of the `slices_dir`
    // the detectors are pointed at.
    let seams = seam_registry_of_slices(&project_root.join("slices"))?;
    if seams.is_empty() {
        return Err(Diag::of_kind(crate::error::Io {
            detail: format!(
                "authoring-integrity: gmeow:Seam registry floor 1 not met (got 0) under {} — \
                 the seam-registry drift comparison would be vacuous, refusing to pass",
                project_root.join("slices").display(),
            ),
        }));
    }

    Ok(())
}

pub fn authoring_integrity_findings(
    project_root: &Path,
    slices_dir: &Path,
) -> Result<Vec<Finding>> {
    let term_corpus = docs_term_corpus(project_root)?;
    let docs = read_docs_documents(project_root)?;
    let docs_terms = docs_gmeow_terms_from_documents(&docs);
    require_non_vacuous_corpus(project_root, &term_corpus.declared, &docs_terms)?;
    let mut findings = shape_iri_collision_findings(project_root)?;
    findings.extend(graft_isolation_findings(project_root)?);
    findings.extend(slice_discipline_findings(slices_dir)?);
    findings.extend(peerage_discipline_findings(slices_dir)?);
    findings.extend(registered_minting_namespace_findings(slices_dir)?);
    findings.extend(profile_closure_findings(project_root)?);
    findings.extend(catalog_closure_findings(project_root)?);
    findings.extend(module_iri_findings(project_root)?);
    findings.extend(docs_undeclared_findings_from_documents(
        project_root,
        &docs,
        &term_corpus.ownership,
    )?);
    findings.extend(example_undeclared_term_findings(
        project_root,
        &term_corpus.declared,
    )?);
    findings.extend(slice_source_untagged_findings(project_root)?);
    findings.extend(nonslice_authored_untagged_findings(project_root)?);
    findings.extend(seam_registry_drift_findings(project_root, slices_dir)?);
    findings.extend(retired_authoring_prefix_findings(project_root, slices_dir)?);
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

/// The core `rights` module must reference zero norms-slice IRIs (the retired
/// `test_graft_axioms_live_extension_side_only`): the graft lives on the extension
/// side only, with zero core churn.
pub fn graft_isolation_findings(repo_root: &Path) -> Result<Vec<Finding>> {
    let path = repo_root.join(CORE_RIGHTS_MODULE);
    let ds = parse_ttl(&path)?;
    Ok(detect_graft_leaks(&ds, &rel(&path, repo_root)))
}

/// The pure graft-leak logic: any norms-slice IRI appearing in subject,
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
                    "core module {source_label} references norms-slice IRI {term} (in {pos}) \
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
const LOGIC_ONTOLOGY: &str = "https://blackcatinformatics.ca/logic/Ontology";
const OWL_IMPORTS: &str = "http://www.w3.org/2002/07/owl#imports";
const ONTOLOGY_IRI: &str = "https://blackcatinformatics.ca/gmeow";
use gmeow_ns::GMEOW_NS;
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
    // A slice `module.ttl` authors its header as `<> a logic:Ontology` after the
    // flip; accept the `owl:` spelling too so the check bites for both authorings.
    let mut subjects = ds
        .subjects_of_type(LOGIC_ONTOLOGY)
        .map_err(|e| parse_err(path, &e.to_string()))?;
    subjects.extend(
        ds.subjects_of_type(OWL_ONTOLOGY)
            .map_err(|e| parse_err(path, &e.to_string()))?,
    );
    subjects.sort();
    subjects.dedup();
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
    vocabulary_declared_terms(repo_root)
}

/// Declared GMEOW vocabulary terms parsed from the merged `vocabulary_source_files`
/// alone. This is the exact corpus behind the public [`declared_ontology_terms`]: its
/// result and failure surface depend only on the vocabulary sources, never on the
/// broader docs-authority sources that [`docs_term_corpus`] additionally parses for
/// ownership.
fn vocabulary_declared_terms(repo_root: &Path) -> Result<BTreeSet<String>> {
    use purrdf::slice::rdf_query::DatasetAccumulator;
    let mut acc = DatasetAccumulator::new();
    for source in vocabulary_source_files(repo_root)? {
        let bytes = std::fs::read(&source).map_err(|e| io_err(&source, &e))?;
        acc.add_turtle(&bytes, None, &source.display().to_string())
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

/// The canonical per-document declaration/import authority for `docs/*.md` examples.
const DOCS_TERM_AUTHORITY_PATH: &str = "docs/term-authority.toml";

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

/// Read the sorted top-level documentation corpus once for all docs authority checks.
fn read_docs_documents(repo_root: &Path) -> Result<Vec<(PathBuf, String)>> {
    docs_md_files(repo_root)?
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).map_err(|e| io_err(&path, &e))?;
            Ok((path, text))
        })
        .collect()
}

/// Collect every referenced GMEOW term from an already-loaded documentation corpus.
fn docs_gmeow_terms_from_documents(docs: &[(PathBuf, String)]) -> BTreeSet<String> {
    docs.iter()
        .flat_map(|(_, text)| extract_gmeow_terms_from_markdown(text))
        .collect()
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DocsAuthorityFile {
    version: u32,
    document: Vec<DocsAuthorityEntry>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DocsAuthorityEntry {
    path: String,
    imports: Vec<String>,
    declares: Vec<String>,
    #[serde(default)]
    resolves: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DocsDocumentAuthority {
    imports: BTreeSet<String>,
    declares: BTreeSet<String>,
    resolves: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DocsTermOwnership {
    by_term: BTreeMap<String, BTreeSet<String>>,
    owners: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DocsTermCorpus {
    declared: BTreeSet<String>,
    ownership: DocsTermOwnership,
}

/// Canonical authored sources that may own a documentation-visible GMEOW term.
/// Generated projections are deliberately absent: authority comes from a typed
/// authored subject and its explicit `rdfs:isDefinedBy`, never from a generated
/// occurrence or the source filename. Slice shapes are discovered from manifests,
/// so a module-less pure-shape slice remains an authority source.
fn docs_authority_source_files(repo_root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = vocabulary_source_files(repo_root)?;
    for manifest in all_manifests(&repo_root.join("slices"))? {
        if let Some(dir) = manifest.parent() {
            let shapes = dir.join("shapes.ttl");
            if shapes.is_file() {
                files.push(shapes);
            }
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

/// Derive the declared-term set (from the vocabulary sources, via
/// [`vocabulary_declared_terms`]) and `term -> exact owning IRIs` (from the authored
/// authority sources, parsed once here). A small number of terms are deliberately
/// declared by more than one authored vocabulary; retaining the exact set lets a
/// document import the authority whose declaration it uses without inventing a
/// repository-global winner.
fn docs_term_corpus(repo_root: &Path) -> Result<DocsTermCorpus> {
    let declared = vocabulary_declared_terms(repo_root)?;
    let mut typed_terms = BTreeSet::new();
    let mut assertions = Vec::new();
    for source in docs_authority_source_files(repo_root)? {
        let dataset = parse_ttl(&source)?;
        dataset.for_each_quad(|subject, predicate, object, _graph| {
            if predicate == RDF_TYPE
                && let Subject::Named(term) = &subject
                && term.starts_with(GMEOW_NS)
                && matches!(object, Object::Named(_))
            {
                typed_terms.insert(term.clone());
            }
            if predicate == RDFS_IS_DEFINED_BY
                && let Subject::Named(term) = &subject
            {
                assertions.push((
                    term.clone(),
                    match object {
                        Object::Named(owner) => Some(owner.clone()),
                        _ => None,
                    },
                    source.clone(),
                ));
            }
        });
    }
    let mut candidates: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut malformed = BTreeSet::new();
    for (term, owner, source) in assertions {
        if !typed_terms.contains(&term) {
            continue;
        }
        match owner {
            Some(owner) => {
                candidates.entry(term).or_default().insert(owner);
            }
            None => {
                malformed.insert((term, source));
            }
        }
    }
    if let Some((term, source)) = malformed.into_iter().next() {
        return Err(parse_err(
            &source,
            &format!("typed GMEOW term <{term}> has a non-IRI rdfs:isDefinedBy authority"),
        ));
    }

    let mut ownership = DocsTermOwnership::default();
    for (term, owners) in candidates {
        ownership.owners.extend(owners.iter().cloned());
        ownership.by_term.insert(term, owners);
    }
    Ok(DocsTermCorpus {
        declared,
        ownership,
    })
}

/// Discover exact documentation term ownership from the shared authored corpus.
fn docs_term_ownership(repo_root: &Path) -> Result<DocsTermOwnership> {
    Ok(docs_term_corpus(repo_root)?.ownership)
}

/// Return the first repeated manifest value in deterministic input order.
fn duplicate_value(values: &[String]) -> Option<&str> {
    let mut seen = BTreeSet::new();
    values
        .iter()
        .find(|value| !seen.insert(value.as_str()))
        .map(String::as_str)
}

/// Parse and validate the authority manifest against the discovered document paths and
/// term-owner index. This is deliberately strict: stale paths, duplicate entries,
/// unknown imports, and attempts to redeclare ontology-owned terms all hard-fail.
fn parse_docs_authority_text(
    manifest_path: &Path,
    text: &str,
    docs: &[PathBuf],
    repo_root: &Path,
    ownership: &DocsTermOwnership,
) -> Result<BTreeMap<String, DocsDocumentAuthority>> {
    let parsed: DocsAuthorityFile =
        toml::from_str(text).map_err(|e| parse_err(manifest_path, &e.to_string()))?;
    if parsed.version != 1 {
        return Err(parse_err(
            manifest_path,
            &format!(
                "unsupported docs term-authority version {}; expected 1",
                parsed.version
            ),
        ));
    }
    let known_docs: BTreeSet<String> = docs.iter().map(|path| rel(path, repo_root)).collect();
    let mut authorities = BTreeMap::new();
    for entry in parsed.document {
        if !known_docs.contains(&entry.path) {
            return Err(parse_err(
                manifest_path,
                &format!(
                    "term-authority entry path `{}` is not an existing top-level docs/*.md file",
                    entry.path
                ),
            ));
        }
        if authorities.contains_key(&entry.path) {
            return Err(parse_err(
                manifest_path,
                &format!("duplicate term-authority entry for `{}`", entry.path),
            ));
        }
        if let Some(value) = duplicate_value(&entry.imports) {
            return Err(parse_err(
                manifest_path,
                &format!(
                    "`{}` imports authority <{value}> more than once",
                    entry.path
                ),
            ));
        }
        if let Some(value) = duplicate_value(&entry.declares) {
            return Err(parse_err(
                manifest_path,
                &format!("`{}` declares term <{value}> more than once", entry.path),
            ));
        }
        for owner in &entry.imports {
            if !ownership.owners.contains(owner) {
                return Err(parse_err(
                    manifest_path,
                    &format!(
                        "`{}` imports unknown term authority <{owner}>; imports must name an \
                         exact authored rdfs:isDefinedBy owner",
                        entry.path
                    ),
                ));
            }
        }
        for term in &entry.declares {
            if !term.starts_with(GMEOW_NS) {
                return Err(parse_err(
                    manifest_path,
                    &format!(
                        "`{}` declares non-GMEOW term <{term}>; document-local declarations \
                         must be full `{GMEOW_NS}` IRIs",
                        entry.path
                    ),
                ));
            }
            if let Some(owners) = ownership.by_term.get(term) {
                return Err(parse_err(
                    manifest_path,
                    &format!(
                        "`{}` declares ontology-owned term <{term}>; import one of its exact \
                         owners ({}) instead",
                        entry.path,
                        owners
                            .iter()
                            .map(|owner| format!("<{owner}>"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
        }
        for (term, selected_owner) in &entry.resolves {
            if !term.starts_with(GMEOW_NS) {
                return Err(parse_err(
                    manifest_path,
                    &format!(
                        "`{}` resolves non-GMEOW term <{term}>; resolution keys must be full \
                         `{GMEOW_NS}` IRIs",
                        entry.path
                    ),
                ));
            }
            let Some(owners) = ownership.by_term.get(term) else {
                return Err(parse_err(
                    manifest_path,
                    &format!(
                        "`{}` resolves term <{term}> with no authored rdfs:isDefinedBy owner",
                        entry.path
                    ),
                ));
            };
            if !owners.contains(selected_owner) {
                return Err(parse_err(
                    manifest_path,
                    &format!(
                        "`{}` resolves term <{term}> to <{selected_owner}>, which is not one of \
                         its exact authored owners ({})",
                        entry.path,
                        owners
                            .iter()
                            .map(|owner| format!("<{owner}>"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
            if !entry.imports.contains(selected_owner) {
                return Err(parse_err(
                    manifest_path,
                    &format!(
                        "`{}` resolves term <{term}> to <{selected_owner}>, but does not import \
                         that exact owner",
                        entry.path
                    ),
                ));
            }
            let imported_matches = owners
                .iter()
                .filter(|owner| entry.imports.contains(owner))
                .count();
            if imported_matches < 2 {
                return Err(parse_err(
                    manifest_path,
                    &format!(
                        "`{}` redundantly resolves term <{term}>; its imports select only one \
                         matching owner",
                        entry.path
                    ),
                ));
            }
        }
        authorities.insert(
            entry.path,
            DocsDocumentAuthority {
                imports: entry.imports.into_iter().collect(),
                declares: entry.declares.into_iter().collect(),
                resolves: entry.resolves,
            },
        );
    }
    Ok(authorities)
}

/// Parse the authority manifest against the already-discovered docs and owners.
fn docs_authorities(
    repo_root: &Path,
    docs: &[PathBuf],
    ownership: &DocsTermOwnership,
) -> Result<BTreeMap<String, DocsDocumentAuthority>> {
    let path = repo_root.join(DOCS_TERM_AUTHORITY_PATH);
    let text = std::fs::read_to_string(&path).map_err(|e| io_err(&path, &e))?;
    parse_docs_authority_text(&path, &text, docs, repo_root, ownership)
}

/// Every `gmeow:` term referenced across the docs corpus — exposed so the gate's
/// non-vacuity guard can prove the fence/inline extractor is not silently
/// yielding an empty set (a broken regex would otherwise pass vacuously).
pub fn docs_gmeow_terms(repo_root: &Path) -> Result<BTreeSet<String>> {
    Ok(docs_gmeow_terms_from_documents(&read_docs_documents(
        repo_root,
    )?))
}

/// Compare an already-loaded docs corpus with its exact local authority map.
/// A multiply-owned term is resolved only when the document imports exactly one
/// of its owners or explicitly selects one imported owner: importing none is
/// undeclared, and importing several without a valid selection is ambiguous.
fn detect_docs_undeclared_terms(
    docs: &[(PathBuf, String)],
    authorities: &BTreeMap<String, DocsDocumentAuthority>,
    ownership: &DocsTermOwnership,
    repo_root: &Path,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (path, text) in docs {
        let terms = extract_gmeow_terms_from_markdown(text);
        if terms.is_empty() {
            continue;
        }
        let file = rel(path, repo_root);
        let Some(authority) = authorities.get(&file) else {
            findings.push(finding(
                Severity::Error,
                codes::AUTHORING_UNDECLARED_TERM,
                format!(
                    "docs example {file} references GMEOW terms but has no per-document entry \
                     in {DOCS_TERM_AUTHORITY_PATH}"
                ),
                None,
            ));
            continue;
        };
        for term in terms {
            if authority.declares.contains(&term) {
                continue;
            }
            let message = match ownership.by_term.get(&term) {
                Some(owners) => {
                    let imported: Vec<&String> = owners.intersection(&authority.imports).collect();
                    match imported.as_slice() {
                        [_] => continue,
                        [] => format!(
                            "docs example {file} references GMEOW term {term} owned by {}, but \
                             none of those exact authorities is imported in \
                             {DOCS_TERM_AUTHORITY_PATH}",
                            owners
                                .iter()
                                .map(|owner| format!("<{owner}>"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        _ if authority
                            .resolves
                            .get(&term)
                            .is_some_and(|selected| imported.contains(&selected)) =>
                        {
                            continue;
                        }
                        _ => format!(
                            "docs example {file} references GMEOW term {term} ambiguously through \
                             multiple imported authorities {}; select its exact owner with \
                             `resolves` in {DOCS_TERM_AUTHORITY_PATH}",
                            imported
                                .iter()
                                .map(|owner| format!("<{owner}>"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    }
                }
                None => format!(
                    "docs example {file} references GMEOW term {term} with no authored \
                     rdfs:isDefinedBy owner and no document-local declaration in \
                     {DOCS_TERM_AUTHORITY_PATH}"
                ),
            };
            findings.push(finding(
                Severity::Error,
                codes::AUTHORING_UNDECLARED_TERM,
                message,
                Some(term),
            ));
        }
    }
    findings.sort_by(|a, b| a.message.cmp(&b.message));
    findings
}

/// Evaluate docs authority from an already-loaded corpus, parsing each ownership source once.
fn docs_undeclared_findings_from_documents(
    repo_root: &Path,
    docs: &[(PathBuf, String)],
    ownership: &DocsTermOwnership,
) -> Result<Vec<Finding>> {
    let paths: Vec<PathBuf> = docs.iter().map(|(path, _)| path.clone()).collect();
    let authorities = docs_authorities(repo_root, &paths, ownership)?;
    Ok(detect_docs_undeclared_terms(
        docs,
        &authorities,
        ownership,
        repo_root,
    ))
}

/// R3e: every user-copyable `gmeow:` reference is authorized by its own document's
/// explicit import or declaration. There is no repository-global or transitive fallback.
pub fn docs_undeclared_findings(repo_root: &Path) -> Result<Vec<Finding>> {
    let docs = read_docs_documents(repo_root)?;
    let ownership = docs_term_ownership(repo_root)?;
    docs_undeclared_findings_from_documents(repo_root, &docs, &ownership)
}

// ── R9: registered minting namespaces ────────────────────────────────────────
//
// purrdf's ownership analyzer decides "which slice owns this term" by testing the
// TERM's own IRI against the namespaces the consumer declared
// (`purrdf::SliceVocab::owns_term`, fed from `gmeow_ns::TERM_NAMESPACES`). A slice
// that mints into a namespace GMEOW never registered is therefore INVISIBLE to
// ownership analysis: its `rdfs:isDefinedBy` claims and its typed vocabulary terms
// are dropped, every cross-slice reference to those terms resolves to no owner, and
// no dependency edge is ever computable. Nothing reports it — the analysis simply
// reports that the slice has no dependents, which is indistinguishable from a slice
// nothing uses.
//
// This gate closes that hole at authoring time, on the SAME predicate the analyzer
// uses, so "a slice may mint here" and "the analyzer can see terms minted here" are
// one fact rather than two that can drift apart.
//
// **What counts as a MINTED TERM.** The gate keys on a VOCABULARY-TERM subject —
// the exact trigger `inspect_rdf_dataset` uses for its `declared_terms` set — and
// then requires that the term be GMEOW's own:
//
//   1. TERM: the subject is typed by one of purrdf's vocabulary-term types
//      ([`VOCAB_TERM_TYPES`]). This is the T-Box mint, and it is the only kind of
//      subject the owner-of-term join resolves a cross-slice reference against.
//   2. GMEOW's: the IRI is under [`gmeow_ns::GMEOW_AUTHORITY`], OR the subject
//      asserts `rdfs:isDefinedBy` at a GMEOW slice IRI.
//
// Both conditions keep the gate honest in the other direction:
//
// * a module that redeclares a FOREIGN term so it validates locally
//   (`dcterms:created a owl:AnnotationProperty`, `ontolex:LexicalEntry a
//   owl:Class`) is describing someone else's vocabulary, not minting: purrdf
//   never treats those as owned either, so flagging them would be a false report
//   about a term GMEOW does not own;
// * a subject that merely APPEARS in a module (a `bfo:` IRI carried by a
//   grounding correspondence) claims nothing and is likewise not gated;
// * an A-BOX INDIVIDUAL is not a term. `slices/core/affect` deliberately mints
//   external classifier-label identities under a separate authority path
//   (`gmeow-registry/…`) so they never occupy the canonical `gmeow:` emotion
//   namespace; they are typed `gmeow:AffectLabelSet`, not `owl:Class`, so purrdf
//   never records them as declared vocabulary and neither does this gate. The
//   ontology-term namespace discipline is about the vocabulary, not about
//   instance identity.

/// The `rdf:type` objects purrdf treats as a vocabulary-term declaration
/// (`purrdf::slice::ownership`'s private `VOCAB_TERM_TYPES`). Mirrored here
/// because the gate must key on EXACTLY the analyzer's notion of a declared term;
/// a divergence in either direction would make the gate lie.
const VOCAB_TERM_TYPES: &[&str] = &[
    "http://www.w3.org/2002/07/owl#Class",
    "http://www.w3.org/2002/07/owl#ObjectProperty",
    "http://www.w3.org/2002/07/owl#DatatypeProperty",
    "http://www.w3.org/2002/07/owl#AnnotationProperty",
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property",
    "http://www.w3.org/2000/01/rdf-schema#Class",
    "http://www.w3.org/2000/01/rdf-schema#Datatype",
];

/// `rdf:type`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdfs:isDefinedBy` — the explicit ownership claim.
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";

/// The IRI prefix every GMEOW slice IRI carries (`gmeow:slices/<group>/<name>`).
/// An `rdfs:isDefinedBy` whose object starts here is a claim of GMEOW ownership.
fn slice_iri_prefix() -> String {
    format!("{GMEOW_NS}slices/")
}

/// R9: every term a slice CLAIMS in its `module.ttl` / `shapes.ttl` must be minted
/// into one of the registered term namespaces ([`gmeow_ns::TERM_NAMESPACES`]).
///
/// Discovered by MANIFEST, so a slice's authored surface is covered whether or not
/// it ships a `module.ttl` (a pure-shape slice is gated too).
pub fn registered_minting_namespace_findings(slices_dir: &Path) -> Result<Vec<Finding>> {
    let mut paths = Vec::new();
    for manifest in all_manifests(slices_dir)? {
        let Some(dir) = manifest.parent() else {
            continue;
        };
        for authored in ["module.ttl", "shapes.ttl"] {
            let p = dir.join(authored);
            if p.is_file() {
                paths.push(p);
            }
        }
    }
    paths.sort();
    let files = load_ttl_files(&paths)?;
    Ok(detect_unregistered_minting(&files, slices_dir))
}

/// The pure R9 logic over already-parsed authored files.
///
/// Each offending subject is reported ONCE per file with the reason it was
/// treated as a claimed term, so the message says what to do: register the
/// namespace in `gmeow_ns::TERM_NAMESPACES`, or mint the term inside an existing
/// registered namespace.
fn detect_unregistered_minting(files: &[(PathBuf, Dataset)], root: &Path) -> Vec<Finding> {
    let slice_prefix = slice_iri_prefix();
    let term_types: BTreeSet<&str> = VOCAB_TERM_TYPES.iter().copied().collect();
    let mut findings = Vec::new();

    for (path, ds) in files {
        // Subjects typed as a vocabulary term — the trigger, mirroring purrdf's
        // `declared_terms`. Only these can be a mint.
        let mut declared_terms: BTreeSet<String> = BTreeSet::new();
        // Subjects asserting GMEOW ownership: qualifies a foreign-authority IRI
        // as GMEOW's own, and is named in the message when present.
        let mut gmeow_owned: BTreeSet<String> = BTreeSet::new();
        ds.for_each_quad(|s, p, o, _g| {
            let Subject::Named(subject) = &s else {
                return;
            };
            if gmeow_ns::registered_term_namespace(subject).is_some() {
                return;
            }
            match (p, &o) {
                (RDF_TYPE, Object::Named(class)) if term_types.contains(class.as_str()) => {
                    declared_terms.insert(subject.clone());
                }
                (RDFS_IS_DEFINED_BY, Object::Named(owner)) if owner.starts_with(&slice_prefix) => {
                    gmeow_owned.insert(subject.clone());
                }
                _ => {}
            }
        });

        // Keep only GMEOW's OWN terms: minted under GMEOW's IRI authority, or
        // claiming GMEOW ownership outright. A foreign term redeclared locally is
        // neither, and purrdf never treats it as owned either.
        let claimed: BTreeMap<String, BTreeSet<&'static str>> = declared_terms
            .into_iter()
            .filter(|subject| {
                subject.starts_with(gmeow_ns::GMEOW_AUTHORITY) || gmeow_owned.contains(subject)
            })
            .map(|subject| {
                let mut reasons = BTreeSet::from(["declared as a vocabulary term"]);
                if gmeow_owned.contains(&subject) {
                    reasons.insert("claims rdfs:isDefinedBy a GMEOW slice");
                }
                (subject, reasons)
            })
            .collect();

        for (subject, reasons) in claimed {
            findings.push(finding(
                Severity::Error,
                codes::AUTHORING_UNREGISTERED_TERM_NAMESPACE,
                format!(
                    "{file} mints {subject} outside every registered term namespace ({reason}) — \
                     purrdf's ownership analyzer only sees terms in [{registered}], so this term \
                     has no owning slice and no dependency edge into it is computable; mint it \
                     inside a registered namespace or register its namespace in \
                     gmeow_ns::TERM_NAMESPACES",
                    file = rel(path, root),
                    reason = reasons.iter().copied().collect::<Vec<_>>().join(" and "),
                    registered = gmeow_ns::TERM_NAMESPACES.join(", "),
                ),
                Some(subject),
            ));
        }
    }
    findings.sort_by(|a, b| a.message.cmp(&b.message));
    findings
}

// ── R10: retired owl: authoring prefix (source-text lint) ────────────────────

/// R10: every slice `module.ttl` is scanned as SOURCE TEXT for a reintroduced
/// retired `owl:` authoring prefix. `logic:` is the canonical authoring
/// vocabulary; the OWL/RDFS surface is a GENERATED projection the pipeline
/// derives, so a hand-authored `owl:` token in a slice module is a forbidden
/// second source of truth. Discovered by MANIFEST (a `module.ttl` is scanned only
/// when present), mirroring the R3c/R9 slice-source lints.
pub fn retired_authoring_prefix_findings(
    project_root: &Path,
    slices_dir: &Path,
) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    for manifest in all_manifests(slices_dir)? {
        let Some(dir) = manifest.parent() else {
            continue;
        };
        let module = dir.join("module.ttl");
        if module.is_file() {
            let text = std::fs::read_to_string(&module).map_err(|e| io_err(&module, &e))?;
            // Label repo-root-relative (e.g. `slices/core/x/module.ttl`), matching every
            // other gate here and the fixtures — never slices_dir-relative or absolute.
            findings.extend(detect_retired_authoring_prefixes(
                &text,
                &rel(&module, project_root),
            ));
        }
    }
    findings.sort_by(|a, b| a.message.cmp(&b.message));
    Ok(findings)
}

/// The pure retired-`owl:`-prefix logic over a module's SOURCE TEXT. Flags a
/// prefixed name (`owl:Foo`) or an `@prefix owl:` declaration at a name/prefix
/// boundary, reporting the file and 1-based line. Deliberately does NOT flag a
/// full IRI (`<…/2002/07/owl#…>` carries no `owl:` token — it is the legitimate
/// correspondence-law target form), a longer prefix (`powl:` / `owlish`), or
/// reworded prose (`OWL X`, no colon).
fn detect_retired_authoring_prefixes(text: &str, source_label: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if RETIRED_OWL_PREFIX_TOKEN.is_match(line) {
            let line_no = idx + 1;
            findings.push(finding(
                Severity::Error,
                codes::AUTHORING_RETIRED_OWL_PREFIX,
                format!(
                    "{source_label}:{line_no} reintroduces a retired owl: authoring prefix — \
                     logic: is the canonical authoring vocabulary and the owl:/RDFS surface is a \
                     GENERATED projection. Author the term in logic: and let the pipeline \
                     derive/project the owl: form (a hand-authored owl: token is a forbidden \
                     second source of truth)"
                ),
                None,
            ));
        }
    }
    findings
}

// ── R7: grounding seam-registry drift ────────────────────────────────────────
//
// The generated seam-registry page (`gmeow_docs::render::Page::SeamRegistry`,
// rendered as `seams/index.md` and materialized at `ontology-docs/seams/index.md`
// by `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs`) is a pure projection of the `gmeow:Seam`
// individuals authored in the grounding slices' manifests (docs/GROUNDING.md,
// "The seam registry"). This gate is a SECOND, INDEPENDENT reader of that same
// governance data — `gmeow-validate` cannot depend on `gmeow-docs` (which itself
// depends on `gmeow-validate`), so drift is caught by comparing the canonical data
// straight off the manifests against the rendered page text, never by re-running
// the renderer.
//
// **The comparison is PER SEAM, never a unioned bag.** The page is parsed back
// into one row per seam — that seam's own direction legs, carrying terms, and
// owning doc — and each field is compared against THAT seam's authored record.
// Unioning every seam's terms before comparing (the shape this gate used to have)
// is blind to exactly the drift that matters: a page that assigns the right terms
// to the wrong seams, or that inverts a `gmeow:seamFromSlice` → `gmeow:seamToSlice`
// leg, unions to the identical set and passes. Direction is the field the peerage
// coverage predicate keys on (`crate::slice_peerage::classify` matches
// `peer ∧ direction-leg(from → to) ∧ exact-term ∈ THAT seam`), so a projection that
// renders it wrong misdocuments precisely the authorization a reader is consulting.
//
// **Exact identity, never substring.** Every comparison in this section is set
// membership over exactly-parsed tokens — a seam NAME parsed out of its own table
// cell, a carrying term parsed as a whole backticked CURIE, a direction leg parsed
// as an ordered pair of slice slugs. The file's standing discipline (see
// `NORMS_EXTENSION_TERMS`: `normIssuer` must never match `normIssuerRole`) applies
// verbatim here; the retired `page_text.contains(&seam.name)` scan violated it.
//
// **Where the comparison actually runs — and why absence is not silence.**
// `ontology-docs/` is written only by a docs-selected sync
// (`crate::dev_sync`'s `SyncOutput::All | SyncOutput::Docs`); the `make check` DAG
// synchronizes with `--outputs generated` and `make validate` runs no sync at all,
// so the materialized page is genuinely absent on the gate path. The gate therefore
// does NOT pretend to have compared anything it did not:
//
//   * seam data that is empty  → HARD FAIL (a vacuous comparison is refused, the
//     same posture as `require_non_vacuous_corpus`);
//   * page present             → the full per-seam comparison, Error on any drift;
//   * `ontology-docs/` present but the seam page missing → Error (a materialized
//     docs tree that dropped the page is a renderer regression, not a cache miss);
//   * `ontology-docs/` absent  → a Warning that says NOT COMPARED and names the
//     command that materializes the page. Never `Ok(vec![])`: this detector cannot
//     return an empty (i.e. "clean") verdict without having compared a real page.
//
// The judgment itself is not left to that on-demand tree: `gmeow-dev doc-lint` — a
// `make check` DAG task — renders the seam page IN MEMORY and drives
// [`detect_seam_registry_drift`] over it on every run, so the per-seam comparison is
// unconditional on-gate and the on-disk leg above is purely an additional check that
// a *materialized* tree agrees.

/// The site-relative path of the generated seam-registry page.
const SEAM_REGISTRY_PAGE_PATH: &str = "ontology-docs/seams/index.md";

/// The materialized docs tree a docs-selected `make sync` reconciles. Its presence
/// is the evidence that a docs render happened in this checkout.
const ONTOLOGY_DOCS_DIR: &str = "ontology-docs";

/// The seam table's header row, emitted verbatim by
/// `gmeow_docs::render::md_seam_registry`. Also the marker that the page carries a
/// table at all.
const SEAM_TABLE_HEADER: &str = "| Seam | Direction | Carrying terms | Owning doc |";

/// The heading that closes the seam table region on the rendered page.
const SEAM_DEFINITIONS_HEADING: &str = "## Definitions";

/// Backtick-wrapped CURIE in one of the four grounding term families
/// (`gmeow:`/`logic:`/`lang:`/`math:`) — generalizes [`GMEOW_INLINE_TERM`] to
/// every family a `gmeow:seamCarryingTerm` may name. Applied to ONE table cell at
/// a time, so a CURIE mentioned in a neighbouring column can never be misread as
/// this seam's carrying term.
static SEAM_PAGE_CARRYING_TERM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"`(gmeow|logic|lang|math):([A-Za-z][A-Za-z0-9_]*)`").expect("valid static regex")
});
/// Backtick-wrapped `NAME.md` design-doc filename (a `gmeow:seamOwningDoc` value).
static SEAM_PAGE_OWNING_DOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`([A-Za-z0-9_-]+\.md)`").expect("valid static regex"));
/// A whole markdown inline link `[text](href)` — the form
/// `gmeow_docs::render::seam_slice_link` emits for a resolvable grounding slice.
static SEAM_PAGE_SLICE_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\[(?P<text>[^\]]*)\]\((?P<href>[^)]*)\)$").expect("valid static regex")
});

// `SeamRecord` + `seam_records_of` — the single reader of the `gmeow:Seam`
// governance data — live in `crate::slice_peerage` (which the peerage-coverage
// engine also needs, extended with the directed `(from, to)` legs and raw
// carrying-term IRIs this text-comparison drift gate never needed). Sharing
// one reader here keeps this drift gate and the coverage engine from ever
// reading the same governance data two different ways.
use crate::slice_peerage::{SeamRecord, seam_records_of};

/// Every `gmeow:Seam` individual authored under `slices_dir`, read through the one
/// shared [`seam_records_of`] reader. Public so the in-memory `gmeow-dev doc-lint`
/// leg drives EXACTLY this discovery rather than re-walking the tree itself.
///
/// # Errors
///
/// Propagates any manifest read/parse failure (no-optionality: an unreadable
/// manifest is a hard fail, never a silently shorter registry).
pub fn seam_registry_of_slices(slices_dir: &Path) -> Result<Vec<SeamRecord>> {
    let mut seams: Vec<SeamRecord> = Vec::new();
    for manifest in all_manifests(slices_dir)? {
        let ds = parse_ttl(&manifest)?;
        seams.extend(seam_records_of(&ds, &manifest)?);
    }
    Ok(seams)
}

/// The seam-table region of the rendered page: from the table header through (but
/// excluding) the `## Definitions` heading. `None` when the page carries no table
/// header at all — an honest "there is nothing here to compare", which the caller
/// turns into a finding rather than into silence.
fn seam_table_region(page_text: &str) -> Option<&str> {
    let start = page_text.find(SEAM_TABLE_HEADER)?;
    let region = &page_text[start..];
    let end = region
        .find(SEAM_DEFINITIONS_HEADING)
        .unwrap_or(region.len());
    Some(&region[..end])
}

/// The characters `gmeow_docs::render::md_escape` backslash-escapes (plus the `|`
/// both it and `code_escape` escape inside a table cell).
const MD_ESCAPED_CHARS: &[char] = &[
    '\\', '`', '*', '_', '{', '}', '[', ']', '(', ')', '#', '+', '-', '.', '!', '<', '>', '|',
];

/// Undo `gmeow_docs::render::md_escape`: drop a backslash that introduces one of
/// the escaped metacharacters, leave every other character alone.
fn md_unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\'
            && chars
                .peek()
                .is_some_and(|next| MD_ESCAPED_CHARS.contains(next))
        {
            out.push(chars.next().expect("peeked character is present"));
        } else {
            out.push(ch);
        }
    }
    out
}

/// Split a markdown table row on its UNESCAPED `|` separators. A cell's own pipe is
/// rendered `\|` (both `md_escape` and `code_escape` do this), so splitting on a raw
/// `|` would shear such a cell in half and silently shift every later column.
/// `| a | b | c | d |` yields six parts: a leading empty, the four cells, a trailing
/// empty.
fn split_row_cells(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
        } else if ch == '\\' {
            current.push(ch);
            escaped = true;
        } else if ch == '|' {
            cells.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }
    cells.push(current);
    cells
}

/// The local name of an IRI: the tail after the last `/` or `#`. A per-module copy
/// of `gmeow_docs::render::local_name` — this crate cannot depend on `gmeow-docs`,
/// and the file's standing posture is a local copy of the shared constant/helper
/// rather than a new cross-crate coupling.
fn iri_local_name(iri: &str) -> &str {
    let cut = iri.rfind(['/', '#']).map(|i| i + 1).unwrap_or(0);
    &iri[cut..]
}

/// Lowercase, collapse every non-alphanumeric run to a single `-`, trim the edges;
/// empty input becomes `unnamed`. A per-module copy of
/// `gmeow_docs::render::slugify` (see [`iri_local_name`] for why it is copied), and
/// the reason a direction leg can be compared at all: the page renders a slice as a
/// link into `slices/<slug>/`, so the slug is the one token both sides share.
fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for ch in name.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            out.push(lc);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// The comparison token for the DATA side of a direction leg: a
/// `gmeow:seamFromSlice`/`seamToSlice` IRI reduced to the slug the page links by.
fn slice_token_of_iri(iri: &str) -> String {
    slugify(iri_local_name(iri))
}

/// The comparison token for the PAGE side of a direction leg. A resolvable slice
/// renders as `[Display](../slices/<slug>/index.md)` — the slug in the href is the
/// identity, not the (possibly retitled, possibly translated) link text. An
/// unresolvable slice renders as the bare escaped local name, which slugifies to the
/// same token.
fn slice_token_of_cell(side: &str) -> String {
    let side = side.trim();
    if let Some(caps) = SEAM_PAGE_SLICE_LINK.captures(side) {
        let href = caps.name("href").map_or("", |m| m.as_str());
        let trimmed = href
            .trim_end_matches("index.md")
            .trim_end_matches('/')
            .trim();
        if let Some(last) = trimmed.rsplit('/').find(|segment| !segment.is_empty()) {
            return slugify(last);
        }
        let text = caps.name("text").map_or("", |m| m.as_str());
        return slugify(&md_unescape(text));
    }
    slugify(&md_unescape(side))
}

/// One seam row exactly as the generated page claims it — the page-side counterpart
/// of a [`SeamRecord`], carrying that seam's OWN fields and nothing unioned in from
/// its neighbours.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct PageSeamRow {
    /// `(from-slug, to-slug)` pairs read off the Direction column, in the order the
    /// arrow renders them — so an inverted leg is a different element, not the same.
    directions: BTreeSet<(String, String)>,
    /// Whole backticked `family:Local` CURIEs read off the Carrying terms column.
    carrying_terms: BTreeSet<String>,
    /// Backticked `NAME.md` filenames read off the Owning doc column.
    owning_docs: BTreeSet<String>,
}

/// Parse the rendered seam table into one [`PageSeamRow`] per seam name.
///
/// Returns the rows keyed by seam name (a name rendered more than once keeps every
/// row so the caller can report the ambiguity rather than silently picking one) plus
/// a complaint per structurally unparsable row — a row whose column count is wrong,
/// whose name cell is empty, or whose Direction cell holds a leg with no `→`. A
/// malformed row is never skipped quietly: the whole point of this gate is that an
/// unreadable projection is drift, not a pass.
fn parse_seam_table(region: &str) -> (BTreeMap<String, Vec<PageSeamRow>>, Vec<String>) {
    let mut rows: BTreeMap<String, Vec<PageSeamRow>> = BTreeMap::new();
    let mut complaints: Vec<String> = Vec::new();
    for line in region.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || trimmed == SEAM_TABLE_HEADER {
            continue;
        }
        let cells = split_row_cells(trimmed);
        if cells.len() != 6 {
            complaints.push(format!(
                "row {trimmed:?} has {n} columns, not the four the seam table declares",
                n = cells.len().saturating_sub(2),
            ));
            continue;
        }
        let body = &cells[1..5];
        // The `| --- | --- | --- | --- |` alignment row.
        if body.iter().all(|cell| {
            let t = cell.trim();
            !t.is_empty() && t.chars().all(|ch| ch == '-' || ch == ':')
        }) {
            continue;
        }
        let name = md_unescape(
            body[0]
                .trim()
                .trim_start_matches("**")
                .trim_end_matches("**")
                .trim(),
        );
        if name.is_empty() {
            complaints.push(format!("row {trimmed:?} names no seam"));
            continue;
        }
        let mut directions: BTreeSet<(String, String)> = BTreeSet::new();
        for leg in body[1].split(';') {
            let leg = leg.trim();
            if leg.is_empty() {
                continue;
            }
            match leg.split_once('→') {
                Some((from, to)) => {
                    directions.insert((slice_token_of_cell(from), slice_token_of_cell(to)));
                }
                None => complaints.push(format!(
                    "seam \"{name}\"'s Direction cell holds {leg:?}, which is not a \
                     `from → to` leg"
                )),
            }
        }
        let carrying_terms: BTreeSet<String> = SEAM_PAGE_CARRYING_TERM
            .captures_iter(&body[2])
            .map(|caps| format!("{}:{}", &caps[1], &caps[2]))
            .collect();
        let owning_docs: BTreeSet<String> = SEAM_PAGE_OWNING_DOC
            .captures_iter(&body[3])
            .map(|caps| caps[1].to_string())
            .collect();
        rows.entry(name).or_default().push(PageSeamRow {
            directions,
            carrying_terms,
            owning_docs,
        });
    }
    (rows, complaints)
}

/// Shorthand for one seam-registry drift Error, hung off the seam's IRI.
fn drift(message: String, seam_iri: Option<String>) -> Finding {
    finding(
        Severity::Error,
        codes::AUTHORING_SEAM_REGISTRY_DRIFT,
        message,
        seam_iri,
    )
}

/// R7: the generated seam-registry page carries exactly the `gmeow:Seam` data
/// authored in the grounding slices' manifests, **seam by seam** — for every seam,
/// that seam's own direction legs, carrying terms, and owning docs, with drift in
/// either direction (data → page and page → data) a distinct Error naming the seam
/// and the field.
///
/// Public so `gmeow-dev doc-lint` can drive it over the page it renders IN MEMORY,
/// which is the leg that makes this comparison unconditional on the `make check`
/// DAG (see the section header: the materialized `ontology-docs/` tree does not
/// exist on that path).
pub fn detect_seam_registry_drift(seams: &[SeamRecord], page_text: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    let Some(region) = seam_table_region(page_text) else {
        if !seams.is_empty() {
            findings.push(drift(
                format!(
                    "seam-registry drift: the generated seam-registry page \
                     ({SEAM_REGISTRY_PAGE_PATH}) carries no seam table, but {n} gmeow:Seam \
                     individual(s) are declared in the grounding manifests",
                    n = seams.len(),
                ),
                None,
            ));
        }
        return findings;
    };

    let (page_rows, complaints) = parse_seam_table(region);
    for complaint in complaints {
        findings.push(drift(
            format!(
                "seam-registry drift: the generated seam-registry page \
                 ({SEAM_REGISTRY_PAGE_PATH}) is unparsable — {complaint}"
            ),
            None,
        ));
    }

    // A seam name is the join key both sides use (the page's own row label), so a
    // name carried twice on either side makes the projection ambiguous rather than
    // merely wrong — report it and refuse to guess which row pairs with which record.
    for (name, rows) in &page_rows {
        if rows.len() > 1 {
            findings.push(drift(
                format!(
                    "seam-registry drift: the generated seam-registry page \
                     ({SEAM_REGISTRY_PAGE_PATH}) renders {n} rows for seam \"{name}\" — the \
                     registry projection must render each seam exactly once",
                    n = rows.len(),
                ),
                None,
            ));
        }
    }

    let mut by_name: BTreeMap<&str, Vec<&SeamRecord>> = BTreeMap::new();
    for seam in seams {
        by_name.entry(seam.name.as_str()).or_default().push(seam);
    }

    for (name, records) in &by_name {
        if records.len() > 1 {
            let iris = records
                .iter()
                .map(|record| record.iri.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(drift(
                format!(
                    "seam-registry drift: {n} gmeow:Seam individuals ({iris}) share the label \
                     \"{name}\" — the generated seam-registry page keys its rows by label, so \
                     the projection cannot be checked per seam",
                    n = records.len(),
                ),
                None,
            ));
            continue;
        }
        let seam = records[0];
        let iri = Some(seam.iri.clone());
        let Some(rows) = page_rows.get(*name) else {
            findings.push(drift(
                format!(
                    "seam-registry drift: seam \"{name}\" is declared in the grounding manifests \
                     but does not appear on the generated seam-registry page \
                     ({SEAM_REGISTRY_PAGE_PATH})"
                ),
                iri,
            ));
            continue;
        };
        if rows.len() > 1 {
            // Already reported as ambiguous above; comparing against an arbitrary
            // one of the duplicate rows would invent a verdict.
            continue;
        }
        let row = &rows[0];

        // Carrying terms — THIS seam's own set, exact CURIE identity both ways.
        for term in seam.carrying_terms.difference(&row.carrying_terms) {
            findings.push(drift(
                format!(
                    "seam-registry drift: seam \"{name}\" declares carrying term {term}, which is \
                     missing from that seam's row on the generated seam-registry page \
                     ({SEAM_REGISTRY_PAGE_PATH})"
                ),
                iri.clone(),
            ));
        }
        for term in row.carrying_terms.difference(&seam.carrying_terms) {
            findings.push(drift(
                format!(
                    "seam-registry drift: the row for seam \"{name}\" on the generated \
                     seam-registry page ({SEAM_REGISTRY_PAGE_PATH}) lists carrying term {term}, \
                     which that seam does not declare"
                ),
                iri.clone(),
            ));
        }

        // Owning docs — THIS seam's own set, both ways.
        for doc in seam.owning_docs.difference(&row.owning_docs) {
            findings.push(drift(
                format!(
                    "seam-registry drift: seam \"{name}\" declares owning doc {doc}, which is \
                     missing from that seam's row on the generated seam-registry page \
                     ({SEAM_REGISTRY_PAGE_PATH})"
                ),
                iri.clone(),
            ));
        }
        for doc in row.owning_docs.difference(&seam.owning_docs) {
            findings.push(drift(
                format!(
                    "seam-registry drift: the row for seam \"{name}\" on the generated \
                     seam-registry page ({SEAM_REGISTRY_PAGE_PATH}) lists owning doc {doc}, which \
                     that seam does not declare"
                ),
                iri.clone(),
            ));
        }

        // Direction legs — ORDERED pairs, so an inverted leg is drift, not a match.
        let data_legs: BTreeSet<(String, String)> = seam
            .directions
            .iter()
            .map(|(from, to)| (slice_token_of_iri(from), slice_token_of_iri(to)))
            .collect();
        for leg in data_legs.difference(&row.directions) {
            let inverted = (leg.1.clone(), leg.0.clone());
            let message = if row.directions.contains(&inverted) {
                format!(
                    "seam-registry drift: seam \"{name}\" declares direction leg {from} → {to} \
                     (gmeow:seamFromSlice → gmeow:seamToSlice), but the generated seam-registry \
                     page ({SEAM_REGISTRY_PAGE_PATH}) renders it INVERTED as {to} → {from}",
                    from = leg.0,
                    to = leg.1,
                )
            } else {
                format!(
                    "seam-registry drift: seam \"{name}\" declares direction leg {from} → {to} \
                     (gmeow:seamFromSlice → gmeow:seamToSlice), which is missing from that seam's \
                     row on the generated seam-registry page ({SEAM_REGISTRY_PAGE_PATH})",
                    from = leg.0,
                    to = leg.1,
                )
            };
            findings.push(drift(message, iri.clone()));
        }
        for leg in row.directions.difference(&data_legs) {
            if data_legs.contains(&(leg.1.clone(), leg.0.clone())) {
                // Already reported, once, as the inversion of the authored leg.
                continue;
            }
            findings.push(drift(
                format!(
                    "seam-registry drift: the row for seam \"{name}\" on the generated \
                     seam-registry page ({SEAM_REGISTRY_PAGE_PATH}) renders direction leg \
                     {from} → {to}, which that seam does not declare",
                    from = leg.0,
                    to = leg.1,
                ),
                iri.clone(),
            ));
        }
    }

    for name in page_rows.keys() {
        if !by_name.contains_key(name.as_str()) {
            findings.push(drift(
                format!(
                    "seam-registry drift: the generated seam-registry page \
                     ({SEAM_REGISTRY_PAGE_PATH}) lists seam \"{name}\", which no gmeow:Seam \
                     individual declares"
                ),
                None,
            ));
        }
    }

    findings.sort_by(|a, b| a.message.cmp(&b.message));
    findings.dedup_by(|a, b| a.message == b.message);
    findings
}

/// R7 over the on-disk corpus: read the authored `gmeow:Seam` registry out of
/// `slices_dir` and compare it, per seam, against the materialized seam-registry
/// page under `project_root`.
///
/// A `slices_dir` that declares no seam at all yields a single Error finding
/// naming the vacuity — never a clean (empty) verdict. On the aggregator path
/// [`require_non_vacuous_corpus`] additionally refuses such a corpus outright; this
/// detector reports rather than aborts so a direct caller pointed at a synthetic
/// slice tree still gets every other finding.
///
/// # Errors
///
/// Hard-fails when a manifest cannot be read or parsed, or when the page exists but
/// cannot be read (no-optionality: an unreadable gate input is never a skip).
pub fn seam_registry_drift_findings(
    project_root: &Path,
    slices_dir: &Path,
) -> Result<Vec<Finding>> {
    let seams = seam_registry_of_slices(slices_dir)?;
    if seams.is_empty() {
        return Ok(vec![drift(
            format!(
                "seam-registry drift: no gmeow:Seam individuals are declared under {dir} — a \
                 drift comparison against an empty registry certifies nothing",
                dir = slices_dir.display(),
            ),
            None,
        )]);
    }

    let page_path = project_root.join(SEAM_REGISTRY_PAGE_PATH);
    match std::fs::read_to_string(&page_path) {
        Ok(text) => Ok(detect_seam_registry_drift(&seams, &text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if project_root.join(ONTOLOGY_DOCS_DIR).is_dir() {
                // A docs render DID happen in this tree and produced no seam page:
                // the projection lost a page the renderer unconditionally emits.
                Ok(vec![drift(
                    format!(
                        "seam-registry drift: the materialized docs tree \
                         ({ONTOLOGY_DOCS_DIR}/) carries no seam-registry page at \
                         {SEAM_REGISTRY_PAGE_PATH}, but {n} gmeow:Seam individual(s) are declared \
                         in the grounding manifests — the docs projection dropped a page it \
                         always renders",
                        n = seams.len(),
                    ),
                    None,
                )])
            } else {
                // No docs tree at all. Report NOT COMPARED — never an empty
                // ("clean") verdict — and name the command that materializes the
                // page. The unconditional leg is `gmeow-dev doc-lint`, which renders
                // the page in memory and drives `detect_seam_registry_drift` on it.
                Ok(vec![finding(
                    Severity::Warning,
                    codes::AUTHORING_SEAM_REGISTRY_DRIFT,
                    format!(
                        "seam-registry drift NOT COMPARED against a materialized page: no \
                         {ONTOLOGY_DOCS_DIR}/ tree in this checkout, so \
                         {SEAM_REGISTRY_PAGE_PATH} does not exist (materialize it with `make \
                         check-sync SYNC_MODE=update SYNC_OUTPUTS=docs`). The {n} declared \
                         gmeow:Seam individual(s) are \
                         compared unconditionally against the in-memory render by `gmeow-dev \
                         doc-lint`.",
                        n = seams.len(),
                    ),
                    None,
                )])
            }
        }
        Err(e) => Err(io_err(&page_path, &e)),
    }
}

#[path = "authoring_integrity.tests.rs"]
#[cfg(test)]
mod tests;
