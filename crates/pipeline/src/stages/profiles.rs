// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `profiles` export leaf (P4): IRI-addressable ontology profiles.
//!
//! A genuine port of `src/gmeow_tools/profiles_gen.py` (no Rust existed). The
//! `full` profile imports the root IRI + every extension slice; each named
//! profile (declared via `gmeow:sliceProfile`) imports its declared members plus
//! their `gmeow:sliceDependsOn` closure (slim, dependency-closed). Output is
//! byte-deterministic → compared byte-for-byte to the committed `generated/profiles/*.ttl`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gmeow_errors::abox::{BOX_TBOX, abox_annotation_turtle_lines};
use purrdf::slice::rdf_query::{Dataset, Object};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::source_load::{all_manifest_files, module_files};

/// Logical-path prefix of the generated profile documents.
pub const PROFILES_DIR: &str = "generated/profiles";

const ONTOLOGY_IRI: &str = "https://blackcatinformatics.ca/gmeow";
const FULL_PROFILE_IRI: &str = "https://blackcatinformatics.ca/gmeow/full";
const NAMED_PROFILE_NS: &str = "https://blackcatinformatics.ca/gmeow/profiles/";
const SLICE_CLASS: &str = "https://blackcatinformatics.ca/gmeow/Slice";
const SLICE_TIER: &str = "https://blackcatinformatics.ca/gmeow/sliceTier";
const SLICE_PROFILE: &str = "https://blackcatinformatics.ca/gmeow/sliceProfile";
const SLICE_DEPENDS_ON: &str = "https://blackcatinformatics.ca/gmeow/sliceDependsOn";
const TIER_CORE: &str = "https://blackcatinformatics.ca/gmeow/tierCore";
const TIER_PROFILE: &str = "https://blackcatinformatics.ca/gmeow/tierProfile";

/// One slice's profile-relevant manifest facts.
pub(crate) struct SliceMeta {
    iri: String,
    is_core: bool,
    /// A `gmeow:tierProfile` pure-selection slice: mints nothing (no `module.ttl`),
    /// and the profiles stage emits its `sliceDependsOn` closure as a profile doc.
    is_profile: bool,
    pub(crate) profiles: Vec<String>,
    pub(crate) depends_on: Vec<String>,
}

/// Discover every slice's profile-relevant manifest facts, keyed by slice IRI.
/// Shared with the `metadata` stage (DCAT profile membership).
pub(crate) fn discover_slices(
    root: &Path,
) -> Result<BTreeMap<String, SliceMeta>, gmeow_errors::Diag> {
    let mut slices: BTreeMap<String, SliceMeta> = BTreeMap::new();
    for module in module_files(root)? {
        let manifest = module.with_file_name("manifest.ttl");
        if !manifest.is_file() {
            continue;
        }
        let meta = parse_manifest(&manifest)?;
        slices.insert(meta.iri.clone(), meta);
    }
    Ok(slices)
}

