// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The dogfooded substrate reconciliation projection (issue 1672).
//!
//! Promotes the purrdf substrate's identity — today scattered across eight
//! mutually-unaware representations (two manifest pins, the lockfile, three
//! compiled-in constants, four shipped `.wasm` SUBSTRATE.txt stamps, and prose) —
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
//! Every claim value is read from a build INPUT — a manifest pin, the lockfile,
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

/// The canonical git-rev abbreviation width used to reconcile an abbreviated
/// manifest rev against the lockfile's full 40-char SHA. The manifest pins the
/// substrate by a short rev (a prefix of the resolved full SHA); truncating every
/// git-rev claim to this width makes two claims of the SAME commit compare equal
/// (no false drift, Audit #7) while a genuinely different commit still differs.
const GIT_REV_ABBREV: usize = 8;

/// The four committed docs `.wasm` engines whose `SUBSTRATE.txt` stamps record the
/// substrate they were statically built against (the shipped-artifact claim site).
const SHIPPED_ENGINES: &[&str] = &["gmn", "query", "reason", "validate"];

// ── claim-site + dimension value-vocabulary individuals (authored in the slice) ──
const SITE_WORKSPACE_MANIFEST: &str = "claimSiteWorkspaceManifest";
const SITE_FUZZ_MANIFEST: &str = "claimSiteFuzzManifest";
const SITE_LOCKFILE: &str = "claimSiteLockfile";
const SITE_LINKED_CONSTANT: &str = "claimSiteLinkedConstant";
const SITE_SHIPPED_ARTIFACT: &str = "claimSiteShippedArtifact";
const SITE_PROSE: &str = "claimSiteProse";

const DIM_CRATE_VERSION: &str = "dimensionCrateVersion";
const DIM_GIT_REV: &str = "dimensionGitRev";
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
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Claim {
    component_slug: String,
    site: &'static str,
    dimension: &'static str,
    value: String,
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
        root.join("docs/research-objects.md"),
    ];
    for engine in SHIPPED_ENGINES {
        paths.push(root.join(format!("crates/docs/assets/{engine}/SUBSTRATE.txt")));
    }
    paths
}

/// Normalize a git rev to its canonical abbreviated form so an abbreviated manifest
/// rev and the lockfile's full SHA for the SAME commit reconcile to one value.
fn normalize_git_rev(rev: &str) -> String {
    rev.chars().take(GIT_REV_ABBREV).collect()
}

/// Extract purrdf's pinned git rev from a Cargo manifest line
/// `purrdf = { git = "...", rev = "<rev>" }`. Returns the raw rev (un-normalized).
fn parse_manifest_git_rev(manifest: &str) -> Option<String> {
    for line in manifest.lines() {
        let trimmed = line.trim_start();
        if !(trimmed.starts_with("purrdf ") || trimmed.starts_with("purrdf=")) {
            continue;
        }
        let Some(idx) = trimmed.find("rev") else {
            continue;
        };
        let after = &trimmed[idx..];
        let Some(q1) = after.find('"') else { continue };
        let rest = &after[q1 + 1..];
        if let Some(q2) = rest.find('"') {
            return Some(rest[..q2].to_string());
        }
    }
    None
}

/// Parse the `[[package]] name = "purrdf"` block of a Cargo.lock, returning
/// `(crate_version, full_git_rev)`. The full rev is the SHA after `#` in `source`.
fn parse_lock_purrdf(lock: &str) -> Option<(String, Option<String>)> {
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "name = \"purrdf\"" {
            let mut version = None;
            let mut full_rev = None;
            for follow in lines.by_ref() {
                let t = follow.trim();
                if t.starts_with("[[") {
                    break;
                }
                if let Some(v) = t.strip_prefix("version = \"") {
                    version = v.strip_suffix('"').map(str::to_string);
                } else if let Some(src) = t.strip_prefix("source = \"")
                    && let Some(hash) = src.find('#')
                {
                    let after = &src[hash + 1..];
                    full_rev = Some(after.trim_end_matches('"').to_string());
                }
            }
            return version.map(|v| (v, full_rev));
        }
    }
    None
}

