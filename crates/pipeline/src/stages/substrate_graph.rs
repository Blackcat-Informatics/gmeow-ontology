// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The dogfooded substrate reconciliation projection.
//!
//! Promotes the PurRDF substrate identity from manifest requirements, lockfiles,
//! compiled-in constants, every shipped wasm asset and source prose
//! to first-class reasoned ontology content in `graph/provenance`: one
//! [`gmeow:SubstrateComponent`](https://blackcatinformatics.ca/gmeow/SubstrateComponent)
//! per external engine/library, one `gmeow:PinClaim` per (site, component,
//! dimension), one `gmeow:ReconciledPin` per (component, dimension) whose present
//! sites agree, and `gmeow:embeds` edges recording what each shipped engine
//! statically carries. The authored `gmeow:PinAgreementConstraint` /
//! `gmeow:PinCoverageConstraint` (slices/core/attestation/module.ttl) reason over
//! this A-Box; drift surfaces as a `gmeow:Finding`, not a bash exit code.
//!
//! ## Non-self-referential (why it folds at carrier time)
//!
//! Every claim value is read from a build INPUT — a manifest requirement, a lockfile,
//! a linked `const`, a committed `SUBSTRATE.txt`, or doc prose — never from a
//! render-derived digest of *this* bundle. So this folds with no fixpoint problem,
//! unlike the per-release bundle digest [`distribution_catalog`] deliberately
//! refuses. [`substrate_input_paths`] enumerates exactly those inputs, and a test
//! asserts none is under `generated/`.
//!
//! ## Determinism
//!
//! Every value is a public string read from a committed input; every collection is
//! sorted and the emitted lines are sorted + deduped, so the bytes are byte-stable
//! across runs (no timestamps, no runtime ids).
//!
//! [`distribution_catalog`]: crate::stages::distribution_catalog

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use gmeow_errors::abox::{AboxObject, X_GMEOW_ENGLISH, abox_annotations};
use gmeow_errors::render::nq_escape;

use crate::stages::provenance_graph::GRAPH_PROVENANCE;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

// The renderer and substrate proof share the same complete shipped asset inventory.
use gmeow_docs::vendored_asset::ALL_ASSETS;

// ── claim-site + dimension value-vocabulary individuals (authored in the slice) ──
const SITE_WORKSPACE_MANIFEST: &str = "claimSiteWorkspaceManifest";
const SITE_FUZZ_MANIFEST: &str = "claimSiteFuzzManifest";
const SITE_LOCKFILE: &str = "claimSiteLockfile";
const SITE_LINKED_CONSTANT: &str = "claimSiteLinkedConstant";
const SITE_SHIPPED_ARTIFACT: &str = "claimSiteShippedArtifact";
const SITE_PROSE: &str = "claimSiteProse";

const DIM_CRATE_VERSION: &str = "dimensionCrateVersion";
const DIM_VERSION_REQUIREMENT: &str = "dimensionVersionRequirement";
const DIM_SHAPES_VERSION: &str = "dimensionShapesVersion";
const DIM_WIRE_VERSION: &str = "dimensionWireVersion";
const DIM_ZSTD_LEVEL: &str = "dimensionZstdLevel";

/// One external component the build depends on or embeds. `name` is its canonical
/// SPDX-facing name; `expected_sites` are the claim-site slugs a
/// `gmeow:PinCoverageConstraint` requires to be witnessed.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Component {
    /// Stable slug → node-IRI local part (a pure function of the public name).
    slug: String,
    /// Canonical component name (e.g. "purrdf").
    name: String,
    /// The claim sites this component MUST be asserted at (drives coverage).
    expected_sites: Vec<&'static str>,
}

/// One per-site assertion of one dimension of one component (already normalized).
/// `witness` distinguishes otherwise-identical claims from separate sources at the
/// same site — the shipping engine for a shipped-artifact stamp — so each engine's
/// stamp is a DISTINCT `gmeow:PinClaim` and cross-engine disagreement is caught by
/// `gmeow:PinAgreementConstraint` rather than collapsing to one node.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Claim {
    component_slug: String,
    site: &'static str,
    dimension: &'static str,
    value: String,
    witness: Option<String>,
}

/// A shipped engine statically embedding a component (the SBOM "contains" edge).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Embed {
    engine_slug: String,
    embedded_slug: String,
}

