// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native alignment-direction lint scaffolding (#936 Task 1) + inverse-direction and
//! domain-range checks (#936 Task 2).
//!
//! This module ports the input-loading and the inverse-direction / domain-range checks
//! from `src/gmeow_tools/alignment_lint.py` into `gmeow-slice`. The remaining checks
//! (property-character, equivalence-collapse, DC refinement) are Tasks 3–4.
//!
//! The diagnostic carrier is the existing [`ProjectionDiagnostic`] from
//! [`crate::projection_lint`]; no new diagnostic struct is introduced.

#![allow(dead_code)] // constants used by Tasks 3–4

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::{GraphNameRef, NamedNode, NamedOrBlankNode, Term};
use oxigraph::store::Store;

use crate::error::SliceError;
use crate::fno_emit::collect_ontology_store;
use crate::mapping_emit::PREFIX_REGISTRY;
use crate::projection_lint::ProjectionDiagnostic;

// ── Predicate / class constants (ported from alignment_lint.py) ───────────────

/// Predicate CURIEs whose alignment asserts (near-)equivalence for properties.
/// PUBLIC: the saturator may materialize cross-vocabulary triples only for these.
pub(crate) const STRONG_PROPERTY_PREDICATES: &[&str] =
    &["owl:equivalentProperty", "skos:exactMatch"];

/// Class-level strong equivalence (the collapse gate's edge set).
pub(crate) const STRONG_CLASS_PREDICATES: &[&str] = &["owl:equivalentClass", "skos:exactMatch"];

/// Intentionally directional/hierarchical predicates — exempt from direction checks.
pub(crate) const HIERARCHICAL_PREDICATES: &[&str] =
    &["skos:broadMatch", "skos:narrowMatch", "rdfs:subPropertyOf"];

/// Strength rank used to pick the canonical term in a self-contradicting pair.
pub(crate) const PREDICATE_RANK: &[(&str, i32)] = &[
    ("owl:equivalentProperty", 3),
    ("skos:exactMatch", 3),
    ("skos:closeMatch", 1),
];

/// OWL property-character types read from `rdf:type` assertions.
pub(crate) const CHARACTER_TYPES: &[&str] = &[
    "http://www.w3.org/2002/07/owl#FunctionalProperty",
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
    "http://www.w3.org/2002/07/owl#TransitiveProperty",
    "http://www.w3.org/2002/07/owl#SymmetricProperty",
];

/// OWL property-typing terms. A target that uses none of these does not speak the
/// OWL characteristic vocabulary, so a character comparison would be noise.
pub(crate) const OWL_PROPERTY_TYPES: &[&str] = &[
    "http://www.w3.org/2002/07/owl#ObjectProperty",
    "http://www.w3.org/2002/07/owl#DatatypeProperty",
    "http://www.w3.org/2002/07/owl#FunctionalProperty",
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
    "http://www.w3.org/2002/07/owl#TransitiveProperty",
    "http://www.w3.org/2002/07/owl#SymmetricProperty",
    "http://www.w3.org/2002/07/owl#AsymmetricProperty",
];

/// dcterms refinements → broader dcterms element (per DCMI specification).
pub(crate) const DCTERMS_REFINEMENTS: &[(&str, &str)] = &[
    // description refinements
    ("dcterms:abstract", "dcterms:description"),
    ("dcterms:tableOfContents", "dcterms:description"),
    // date refinements
    ("dcterms:created", "dcterms:date"),
    ("dcterms:modified", "dcterms:date"),
    ("dcterms:issued", "dcterms:date"),
    ("dcterms:valid", "dcterms:date"),
    ("dcterms:available", "dcterms:date"),
    ("dcterms:dateAccepted", "dcterms:date"),
    ("dcterms:dateCopyrighted", "dcterms:date"),
    ("dcterms:dateSubmitted", "dcterms:date"),
    // relation refinements
    ("dcterms:references", "dcterms:relation"),
    ("dcterms:isReferencedBy", "dcterms:relation"),
    ("dcterms:requires", "dcterms:relation"),
    ("dcterms:isRequiredBy", "dcterms:relation"),
    ("dcterms:replaces", "dcterms:relation"),
    ("dcterms:isReplacedBy", "dcterms:relation"),
    ("dcterms:hasPart", "dcterms:relation"),
    ("dcterms:isPartOf", "dcterms:relation"),
    ("dcterms:hasVersion", "dcterms:relation"),
    ("dcterms:isVersionOf", "dcterms:relation"),
    ("dcterms:conformsTo", "dcterms:relation"),
    // rights refinements
    ("dcterms:license", "dcterms:rights"),
    ("dcterms:rightsHolder", "dcterms:rights"),
    ("dcterms:accessRights", "dcterms:rights"),
    // coverage refinements
    ("dcterms:spatial", "dcterms:coverage"),
    ("dcterms:temporal", "dcterms:coverage"),
    // format refinements
    ("dcterms:extent", "dcterms:format"),
    ("dcterms:medium", "dcterms:format"),
    // identifier refinements
    ("dcterms:bibliographicCitation", "dcterms:identifier"),
];

/// Grandfathered hand-authored `dc:` alignments (existing before issue #60).
pub(crate) const GRANDFATHERED_DC: &[&str] = &["dc:rights"];

// ── Namespace constants ───────────────────────────────────────────────────────

const GMEOW_PREFIX: &str = "gmeow:";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_SUB_CLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_SYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";

const SCHEMA_INVERSE_OF: &str = "https://schema.org/inverseOf";
const SCHEMA_DOMAIN_INCLUDES: &str = "https://schema.org/domainIncludes";
const SCHEMA_RANGE_INCLUDES: &str = "https://schema.org/rangeIncludes";

// ── Native model ───────────────────────────────────────────────────────────────

/// One SSSOM mapping row — the subset the alignment-direction lint consumes.
/// Mirrors the Python `Mapping` dataclass (subject_id, predicate_id, object_id,
/// confidence, mapping_justification).
#[derive(Debug, Clone)]
pub(crate) struct Mapping {
    pub subject_id: String,
    pub predicate_id: String,
    pub object_id: String,
    pub confidence: String,
    pub mapping_justification: String,
}

