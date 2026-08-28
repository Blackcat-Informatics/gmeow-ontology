// SPDX-License-Identifier: AGPL-3.0-only
// Conformance twins migrated from tests/test_rights.py

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

// ── Tests migrated from tests/test_rights.py ─────────────────────────────────

#[batch_cases]
#[case::wellformed_rights_fixture_conforms(Case::file("shapes", "rights-wellformed"))]
#[case::malformed_rights_fixture_is_flagged(
    Case::file("shapes", "rights-malformed")
        .fails()
        .violations(&[
            "must govern exactly one asset",
            "must regulate exactly one action",
            "must have at least one holder",
            "must name at least one licensor",
            "exactly one mark",
        ])
)]
#[case::expired_trademark_warns_but_does_not_fail(
    Case::file("shapes", "rights-expired-warning")
        .warnings(&["displayable false"])
)]
fn rights(#[case] case: Case) {
    case.run();
}

// ── GraphStore / projection twins migrated from tests/test_rights.py ──────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const ODRL: &str = "http://www.w3.org/ns/odrl/2/";
const CC: &str = "http://creativecommons.org/ns#";
const SCHEMA: &str = "https://schema.org/";
const DCTERMS: &str = "http://purl.org/dc/terms/";
const SPDX: &str = "http://spdx.org/rdf/terms#";
const EX_RIGHTS: &str = "https://example.org/rights/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}
fn odrl(local: &str) -> String {
    format!("{ODRL}{local}")
}
fn cc(local: &str) -> String {
    format!("{CC}{local}")
}
fn schema(local: &str) -> String {
    format!("{SCHEMA}{local}")
}
fn dcterms(local: &str) -> String {
    format!("{DCTERMS}{local}")
}
fn spdx(local: &str) -> String {
    format!("{SPDX}{local}")
}
fn ex(local: &str) -> String {
    format!("{EX_RIGHTS}{local}")
}

/// The merged ontology + the rights coverage fixture — the `_projection_source()` twin.
fn rights_source() -> GraphStore {
    GraphStore::ontology_plus_ttl_file(&repo_root().join("tests/fixtures/coverage/rights.ttl"))
}

/// Twin of `test_expanded_action_vocabulary_is_seeded`: the ODRL Common-Vocabulary
/// actions are seeded (maximal, not a thin stub) — ≥45 `gmeow:RightsAction`
/// individuals, including the four representative newly-seeded ones.
#[gmeow_test_batch_macros::batch_test]
fn expanded_action_vocabulary_is_seeded() {
    let g = GraphStore::ontology();
    let actions = g.subjects_of_type(&gm("RightsAction"));
    assert!(
        actions.len() >= 45,
        "expected ≥45 RightsAction, got {}",
        actions.len()
    );
    for a in [
        "actionSell",
        "actionStream",
        "actionModify",
        "actionAnonymize",
    ] {
        assert!(actions.contains(&gm(a)), "missing seeded RightsAction {a}");
    }
}

/// Twin of `test_odrl_projection_emits_a_policy_with_rules`.
#[gmeow_test_batch_macros::batch_test]
fn odrl_projection_emits_a_policy_with_rules() {
    let out = rights_source().construct(&[], &read_query("generated/queries/odrl.rq"));
    assert!(out.has(
        Some(&ex("photo-rights")),
        Some(RDF_TYPE),
        Some(&odrl("Set"))
    ));
    // Permission rule with action, target, assignee.
    assert!(out.has(
        Some(&ex("photo-rights")),
        Some(&odrl("permission")),
        Some(&ex("perm-reproduce"))
    ));
    assert!(out.has(
        Some(&ex("perm-reproduce")),
        Some(RDF_TYPE),
        Some(&odrl("Permission"))
    ));
    assert!(out.has(
        Some(&ex("perm-reproduce")),
        Some(&odrl("action")),
        Some(&odrl("reproduce"))
    ));
    assert!(out.has(
        Some(&ex("perm-reproduce")),
        Some(&odrl("target")),
        Some(&ex("photo"))
    ));
    assert!(out.has(
        Some(&ex("perm-reproduce")),
        Some(&odrl("assignee")),
        Some(&ex("acme"))
    ));
    // Prohibition + duty.
    assert!(out.has(
        Some(&ex("photo-rights")),
        Some(&odrl("prohibition")),
        Some(&ex("proh-commercial"))
    ));
    assert!(out.has(
        Some(&ex("proh-commercial")),
        Some(RDF_TYPE),
        Some(&odrl("Prohibition"))
    ));
    assert!(out.has(
        Some(&ex("photo-rights")),
        Some(&odrl("obligation")),
        Some(&ex("duty-attribute"))
    ));
    assert!(out.has(
        Some(&ex("duty-attribute")),
        Some(RDF_TYPE),
        Some(&odrl("Duty"))
    ));
    // The licence becomes an ODRL Offer with its assigner.
    assert!(out.has(Some(&ex("cc-by-4")), Some(RDF_TYPE), Some(&odrl("Offer"))));
    assert!(out.has(
        Some(&ex("cc-by-4")),
        Some(&odrl("assigner")),
        Some(&ex("jane"))
    ));
}