/// The exact build INPUT files the substrate projection reads — enumerated so a
/// test can assert none is under `generated/` (the non-fixpoint property).
#[must_use]
pub fn substrate_input_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = vec![
        root.join("Cargo.toml"),
        root.join("fuzz/Cargo.toml"),
        root.join("Cargo.lock"),
        root.join("fuzz/Cargo.lock"),
        root.join("docs/research-objects.md"),
    ];
    for asset in ALL_ASSETS {
        let engine = asset.name;
        paths.push(root.join(format!("crates/docs/assets/{engine}/SUBSTRATE.txt")));
    }
    paths
}

/// Validate that `name` is a safe IRI local part before it becomes a substrate node
/// IRI (a stamp could otherwise inject characters that malform the emitted IRI).
fn checked_slug(name: &str) -> Result<String, gmeow_errors::Diag> {
    if !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        Ok(name.to_string())
    } else {
        Err(stage_err(&format!(
            "substrate carrier: component name {name:?} is not a valid IRI local part"
        )))
    }
}

/// Parse a `SUBSTRATE.txt` stamp of the form
/// `purrdf 0.12.0; wasm-bindgen 0.2.125; binaryen version_130` into `(name, version)`
/// pairs. Every non-blank line and every non-blank `;`-separated part is processed;
/// a part that is not exactly `<name> <version>` is a HARD FAIL (no silent skipping
/// of stamp data), and an empty stamp is rejected.
fn parse_substrate_stamp(stamp: &str) -> Result<Vec<(String, String)>, gmeow_errors::Diag> {
    let mut out = Vec::new();
    for line in stamp.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        for part in line.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let mut it = part.split_whitespace();
            match (it.next(), it.next(), it.next()) {
                (Some(name), Some(version), None) => {
                    out.push((name.to_string(), version.to_string()));
                }
                _ => {
                    return Err(stage_err(&format!(
                        "substrate carrier: malformed SUBSTRATE.txt stamp part {part:?} — \
                         expected exactly '<name> <version>'"
                    )));
                }
            }
        }
    }
    if out.is_empty() {
        return Err(stage_err(
            "substrate carrier: SUBSTRATE.txt stamp has no '<name> <version>' entries",
        ));
    }
    Ok(out)
}