/// Keys judged by the inverse-direction check so domain-range does not double-report.
type JudgedSet = BTreeSet<(String, String, String)>;

// ── Public entry point ─────────────────────────────────────────────────────────

/// Lint SSSOM property mappings for inverse / mismatched target terms.
///
/// Loads the ontology, SSSOM mapping tables, and target-axiom snapshots/fixtures,
/// then runs the inverse-direction and domain-range checks.
///
/// # Errors
///
/// Returns [`SliceError`] on any missing/unparsable required source (the ontology,
/// SSSOM mapping tables) — no degraded fallback for required inputs.
pub(crate) fn lint_alignment_directions(
    root: &Path,
    allow_network: bool,
) -> Result<Vec<ProjectionDiagnostic>, SliceError> {
    let onto = collect_ontology_store(root)?;
    let mappings = load_sssom_mappings(root)?;

    // Group the property mappings (subject is a GMEOW property, object is a known
    // alignment target) by the GMEOW property they align.
    let mut gmeow_props: BTreeMap<String, Vec<Mapping>> = BTreeMap::new();
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    for m in &mappings {
        if !m.subject_id.starts_with(GMEOW_PREFIX) {
            continue;
        }
        let Some(prefix) = prefix_of(&m.object_id) else {
            continue;
        };
        let Some(subj_iri) = expand_curie(&m.subject_id) else {
            continue;
        };
        if !is_property(&onto, &subj_iri) {
            continue;
        }
        gmeow_props.entry(subj_iri).or_default().push(m.clone());
        referenced.insert(prefix);
    }

    let mut findings: Vec<ProjectionDiagnostic> = Vec::new();

    if allow_network {
        // TODO(#936): implement network fetch for --network.
        findings.push(ProjectionDiagnostic {
            severity: "INFO".to_owned(),
            check: "domain-range".to_owned(),
            code: "domain-range".to_owned(),
            message: "network fetch for target axioms is not yet implemented (#936)".to_owned(),
            instance: None,
        });
    }

    let target_graphs = load_target_axiom_stores(root, &referenced)?;

    // Emit an INFO finding for every referenced prefix with no axioms available
    // (per mapping row, matching Python `_info_unavailable`).
    for prop_mappings in gmeow_props.values() {
        for m in prop_mappings {
            let prefix = prefix_of(&m.object_id).expect("filtered to alignment targets");
            if !target_graphs.contains_key(&prefix) {
                findings.push(info_unavailable(m, &prefix));
            }
        }
    }

    // Built after target graphs so the bridge can ingest their internal taxonomies
    // (and GMEOW's) alongside the cross-vocabulary SSSOM mappings.
    let bridge = build_class_bridge(&mappings, &onto, &target_graphs);

    let (inverse_findings, judged) =
        check_inverse_direction(&gmeow_props, &onto, &target_graphs, &bridge)?;
    findings.extend(inverse_findings);

    let domain_findings =
        check_domain_range(&gmeow_props, &onto, &target_graphs, &bridge, &judged)?;
    findings.extend(domain_findings);

    // Stable severity-first ordering, matching Python.
    findings.sort_by(|a, b| {
        let order = |s: &str| match s {
            "ERROR" => 0,
            "WARNING" => 1,
            "INFO" => 2,
            _ => 3,
        };
        order(&a.severity)
            .cmp(&order(&b.severity))
            .then_with(|| a.check.cmp(&b.check))
            .then_with(|| a.instance.cmp(&b.instance))
    });

    Ok(findings)
}

// ── Inverse-direction check ────────────────────────────────────────────────────

