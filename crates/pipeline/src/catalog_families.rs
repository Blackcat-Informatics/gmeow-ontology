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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// Both registry views lowered from one original authored document.
///
/// Every required binding is mandatory: a family with no name, no namespace stem,
/// no owner, or no minimum cannot drive the gate, so it is a HARD FAIL rather than
/// a silently defaulted row. An empty registry, a duplicate family name, and a
/// namespace stem that is a prefix of another family's stem (which would make some
/// target match two families by construction) are equally hard failures.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CatalogRegistry {
    /// Admitted external catalog families.
    pub families: Vec<CatalogFamily>,
    /// Declared, bounded residue exemptions.
    pub exemptions: Vec<ResidueExemption>,
}

/// Compact production observations shared by registry and bundle consumers.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RegistryObservations {
    /// Both native registry views from the exact original document.
    pub registry: CatalogRegistry,
    /// Guarded namespace authority selected by the producer's rubric gate.
    pub guarded_namespaces: BTreeSet<String>,
}

/// Private pipeline artifact containing the producer's validated registry views.
pub const REGISTRY_CHANNEL: &str = "pipeline/catalog-family-observations.json";

/// Load both registry views with one source read and parse.
pub fn load_catalog_registry(root: &Path) -> Result<CatalogRegistry, gmeow_errors::Diag> {
    let path = root.join(CATALOG_FAMILIES_PATH);
    let bytes = std::fs::read(&path)
        .map_err(|error| registry_err(format!("read {}: {error}", path.display())))?;
    let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
        .map_err(|error| registry_err(format!("parse {}: {error}", path.display())))?;
    Ok(CatalogRegistry {
        families: catalog_families_from_dataset(&dataset)?,
        exemptions: residue_exemptions_from_dataset(&dataset)?,
    })
}

/// Read the producer-selected registry without opening authored sources.
/// Missing or stale fixture identities fail closed; no loader fallback exists.
pub fn authenticated_registry(root: &Path) -> Result<RegistryObservations, gmeow_errors::Diag> {
    let bytes = crate::fixture::authenticated_artifact(root, "stage-mappings", REGISTRY_CHANNEL)?;
    serde_json::from_slice(&bytes).map_err(|error| {
        registry_err(format!(
            "decode authenticated registry observations: {error}"
        ))
    })
}

/// Lower and validate the family declarations in one original registry document.
pub fn catalog_families_from_dataset(
    dataset: &purrdf::RdfDataset,
) -> Result<Vec<CatalogFamily>, gmeow_errors::Diag> {
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

/// One registered residue-ratchet exemption, as authored.
///
/// The complement of a guarded `gmeow:ProjectionVocabulary`: a catalog family the
/// residue ratchet does NOT guard because it has no single grounding-slice owner, made
/// countable so the carve-out cannot widen without a reviewed ontology edit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResidueExemption {
    /// IRI of the `gmeow:ResidueRatchetExemption` individual (identity).
    pub iri: String,
    /// `gmeow:exemptCatalogFamily` — the exempted family's IRI.
    pub family_iri: String,
    /// `gmeow:exemptRationale` — why the family has no single owner.
    pub rationale: String,
    /// `gmeow:exemptRowCeiling` — the lower-only cap on the exempted family's shipped
    /// grounding-correspondence count.
    pub row_ceiling: usize,
}

/// Lower residue exemptions from the same original document as the family registry.
///
/// Every binding is mandatory for the same reason the family loader's are: an
/// exemption with no family exempts nothing, one with no rationale cannot be
/// re-examined, and one with no ceiling bounds nothing.
///
/// # Errors
/// A malformed binding, a repeated single-valued binding, or a non-integer ceiling.
pub fn residue_exemptions_from_dataset(
    dataset: &purrdf::RdfDataset,
) -> Result<Vec<ResidueExemption>, gmeow_errors::Diag> {
    let exemption_type = format!("{GMEOW}ResidueRatchetExemption");
    let family_p = format!("{GMEOW}exemptCatalogFamily");
    let rationale_p = format!("{GMEOW}exemptRationale");
    let ceiling_p = format!("{GMEOW}exemptRowCeiling");

    let mut subjects: BTreeSet<String> = BTreeSet::new();
    let mut family_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut rationale_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut ceiling_of: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for quad in dataset.owned_quads() {
        let purrdf::RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        match quad.predicate.as_str() {
            RDF_TYPE => {
                if matches!(&quad.object, purrdf::RdfTerm::Iri(o) if *o == exemption_type) {
                    subjects.insert(subject.clone());
                }
            }
            p if p == family_p => {
                let purrdf::RdfTerm::Iri(family) = &quad.object else {
                    return Err(registry_err(format!(
                        "{subject}: gmeow:exemptCatalogFamily must be a gmeow:CatalogFamily IRI"
                    )));
                };
                family_of
                    .entry(subject.clone())
                    .or_default()
                    .push(family.clone());
            }
            p if p == rationale_p => {
                let purrdf::RdfTerm::Literal(lit) = &quad.object else {
                    return Err(registry_err(format!(
                        "{subject}: gmeow:exemptRationale must be a literal"
                    )));
                };
                rationale_of
                    .entry(subject.clone())
                    .or_default()
                    .push(lit.lexical_form.clone());
            }
            p if p == ceiling_p => {
                let purrdf::RdfTerm::Literal(lit) = &quad.object else {
                    return Err(registry_err(format!(
                        "{subject}: gmeow:exemptRowCeiling must be an integer literal"
                    )));
                };
                ceiling_of
                    .entry(subject.clone())
                    .or_default()
                    .push(lit.lexical_form.clone());
            }
            _ => {}
        }
    }

    let single = |map: &BTreeMap<String, Vec<String>>, iri: &str, property: &str| {
        let values = map.get(iri).cloned().unwrap_or_default();
        match values.len() {
            1 => Ok(values.into_iter().next().expect("length checked")),
            n => Err(registry_err(format!(
                "{iri} has {n} {property} values — an exemption binds exactly one"
            ))),
        }
    };

    let mut out = Vec::new();
    for iri in &subjects {
        let family_iri = single(&family_of, iri, "gmeow:exemptCatalogFamily")?;
        let rationale = single(&rationale_of, iri, "gmeow:exemptRationale")?;
        let raw = single(&ceiling_of, iri, "gmeow:exemptRowCeiling")?;
        let row_ceiling: usize = raw.parse().map_err(|_| {
            registry_err(format!(
                "{iri}: gmeow:exemptRowCeiling {raw:?} is not a non-negative integer"
            ))
        })?;
        if rationale.trim().is_empty() {
            return Err(registry_err(format!(
                "{iri}: gmeow:exemptRationale is blank — an exemption whose reason is not \
                 written down cannot be re-examined, which is how a carve-out becomes permanent"
            )));
        }
        out.push(ResidueExemption {
            iri: iri.clone(),
            family_iri,
            rationale,
            row_ceiling,
        });
    }
    out.sort_by(|a, b| a.iri.cmp(&b.iri));
    Ok(out)
}