/// Twin of `test_odrl_projection_emits_constraint_and_conflict_logic`.
#[gmeow_test_batch_macros::batch_test]
fn odrl_projection_emits_constraint_and_conflict_logic() {
    let out = rights_source().construct(&[], &read_query("generated/queries/odrl.rq"));
    // The temporal constraint (valid until 2036) projects to an ODRL constraint.
    assert!(out.has(
        Some(&ex("perm-reproduce")),
        Some(&odrl("constraint")),
        Some(&ex("until-2036"))
    ));
    assert!(out.has(
        Some(&ex("until-2036")),
        Some(RDF_TYPE),
        Some(&odrl("Constraint"))
    ));
    assert!(out.has(
        Some(&ex("until-2036")),
        Some(&odrl("leftOperand")),
        Some(&odrl("dateTime"))
    ));
    assert!(out.has(
        Some(&ex("until-2036")),
        Some(&odrl("operator")),
        Some(&odrl("lteq"))
    ));
    assert!(
        !out.objects_lex(&ex("until-2036"), &odrl("rightOperand"))
            .is_empty(),
        "constraint must carry a rightOperand"
    );
    // Conflict-resolution strategy + a prohibition's remedy.
    assert!(out.has(
        Some(&ex("photo-rights")),
        Some(&odrl("conflict")),
        Some(&odrl("prohibit"))
    ));
    assert!(out.has(
        Some(&ex("proh-commercial")),
        Some(&odrl("remedy")),
        Some(&ex("duty-compensate"))
    ));
    // Asset + party typing.
    assert!(out.has(Some(&ex("photo")), Some(RDF_TYPE), Some(&odrl("Asset"))));
    assert!(out.has(Some(&ex("acme")), Some(RDF_TYPE), Some(&odrl("Party"))));
}

/// Twin of `test_spdx_projection_emits_listed_license`.
#[gmeow_test_batch_macros::batch_test]
fn spdx_projection_emits_listed_license() {
    let out = rights_source().construct(&[], &read_query("generated/queries/spdx.rq"));
    assert!(out.has(Some(&ex("cc-by-4")), Some(RDF_TYPE), Some(&spdx("License"))));
    assert!(
        out.objects_lex(&ex("cc-by-4"), &spdx("licenseId"))
            .contains("CC-BY-4.0"),
        "spdx:licenseId must carry the SPDX id"
    );
    assert!(!out.objects_lex(&ex("cc-by-4"), &spdx("name")).is_empty());
    assert!(
        !out.objects_lex(&ex("cc-by-4"), &spdx("licenseText"))
            .is_empty()
    );
}

/// Twin of `test_cc_projection_emits_license_and_attribution`.
#[gmeow_test_batch_macros::batch_test]
fn cc_projection_emits_license_and_attribution() {
    let out = rights_source().construct(&[], &read_query("generated/queries/cc.rq"));
    assert!(out.has(
        Some(&ex("photo")),
        Some(&cc("license")),
        Some(&ex("cc-by-4"))
    ));
    assert!(out.has(Some(&ex("cc-by-4")), Some(RDF_TYPE), Some(&cc("License"))));
    assert!(
        out.objects_lex(&ex("photo"), &cc("attributionName"))
            .contains("Photo by Jane Doe / CC BY 4.0"),
        "cc:attributionName must carry the attribution string"
    );
}

/// Twin of `test_dcterms_projection_emits_flat_rights`.
#[gmeow_test_batch_macros::batch_test]
fn dcterms_projection_emits_flat_rights() {
    let out = rights_source().construct(&[], &read_query("generated/queries/dcterms.rq"));
    assert!(out.has(
        Some(&ex("photo")),
        Some(&dcterms("license")),
        Some(&ex("cc-by-4"))
    ));
    assert!(out.has(
        Some(&ex("photo")),
        Some(&dcterms("rightsHolder")),
        Some(&ex("jane"))
    ));
    assert!(
        out.objects_lex(&ex("photo"), &dcterms("rights"))
            .contains("© 2026 Jane Doe"),
        "dcterms:rights must carry the flattened rights string"
    );
}

/// Twin of `test_schema_projection_emits_rights_cluster`.
#[gmeow_test_batch_macros::batch_test]
fn schema_projection_emits_rights_cluster() {
    let out = rights_source().construct(&[], &read_query("generated/queries/schema-org.rq"));
    assert!(out.has(
        Some(&ex("photo")),
        Some(&schema("copyrightHolder")),
        Some(&ex("jane"))
    ));
    assert!(out.has(
        Some(&ex("photo")),
        Some(&schema("license")),
        Some(&ex("cc-by-4"))
    ));
    assert!(out.has(
        Some(&ex("acme-mark")),
        Some(RDF_TYPE),
        Some(&schema("Brand"))
    ));
    assert!(
        !out.objects_lex(&ex("photo"), &schema("copyrightYear"))
            .is_empty()
    );
    assert!(
        !out.objects_lex(&ex("photo"), &schema("copyrightNotice"))
            .is_empty()
    );
    assert!(
        out.objects_lex(&ex("photo"), &schema("creditText"))
            .contains("Photo by Jane Doe / CC BY 4.0"),
        "schema:creditText must carry the credit string"
    );
}
