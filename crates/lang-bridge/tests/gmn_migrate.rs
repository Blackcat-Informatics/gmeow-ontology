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

use std::collections::BTreeSet;
use std::sync::Arc;

use gmeow_lang_bridge::gmn1_codec::resolve_dialect_acceptance;
use gmeow_lang_bridge::{
    GmnDictionary, GmnMigrateError, GmnMigration, GmnRecordSet, OperatorOccurrence,
    PRED_GMN_SCHEMA_VERSION, derive_target_inventory, extract_operators, header_schema_major,
    reemit_migrated_document, resolved_schema_version, source_operator_table, tag_schema_version,
};
use gmeow_logic_compile::ir::PreservationKind;
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfTerm, parse_dataset};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/lang/";
const LOGIC_NOT: &str = "https://blackcatinformatics.ca/logic/not";

fn lang_module_dataset() -> Arc<RdfDataset> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../slices/grounding/lang/module.ttl"
    );
    let bytes = std::fs::read(path).expect("lang module.ttl is readable");
    parse_dataset(&bytes, "text/turtle", None).expect("lang module.ttl parses")
}

fn demonstrator_dataset() -> Arc<RdfDataset> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../slices/grounding/lang/examples/gmn-migration.ttl"
    );
    let bytes = std::fs::read(path).expect("gmn-migration.ttl is readable");
    parse_dataset(&bytes, "text/turtle", None).expect("gmn-migration.ttl parses")
}

fn demonstrator_migration() -> GmnMigration {
    GmnMigration::from_dataset(
        &demonstrator_dataset(),
        &format!("{EX}gmnMigrationVSrcToVTgt"),
    )
    .expect("the authored migration leg loads")
}

/// The demonstrator delta applies: the target-version output re-spells `logic:not` ¬→! and
/// re-binds it 90→80, bridges the retired ⊻ operator to ^, and carries the crossing's
/// preservation JUDGMENT (asserted as `logic:ExactPreservation`, never a boolean).
#[test]
fn migration_executor_discharges_authored_delta() {
    let migration = demonstrator_migration();

    // A stored source-major document using both operators: ¬ (logic:not, prec 90) and ⊻ (the
    // retired xor operator, no recorded precedence).
    let legacy_xor = format!("{EX}gmnLegacyXorOp");
    let doc = GmnRecordSet {
        operators: vec![
            OperatorOccurrence::new(LOGIC_NOT, "¬", Some(90)),
            OperatorOccurrence::new(legacy_xor.clone(), "⊻", None),
        ],
    };

    // The target major still defines logic:not natively; it has RETIRED the xor operator (the
    // rewrite is what bridges that drop).
    let mut target_inventory = BTreeSet::new();
    target_inventory.insert(LOGIC_NOT.to_owned());

    let migrated = migration
        .migrate(&doc, &target_inventory)
        .expect("every source operator is bridged, so the crossing applies");

    assert_eq!(migrated.target_version, "2");
    // The preservation judgment is respected — assert the JUDGMENT, not a boolean.
    assert_eq!(migrated.preservation, PreservationKind::Exact);
    assert_eq!(migrated.preservation, migration.preservation());

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

    // WITH an authored leg (the demonstrator's ⊻→^ rewrite): the SAME drop succeeds.
    let bridged = demonstrator_migration();
    assert!(bridged.rewrite_for(&legacy_xor).is_some());
    let migrated = bridged
        .migrate(&doc, &target_inventory)
        .expect("the authored rewrite bridges the same drop");
    assert_eq!(migrated.operators.len(), 1);
    assert_eq!(migrated.operators[0].glyph, "^");
    assert!(migrated.operators[0].rewritten);
}

// ── Document-level migration helpers (the `gmeow gmn migrate` plumbing) ──────────────

/// A stored source-major GMN-1 document exercising all three non-failing branches: logic:not
/// (¬, rewritten), logic:subClassOf (⊑, native survivor), and the retired xor operator (⊻,
/// resolved through the migration leg's own rewrite, since it is not in the current registry).
const STORED_V1_DOC: &str = "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n\
     @ℒ{s:ex__a,p:ex__rel,o:¬}\n\
     @ℒ{s:ex__b,p:ex__rel,o:⊑}\n\
     @ℒ{s:ex__c,p:ex__rel,o:⊻}\n";

const LOGIC_SUBCLASS_OF: &str = "https://blackcatinformatics.ca/logic/subClassOf";

fn lang_dictionary() -> GmnDictionary {
    GmnDictionary::from_dataset(&lang_module_dataset()).expect("dict loads from the lang module")
}