/// Self-contradiction detector + domain/range orientation fallback.
///
/// Returns the findings and the set of `(subject_id, predicate_id, object_id)` keys
/// it has already judged (so the domain/range check does not double-report them).
fn check_inverse_direction(
    gmeow_props: &BTreeMap<String, Vec<Mapping>>,
    onto: &Store,
    target_graphs: &BTreeMap<String, Store>,
    bridge: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(Vec<ProjectionDiagnostic>, JudgedSet), SliceError> {
    let mut findings: Vec<ProjectionDiagnostic> = Vec::new();
    let mut judged: BTreeSet<(String, String, String)> = BTreeSet::new();

    for (prop, prop_mappings) in gmeow_props {
        if has_type(onto, prop, OWL_SYMMETRIC_PROPERTY)? {
            continue; // a symmetric property may legitimately map to inverses
        }

        // Index this property's target mappings by the resolved object IRI.
        let mut by_iri: BTreeMap<String, Mapping> = BTreeMap::new();
        for m in prop_mappings {
            if HIERARCHICAL_PREDICATES.contains(&m.predicate_id.as_str()) {
                continue;
            }
            if let Some(obj_iri) = expand_curie(&m.object_id) {
                by_iri.insert(obj_iri, m.clone());
            }
        }

        let g_dom = objects_iri(onto, prop, RDFS_DOMAIN)?;
        let g_rng = objects_iri(onto, prop, RDFS_RANGE)?;

        let mut seen_pairs: BTreeSet<[String; 2]> = BTreeSet::new();
        for (target_iri, m) in &by_iri {
            let Some(prefix) = prefix_of(&m.object_id) else {
                continue;
            };
            let Some(graph) = target_graphs.get(&prefix) else {
                continue;
            };
            let inverses = target_inverses(graph, target_iri)?;

            // (1) Self-contradiction: the property maps to both T and an inverse.
            for inv in &inverses {
                if inv == target_iri {
                    continue; // a self-inverse (symmetric) target is not a conflict
                }
                let Some(m_inv) = by_iri.get(inv) else {
                    continue;
                };
                let mut pair = [target_iri.clone(), inv.clone()];
                pair.sort();
                if seen_pairs.contains(&pair) {
                    continue;
                }
                seen_pairs.insert(pair);

                let (canonical, offender) = rank_pair(m, m_inv);
                let key = (
                    offender.subject_id.clone(),
                    offender.predicate_id.clone(),
                    offender.object_id.clone(),
                );
                judged.insert(key);

                // The contradiction is definite only when one side is a strong
                // equivalence anchoring the canonical direction; two unanchored
                // closeMatches to inverse terms is suspicious but not conclusive.
                let severity =
                    if STRONG_PROPERTY_PREDICATES.contains(&canonical.predicate_id.as_str()) {
                        "ERROR"
                    } else {
                        "WARNING"
                    };
                let message = format!(
                    "mapped to {}, but the property is also mapped to its declared inverse {} \
                     (via {}) — one direction is wrong",
                    offender.object_id, canonical.object_id, canonical.predicate_id
                );
                let suggestion = canonical.object_id.clone();
                findings.push(ProjectionDiagnostic {
                    severity: severity.to_owned(),
                    check: "inverse-direction".to_owned(),
                    code: "inverse-direction".to_owned(),
                    message: format!("{message} — did you mean {suggestion}?"),
                    instance: expand_curie(&offender.object_id),
                });
            }

            // (2) Orientation fallback: only the wrong term is mapped, but its
            //     inverse fits the GMEOW direction and it does not.
            let key = (
                m.subject_id.clone(),
                m.predicate_id.clone(),
                m.object_id.clone(),
            );
            if judged.contains(&key) {
                continue;
            }
            let t_dom = target_domain(graph, target_iri)?;
            let t_rng = target_range(graph, target_iri)?;
            if t_dom.is_empty() || t_rng.is_empty() || g_dom.is_empty() || g_rng.is_empty() {
                continue;
            }
            let direct_fit = overlaps(&g_dom, &t_dom, bridge) && overlaps(&g_rng, &t_rng, bridge);
            if direct_fit {
                continue;
            }
            for inv in &inverses {
                if inv == target_iri {
                    continue; // self-inverse: its orientation equals the direct one
                }
                let inv_dom = target_domain(graph, inv)?;
                let inv_rng = target_range(graph, inv)?;
                if inv_dom.is_empty() || inv_rng.is_empty() {
                    continue;
                }
                if overlaps(&g_dom, &inv_dom, bridge) && overlaps(&g_rng, &inv_rng, bridge) {
                    judged.insert(key);
                    let inv_curie = shorten_iri(inv);
                    findings.push(ProjectionDiagnostic {
                        severity: severity_for(&m.predicate_id).to_owned(),
                        check: "inverse-direction".to_owned(),
                        code: "inverse-direction".to_owned(),
                        message: format!(
                            "{}'s domain/range is inverted relative to {}; its inverse {} \
                             matches the direction — did you mean {}?",
                            m.object_id, m.subject_id, inv_curie, inv_curie
                        ),
                        instance: expand_curie(&m.object_id),
                    });
                    break;
                }
            }
        }
    }

    Ok((findings, judged))
}

// ── Domain-range check ─────────────────────────────────────────────────────────

/// Flag mappings whose GMEOW domain/range is incompatible with the target's.
fn check_domain_range(
    gmeow_props: &BTreeMap<String, Vec<Mapping>>,
    onto: &Store,
    target_graphs: &BTreeMap<String, Store>,
    bridge: &BTreeMap<String, BTreeSet<String>>,
    judged: &JudgedSet,
) -> Result<Vec<ProjectionDiagnostic>, SliceError> {
    let mut findings: Vec<ProjectionDiagnostic> = Vec::new();
    for (prop, prop_mappings) in gmeow_props {
        let g_dom = objects_iri(onto, prop, RDFS_DOMAIN)?;
        let g_rng = objects_iri(onto, prop, RDFS_RANGE)?;
        for m in prop_mappings {
            if HIERARCHICAL_PREDICATES.contains(&m.predicate_id.as_str()) {
                continue;
            }
            let key = (
                m.subject_id.clone(),
                m.predicate_id.clone(),
                m.object_id.clone(),
            );
            if judged.contains(&key) {
                continue;
            }
            let Some(prefix) = prefix_of(&m.object_id) else {
                continue;
            };
            let Some(graph) = target_graphs.get(&prefix) else {
                continue;
            };
            let Some(target_iri) = expand_curie(&m.object_id) else {
                continue;
            };
            let t_dom = target_domain(graph, &target_iri)?;
            let t_rng = target_range(graph, &target_iri)?;
            if t_dom.is_empty() || t_rng.is_empty() {
                findings.push(info_not_checkable(
                    m,
                    "target term declares no domain/range to check against",
                ));
                continue;
            }
            if g_dom.is_empty() || g_rng.is_empty() {
                findings.push(info_not_checkable(
                    m,
                    "GMEOW term declares no domain/range to check against",
                ));
                continue;
            }
            if overlaps(&g_dom, &t_dom, bridge) && overlaps(&g_rng, &t_rng, bridge) {
                continue; // direct orientation agrees
            }
            let swapped = overlaps(&g_dom, &t_rng, bridge) && overlaps(&g_rng, &t_dom, bridge);
            if !swapped {
                findings.push(ProjectionDiagnostic {
                    severity: "INFO".to_owned(),
                    check: "domain-range".to_owned(),
                    code: "domain-range".to_owned(),
                    message: "domain/range overlap could not be established \
                              (no class bridge to the target's domain/range)"
                        .to_owned(),
                    instance: Some(target_iri),
                });
                continue;
            }
            findings.push(ProjectionDiagnostic {
                severity: severity_for(&m.predicate_id).to_owned(),
                check: "domain-range".to_owned(),
                code: "domain-range".to_owned(),
                message: "domain/range are inverted relative to the target term".to_owned(),
                instance: Some(target_iri),
            });
        }
    }
    Ok(findings)
}

// ── Class-equivalence bridge ───────────────────────────────────────────────────

/// Build a class-compatibility closure for domain/range overlap testing.
///
/// Three sources feed the closure:
/// * the cross-vocabulary SSSOM class mappings (`owl:equivalentClass`/
///   `skos:exactMatch` link both directions; `rdfs:subClassOf` links the
///   subclass up to its superclass);
/// * GMEOW-internal `rdfs:subClassOf`/`owl:equivalentClass` axioms;
/// * the same axioms inside each target snapshot.
fn build_class_bridge(
    mappings: &[Mapping],
    onto: &Store,
    target_graphs: &BTreeMap<String, Store>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut adjacency: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    let mut link = |a: String, b: String| {
        if a != b {
            adjacency.entry(a).or_default().insert(b);
        }
    };

    for m in mappings {
        let (Some(subj), Some(obj)) = (expand_curie(&m.subject_id), expand_curie(&m.object_id))
        else {
            continue;
        };
        if STRONG_CLASS_PREDICATES.contains(&m.predicate_id.as_str()) {
            link(subj.clone(), obj.clone());
            link(obj, subj);
        } else if m.predicate_id == "rdfs:subClassOf" {
            link(subj, obj);
        }
    }

    // Internal taxonomy of GMEOW and of every loaded target snapshot.
    let mut graphs: Vec<&Store> = vec![onto];
    graphs.extend(target_graphs.values());
    for graph in graphs {
        for (sub, sup) in subject_objects_iri(graph, RDFS_SUB_CLASS_OF).unwrap_or_default() {
            link(sub, sup);
        }
        for (a, b) in subject_objects_iri(graph, OWL_EQUIVALENT_CLASS).unwrap_or_default() {
            link(a.clone(), b.clone());
            link(b, a);
        }
    }

    // Transitive closure (the graph is tiny — a simple fixpoint suffices).
    let mut closure: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for start in adjacency.keys() {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut stack = vec![start.clone()];
        while let Some(node) = stack.pop() {
            for nxt in adjacency.get(&node).into_iter().flatten() {
                if seen.insert(nxt.clone()) {
                    stack.push(nxt.clone());
                }
            }
        }
        closure.insert(start.clone(), seen);
    }
    closure
}

/// Return `iri` plus every class it is bridge-compatible with.
fn resolve_class(iri: &str, bridge: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    out.insert(iri.to_owned());
    if let Some(set) = bridge.get(iri) {
        out.extend(set.iter().cloned());
    }
    out
}

/// Whether any GMEOW class (bridge-expanded) meets any target class.
fn overlaps(
    gmeow_classes: &[String],
    target_classes: &[String],
    bridge: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    if gmeow_classes.is_empty() || target_classes.is_empty() {
        return false;
    }
    let target_set: BTreeSet<String> = target_classes.iter().cloned().collect();
    let mut expanded: BTreeSet<String> = BTreeSet::new();
    for cls in gmeow_classes {
        expanded.extend(resolve_class(cls, bridge));
    }
    expanded.iter().any(|c| target_set.contains(c))
}

// ── Target-axiom accessors ─────────────────────────────────────────────────────

/// Return a term's declared domains, normalizing `rdfs:domain` and
/// `schema:domainIncludes`.
fn target_domain(graph: &Store, term: &str) -> Result<Vec<String>, SliceError> {
    let mut out = objects_iri(graph, term, RDFS_DOMAIN)?;
    out.extend(objects_iri(graph, term, SCHEMA_DOMAIN_INCLUDES)?);
    Ok(out)
}

/// Return a term's declared ranges, normalizing `rdfs:range` and
/// `schema:rangeIncludes`.
fn target_range(graph: &Store, term: &str) -> Result<Vec<String>, SliceError> {
    let mut out = objects_iri(graph, term, RDFS_RANGE)?;
    out.extend(objects_iri(graph, term, SCHEMA_RANGE_INCLUDES)?);
    Ok(out)
}

/// Return a term's inverses, reading `owl:inverseOf`/`schema:inverseOf` both ways.
fn target_inverses(graph: &Store, term: &str) -> Result<Vec<String>, SliceError> {
    let node = named_node(term)?;
    let mut out = objects_iri(graph, term, OWL_INVERSE_OF)?;
    out.extend(objects_iri(graph, term, SCHEMA_INVERSE_OF)?);
    out.extend(subjects_iri(graph, OWL_INVERSE_OF, &node)?);
    out.extend(subjects_iri(graph, SCHEMA_INVERSE_OF, &node)?);
    out.sort();
    out.dedup();
    Ok(out)
}

// ── Target-axiom loading ───────────────────────────────────────────────────────

/// Load the axiom graph for each referenced target prefix.
///
/// Returns a map from prefix to its merged store (snapshot + fixture). Prefixes
/// with no available axioms are omitted; callers emit INFO findings for those.
fn load_target_axiom_stores(
    root: &Path,
    prefixes: &BTreeSet<String>,
) -> Result<BTreeMap<String, Store>, SliceError> {
    let mut out: BTreeMap<String, Store> = BTreeMap::new();
    for prefix in prefixes {
        let mut store = new_store()?;
        let mut has_axioms = false;

        if let Some(snapshot) = load_target_snapshot(root, prefix)? {
            merge_store(&mut store, &snapshot)?;
            has_axioms = true;
        }
        if let Some(fixture) = load_fixture(root, prefix)? {
            merge_store(&mut store, &fixture)?;
            has_axioms = true;
        }

        if has_axioms {
            out.insert(prefix.clone(), store);
        }
    }
    Ok(out)
}

/// Load a vendored target axiom snapshot from `imports/targets/<prefix>.ttl`, if
/// it exists.
fn load_target_snapshot(root: &Path, prefix: &str) -> Result<Option<Store>, SliceError> {
    let path = root
        .join("imports")
        .join("targets")
        .join(format!("{prefix}.ttl"));
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(parse_ttl(&path)?))
}

