// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The registered external catalog families a grounding correspondence may target.
//!
//! Every `logic:GroundingCorrespondence` names an external `logic:targetEndpoint`.
//! The set of catalogs those endpoints may belong to is CLOSED and lives in the
//! ontology as `gmeow:CatalogFamily` individuals
//! ([`CATALOG_FAMILIES_PATH`]), each carrying its IRI stem(s)
//! (`gmeow:catalogNamespace`), its authoring grounding slice(s)
//! (`gmeow:catalogOwner`), and its raise-only correspondence-count floor
//! (`gmeow:catalogTargetMinimum`).
//!
//! Rust ships NO family list. The registry is read from the authored ontology, in
//! the same shape the sibling guarded-vocabulary registry
//! (`gmeow:ProjectionVocabulary`, read by `gmeow_slice_quality::rubric`) is read
//! from `slices/core/slice-quality-rubric/module.ttl`: a hardcoded array in Rust
//! would make admitting a new external surface an invisible code edit instead of a
//! reviewable ontology edit, and would put the gate's own configuration outside the
//! corpus it gates.
//!
//! Two hard failures, never a warning and never a skip (no-optionality):
//!
//! * a shipped grounding target matching NO registered family, or matching MORE
//!   THAN ONE — the closed-set check that keeps an unvetted external namespace out
//!   of the catalog;
//! * a family whose measured shipped count falls below its
//!   `gmeow:catalogTargetMinimum` — the raise-only ratchet that turns a silently
//!   deleted bridge cell red.
//!
//! Both are enforced in production (the mappings stage, over the correspondences it
//! has just lowered) and again over the shipped bundle
//! (`tests/correspondence_laws_bundle.rs`), from this one loader.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Repo-relative path of the authored catalog-family registry.
pub const CATALOG_FAMILIES_PATH: &str = "dsl/mappings/catalog-families.ttl";

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// One registered external catalog family, as authored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogFamily {
    /// IRI of the `gmeow:CatalogFamily` individual (identity).
    pub iri: String,
    /// `gmeow:catalogFamilyName` — the stable report key, unique across the registry.
    pub name: String,
    /// `gmeow:catalogNamespace` — the IRI stems a member target is minted under
    /// (sorted, deduplicated; never empty).
    pub namespaces: Vec<String>,
    /// `gmeow:catalogOwner` — the grounding slice(s) authoring this family's bridge
    /// cells (sorted, deduplicated; never empty).
    pub owners: Vec<String>,
    /// `gmeow:catalogTargetMinimum` — the raise-only floor on the shipped count.
    pub minimum: usize,
}

/// Build the registry-load hard failure.
fn registry_err(message: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-mappings".to_string(),
        message: format!("catalog-family registry ({CATALOG_FAMILIES_PATH}): {message}"),
    })
}

