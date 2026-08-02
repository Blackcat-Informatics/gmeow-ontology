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

/// One registered residue-ratchet exemption, as authored.
///
/// The complement of a guarded `gmeow:ProjectionVocabulary`: a catalog family the
/// residue ratchet does NOT guard because it has no single grounding-slice owner, made
/// countable so the carve-out cannot widen without a reviewed ontology edit.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Load the authored `gmeow:ResidueRatchetExemption` registry from `root` (the SAME
/// file as the family registry — an exemption is a statement about a family, and
/// splitting them would let one drift from the other).
///
/// Every binding is mandatory for the same reason the family loader's are: an
/// exemption with no family exempts nothing, one with no rationale cannot be
/// re-examined, and one with no ceiling bounds nothing.
///
/// # Errors
/// A missing/unreadable/unparsable registry file, a malformed binding, a repeated
/// single-valued binding, or a non-integer ceiling.
pub fn load_residue_exemptions(root: &Path) -> Result<Vec<ResidueExemption>, gmeow_errors::Diag> {
    let path = root.join(CATALOG_FAMILIES_PATH);
    let bytes =
        std::fs::read(&path).map_err(|e| registry_err(format!("read {}: {e}", path.display())))?;
    let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
        .map_err(|e| registry_err(format!("parse {}: {e}", path.display())))?;

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

    /// The guarded `gmeow:ProjectionVocabulary` namespace set, read from the
    /// ontology-resident rubric registry exactly as the production gate reads it.
    fn guarded_namespaces() -> BTreeSet<String> {
        gmeow_slice_quality::load_repo_rubric(&repo_root())
            .expect("the rubric registry loads")
            .floors
            .vocabularies
            .iter()
            .flat_map(|vocab| vocab.namespaces.iter().cloned())
            .collect()
    }

    /// The authored carve-out is well-formed, and every exempted family is genuinely
    /// UNGUARDED — the record describes a real absence, not a stale claim.
    #[test]
    fn the_authored_carve_out_is_registered_and_genuinely_unguarded() {
        let families = load_catalog_families(&repo_root()).expect("registry loads");
        let exemptions = load_residue_exemptions(&repo_root()).expect("exemptions load");
        assert!(
            !exemptions.is_empty(),
            "the carve-out is a REGISTRY, not prose: the exempted families must be rows"
        );
        // Every exemption carries a substantive reason, not a restatement.
        for exemption in &exemptions {
            assert!(
                exemption.rationale.len() > 80,
                "{} states no substantive reason: {:?}",
                exemption.iri,
                exemption.rationale
            );
        }
        // Measured exactly at the pinned ceiling: the carve-out is at its recorded size.
        let measured: BTreeMap<String, usize> = exemptions
            .iter()
            .map(|e| {
                let family = families
                    .iter()
                    .find(|f| f.iri == e.family_iri)
                    .unwrap_or_else(|| panic!("{} names an unregistered family", e.iri));
                (family.name.clone(), e.row_ceiling)
            })
            .collect();
        check_residue_exemptions(
            &families,
            &exemptions,
            &guarded_namespaces(),
            &measured,
            "unit",
        )
        .expect("the authored carve-out holds its own ceilings and names no guarded family");
    }

    /// The carve-out cannot widen implicitly: ONE more shipped correspondence onto an
    /// exempted family reds, because a row riding an exemption is a row under no residue
    /// count, no ceiling, and no monotonicity ratchet.
    #[test]
    fn a_grown_carve_out_hard_fails() {
        let families = load_catalog_families(&repo_root()).expect("registry loads");
        let exemptions = load_residue_exemptions(&repo_root()).expect("exemptions load");
        let guarded = guarded_namespaces();
        for exemption in &exemptions {
            let family = families
                .iter()
                .find(|f| f.iri == exemption.family_iri)
                .expect("registered");
            let measured: BTreeMap<String, usize> =
                BTreeMap::from([(family.name.clone(), exemption.row_ceiling + 1)]);
            let error = check_residue_exemptions(
                &families,
                std::slice::from_ref(exemption),
                &guarded,
                &measured,
                "unit",
            )
            .expect_err("one more row into the carve-out must hard-fail");
            assert!(
                error.to_string().contains("carve-out GREW"),
                "unexpected message: {error}"
            );
        }
    }

    /// An exemption for a family that IS guarded is refused — the record may not outlive
    /// its reason and keep asserting an absence that has since been closed.
    #[test]
    fn an_exemption_for_a_guarded_family_hard_fails() {
        let families = load_catalog_families(&repo_root()).expect("registry loads");
        let guarded = guarded_namespaces();
        // P-Plan is guarded (it has a single logic: owner), so exempting it is a lie.
        let pplan = families
            .iter()
            .find(|f| f.name == "P-Plan")
            .expect("P-Plan is registered");
        let stale = ResidueExemption {
            iri: "https://blackcatinformatics.ca/gmeow/residueExemption-stale".to_string(),
            family_iri: pplan.iri.clone(),
            rationale: "a reason that no longer holds because the vocabulary gained an owner"
                .to_string(),
            row_ceiling: 99,
        };
        let error = check_residue_exemptions(
            &families,
            std::slice::from_ref(&stale),
            &guarded,
            &BTreeMap::new(),
            "unit",
        )
        .expect_err("exempting a guarded family must hard-fail");
        assert!(
            error.to_string().contains("guarded or exempt, never both"),
            "unexpected message: {error}"
        );
    }

    /// An exemption naming no registered family is a dead row that exempts nothing.
    #[test]
    fn a_dangling_exemption_hard_fails() {
        let families = load_catalog_families(&repo_root()).expect("registry loads");
        let dangling = ResidueExemption {
            iri: "https://blackcatinformatics.ca/gmeow/residueExemption-ghost".to_string(),
            family_iri: "https://blackcatinformatics.ca/gmeow/catalogFamily-nonexistent"
                .to_string(),
            rationale: "a reason attached to nothing at all".to_string(),
            row_ceiling: 0,
        };
        let error = check_residue_exemptions(
            &families,
            std::slice::from_ref(&dangling),
            &guarded_namespaces(),
            &BTreeMap::new(),
            "unit",
        )
        .expect_err("a dangling exemption must hard-fail");
        assert!(
            error.to_string().contains("dead exemption row"),
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