/// Gate the residue-ratchet carve-out: every exemption is well-formed, names a
/// registered and genuinely UNGUARDED family, and bounds a shipped row count that has
/// not grown past its `gmeow:exemptRowCeiling`.
///
/// `guarded_namespaces` is the guarded `gmeow:ProjectionVocabulary` namespace set (read
/// from the ontology-resident rubric registry, never a Rust list). `measured` is the
/// per-family shipped count [`check_target_catalogs`] returns.
///
/// Three hard failures, each closing a way the carve-out could widen unseen:
///
/// * an exemption naming an unregistered family — a dead row exempting nothing;
/// * an exemption for a family that IS guarded — the record outlived its reason and
///   would keep asserting an absence that is no longer true;
/// * a measured count above the ceiling — a correspondence was added into the carve-out
///   rather than onto a guarded, owned surface. Paired with the family's raise-only
///   `gmeow:catalogTargetMinimum`, this pins the exempt row count from both sides.
///
/// # Errors
/// As above; each names the exemption, the family, and the numbers.
pub fn check_residue_exemptions(
    families: &[CatalogFamily],
    exemptions: &[ResidueExemption],
    guarded_namespaces: &BTreeSet<String>,
    measured: &BTreeMap<String, usize>,
    context: &str,
) -> Result<(), gmeow_errors::Diag> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for exemption in exemptions {
        if !seen.insert(exemption.family_iri.as_str()) {
            return Err(registry_err(format!(
                "{context}: two gmeow:ResidueRatchetExemption rows cover {} — each would carry \
                 half the carve-out's size and hide growth between them",
                exemption.family_iri
            )));
        }
        let Some(family) = families.iter().find(|f| f.iri == exemption.family_iri) else {
            return Err(registry_err(format!(
                "{context}: {} exempts {}, which is not a registered gmeow:CatalogFamily — a \
                 dead exemption row exempts nothing",
                exemption.iri, exemption.family_iri
            )));
        };
        // Guarded ⇔ some guarded vocabulary namespace and the family's stem name the
        // same surface (either may be the longer, more specific form).
        if let Some(stem) = family.namespaces.iter().find(|stem| {
            guarded_namespaces
                .iter()
                .any(|ns| stem.starts_with(ns.as_str()) || ns.starts_with(stem.as_str()))
        }) {
            return Err(registry_err(format!(
                "{context}: {} exempts {} ({}) from the residue ratchet, but {stem} IS a guarded \
                 gmeow:ProjectionVocabulary surface — a family is guarded or exempt, never both. \
                 Remove the exemption now that the vocabulary has an owner",
                exemption.iri, exemption.family_iri, family.name
            )));
        }
        let count = measured.get(&family.name).copied().unwrap_or(0);
        if count > exemption.row_ceiling {
            return Err(registry_err(format!(
                "{context}: the residue-ratchet carve-out GREW — {} ({}) now carries {count} \
                 shipped grounding correspondence(s), above its gmeow:exemptRowCeiling {}. Rows \
                 riding an exemption sit under no residue count, no ceiling and no monotonicity \
                 ratchet, so the carve-out may not widen implicitly: ground the new row through \
                 an owned, guarded surface, or raise the ceiling deliberately and say why",
                exemption.iri, family.name, exemption.row_ceiling
            )));
        }
    }
    Ok(())
}

#[path = "catalog_families.tests.rs"]
#[cfg(test)]
mod tests;
