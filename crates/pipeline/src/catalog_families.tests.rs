// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root")
}

fn observations() -> &'static RegistryObservations {
    static OBSERVED: std::sync::OnceLock<Result<RegistryObservations, String>> =
        std::sync::OnceLock::new();
    OBSERVED
        .get_or_init(|| authenticated_registry(&repo_root()).map_err(|error| error.to_string()))
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated catalog registry: {error}"))
}

#[test]
fn authored_catalog_contracts() {
    let contracts: [(&str, fn()); 7] = [
        (
            "the_authored_registry_loads_and_is_well_formed",
            the_authored_registry_loads_and_is_well_formed,
        ),
        (
            "an_unregistered_target_hard_fails",
            an_unregistered_target_hard_fails,
        ),
        (
            "the_authored_carve_out_is_registered_and_genuinely_unguarded",
            the_authored_carve_out_is_registered_and_genuinely_unguarded,
        ),
        ("a_grown_carve_out_hard_fails", a_grown_carve_out_hard_fails),
        (
            "an_exemption_for_a_guarded_family_hard_fails",
            an_exemption_for_a_guarded_family_hard_fails,
        ),
        (
            "a_dangling_exemption_hard_fails",
            a_dangling_exemption_hard_fails,
        ),
        (
            "a_family_below_its_floor_hard_fails",
            a_family_below_its_floor_hard_fails,
        ),
    ];
    let mut failures = Vec::new();
    for (name, contract) in contracts {
        if std::panic::catch_unwind(contract).is_err() {
            failures.push(name);
        }
    }
    assert!(
        failures.is_empty(),
        "7 catalog contracts executed; failed: {failures:?}"
    );
}

fn the_authored_registry_loads_and_is_well_formed() {
    let families = observations().registry.families.clone();
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

fn an_unregistered_target_hard_fails() {
    let families = observations().registry.families.clone();
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
    observations().guarded_namespaces.clone()
}

/// The authored carve-out is well-formed, and every exempted family is genuinely
/// UNGUARDED — the record describes a real absence, not a stale claim.
fn the_authored_carve_out_is_registered_and_genuinely_unguarded() {
    let families = observations().registry.families.clone();
    let exemptions = observations().registry.exemptions.clone();
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
fn a_grown_carve_out_hard_fails() {
    let families = observations().registry.families.clone();
    let exemptions = observations().registry.exemptions.clone();
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
fn an_exemption_for_a_guarded_family_hard_fails() {
    let families = observations().registry.families.clone();
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
fn a_dangling_exemption_hard_fails() {
    let families = observations().registry.families.clone();
    let dangling = ResidueExemption {
        iri: "https://blackcatinformatics.ca/gmeow/residueExemption-ghost".to_string(),
        family_iri: "https://blackcatinformatics.ca/gmeow/catalogFamily-nonexistent".to_string(),
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

fn a_family_below_its_floor_hard_fails() {
    let families = observations().registry.families.clone();
    let error = check_target_catalogs(&families, [], "unit")
        .expect_err("an empty catalog must breach every non-zero floor");
    assert!(
        error.to_string().contains("target-count ratchet"),
        "unexpected message: {error}"
    );
}