/// The source operator surface table is GRAPH-DERIVED — the dictionary's executable registry
/// (with precedence read off each operator's denoted form) UNIONED with the migration leg's
/// authored rewrites (which carry the source surface of the bridged/retired ⊻ the current
/// registry no longer lists). [`extract_operators`] then projects a stored document's tokens
/// back to their version-stable terms, in document order.
#[test]
fn source_table_and_extract_are_graph_derived() {
    let dict = lang_dictionary();
    let migration = demonstrator_migration();
    let table = source_operator_table(&dict, &migration, &lang_module_dataset());

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

    let record_set = extract_operators(STORED_V1_DOC, &table);
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
    let inventory = derive_target_inventory(
        &demonstrator_dataset(),
        &format!("{EX}gmnMigrationVSrcToVTgt"),
    )
    .expect("the target inventory reads off the authored leg");
    let expected: BTreeSet<String> = [LOGIC_SUBCLASS_OF.to_owned()].into_iter().collect();
    assert_eq!(inventory, expected);
}

/// Re-emitting the migrated document substitutes each operator's source glyph with its
/// target-major glyph (¬→!, the ⊻→^ bridge) and re-stamps the `@gmn{v: …}` header to the target
/// major, while the ⊑ native survivor and every non-operator byte are preserved verbatim.
#[test]
fn reemit_substitutes_glyphs_and_restamps_header() {
    let dict = lang_dictionary();
    let migration = demonstrator_migration();
    let table = source_operator_table(&dict, &migration, &lang_module_dataset());
    let record_set = extract_operators(STORED_V1_DOC, &table);

    let inventory = derive_target_inventory(
        &demonstrator_dataset(),
        &format!("{EX}gmnMigrationVSrcToVTgt"),
    )
    .expect("target inventory");
    let migrated = migration
        .migrate(&record_set, &inventory)
        .expect("every source operator is bridged or survives");

    let reemitted = reemit_migrated_document(STORED_V1_DOC, &record_set, &migrated, "2");
    assert_eq!(
        reemitted,
        "@gmn{v: 2, aliases: dict-v3, glyphs: 2}\n\
         @ℒ{s:ex__a,p:ex__rel,o:!}\n\
         @ℒ{s:ex__b,p:ex__rel,o:⊑}\n\
         @ℒ{s:ex__c,p:ex__rel,o:^}\n",
        "the ¬→! rename, the ⊑ survivor, the ⊻→^ bridge, and the 1→2 header re-stamp"
    );
}

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

/// A tagged record carries exactly one `gmeow:gmnSchemaVersion` matching the resolved codebook
/// major — the graph-resolved acceptance policy, not a Rust constant, and single-valued.
#[test]
fn resolved_version_provenance_is_single_valued() {
    let ds = lang_module_dataset();
    let dict = GmnDictionary::from_dataset(&ds).expect("dict loads from the carrier");
    let resolved = resolved_schema_version(&dict);

    // The stamped value IS the graph-resolved dialect acceptance's latest major.
    let acceptance = resolve_dialect_acceptance(&ds)
        .expect("resolve acceptance")
        .expect("lang module carries the dialect lineage");
    assert_eq!(resolved, acceptance.latest_major_key());
    assert_eq!(resolved, dict.schema_major());

    // Tag two distinct emitted records and fold them into a dataset.
    let rec_a = format!("{GMEOW}examples/lang/metricRowA");
    let rec_b = format!("{GMEOW}examples/lang/verbalizationRowB");
    let mut b = RdfDatasetBuilder::new();
    b.push_owned_quad(&tag_schema_version(&rec_a, &dict));
    b.push_owned_quad(&tag_schema_version(&rec_b, &dict));
    let tagged = b.freeze().expect("freeze");

    // Each record carries EXACTLY ONE gmnSchemaVersion, and every value is the resolved major.
    for record in [&rec_a, &rec_b] {
        let values: Vec<String> = tagged
            .owned_quads()
            .filter(|q| {
                q.predicate == PRED_GMN_SCHEMA_VERSION
                    && matches!(&q.subject, RdfTerm::Iri(s) if s == record)
            })
            .filter_map(|q| match q.object {
                RdfTerm::Literal(l) => Some(l.lexical_form),
                _ => None,
            })
            .collect();
        assert_eq!(
            values.len(),
            1,
            "record {record} carries exactly one schema-version stamp"
        );
        assert_eq!(
            values[0], resolved,
            "the stamp is the resolved codebook major"
        );
    }

    // Determinism: re-stamping the same record yields the byte-identical quad.
    assert_eq!(
        tag_schema_version(&rec_a, &dict),
        tag_schema_version(&rec_a, &dict)
    );
}