/// Load a hand-authored target fixture from `tests/fixtures/target_axioms/<prefix>.ttl`,
/// if it exists.
fn load_fixture(root: &Path, prefix: &str) -> Result<Option<Store>, SliceError> {
    let path = root
        .join("tests")
        .join("fixtures")
        .join("target_axioms")
        .join(format!("{prefix}.ttl"));
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(parse_ttl(&path)?))
}

// ── SSSOM mapping loading ──────────────────────────────────────────────────────

/// Load all SSSOM mapping rows from `generated/mappings/*.sssom.tsv`.
///
/// Mirrors Python `load_mappings(MAPPINGS_DIR)`, reading the committed generated
/// SSSOM tables. Comment/header lines starting with `#` are skipped; the first
/// non-comment line is the TSV header.
fn load_sssom_mappings(root: &Path) -> Result<Vec<Mapping>, SliceError> {
    let mappings_dir = root.join("generated").join("mappings");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    if mappings_dir.is_dir() {
        for entry in std::fs::read_dir(&mappings_dir).map_err(SliceError::Io)? {
            let entry = entry.map_err(SliceError::Io)?;
            let path = entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".sssom.tsv"))
            {
                files.push(path);
            }
        }
    }
    files.sort();

    let mut mappings: Vec<Mapping> = Vec::new();
    for path in &files {
        mappings.extend(parse_sssom_tsv(path)?);
    }
    Ok(mappings)
}

