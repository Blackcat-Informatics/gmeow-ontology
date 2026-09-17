// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Executable teeth for the GMN version-migration executor and the version-provenance stamp
//! (`crates/lang-bridge/src/gmn_migrate.rs`, including migration and provenance handling).
//!
//! The authored demonstrator is `slices/grounding/lang/examples/gmn-migration.ttl`: a
//! synthetic v1 → v2 crossing carrying a real schema delta (a ¬→! glyph rename with a 90→80
//! precedence change on `logic:not`, plus a bridged ⊻→^ drop of a retired operator). The
//! preservation judgment is a `logic:Correspondence`, never a boolean flag; these tests assert
//! the JUDGMENT and hard-fail on an unbridged drop.

use gmeow_lang_bridge::OperatorOccurrence;
use gmeow_logic_compile::ir::PreservationKind;
use std::collections::BTreeSet;

const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/lang/";
const LOGIC_NOT: &str = "https://blackcatinformatics.ca/logic/not";
const LOGIC_SUBCLASS_OF: &str = "https://blackcatinformatics.ca/logic/subClassOf";

fn observation() -> &'static super::super::gmn_migration::Observation {
    super::gmn_grounding::observations()
        .sources
        .get(super::super::gmn_migration::SOURCE)
        .expect("selected migration demonstrator")
        .migration
        .as_ref()
        .expect("producer observed selected migration")
        .as_ref()
        .expect("authored migration leg loads")
}

/// The demonstrator delta applies: the target-version output re-spells `logic:not` ¬→! and
/// re-binds it 90→80, bridges the retired ⊻ operator to ^, and carries the crossing's
/// preservation JUDGMENT (asserted as `logic:ExactPreservation`, never a boolean).
#[test]
fn migration_executor_discharges_authored_delta() {
    let observed = observation();
    let legacy_xor = format!("{EX}gmnLegacyXorOp");
    let migrated = observed
        .executor
        .as_ref()
        .expect("every source operator is bridged, so the crossing applies");

    assert_eq!(migrated.target_version, "2");
    // The preservation judgment is respected — assert the JUDGMENT, not a boolean.
    assert_eq!(migrated.preservation, PreservationKind::Exact);
    assert_eq!(migrated.preservation, observed.declared_preservation);

    let not_op = migrated
        .operators
        .iter()
        .find(|o| o.term == LOGIC_NOT)
        .expect("logic:not survives the crossing");
    assert_eq!(not_op.glyph, "!", "the ¬→! rename is applied");
    assert_eq!(
        not_op.precedence,
        Some(80),
        "the 90→80 precedence change is applied"
    );
    assert!(not_op.rewritten);

    let xor_op = migrated
        .operators
        .iter()
        .find(|o| o.term == legacy_xor)
        .expect("the retired xor operator is re-surfaced, not dropped");
    assert_eq!(xor_op.glyph, "^", "the ⊻→^ bridged drop is applied");
    assert!(xor_op.rewritten);
}

#[test]
fn authored_leg_bridges_the_same_dropped_operator() {
    let observed = observation();
    assert!(observed.bridges_legacy_xor);
    let migrated = observed
        .bridged_drop
        .as_ref()
        .expect("the authored rewrite bridges the same drop");
    assert_eq!(migrated.operators.len(), 1);
    assert_eq!(migrated.operators[0].glyph, "^");
    assert!(migrated.operators[0].rewritten);
}