/// Render every profile document under `root`: `{filename → Turtle}`.
pub fn render_profiles(root: &Path) -> Result<BTreeMap<String, String>, gmeow_errors::Diag> {
    let slices = discover_slices(root)?;

    let mut out: BTreeMap<String, String> = BTreeMap::new();

    // full.ttl — root IRI + every extension slice (sorted).
    let extensions: Vec<String> = slices
        .values()
        .filter(|s| !s.is_core)
        .map(|s| s.iri.clone())
        .collect();
    let mut full_imports = vec![ONTOLOGY_IRI.to_string()];
    full_imports.extend(extensions); // BTreeMap values() are IRI-sorted already
    out.insert(
        "full.ttl".to_string(),
        profile_document(
            FULL_PROFILE_IRI,
            "GMEOW — full profile",
            "The everything-aggregation: the core profile (the root IRI's generated tierCore closure) plus every extension slice. The kitchen-sink consumer's single import target, and the input to the global reasoning gate.",
            &full_imports,
        )?,
    );

    // Named profiles — declared members + their sliceDependsOn closure.
    for (name, members) in group_named_profiles(&slices) {
        let imports = dependency_closure(&name, &members, &slices)?;
        out.insert(
            format!("{name}.ttl"),
            profile_document(
                &format!("{NAMED_PROFILE_NS}{name}"),
                &format!("GMEOW — {name} profile"),
                &format!(
                    "The {name} profile: a slim, dependency-closed slice set aggregated as its own dereferenceable, citable, reasonable ontology — declared members plus their sliceDependsOn closure, and nothing else. Membership is declared per-slice with gmeow:sliceProfile."
                ),
                &imports,
            )?,
        );
    }

    // Profile-tier slices — a pure-selection slice (gmeow:tierProfile) that mints
    // NOTHING (no module.ttl, so it is absent from `slices`/`full.ttl` — no
    // over-capture) and declares its selection directly via its own manifest's
    // gmeow:sliceDependsOn. Emit each as its dependency-closed composition ontology,
    // reusing the same closure + document machinery as named profiles.
    for profile in discover_profile_slices(root)? {
        let name = profile_slug(&profile.iri);
        let imports = dependency_closure(&name, &profile.depends_on, &slices)?;
        out.insert(
            format!("{name}.ttl"),
            profile_document(
                &profile.iri,
                &format!("GMEOW — {name} profile"),
                &format!(
                    "The {name} profile-tier slice: a pure selection (Principle 16 — mints nothing) aggregating its declared gmeow:sliceDependsOn slices and their closure as one dereferenceable, citable, reasonable sub-ontology for a cohesive audience."
                ),
                &imports,
            )?,
        );
    }
    Ok(out)
}

/// The short name of a slice IRI — the final path segment (e.g.
/// `…/slices/agent-runtime` → `agent-runtime`).
fn profile_slug(iri: &str) -> String {
    iri.rsplit('/').next().unwrap_or(iri).to_string()
}

/// Discover the `gmeow:tierProfile` pure-selection slices (manifest-only, no
/// `module.ttl`). Sorted by IRI for deterministic emission.
fn discover_profile_slices(root: &Path) -> Result<Vec<SliceMeta>, gmeow_errors::Diag> {
    let mut out: Vec<SliceMeta> = Vec::new();
    for manifest in all_manifest_files(root)? {
        let meta = parse_manifest(&manifest)?;
        if meta.is_profile {
            out.push(meta);
        }
    }
    out.sort_by(|a, b| a.iri.cmp(&b.iri));
    Ok(out)
}

/// Profile name → sorted member IRIs (from `gmeow:sliceProfile` declarations).
pub(crate) fn group_named_profiles(
    slices: &BTreeMap<String, SliceMeta>,
) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for s in slices.values() {
        for name in &s.profiles {
            out.entry(name.clone()).or_default().push(s.iri.clone());
        }
    }
    for iris in out.values_mut() {
        iris.sort();
        iris.dedup();
    }
    out
}

/// Members + every slice reachable through `sliceDependsOn`, sorted. Hard-fails
/// if a chain escapes the registry or a profile has no members.
pub(crate) fn dependency_closure(
    name: &str,
    members: &[String],
    slices: &BTreeMap<String, SliceMeta>,
) -> Result<Vec<String>, gmeow_errors::Diag> {
    if members.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-export-profiles".to_string(),
            message: format!("named profile {name} has no declared members"),
        }));
    }
    let mut closed: BTreeSet<String> = BTreeSet::new();
    let mut frontier: Vec<String> = members.to_vec();
    while let Some(iri) = frontier.pop() {
        if closed.contains(&iri) {
            continue;
        }
        let s = slices.get(&iri).ok_or_else(|| gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-export-profiles".to_string(),
            message: format!(
                "profile dependency closure escapes the registry: {iri} is not a discovered slice"
            ),
        }))?;
        closed.insert(iri.clone());
        frontier.extend(s.depends_on.iter().cloned());
    }
    Ok(closed.into_iter().collect())
}