/// Parse one SSSOM TSV file into [`Mapping`] rows.
fn parse_sssom_tsv(path: &Path) -> Result<Vec<Mapping>, SliceError> {
    let text = std::fs::read_to_string(path).map_err(SliceError::Io)?;
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    if lines.is_empty() {
        return Ok(Vec::new());
    }

    let header = lines.remove(0);
    let columns: Vec<&str> = header.split('\t').collect();
    let idx = |name: &str| columns.iter().position(|c| *c == name);

    let subject_idx = idx("subject_id").ok_or_else(|| {
        SliceError::Parse(format!("{} missing subject_id column", path.display()))
    })?;
    let predicate_idx = idx("predicate_id").ok_or_else(|| {
        SliceError::Parse(format!("{} missing predicate_id column", path.display()))
    })?;
    let object_idx = idx("object_id")
        .ok_or_else(|| SliceError::Parse(format!("{} missing object_id column", path.display())))?;
    let justification_idx = idx("mapping_justification");
    let confidence_idx = idx("confidence");

    let mut rows: Vec<Mapping> = Vec::new();
    for line in &lines {
        if line.is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        let get = |i: usize| cells.get(i).unwrap_or(&"").to_string();
        rows.push(Mapping {
            subject_id: get(subject_idx),
            predicate_id: get(predicate_idx),
            object_id: get(object_idx),
            confidence: confidence_idx.map(get).unwrap_or_default(),
            mapping_justification: justification_idx.map(get).unwrap_or_default(),
        });
    }
    Ok(rows)
}

// ── CURIE / prefix helpers ─────────────────────────────────────────────────────

/// Known alignment-target prefixes (the keys of Python `ALIGNMENT_TARGETS`).
const ALIGNMENT_TARGETS: &[&str] = &[
    "gufo",
    "umbel",
    "dolce",
    "bfo",
    "foaf",
    "rel",
    "doap",
    "prov",
    "dqv",
    "org",
    "time",
    "schema",
    "dcterms",
    "mo",
    "mbz",
    "discogs",
    "afo",
    "afv",
    "jams",
    "pon",
    "chord",
    "gedcom",
    "vcard",
    "geo",
    "wgs84",
    "tgn",
    "gvp",
    "frbr",
    "fabio",
    "lrmoo",
    "bibo",
    "bibframe",
    "sioc",
    "skos",
    "nmo",
    "wot",
    "odrl",
    "cc",
    "premis",
    "rstmt",
    "spdx",
    "spdxlic",
    "codemeta",
    "forgefed",
    "ma",
    "gsso",
    "homosaurus",
    "fhir",
    "bio",
    "gedcomx",
    "geonames",
    "wikidata",
    "lexvo",
    "glottolog",
    "ontolex",
    "lime",
    "qudt",
    "gtfs",
    "fibo-fnd-acc-cur",
    "fibo-iso4217",
    "fibo-fnd-acc-ae",
    "fibo-fbc-fi-fi",
    "fibo-fbc-pas-fpas",
    "fibo-fnd-pas-ps",
    "brick",
    "bot",
    "ifc",
    "crmsci",
    "lvont",
    "moat",
    "tags",
    "qb",
    "mf",
    "faldo",
    "so",
    "crmarc",
    "crmdig",
    "exif",
    "iiif",
    "obscore",
    "ivoa",
    "bbc",
    "iptc",
    "loinc",
    "snomed",
];

/// Return the CURIE prefix of `curie` if it is a known alignment target.
fn prefix_of(curie: &str) -> Option<String> {
    let (prefix, _) = curie.split_once(':')?;
    if ALIGNMENT_TARGETS.contains(&prefix) {
        Some(prefix.to_owned())
    } else {
        None
    }
}

/// Expand a CURIE to an absolute IRI using the curated prefix registry.
fn expand_curie(curie: &str) -> Option<String> {
    let (prefix, local) = curie.split_once(':')?;
    let (_, ns) = PREFIX_REGISTRY.iter().find(|(p, _)| p == &prefix)?;
    Some(format!("{ns}{local}"))
}

/// Render an absolute IRI as a CURIE using the curated prefix registry.
fn shorten_iri(iri: &str) -> String {
    // Prefer the longest matching namespace; registry order is the tie-break.
    let mut best: Option<(&str, &str)> = None;
    for (prefix, ns) in PREFIX_REGISTRY {
        if iri.starts_with(ns) {
            match best {
                Some((_, best_ns)) if ns.len() <= best_ns.len() => {}
                _ => best = Some((prefix, ns)),
            }
        }
    }
    match best {
        Some((prefix, ns)) => format!("{prefix}:{}", &iri[ns.len()..]),
        None => iri.to_owned(),
    }
}

// ── Severity / ranking helpers ─────────────────────────────────────────────────

