// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Executable teeth for the GMN version-migration executor and the version-provenance stamp
//! (`crates/lang-bridge/src/gmn_migrate.rs`, including migration and provenance handling).
//!
//! The authored demonstrator is `slices/grounding/lang/examples/gmn-migration.ttl`: a
//! synthetic v1 → v2 crossing carrying a real schema delta (a ¬→! glyph rename with a 90→80
//! precedence change on `logic:not`, plus a bridged ⊻→^ drop of a retired operator). The
//! preservation judgment is a `logic:Correspondence`, never a boolean flag; the producer-backed corpus tests assert
//! the JUDGMENT and hard-fail on an unbridged drop.

use std::collections::BTreeSet;

use gmeow_lang_bridge::{
    GmnMigrateError, GmnMigration, GmnRecordSet, OperatorOccurrence, header_schema_major,
};
use purrdf::parse_dataset;

const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/lang/";

/// A source operator DROPPED by the target major with NO covering rewrite hard-fails with the
/// named `lang:GmnUnbridgedGlyphDrop` class; the SAME drop WITH an authored rewrite succeeds.
#[test]
fn migration_dropping_glyph_without_leg_hard_fails() {
    let legacy_xor = format!("{EX}gmnLegacyXorOp");
    // A document using the retired xor operator; the target major does NOT define it.
    let doc = GmnRecordSet {
        operators: vec![OperatorOccurrence::new(legacy_xor.clone(), "⊻", None)],
    };
    let target_inventory = BTreeSet::new(); // target retires xor

    // WITHOUT a bridging leg: a synthetic migration that migrates v1→v2 but authors NO rewrite
    // for the dropped operator → HARD FAIL with the named conformance class.
    let no_leg_ttl = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/lang/> .
ex:vSrc owl:versionInfo "1" .
ex:vTgt owl:versionInfo "2" .
ex:noLegMigration a logic:Correspondence ;
    gmeow:gmnMigratesFrom ex:vSrc ;
    gmeow:gmnMigratesTo ex:vTgt ;
    logic:preservationKind logic:ExactPreservation ;
    logic:mnemomorphic true .
"#;
    let no_leg_ds = parse_dataset(no_leg_ttl.as_bytes(), "text/turtle", None).expect("parses");
    let no_leg = GmnMigration::from_dataset(&no_leg_ds, &format!("{EX}noLegMigration"))
        .expect("the rewrite-free leg loads");
    assert!(no_leg.rewrite_for(&legacy_xor).is_none());

    let err = no_leg
        .migrate(&doc, &target_inventory)
        .expect_err("an unbridged glyph drop must hard-fail");
    assert_eq!(
        err,
        GmnMigrateError::UnbridgedGlyphDrop {
            term: legacy_xor.clone(),
            glyph: "⊻".to_owned(),
        }
    );
    assert_eq!(
        err.failure_class(),
        Some(GmnMigrateError::CLASS_UNBRIDGED_GLYPH_DROP),
        "the drop names the lang:GmnUnbridgedGlyphDrop conformance class"
    );
}

/// A stored source-major GMN-1 document exercising all three non-failing branches: logic:not
/// (¬, rewritten), logic:subClassOf (⊑, native survivor), and the retired xor operator (⊻,
/// resolved through the migration leg's own rewrite, since it is not in the current registry).
const STORED_V1_DOC: &str = "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n\
     @ℒ{s:ex__a,p:ex__rel,o:¬}\n\
     @ℒ{s:ex__b,p:ex__rel,o:⊑}\n\
     @ℒ{s:ex__c,p:ex__rel,o:⊻}\n";

/// [`header_schema_major`] reads the `v:` coordinate off the leading `@gmn{…}` header, and a
/// document with no such header is a malformed input (None), never migrated on a guess.
#[test]
fn header_schema_major_reads_the_v_coordinate() {
    assert_eq!(header_schema_major(STORED_V1_DOC).as_deref(), Some("1"));
    assert_eq!(
        header_schema_major("@ℒ{s:ex__a,p:ex__rel,o:¬}\n"),
        None,
        "a headerless document pins no source major"
    );
}