/// Parse a `SUBSTRATE.txt` stamp of the form
/// `purrdf 0.12.0; wasm-bindgen 0.2.125; binaryen version_130` into `(name, version)`
/// pairs, in file order.
fn parse_substrate_stamp(stamp: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let line = stamp.lines().next().unwrap_or("").trim();
    for part in line.split(';') {
        let mut it = part.split_whitespace();
        if let (Some(name), Some(version)) = (it.next(), it.next()) {
            out.push((name.to_string(), version.to_string()));
        }
    }
    out
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

/// Emit the four canonical assertional-tier A-Box annotations for `subject_iri`
/// (`rdfs:label` / `skos:definition`-free per assertional tier /
/// `rdfs:isDefinedBy <graph/provenance>` / `gmeow:graphBoxRole gmeow:boxABox`),
/// routed through the shared [`abox_annotations`] contract every generated
/// individual satisfies identically.
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
        let claim_slug = format!(
            "{}-{}-{}",
            claim.component_slug,
            site_local(claim.site),
            dim_local(claim.dimension)
        );
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

/// Read the eight substrate claim sites from `root` (all build INPUTS) plus the
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

    // #1 workspace manifest git rev, #2 fuzz manifest git rev.
    let ws_rev = parse_manifest_git_rev(&read("Cargo.toml")?)
        .ok_or_else(|| stage_err("substrate carrier: no purrdf rev in Cargo.toml"))?;
    claims.push(Claim {
        component_slug: purrdf.into(),
        site: SITE_WORKSPACE_MANIFEST,
        dimension: DIM_GIT_REV,
        value: normalize_git_rev(&ws_rev),
    });
    let fuzz_rev = parse_manifest_git_rev(&read("fuzz/Cargo.toml")?)
        .ok_or_else(|| stage_err("substrate carrier: no purrdf rev in fuzz/Cargo.toml"))?;
    claims.push(Claim {
        component_slug: purrdf.into(),
        site: SITE_FUZZ_MANIFEST,
        dimension: DIM_GIT_REV,
        value: normalize_git_rev(&fuzz_rev),
    });

    // #3 lockfile crate version + full git rev.
    let (lock_version, lock_full_rev) = parse_lock_purrdf(&read("Cargo.lock")?)
        .ok_or_else(|| stage_err("substrate carrier: no purrdf entry in Cargo.lock"))?;
    claims.push(Claim {
        component_slug: purrdf.into(),
        site: SITE_LOCKFILE,
        dimension: DIM_CRATE_VERSION,
        value: lock_version.clone(),
    });
    if let Some(full) = lock_full_rev {
        claims.push(Claim {
            component_slug: purrdf.into(),
            site: SITE_LOCKFILE,
            dimension: DIM_GIT_REV,
            value: normalize_git_rev(&full),
        });
    }

    // #4/#5/#6 linked constants (compiled into this binary from the pinned dep).
    claims.push(Claim {
        component_slug: purrdf.into(),
        site: SITE_LINKED_CONSTANT,
        dimension: DIM_SHAPES_VERSION,
        value: purrdf::shapes::VERSION.to_string(),
    });
    claims.push(Claim {
        component_slug: purrdf.into(),
        site: SITE_LINKED_CONSTANT,
        dimension: DIM_WIRE_VERSION,
        value: purrdf::gts::wire::VERSION.to_string(),
    });
    claims.push(Claim {
        component_slug: purrdf.into(),
        site: SITE_LINKED_CONSTANT,
        dimension: DIM_ZSTD_LEVEL,
        value: purrdf::gts_compose::DIST_ZSTD_LEVEL.to_string(),
    });

    // #7 shipped artifacts: each engine's SUBSTRATE.txt stamp → per-engine claims +
    // embeds edges. Every embedded component becomes a Component (SBOM package).
    let mut embedded_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for engine in SHIPPED_ENGINES {
        let stamp = read(&format!("crates/docs/assets/{engine}/SUBSTRATE.txt"))?;
        let engine_slug = format!("{engine}-engine");
        components.push(Component {
            slug: engine_slug.clone(),
            name: format!("{engine}-engine"),
            expected_sites: Vec::new(),
        });
        for (name, version) in parse_substrate_stamp(&stamp) {
            let slug = name.clone();
            embedded_names.insert(name.clone());
            embeds.push(Embed {
                engine_slug: engine_slug.clone(),
                embedded_slug: slug.clone(),
            });
            claims.push(Claim {
                component_slug: slug,
                site: SITE_SHIPPED_ARTIFACT,
                dimension: DIM_CRATE_VERSION,
                value: version,
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

fn stage_err(msg: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-substrate".to_string(),
        message: msg.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (Vec<Component>, Vec<Claim>, Vec<Embed>) {
        let components = vec![
            Component {
                slug: "purrdf".into(),
                name: "purrdf".into(),
                expected_sites: vec![SITE_LOCKFILE, SITE_SHIPPED_ARTIFACT],
            },
            Component {
                slug: "gmn-engine".into(),
                name: "gmn-engine".into(),
                expected_sites: vec![],
            },
        ];
        let claims = vec![
            Claim {
                component_slug: "purrdf".into(),
                site: SITE_LOCKFILE,
                dimension: DIM_CRATE_VERSION,
                value: "0.12.0".into(),
            },
            Claim {
                component_slug: "purrdf".into(),
                site: SITE_SHIPPED_ARTIFACT,
                dimension: DIM_CRATE_VERSION,
                value: "0.12.0".into(),
            },
        ];
        let embeds = vec![Embed {
            engine_slug: "gmn-engine".into(),
            embedded_slug: "purrdf".into(),
        }];
        (components, claims, embeds)
    }

    #[test]
    fn projection_is_byte_deterministic() {
        let (c, cl, e) = sample();
        let a = project_substrate_graph(&c, &cl, &e);
        let mut c2 = c.clone();
        c2.reverse();
        let mut cl2 = cl.clone();
        cl2.reverse();
        let b = project_substrate_graph(&c2, &cl2, &e);
        assert_eq!(a, b, "projection must be byte-stable across input order");
    }

    #[test]
    fn projection_carries_no_runtime_ids() {
        // Every substrate node IRI is built from a PUBLIC slug (component name, site,
        // dimension) — never an opaque runtime id. So no IRI in the gmeow substrate
        // namespace carries a `#` fragment (RDF predicate IRIs like `…-ns#type`
        // legitimately do, so the check is scoped to the substrate namespace) and none
        // carries a synthetic `unit#`/`artifact#`/`origin-set#` id.
        let (c, cl, e) = sample();
        let nt = project_substrate_graph(&c, &cl, &e);
        for id in ["unit#", "artifact#", "origin-set#"] {
            assert!(
                !nt.contains(id),
                "no runtime {id} id may leak into the graph"
            );
        }
        for token in nt.split_whitespace() {
            if let Some(rest) =
                token.strip_prefix("<https://blackcatinformatics.ca/gmeow/substrate/")
            {
                let iri = rest.trim_end_matches('>');
                assert!(
                    !iri.contains('#'),
                    "substrate IRI must be built from a public slug, not an opaque id: {token}"
                );
            }
        }
    }

    #[test]
    fn substrate_inputs_are_all_build_inputs_never_generated() {
        // The non-fixpoint property (Audit #7 / SOLID finding 4): every claim value
        // derives from a repo INPUT, never a render-produced digest under generated/.
        let root = Path::new("/repo");
        for p in substrate_input_paths(root) {
            let s = p.to_string_lossy();
            assert!(
                !s.contains("/generated/"),
                "substrate input {s} must be a build input, not a generated artifact"
            );
        }
    }

    #[test]
    fn agreeing_sites_reconcile_disagreeing_do_not() {
        let agree = vec![
            Claim {
                component_slug: "p".into(),
                site: SITE_LOCKFILE,
                dimension: DIM_CRATE_VERSION,
                value: "0.12.0".into(),
            },
            Claim {
                component_slug: "p".into(),
                site: SITE_PROSE,
                dimension: DIM_CRATE_VERSION,
                value: "0.12.0".into(),
            },
        ];
        assert_eq!(
            reconcile(&agree).len(),
            1,
            "agreeing sites reconcile to one value"
        );
        let disagree = vec![
            Claim {
                component_slug: "p".into(),
                site: SITE_LOCKFILE,
                dimension: DIM_CRATE_VERSION,
                value: "0.12.0".into(),
            },
            Claim {
                component_slug: "p".into(),
                site: SITE_PROSE,
                dimension: DIM_CRATE_VERSION,
                value: "0.13.0".into(),
            },
        ];
        assert!(
            reconcile(&disagree).is_empty(),
            "disagreeing sites leave no reconciled pin (drift)"
        );
    }

    #[test]
    fn abbreviated_and_full_git_rev_reconcile() {
        // Audit #7: the manifest's abbreviated rev and the lockfile's full SHA for the
        // same commit must reconcile to one value (no false drift on the first build).
        let short = normalize_git_rev("a59d9f2d");
        let full = normalize_git_rev("a59d9f2dd538594d1390775cb70f85f3be7673e7");
        assert_eq!(
            short, full,
            "same-commit revs of different width must normalize equal"
        );
        let other = normalize_git_rev("b1234567deadbeef");
        assert_ne!(short, other, "a genuinely different commit stays distinct");
    }

    #[test]
    fn parses_manifest_lock_stamp_and_prose() {
        assert_eq!(
            parse_manifest_git_rev(r#"purrdf = { git = "https://x", rev = "a59d9f2d" }"#)
                .as_deref(),
            Some("a59d9f2d")
        );
        let (v, rev) = parse_lock_purrdf(
            "[[package]]\nname = \"purrdf\"\nversion = \"0.12.0\"\nsource = \"git+https://x?rev=a59d9f2d#a59d9f2dd538\"\n[[package]]\n",
        )
        .expect("lock parses");
        assert_eq!(v, "0.12.0");
        assert_eq!(rev.as_deref(), Some("a59d9f2dd538"));
        let stamp =
            parse_substrate_stamp("purrdf 0.12.0; wasm-bindgen 0.2.125; binaryen version_130\n");
        assert_eq!(stamp[0], ("purrdf".into(), "0.12.0".into()));
        assert_eq!(stamp[2], ("binaryen".into(), "version_130".into()));
        assert_eq!(
            parse_prose_purrdf_version("projected by the Rust **purrdf 0.12.0 engine").as_deref(),
            Some("0.12.0")
        );
    }

    #[test]
    fn covers_all_six_claim_sites_and_reconciles_purrdf() {
        // The full build read against the real repo: purrdf appears at all six sites,
        // its crate version and git rev reconcile, and the graph carries embeds edges.
        let root = crate_repo_root();
        let nt = build_substrate_projection(&root).expect("substrate projection builds");
        for site in [
            SITE_WORKSPACE_MANIFEST,
            SITE_FUZZ_MANIFEST,
            SITE_LOCKFILE,
            SITE_LINKED_CONSTANT,
            SITE_SHIPPED_ARTIFACT,
            SITE_PROSE,
        ] {
            assert!(
                nt.contains(&format!("{GMEOW}{site}")),
                "the substrate graph must carry a claim at every site; missing {site}"
            );
        }
        assert!(
            nt.contains(&format!("{GMEOW}SubstrateComponent")),
            "≥1 SubstrateComponent"
        );
        assert!(
            nt.contains(&format!("{GMEOW}ReconciledPin")),
            "≥1 ReconciledPin (sites agree)"
        );
        assert!(
            nt.contains(&format!("{GMEOW}embeds")),
            "≥1 embeds edge (SBOM contains)"
        );
    }

    fn crate_repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .to_path_buf()
    }
}