/// Map a mapping predicate to the severity of a conflict on it.
fn severity_for(predicate_id: &str) -> &'static str {
    if STRONG_PROPERTY_PREDICATES.contains(&predicate_id) {
        "ERROR"
    } else {
        "WARNING"
    }
}

/// Return the predicate/confidence score for a mapping.
fn score_mapping(m: &Mapping) -> (i32, f64) {
    let rank = PREDICATE_RANK
        .iter()
        .find(|(p, _)| p == &m.predicate_id)
        .map(|(_, r)| *r)
        .unwrap_or(0);
    let conf = m.confidence.parse::<f64>().unwrap_or(0.0);
    (rank, conf)
}

/// Return `(canonical, offender)` for a self-contradicting mapping pair.
fn rank_pair<'a>(a: &'a Mapping, b: &'a Mapping) -> (&'a Mapping, &'a Mapping) {
    if score_mapping(a) >= score_mapping(b) {
        (a, b)
    } else {
        (b, a)
    }
}

// ── Diagnostic builders ────────────────────────────────────────────────────────

/// Build an INFO diagnostic for a target prefix whose axioms are unavailable.
fn info_unavailable(m: &Mapping, prefix: &str) -> ProjectionDiagnostic {
    ProjectionDiagnostic {
        severity: "INFO".to_owned(),
        check: "domain-range".to_owned(),
        code: "domain-range".to_owned(),
        message: format!(
            "skipped — no axioms available for target '{prefix}' \
             (vendor a snapshot or run with --network)"
        ),
        instance: expand_curie(&m.object_id),
    }
}

/// Build an INFO diagnostic when a row cannot be checked for a structural reason.
fn info_not_checkable(m: &Mapping, reason: &str) -> ProjectionDiagnostic {
    ProjectionDiagnostic {
        severity: "INFO".to_owned(),
        check: "domain-range".to_owned(),
        code: "domain-range".to_owned(),
        message: format!("direction not checked — {reason}"),
        instance: expand_curie(&m.object_id),
    }
}

// ── oxigraph store helpers ─────────────────────────────────────────────────────

fn new_store() -> Result<Store, SliceError> {
    Store::new().map_err(|e| SliceError::Parse(format!("store creation failed: {e}")))
}