/// Render one profile ontology document (Turtle), byte-identical to the Python.
/// Render one profile as the canonical Turtle fold of its `owl:Ontology` graph —
/// no banner: the file IS the fold of the named graph the snapshot carries, so the
/// superset law reconstructs it byte-for-byte (PIPELINE_SPINE §5). Provenance lives
/// in the bundle's graphs, never in a non-triple comment side-channel.
fn profile_document(
    iri: &str,
    label: &str,
    comment: &str,
    imports: &[String],
) -> Result<String, gmeow_errors::Diag> {
    let mut lines: Vec<String> = vec![
        "@prefix owl:  <http://www.w3.org/2002/07/owl#> .".to_string(),
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .".to_string(),
        "@prefix skos: <http://www.w3.org/2004/02/skos/core#> .".to_string(),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .".to_string(),
        String::new(),
        format!("<{iri}>"),
        "    a owl:Ontology ;".to_string(),
    ];
    lines.extend(abox_annotation_turtle_lines(
        iri, label, comment, iri, BOX_TBOX, "    ",
    ));
    lines.push("    owl:imports".to_string());
    for imp in &imports[..imports.len() - 1] {
        lines.push(format!("        <{imp}> ,"));
    }
    lines.push(format!("        <{}> .", imports[imports.len() - 1]));
    let body = lines.join("\n");
    // Canonicalize through the shared renderer so `file == render(graph)`: the same
    // serializer (and prefix authority) the superset gate folds the carried named
    // graph with. Fail-closed — a malformed profile body must not ship.
    purrdf::turtle_normalize::canonical_turtle(
        body.as_bytes(),
        &crate::stages::superset::rdf_prefixes(),
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-export-profiles".to_string(),
            message: format!("canonicalize profile <{iri}>: {e}"),
        })
    })
}

fn parse_manifest(path: &Path) -> Result<SliceMeta, gmeow_errors::Diag> {
    let bytes = std::fs::read(path)?;
    let dataset =
        Dataset::parse_turtle(&bytes, None, &path.display().to_string()).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: e.to_string(),
            })
        })?;

    // The slice IRI: the last named subject of `?s a gmeow:Slice` (the old scan kept
    // the final match, so mirror that — `subjects_of_type` is in dataset order).
    let iri = dataset
        .subjects_of_type(SLICE_CLASS)
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: e.to_string(),
            })
        })?
        .into_iter()
        .next_back()
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("no `a gmeow:Slice` in {}", path.display()),
            })
        })?;

    // sliceTier: core iff any named-node object equals tierCore; profile iff tierProfile.
    let tiers = dataset.object_iris(&iri, SLICE_TIER).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: e.to_string(),
        })
    })?;
    let is_core = tiers.iter().any(|n| n == TIER_CORE);
    let is_profile = tiers.iter().any(|n| n == TIER_PROFILE);

    // sliceProfile: every literal-object lexical value, in dataset order.
    let profiles: Vec<String> = dataset
        .objects(&iri, SLICE_PROFILE)
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: e.to_string(),
            })
        })?
        .into_iter()
        .filter_map(|o| match o {
            Object::Literal { value, .. } => Some(value),
            _ => None,
        })
        .collect();

    // sliceDependsOn: every named-node object, in dataset order.
    let depends_on = dataset.object_iris(&iri, SLICE_DEPENDS_ON).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: e.to_string(),
        })
    })?;

    Ok(SliceMeta {
        iri,
        is_core,
        is_profile,
        profiles,
        depends_on,
    })
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `profiles` export-leaf stage.
pub struct ProfilesStage;

impl Stage for ProfilesStage {
    fn id(&self) -> &str {
        "stage-export-profiles"
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "profiles.v1"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // Pure source read: profile membership + dependency closures are derived
        // from the slice manifests (`gmeow:sliceProfile` / `sliceTier` /
        // `sliceDependsOn` live in `manifest.ttl`, NOT the composed fold). Declare
        // the manifests so a membership/dependency change busts the cache.
        crate::stages::source_load::manifest_files(root)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let docs = render_profiles(input.root)?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for (name, text) in docs {
            artifacts.insert(format!("{PROFILES_DIR}/{name}"), text.into_bytes());
        }
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
    }
}