/// The source operator surface table is GRAPH-DERIVED — the dictionary's executable registry
/// (with precedence read off each operator's denoted form) UNIONED with the migration leg's
/// authored rewrites (which carry the source surface of the bridged/retired ⊻ the current
/// registry no longer lists). [`gmeow_lang_bridge::extract_operators`] then projects a stored document's tokens
/// back to their version-stable terms, in document order.
#[test]
fn source_table_and_extract_are_graph_derived() {
    let observed = observation();
    let table = &observed.source_table;

    // ¬ resolves to logic:not with the leg's authored source precedence 90; ⊑ resolves to
    // logic:subClassOf via the registry; ⊻ resolves to the retired xor operator via the leg.
    assert_eq!(
        table.get("¬"),
        Some(&OperatorOccurrence::new(LOGIC_NOT, "¬", Some(90)))
    );
    let sub = table.get("⊑").expect("⊑ resolves via the registry");
    assert_eq!(sub.term, LOGIC_SUBCLASS_OF);
    assert!(
        sub.precedence.is_some(),
        "the survivor's source precedence is read off its denoted form"
    );
    assert_eq!(
        table.get("⊻"),
        Some(&OperatorOccurrence::new(
            format!("{EX}gmnLegacyXorOp"),
            "⊻",
            None
        ))
    );

    let record_set = &observed.extracted;
    let terms: Vec<&str> = record_set
        .operators
        .iter()
        .map(|o| o.term.as_str())
        .collect();
    assert_eq!(
        terms,
        vec![LOGIC_NOT, LOGIC_SUBCLASS_OF, &format!("{EX}gmnLegacyXorOp")],
        "the operators are the distinct terms used, in document order"
    );
}

/// The target major's native operator inventory is READ FROM THE GRAPH — the
/// `gmeow:gmnVersionDefinesOperator` set on the correspondence's `gmeow:gmnMigratesTo` version
/// entity — never a Rust constant. The demonstrator declares logic:subClassOf as the survivor.
#[test]
fn target_inventory_is_read_from_the_graph() {
    let inventory = observation()
        .inventory
        .as_ref()
        .expect("the target inventory reads off the authored leg");
    let expected: BTreeSet<String> = [LOGIC_SUBCLASS_OF.to_owned()].into_iter().collect();
    assert_eq!(inventory, &expected);
}

/// Re-emitting the migrated document substitutes each operator's source glyph with its
/// target-major glyph (¬→!, the ⊻→^ bridge) and re-stamps the `@gmn{v: …}` header to the target
/// major, while the ⊑ native survivor and every non-operator byte are preserved verbatim.
#[test]
fn reemit_substitutes_glyphs_and_restamps_header() {
    let document = observation()
        .document
        .as_ref()
        .expect("every source operator is bridged or survives");
    let reemitted = &document.text;
    assert_eq!(
        reemitted,
        "@gmn{v: 2, aliases: dict-v3, glyphs: 2}\n\
         @ℒ{s:ex__a,p:ex__rel,o:!}\n\
         @ℒ{s:ex__b,p:ex__rel,o:⊑}\n\
         @ℒ{s:ex__c,p:ex__rel,o:^}\n",
        "the ¬→! rename, the ⊑ survivor, the ⊻→^ bridge, and the 1→2 header re-stamp"
    );
}

#[test]
fn loads_the_authored_demonstrator_leg() {
    let migration = observation();
    assert_eq!(migration.from_version, "1");
    assert_eq!(migration.to_version, "2");
    assert_eq!(migration.declared_preservation, PreservationKind::Exact);
    assert!(migration.mnemomorphic);
    let rename = migration
        .rewrites
        .iter()
        .find(|rewrite| rewrite.term == LOGIC_NOT)
        .expect("logic:not rewrite");
    assert!(rename.is_rename() && rename.is_precedence_change());
    assert_eq!(
        (rename.from_glyph.as_str(), rename.to_glyph.as_str()),
        ("¬", "!")
    );
    assert_eq!(
        (rename.from_precedence, rename.to_precedence),
        (Some(90), Some(80))
    );

    // The bridged-drop rewrite (a glyph rename with no precedence legs).
    let bridged = migration
        .rewrites
        .iter()
        .find(|rewrite| rewrite.term == format!("{EX}gmnLegacyXorOp"))
        .expect("xor rewrite");
    assert!(bridged.is_rename() && !bridged.is_precedence_change());
    assert_eq!(
        (bridged.from_glyph.as_str(), bridged.to_glyph.as_str()),
        ("⊻", "^")
    );
    assert_eq!(
        (bridged.from_precedence, bridged.to_precedence),
        (None, None)
    );
}