/// Load the authored `gmeow:CatalogFamily` registry from `root`.
///
/// Every required binding is mandatory: a family with no name, no namespace stem,
/// no owner, or no minimum cannot drive the gate, so it is a HARD FAIL rather than
/// a silently defaulted row. An empty registry, a duplicate family name, and a
/// namespace stem that is a prefix of another family's stem (which would make some
/// target match two families by construction) are equally hard failures.
pub fn load_catalog_families(root: &Path) -> Result<Vec<CatalogFamily>, gmeow_errors::Diag> {
    let path = root.join(CATALOG_FAMILIES_PATH);
    let bytes =
        std::fs::read(&path).map_err(|e| registry_err(format!("read {}: {e}", path.display())))?;
    let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
        .map_err(|e| registry_err(format!("parse {}: {e}", path.display())))?;

    let family_type = format!("{GMEOW}CatalogFamily");
    let name_p = format!("{GMEOW}catalogFamilyName");
    let namespace_p = format!("{GMEOW}catalogNamespace");
    let owner_p = format!("{GMEOW}catalogOwner");
    let minimum_p = format!("{GMEOW}catalogTargetMinimum");

    let mut subjects: BTreeSet<String> = BTreeSet::new();
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut namespaces: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut minimums: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for quad in dataset.owned_quads() {
        let purrdf::RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        match quad.predicate.as_str() {
            RDF_TYPE => {
                if matches!(&quad.object, purrdf::RdfTerm::Iri(o) if *o == family_type) {
                    subjects.insert(subject.clone());
                }
            }
            p if p == name_p => {
                let purrdf::RdfTerm::Literal(lit) = &quad.object else {
                    return Err(registry_err(format!(
                        "{subject}: gmeow:catalogFamilyName must be a literal"
                    )));
                };
                if let Some(previous) = names.insert(subject.clone(), lit.lexical_form.clone()) {
                    return Err(registry_err(format!(
                        "{subject}: two gmeow:catalogFamilyName values ({previous:?} and {:?}) — \
                         a family has exactly one report key",
                        lit.lexical_form
                    )));
                }
            }
            p if p == namespace_p => {
                let purrdf::RdfTerm::Literal(lit) = &quad.object else {
                    return Err(registry_err(format!(
                        "{subject}: gmeow:catalogNamespace must be a literal IRI stem"
                    )));
                };
                namespaces
                    .entry(subject.clone())
                    .or_default()
                    .insert(lit.lexical_form.clone());
            }
            p if p == owner_p => {
                let purrdf::RdfTerm::Iri(owner) = &quad.object else {
                    return Err(registry_err(format!(
                        "{subject}: gmeow:catalogOwner must be a grounding-slice IRI"
                    )));
                };
                owners
                    .entry(subject.clone())
                    .or_default()
                    .insert(owner.clone());
            }
            p if p == minimum_p => {
                let purrdf::RdfTerm::Literal(lit) = &quad.object else {
                    return Err(registry_err(format!(
                        "{subject}: gmeow:catalogTargetMinimum must be an integer literal"
                    )));
                };
                minimums
                    .entry(subject.clone())
                    .or_default()
                    .push(lit.lexical_form.clone());
            }
            _ => {}
        }
    }

    if subjects.is_empty() {
        return Err(registry_err(
            "no gmeow:CatalogFamily individuals — the closed target-catalog set is empty, so \
             every grounding target would be unregistered"
                .to_string(),
        ));
    }

    let mut families: Vec<CatalogFamily> = Vec::new();
    let mut seen_names: BTreeSet<String> = BTreeSet::new();
    for iri in &subjects {
        let name = names
            .get(iri)
            .cloned()
            .ok_or_else(|| registry_err(format!("{iri} has no gmeow:catalogFamilyName")))?;
        if !seen_names.insert(name.clone()) {
            return Err(registry_err(format!(
                "duplicate gmeow:catalogFamilyName {name:?} ({iri}) — two families with the \
                 same key collapse in the name-keyed count map and hide one catalog's loss"
            )));
        }
        let stems: Vec<String> = namespaces
            .get(iri)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();
        if stems.is_empty() {
            return Err(registry_err(format!(
                "{iri} ({name}) has no gmeow:catalogNamespace — it could never recognize a target"
            )));
        }
        let owner_iris: Vec<String> = owners
            .get(iri)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();
        if owner_iris.is_empty() {
            return Err(registry_err(format!(
                "{iri} ({name}) has no gmeow:catalogOwner — it names no authoring boundary"
            )));
        }
        let raw = minimums.get(iri).ok_or_else(|| {
            registry_err(format!("{iri} ({name}) has no gmeow:catalogTargetMinimum"))
        })?;
        if raw.len() != 1 {
            return Err(registry_err(format!(
                "{iri} ({name}) has {} gmeow:catalogTargetMinimum values — the ratchet floor is \
                 single-valued",
                raw.len()
            )));
        }
        let minimum: usize = raw[0].parse().map_err(|_| {
            registry_err(format!(
                "{iri} ({name}): gmeow:catalogTargetMinimum {:?} is not a non-negative integer",
                raw[0]
            ))
        })?;
        families.push(CatalogFamily {
            iri: iri.clone(),
            name,
            namespaces: stems,
            owners: owner_iris,
            minimum,
        });
    }
    families.sort_by(|a, b| a.name.cmp(&b.name));

    // A stem that prefixes another family's stem makes every target under the longer
    // stem match BOTH families, so the exactly-one-family check could never pass for
    // it. Catch the registry defect at load time rather than as a confusing
    // per-correspondence failure later.
    for outer in &families {
        for inner in &families {
            if outer.iri == inner.iri {
                continue;
            }
            for a in &outer.namespaces {
                for b in &inner.namespaces {
                    if b.starts_with(a.as_str()) {
                        return Err(registry_err(format!(
                            "gmeow:catalogNamespace {a:?} ({}) is a prefix of {b:?} ({}) — every \
                             target under the longer stem would match both families",
                            outer.name, inner.name
                        )));
                    }
                }
            }
        }
    }

    Ok(families)
}