/// Parse a Turtle file into a fresh oxigraph store (lenient, so GMEOW's
/// `@x-gmeow-*` language tags parse).
fn parse_ttl(path: &Path) -> Result<Store, SliceError> {
    let store = new_store()?;
    let bytes = std::fs::read(path).map_err(SliceError::Io)?;
    for quad in RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_reader(bytes.as_slice())
    {
        let quad = quad
            .map_err(|e| SliceError::Parse(format!("syntax error in {}: {e}", path.display())))?;
        store
            .insert(&quad)
            .map_err(|e| SliceError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(store)
}

/// Parse Turtle text into a fresh store (test helper).
#[cfg(test)]
fn parse_ttl_text(text: &str) -> Result<Store, SliceError> {
    let store = new_store()?;
    for quad in RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_reader(text.as_bytes())
    {
        let quad = quad.map_err(|e| SliceError::Parse(format!("syntax error: {e}")))?;
        store
            .insert(&quad)
            .map_err(|e| SliceError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(store)
}

/// Merge every quad from `source` into `target`.
fn merge_store(target: &mut Store, source: &Store) -> Result<(), SliceError> {
    for quad in source.iter() {
        let quad = quad.map_err(|e| SliceError::Parse(format!("store iteration failed: {e}")))?;
        target
            .insert(&quad)
            .map_err(|e| SliceError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(())
}

fn named_node(iri: &str) -> Result<NamedNode, SliceError> {
    NamedNode::new(iri).map_err(|e| SliceError::Parse(format!("invalid IRI {iri}: {e}")))
}

/// Every IRI object of `<subject> <pred> ?o`.
fn objects_iri(store: &Store, subject: &str, pred: &str) -> Result<Vec<String>, SliceError> {
    let subject = named_node(subject)?;
    let predicate = named_node(pred)?;
    let mut out = Vec::new();
    for quad in store.quads_for_pattern(
        Some(subject.as_ref().into()),
        Some(predicate.as_ref()),
        None,
        Some(GraphNameRef::DefaultGraph),
    ) {
        let quad = quad.map_err(|e| SliceError::Parse(e.to_string()))?;
        if let Term::NamedNode(nn) = quad.object {
            out.push(nn.as_str().to_owned());
        }
    }
    Ok(out)
}

/// Every IRI subject of `?s <pred> <object>`.
fn subjects_iri(store: &Store, pred: &str, object: &NamedNode) -> Result<Vec<String>, SliceError> {
    let predicate = named_node(pred)?;
    let mut out = Vec::new();
    for quad in store.quads_for_pattern(
        None,
        Some(predicate.as_ref()),
        Some(object.as_ref().into()),
        Some(GraphNameRef::DefaultGraph),
    ) {
        let quad = quad.map_err(|e| SliceError::Parse(e.to_string()))?;
        if let NamedOrBlankNode::NamedNode(nn) = quad.subject {
            out.push(nn.as_str().to_owned());
        }
    }
    Ok(out)
}

/// Every `(subject, object)` pair of `?s <pred> ?o` where both are named nodes.
fn subject_objects_iri(store: &Store, pred: &str) -> Result<Vec<(String, String)>, SliceError> {
    let predicate = named_node(pred)?;
    let mut out = Vec::new();
    for quad in store.quads_for_pattern(
        None,
        Some(predicate.as_ref()),
        None,
        Some(GraphNameRef::DefaultGraph),
    ) {
        let quad = quad.map_err(|e| SliceError::Parse(e.to_string()))?;
        if let (NamedOrBlankNode::NamedNode(subj), Term::NamedNode(obj)) =
            (&quad.subject, &quad.object)
        {
            out.push((subj.as_str().to_owned(), obj.as_str().to_owned()));
        }
    }
    Ok(out)
}

/// Whether `term` has `term a type_iri` in `store`.
fn has_type(store: &Store, term: &str, type_iri: &str) -> Result<bool, SliceError> {
    let subject = named_node(term)?;
    let predicate = named_node(RDF_TYPE)?;
    let object = named_node(type_iri)?;
    let mut iter = store.quads_for_pattern(
        Some(subject.as_ref().into()),
        Some(predicate.as_ref()),
        Some(object.as_ref().into()),
        Some(GraphNameRef::DefaultGraph),
    );
    Ok(iter.next().is_some())
}

/// Whether the expanded IRI is declared as an OWL ObjectProperty or DatatypeProperty.
fn is_property(store: &Store, iri: &str) -> bool {
    has_type(store, iri, OWL_OBJECT_PROPERTY).unwrap_or(false)
        || has_type(store, iri, OWL_DATATYPE_PROPERTY).unwrap_or(false)
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strong_property_predicates_contain_expected_curies() {
        assert!(STRONG_PROPERTY_PREDICATES.contains(&"owl:equivalentProperty"));
        assert!(STRONG_PROPERTY_PREDICATES.contains(&"skos:exactMatch"));
    }

    #[test]
    fn strong_class_predicates_contain_expected_curies() {
        assert!(STRONG_CLASS_PREDICATES.contains(&"owl:equivalentClass"));
        assert!(STRONG_CLASS_PREDICATES.contains(&"skos:exactMatch"));
    }

    #[test]
    fn hierarchical_predicates_and_ranks_are_present() {
        assert!(HIERARCHICAL_PREDICATES.contains(&"skos:broadMatch"));
        assert!(HIERARCHICAL_PREDICATES.contains(&"skos:narrowMatch"));
        assert!(HIERARCHICAL_PREDICATES.contains(&"rdfs:subPropertyOf"));
        assert!(PREDICATE_RANK
            .iter()
            .any(|(p, _)| *p == "owl:equivalentProperty"));
        assert!(PREDICATE_RANK.iter().any(|(p, _)| *p == "skos:exactMatch"));
        assert!(PREDICATE_RANK.iter().any(|(p, _)| *p == "skos:closeMatch"));
    }

    #[test]
    fn character_and_owl_property_types_are_present() {
        assert!(CHARACTER_TYPES.contains(&"http://www.w3.org/2002/07/owl#FunctionalProperty"));
        assert!(CHARACTER_TYPES.contains(&"http://www.w3.org/2002/07/owl#TransitiveProperty"));
        assert!(OWL_PROPERTY_TYPES.contains(&"http://www.w3.org/2002/07/owl#ObjectProperty"));
        assert!(OWL_PROPERTY_TYPES.contains(&"http://www.w3.org/2002/07/owl#AsymmetricProperty"));
    }

    #[test]
    fn dcterms_refinements_and_grandfathered_dc_are_present() {
        assert!(DCTERMS_REFINEMENTS
            .iter()
            .any(|(r, b)| *r == "dcterms:abstract" && *b == "dcterms:description"));
        assert!(GRANDFATHERED_DC.contains(&"dc:rights"));
    }

    /// Target snapshot loading succeeds for at least one vendored target and
    /// produces a non-empty store.
    #[test]
    fn target_snapshot_loads_with_content() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let store = load_target_snapshot(root, "org")
            .expect("loading org snapshot should not fail")
            .expect("org snapshot should exist");
        let len = store.len().expect("store length should be readable");
        assert!(len > 0, "org snapshot should contain triples");
    }

    /// SSSOM mapping loading returns rows from the committed generated tables.
    #[test]
    fn sssom_mapping_loading_returns_rows() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let mappings = load_sssom_mappings(root).expect("loading mappings should not fail");
        assert!(
            !mappings.is_empty(),
            "expected at least one SSSOM mapping row"
        );
        assert!(mappings.iter().any(|m| m.subject_id.starts_with("gmeow:")));
    }

    /// Missing target snapshots produce INFO findings, not errors or panics.
    #[test]
    fn missing_target_snapshot_produces_info_finding() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let findings = lint_alignment_directions(root, false)
            .expect("lint_alignment_directions should not error");
        let info: Vec<_> = findings.iter().filter(|f| f.severity == "INFO").collect();
        assert!(
            !info.is_empty(),
            "expected at least one INFO finding for unavailable targets"
        );
    }

    /// A property mapped to both a term and its inverse is flagged as an ERROR.
    #[test]
    fn test_detects_self_contradicting_inverse_mapping() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let mappings_dir = root.join("generated").join("mappings");
        let fixtures_dir = root.join("tests").join("fixtures").join("target_axioms");
        let ontology_dir = root.join("ontology");
        std::fs::create_dir_all(&mappings_dir).unwrap();
        std::fs::create_dir_all(&fixtures_dir).unwrap();
        std::fs::create_dir_all(&ontology_dir).unwrap();

        // Minimal ontology: declare the GMEOW property as an OWL object property.
        std::fs::write(
            ontology_dir.join("gmeow.ttl"),
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:subOrganizationOf a owl:ObjectProperty .\n",
        )
        .unwrap();

        // Schema fixture: the inverse pair that creates the contradiction.
        std::fs::write(
            fixtures_dir.join("schema.ttl"),
            "@prefix schema: <https://schema.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             schema:subOrganization a owl:ObjectProperty ;\n\
             \towl:inverseOf schema:parentOrganization .\n\
             schema:parentOrganization a owl:ObjectProperty ;\n\
             \towl:inverseOf schema:subOrganization .\n",
        )
        .unwrap();

        // Two mappings to mutually inverse terms.
        let header = "subject_id\tpredicate_id\tobject_id\tmapping_justification\tconfidence\n";
        let rows = [
            (
                "gmeow:subOrganizationOf",
                "owl:equivalentProperty",
                "schema:parentOrganization",
                "0.9",
            ),
            (
                "gmeow:subOrganizationOf",
                "skos:closeMatch",
                "schema:subOrganization",
                "0.6",
            ),
        ];
        let just = "semapv:ManualMappingCuration";
        let body: String = rows
            .iter()
            .map(|(s, p, o, c)| format!("{s}\t{p}\t{o}\t{just}\t{c}\n"))
            .collect();
        std::fs::write(
            mappings_dir.join("bug.sssom.tsv"),
            header.to_owned() + &body,
        )
        .unwrap();

        let findings = lint_alignment_directions(root, false).unwrap();
        let errors: Vec<_> = findings
            .iter()
            .filter(|f| f.severity == "ERROR" && f.check == "inverse-direction")
            .collect();
        assert!(
            !errors.is_empty(),
            "self-contradicting inverse mapping was not flagged"
        );
        let flagged = errors[0];
        assert_eq!(
            flagged.instance.as_deref(),
            Some("https://schema.org/subOrganization")
        );
        assert!(flagged.message.contains("schema:parentOrganization"));
        assert!(flagged
            .message
            .contains("did you mean schema:parentOrganization?"));
    }

    /// A symmetric target (T owl:inverseOf T) must not self-contradict.
    #[test]
    fn test_self_inverse_target_is_not_flagged() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let mappings_dir = root.join("generated").join("mappings");
        let fixtures_dir = root.join("tests").join("fixtures").join("target_axioms");
        let ontology_dir = root.join("ontology");
        std::fs::create_dir_all(&mappings_dir).unwrap();
        std::fs::create_dir_all(&fixtures_dir).unwrap();
        std::fs::create_dir_all(&ontology_dir).unwrap();

        std::fs::write(
            ontology_dir.join("gmeow.ttl"),
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:hasMet a owl:ObjectProperty .\n",
        )
        .unwrap();

        std::fs::write(
            fixtures_dir.join("foaf.ttl"),
            "@prefix foaf: <http://xmlns.com/foaf/0.1/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             foaf:knows a owl:ObjectProperty ; owl:inverseOf foaf:knows .\n",
        )
        .unwrap();

        let header = "subject_id\tpredicate_id\tobject_id\tmapping_justification\tconfidence\n";
        std::fs::write(
            mappings_dir.join("m.sssom.tsv"),
            format!("{header}gmeow:hasMet\tskos:closeMatch\tfoaf:knows\tsemapv:ManualMappingCuration\t0.8\n"),
        )
        .unwrap();

        let findings = lint_alignment_directions(root, false).unwrap();
        let inverse: Vec<_> = findings
            .iter()
            .filter(|f| f.check == "inverse-direction")
            .collect();
        assert!(
            inverse.is_empty(),
            "self-inverse target wrongly flagged: {inverse:?}"
        );
    }

    /// Domain/range synthetic tests: inverted, compatible, and unavailable.
    #[test]
    fn test_domain_range_synthetic() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let mappings_dir = root.join("generated").join("mappings");
        let fixtures_dir = root.join("tests").join("fixtures").join("target_axioms");
        let ontology_dir = root.join("ontology");
        std::fs::create_dir_all(&mappings_dir).unwrap();
        std::fs::create_dir_all(&fixtures_dir).unwrap();
        std::fs::create_dir_all(&ontology_dir).unwrap();

        std::fs::write(
            ontology_dir.join("gmeow.ttl"),
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix schema: <https://schema.org/> .\n\
             gmeow:Child a owl:Class ; owl:equivalentClass schema:Child .\n\
             gmeow:Parent a owl:Class ; owl:equivalentClass schema:Parent .\n\
             gmeow:childOf a owl:ObjectProperty ;\n\
             \trdfs:domain gmeow:Child ;\n\
             \trdfs:range gmeow:Parent .\n",
        )
        .unwrap();

        std::fs::write(
            fixtures_dir.join("schema.ttl"),
            "@prefix schema: <https://schema.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             schema:Child a owl:Class .\n\
             schema:Parent a owl:Class .\n\
             schema:childOf a owl:ObjectProperty ;\n\
             \trdfs:domain schema:Child ;\n\
             \trdfs:range schema:Parent .\n\
             schema:parentOf a owl:ObjectProperty ;\n\
             \trdfs:domain schema:Parent ;\n\
             \trdfs:range schema:Child .\n",
        )
        .unwrap();

        let header = "subject_id\tpredicate_id\tobject_id\tmapping_justification\tconfidence\n";
        let body = "gmeow:childOf\tskos:closeMatch\tschema:childOf\tsemapv:ManualMappingCuration\t0.6\n\
                    gmeow:childOf\tskos:closeMatch\tschema:parentOf\tsemapv:ManualMappingCuration\t0.6\n\
                    gmeow:childOf\tskos:closeMatch\tfoaf:noSuchTerm\tsemapv:ManualMappingCuration\t0.6\n";
        std::fs::write(
            mappings_dir.join("domain.sssom.tsv"),
            header.to_owned() + body,
        )
        .unwrap();

        let findings = lint_alignment_directions(root, false).unwrap();

        let compatible = findings
            .iter()
            .any(|f| f.instance.as_deref() == Some("https://schema.org/childOf"));
        assert!(!compatible, "compatible mapping should not be flagged");

        let inverted: Vec<_> = findings
            .iter()
            .filter(|f| {
                f.instance.as_deref() == Some("https://schema.org/parentOf")
                    && f.check == "domain-range"
            })
            .collect();
        assert_eq!(
            inverted.len(),
            1,
            "expected exactly one inverted domain-range finding"
        );
        assert_eq!(inverted[0].severity, "WARNING");
        assert!(inverted[0]
            .message
            .contains("domain/range are inverted relative to the target term"));

        let unavailable: Vec<_> = findings
            .iter()
            .filter(|f| {
                f.instance.as_deref() == Some("http://xmlns.com/foaf/0.1/noSuchTerm")
                    && f.check == "domain-range"
                    && f.severity == "INFO"
            })
            .collect();
        assert!(
            !unavailable.is_empty(),
            "expected an INFO finding for unavailable target axioms"
        );
        assert!(unavailable[0]
            .message
            .contains("no axioms available for target 'foaf'"));
    }
}