/// Find purrdf's version as mentioned in documentation prose (`purrdf <version>`),
/// returning the first plausible semver-looking mention.
fn parse_prose_purrdf_version(prose: &str) -> Option<String> {
    for token_window in prose.split_whitespace().collect::<Vec<_>>().windows(2) {
        let word = token_window[0].trim_matches(|c: char| !c.is_alphanumeric());
        if word == "purrdf" {
            let candidate =
                token_window[1].trim_matches(|c: char| !(c.is_ascii_digit() || c == '.'));
            if candidate.contains('.')
                && candidate.chars().next().is_some_and(|c| c.is_ascii_digit())
            {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

/// A pin claim keyed by (component, dimension); reconciliation groups over this key.
fn reconcile(claims: &[Claim]) -> Vec<(String, &'static str, String)> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<(String, &'static str), Vec<String>> = BTreeMap::new();
    for c in claims {
        groups
            .entry((c.component_slug.clone(), c.dimension))
            .or_default()
            .push(c.value.clone());
    }
    let mut out = Vec::new();
    for ((comp, dim), values) in groups {
        let mut distinct: Vec<String> = values.clone();
        distinct.sort();
        distinct.dedup();
        // Present sites agree ⇒ one reconciled value. Disagreement leaves NO
        // ReconciledPin — its absence is the drift signal (and the constraint fires).
        if distinct.len() == 1 {
            out.push((comp, dim, distinct.into_iter().next().unwrap()));
        }
    }
    out
}

/// Render one substrate `component`/`slug` IRI (a pure function of the public name).
fn iri(kind: &str, slug: &str) -> String {
    format!("{GMEOW}substrate/{kind}/{slug}")
}

fn triple_iri(out: &mut String, s: &str, p: &str, o: &str) {
    writeln!(out, "<{s}> <{p}> <{o}> .").expect("write to String");
}

fn triple_lit(out: &mut String, s: &str, p: &str, lit: &str) {
    writeln!(out, "<{s}> <{p}> \"{}\" .", nq_escape(lit)).expect("write to String");
}

/// Emit the assertional-tier A-Box annotations for `subject_iri` through the shared
/// [`abox_annotations`] contract — exactly the predicate/object set every generated
/// individual carries, rooted at `graph/provenance`. The precise set is defined by
/// that contract, not restated here.
fn annotate(out: &mut String, subject_iri: &str, label: &str, definition: &str) {
    for (predicate, object) in abox_annotations(subject_iri, label, definition, GRAPH_PROVENANCE) {
        let object_text = match object {
            AboxObject::Iri(i) => format!("<{i}>"),
            AboxObject::CarrierLiteral(value) => {
                format!("\"{}\"@{X_GMEOW_ENGLISH}", nq_escape(&value))
            }
        };
        writeln!(out, "<{subject_iri}> <{predicate}> {object_text} .").expect("write to String");
    }
}

/// Project the reconciliation A-Box (components, claims, reconciled pins, embeds)
/// into deterministic N-Triples for the `graph/provenance` named graph.
fn project_substrate_graph(components: &[Component], claims: &[Claim], embeds: &[Embed]) -> String {
    let mut out = String::new();

    // ── components ───────────────────────────────────────────────────────────────
    for comp in components {
        let c_iri = iri("component", &comp.slug);
        // Dual-typed SubstrateComponent AND the generic gmeow:Component so the
        // generic gmeow:PinCoverageConstraint (sh:targetClass gmeow:Component) targets
        // it without depending on subclass materialization at the shape stage.
        triple_iri(
            &mut out,
            &c_iri,
            RDF_TYPE,
            &format!("{GMEOW}SubstrateComponent"),
        );
        triple_iri(&mut out, &c_iri, RDF_TYPE, &format!("{GMEOW}Component"));
        triple_lit(
            &mut out,
            &c_iri,
            &format!("{GMEOW}componentName"),
            &comp.name,
        );
        for site in &comp.expected_sites {
            triple_iri(
                &mut out,
                &c_iri,
                &format!("{GMEOW}expectedClaimSite"),
                &format!("{GMEOW}{site}"),
            );
        }
        annotate(
            &mut out,
            &c_iri,
            &format!("substrate component {}", comp.name),
            &format!(
                "The {} build component, reconciled across its claim sites.",
                comp.name
            ),
        );
    }

    // ── per-site pin claims ──────────────────────────────────────────────────────
    for claim in claims {
        // The witness (e.g. the shipping engine) keeps otherwise-identical same-site
        // claims DISTINCT, so cross-witness disagreement is caught by
        // gmeow:PinAgreementConstraint instead of collapsing to one IRI.
        let claim_slug = match &claim.witness {
            Some(w) => format!(
                "{}-{}-{}-{}",
                claim.component_slug,
                site_local(claim.site),
                dim_local(claim.dimension),
                w
            ),
            None => format!(
                "{}-{}-{}",
                claim.component_slug,
                site_local(claim.site),
                dim_local(claim.dimension)
            ),
        };
        let claim_iri = iri("claim", &claim_slug);
        let comp_iri = iri("component", &claim.component_slug);
        triple_iri(&mut out, &claim_iri, RDF_TYPE, &format!("{GMEOW}PinClaim"));
        triple_iri(
            &mut out,
            &claim_iri,
            &format!("{GMEOW}claimedComponent"),
            &comp_iri,
        );
        triple_iri(
            &mut out,
            &claim_iri,
            &format!("{GMEOW}claimSite"),
            &format!("{GMEOW}{}", claim.site),
        );
        triple_iri(
            &mut out,
            &claim_iri,
            &format!("{GMEOW}claimDimension"),
            &format!("{GMEOW}{}", claim.dimension),
        );
        triple_lit(
            &mut out,
            &claim_iri,
            &format!("{GMEOW}claimedValue"),
            &claim.value,
        );
        annotate(
            &mut out,
            &claim_iri,
            &format!("pin claim {claim_slug}"),
            &format!(
                "The {} site asserts {} = {} for {}.",
                site_local(claim.site),
                dim_local(claim.dimension),
                claim.value,
                claim.component_slug
            ),
        );
    }

    // ── reconciled consensus pins (present sites agree) ──────────────────────────
    for (comp_slug, dim, value) in reconcile(claims) {
        let recon_slug = format!("{}-{}", comp_slug, dim_local(dim));
        let recon_iri = iri("reconciled", &recon_slug);
        let comp_iri = iri("component", &comp_slug);
        triple_iri(
            &mut out,
            &recon_iri,
            RDF_TYPE,
            &format!("{GMEOW}ReconciledPin"),
        );
        triple_iri(
            &mut out,
            &recon_iri,
            &format!("{GMEOW}reconciledComponent"),
            &comp_iri,
        );
        triple_iri(
            &mut out,
            &recon_iri,
            &format!("{GMEOW}reconciledDimension"),
            &format!("{GMEOW}{dim}"),
        );
        triple_lit(
            &mut out,
            &recon_iri,
            &format!("{GMEOW}reconciledValue"),
            &value,
        );
        // The reconciled crate version is the component's headline version.
        if dim == DIM_CRATE_VERSION {
            triple_lit(
                &mut out,
                &comp_iri,
                &format!("{GMEOW}componentVersion"),
                &value,
            );
        }
        annotate(
            &mut out,
            &recon_iri,
            &format!("reconciled pin {recon_slug}"),
            &format!(
                "The reconciled {} for {} is {} (all present sites agree).",
                dim_local(dim),
                comp_slug,
                value
            ),
        );
    }

    // ── embeds edges (SBOM contains) ─────────────────────────────────────────────
    for embed in embeds {
        let engine_iri = iri("component", &embed.engine_slug);
        let embedded_iri = iri("component", &embed.embedded_slug);
        triple_iri(
            &mut out,
            &engine_iri,
            &format!("{GMEOW}embeds"),
            &embedded_iri,
        );
    }

    // Byte-stable independent of emission order.
    let mut lines: Vec<&str> = out.lines().collect();
    lines.sort_unstable();
    lines.dedup();
    let mut sorted = lines.join("\n");
    sorted.push('\n');
    sorted
}

fn site_local(site: &str) -> &str {
    site.strip_prefix("claimSite").unwrap_or(site)
}
fn dim_local(dim: &str) -> &str {
    dim.strip_prefix("dimension").unwrap_or(dim)
}

/// Read every substrate claim site from `root` (all build inputs) plus the
/// compiled-in purrdf constants, reconcile, and project the `graph/provenance`
/// A-Box. A missing manifest/lock/stamp is a HARD FAIL (no silent degradation).
pub fn build_substrate_projection(root: &Path) -> Result<String, gmeow_errors::Diag> {
    let read = |rel: &str| -> Result<String, gmeow_errors::Diag> {
        std::fs::read_to_string(root.join(rel))
            .map_err(|e| stage_err(&format!("substrate carrier: reading {rel}: {e}")))
    };

    let purrdf = "purrdf";
    let mut components: Vec<Component> = Vec::new();
    let mut claims: Vec<Claim> = Vec::new();
    let mut embeds: Vec<Embed> = Vec::new();

    // A manifest declares compatibility; its lockfile selects an exact release.
    // Check all PurRDF component identities, then retain these as distinct claim
    // dimensions so a compatibility range is never emitted as a concrete version.
    gmeow_validate::substrate::verify_fuzz_substrate(root)?;
    for (manifest, lock, site, witness) in [
        (
            "Cargo.toml",
            "Cargo.lock",
            SITE_WORKSPACE_MANIFEST,
            "workspace",
        ),
        (
            "fuzz/Cargo.toml",
            "fuzz/Cargo.lock",
            SITE_FUZZ_MANIFEST,
            "fuzz",
        ),
    ] {
        let resolution =
            gmeow_validate::substrate::resolve_purrdf(&root.join(manifest), &root.join(lock))?;
        claims.push(Claim {
            component_slug: purrdf.into(),
            site,
            dimension: DIM_VERSION_REQUIREMENT,
            value: resolution.requirement,
            witness: None,
        });
        claims.push(Claim {
            component_slug: purrdf.into(),
            site: SITE_LOCKFILE,
            dimension: DIM_CRATE_VERSION,
            value: resolution.version,
            witness: Some(witness.into()),
        });
    }

    // #4/#5/#6 linked constants (compiled into this binary from the pinned dep).
    claims.push(Claim {
        component_slug: purrdf.into(),
        site: SITE_LINKED_CONSTANT,
        dimension: DIM_SHAPES_VERSION,
        value: purrdf::shapes::VERSION.to_string(),
        witness: None,
    });
    claims.push(Claim {
        component_slug: purrdf.into(),
        site: SITE_LINKED_CONSTANT,
        dimension: DIM_WIRE_VERSION,
        value: purrdf::gts::wire::VERSION.to_string(),
        witness: None,
    });
    claims.push(Claim {
        component_slug: purrdf.into(),
        site: SITE_LINKED_CONSTANT,
        dimension: DIM_ZSTD_LEVEL,
        value: purrdf::gts_compose::DIST_ZSTD_LEVEL.to_string(),
        witness: None,
    });

    // #7 shipped artifacts: each engine's SUBSTRATE.txt stamp → per-engine claims +
    // embeds edges. Every embedded component becomes a Component (SBOM package).
    let mut embedded_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for asset in ALL_ASSETS {
        let engine = asset.name;
        let stamp = read(&format!("crates/docs/assets/{engine}/SUBSTRATE.txt"))?;
        let engine_slug = format!("{engine}-engine");
        components.push(Component {
            slug: engine_slug.clone(),
            name: format!("{engine}-engine"),
            expected_sites: Vec::new(),
        });
        for (name, version) in parse_substrate_stamp(&stamp)? {
            let slug = checked_slug(&name)?;
            embedded_names.insert(slug.clone());
            embeds.push(Embed {
                engine_slug: engine_slug.clone(),
                embedded_slug: slug.clone(),
            });
            // The shipping engine is the witness, so each engine's stamp is a distinct
            // gmeow:PinClaim (drift BETWEEN engines is caught, not collapsed).
            claims.push(Claim {
                component_slug: slug,
                site: SITE_SHIPPED_ARTIFACT,
                dimension: DIM_CRATE_VERSION,
                value: version,
                witness: Some(engine.to_string()),
            });
        }
    }

    // #8 prose mention.
    if let Some(prose_version) = parse_prose_purrdf_version(&read("docs/research-objects.md")?) {
        claims.push(Claim {
            component_slug: purrdf.into(),
            site: SITE_PROSE,
            dimension: DIM_CRATE_VERSION,
            value: prose_version,
            witness: None,
        });
    }

    // Declare the components. purrdf is asserted at all six sites (its full set);
    // toolchain libraries (wasm-bindgen, binaryen) only at the shipped artifact.
    components.push(Component {
        slug: purrdf.into(),
        name: purrdf.into(),
        expected_sites: vec![
            SITE_WORKSPACE_MANIFEST,
            SITE_FUZZ_MANIFEST,
            SITE_LOCKFILE,
            SITE_LINKED_CONSTANT,
            SITE_SHIPPED_ARTIFACT,
            SITE_PROSE,
        ],
    });
    for name in embedded_names {
        if name == purrdf {
            continue;
        }
        components.push(Component {
            slug: name.clone(),
            name,
            expected_sites: vec![SITE_SHIPPED_ARTIFACT],
        });
    }

    // Determinism: sort every input collection before projection.
    components.sort_by(|a, b| a.slug.cmp(&b.slug));
    components.dedup();
    claims.sort();
    claims.dedup();
    embeds.sort();
    embeds.dedup();

    Ok(project_substrate_graph(&components, &claims, &embeds))
}

/// Project the reconciled substrate A-Box into an SPDX SBOM through the compiled
/// `spdx.rq` CONSTRUCT — the SAME projection authority every consumer view runs
/// through ([`crate::projections::project_graph`]), never a hand-authored second
/// emitter (projection purity). `spdx_rq` is the compiled query text threaded in from
/// the consumed stage-mappings product (`generated/queries/spdx.rq`), and the source is
/// [`build_substrate_projection`]'s reconciliation A-Box — all build INPUTS, so this
/// fold is non-self-referential like the A-Box it projects. Returns the SBOM as
/// N-Triples: one `spdx:Package` per engine/library, `spdx:name` + `spdx:versionInfo`
/// from the reconciled pin, and an `spdx:relationship … contains` per `gmeow:embeds`
/// edge. Deterministic: `project_graph` freeze-sorts its output.
pub fn build_substrate_sbom_projection(
    root: &Path,
    spdx_rq: &str,
) -> Result<String, gmeow_errors::Diag> {
    let abox = build_substrate_projection(root)?;
    crate::projections::project_graph(&abox, spdx_rq, &crate::projections::TagMap::default())
}

fn stage_err(msg: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-substrate".to_string(),
        message: msg.to_string(),
    })
}

#[path = "substrate_graph.tests.rs"]
#[cfg(test)]
mod tests;