/// The registered families a target endpoint belongs to (by IRI-stem prefix).
pub fn families_for_target<'a>(
    families: &'a [CatalogFamily],
    target: &str,
) -> Vec<&'a CatalogFamily> {
    families
        .iter()
        .filter(|family| {
            family
                .namespaces
                .iter()
                .any(|stem| target.starts_with(stem.as_str()))
        })
        .collect()
}

/// Classify `targets` against `families`, HARD-failing on any target that belongs to
/// no registered family or to more than one, and on any family whose measured count
/// falls below its `gmeow:catalogTargetMinimum`.
///
/// `targets` is the multiset of shipped grounding `logic:targetEndpoint` IRIs.
/// `context` names the surface being checked, so the two call sites (the lowered
/// correspondences in the mappings stage, the shipped bundle in the acceptance
/// suite) report distinguishably. Returns the per-family measured counts on success
/// so a caller may report them.
pub fn check_target_catalogs<'a, I>(
    families: &[CatalogFamily],
    targets: I,
    context: &str,
) -> Result<BTreeMap<String, usize>, gmeow_errors::Diag>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut measured: BTreeMap<String, usize> =
        families.iter().map(|f| (f.name.clone(), 0)).collect();
    for (correspondence, target) in targets {
        let matches = families_for_target(families, target);
        if matches.len() != 1 {
            let named: Vec<&str> = matches.iter().map(|f| f.name.as_str()).collect();
            return Err(registry_err(format!(
                "{context}: {correspondence} targets {target}, which belongs to {} registered \
                 catalog families ({named:?}) — every grounding target must belong to exactly \
                 one; register the catalog as a gmeow:CatalogFamily before bridging onto it",
                matches.len()
            )));
        }
        *measured
            .get_mut(matches[0].name.as_str())
            .expect("measured map is seeded from the same family list") += 1;
    }
    let mut below: Vec<String> = Vec::new();
    for family in families {
        let count = measured[&family.name];
        if count < family.minimum {
            below.push(format!(
                "{} measured {count} < gmeow:catalogTargetMinimum {}",
                family.name, family.minimum
            ));
        }
    }
    if !below.is_empty() {
        return Err(registry_err(format!(
            "{context}: {} catalog family target-count ratchet(s) breached: {}",
            below.len(),
            below.join("; ")
        )));
    }
    Ok(measured)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .expect("repo root")
    }

    #[test]
    fn the_authored_registry_loads_and_is_well_formed() {
        let families = load_catalog_families(&repo_root()).expect("registry loads");
        assert!(
            families.len() >= 52,
            "the authored catalog-family registry shrank: {} families",
            families.len()
        );
        // Spot-check the shape rather than restate the registry: every row carries a
        // stem, an owner, and a floor, because the loader hard-fails otherwise.
        for family in &families {
            assert!(!family.namespaces.is_empty(), "{}: no stem", family.name);
            assert!(!family.owners.is_empty(), "{}: no owner", family.name);
        }
    }

    #[test]
    fn an_unregistered_target_hard_fails() {
        let families = load_catalog_families(&repo_root()).expect("registry loads");
        let error = check_target_catalogs(
            &families,
            [("ex:cell", "https://example.invalid/unvetted#Thing")],
            "unit",
        )
        .expect_err("an unregistered target must hard-fail");
        assert!(
            error.to_string().contains("belongs to 0 registered"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn a_family_below_its_floor_hard_fails() {
        let families = load_catalog_families(&repo_root()).expect("registry loads");
        let error = check_target_catalogs(&families, [], "unit")
            .expect_err("an empty catalog must breach every non-zero floor");
        assert!(
            error.to_string().contains("target-count ratchet"),
            "unexpected message: {error}"
        );
    }
}
