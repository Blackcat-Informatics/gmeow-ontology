// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Migration observations over the selected demonstrator and shared native dictionary.
//! Recorded preservation is the authored judgment, never rewrite certification.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_lang_bridge::{
    GmnDictionary, GmnMigrateError, GmnMigration, GmnRecordSet, MigratedRecordSet,
    OperatorOccurrence, derive_target_inventory, extract_operators, reemit_migrated_document,
};
use gmeow_logic_compile::ir::PreservationKind;
use purrdf::RdfDataset;
use serde::{Deserialize, Serialize};

pub(super) const SOURCE: &str = "slices/grounding/lang/examples/gmn-migration.ttl";
const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/lang/";
const LOGIC_NOT: &str = "https://blackcatinformatics.ca/logic/not";
const STORED_V1_DOC: &str = "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n\
     @ℒ{s:ex__a,p:ex__rel,o:¬}\n\
     @ℒ{s:ex__b,p:ex__rel,o:⊑}\n\
     @ℒ{s:ex__c,p:ex__rel,o:⊻}\n";

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Observation {
    pub correspondence: String,
    pub from_version: String,
    pub to_version: String,
    pub mnemomorphic: bool,
    pub rewrites: Vec<gmeow_lang_bridge::GlyphRewrite>,
    pub declared_preservation: PreservationKind,
    pub executor: Result<MigratedRecordSet, GmnMigrateError>,
    pub bridges_legacy_xor: bool,
    pub bridged_drop: Result<MigratedRecordSet, GmnMigrateError>,
    pub source_table: BTreeMap<String, OperatorOccurrence>,
    pub extracted: GmnRecordSet,
    pub inventory: Result<BTreeSet<String>, GmnMigrateError>,
    pub document: Result<MigratedDocument, GmnMigrateError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct MigratedDocument {
    pub migrated: MigratedRecordSet,
    pub text: String,
}

pub(super) fn observe(
    demonstrator: &RdfDataset,
    lang: &RdfDataset,
    dictionary: &GmnDictionary,
) -> Result<Observation, GmnMigrateError> {
    let correspondence = format!("{EX}gmnMigrationVSrcToVTgt");
    let migration = GmnMigration::from_dataset(demonstrator, &correspondence)?;
    let legacy_xor = format!("{EX}gmnLegacyXorOp");
    let doc = GmnRecordSet {
        operators: vec![
            OperatorOccurrence::new(LOGIC_NOT, "¬", Some(90)),
            OperatorOccurrence::new(&legacy_xor, "⊻", None),
        ],
    };
    let executor = migration.migrate(&doc, &BTreeSet::from([LOGIC_NOT.to_owned()]));
    let bridged_drop = migration.migrate(
        &GmnRecordSet {
            operators: vec![OperatorOccurrence::new(&legacy_xor, "⊻", None)],
        },
        &BTreeSet::new(),
    );
    let source_table = gmeow_lang_bridge::source_operator_table(dictionary, &migration, lang);
    let extracted = extract_operators(STORED_V1_DOC, &source_table);
    let inventory = derive_target_inventory(demonstrator, &correspondence);
    let document = inventory.clone().and_then(|inventory| {
        let migrated = migration.migrate(&extracted, &inventory)?;
        let text = reemit_migrated_document(STORED_V1_DOC, &extracted, &migrated, "2");
        Ok(MigratedDocument { migrated, text })
    });
    Ok(Observation {
        correspondence,
        from_version: migration.from_version().to_owned(),
        to_version: migration.to_version().to_owned(),
        mnemomorphic: migration.mnemomorphic(),
        rewrites: migration.rewrites().cloned().collect(),
        declared_preservation: migration.preservation(),
        executor,
        bridges_legacy_xor: migration.rewrite_for(&legacy_xor).is_some(),
        bridged_drop,
        source_table,
        extracted,
        inventory,
        document,
    })
}
