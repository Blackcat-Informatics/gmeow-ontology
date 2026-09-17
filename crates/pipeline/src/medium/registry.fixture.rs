// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A minimal but COMPLETE medium declaration, in the same shape
//! `slices/core/gts/module.ttl` carries. Used by every module in `medium::`, so
//! the tests exercise the real reader against real Turtle rather than a
//! hand-built registry struct that could drift from what the carrier says.

use std::sync::Arc;

use purrdf::RdfDataset;

/// The declaration under test, with `{extra}` spliced in for the negative cases.
pub(crate) fn turtle(extra: &str) -> String {
    format!(
        r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:corpusTrainingSplitV1 a gmeow:CorpusTrainingSplit ;
    gmeow:splitHeldOutStride 8 ;
    gmeow:splitHeldOutOffset 0 .

gmeow:corpusCore a gmeow:DictionaryCorpus ;
    gmeow:corpusSelectsBlobRep "cells-archive" ;
    gmeow:corpusSelectsPathPrefix "slices/core/" .

gmeow:corpusTerms a gmeow:DictionaryCorpus ;
    gmeow:corpusSelectsGraph <https://blackcatinformatics.ca/gmeow/graph/statements> .

gmeow:dictCore a gmeow:CompressionDictionary ;
    gmeow:dictionaryId "gmeow-core-v1" ;
    gmeow:dictionaryVersion "1" ;
    gmeow:dictionaryStrategy gmeow:dictStrategyTrained ;
    gmeow:dictionaryTargetLength 4096 ;
    gmeow:trainsOverCorpus gmeow:corpusCore .

gmeow:dictTerms a gmeow:CompressionDictionary ;
    gmeow:dictionaryId "gmeow-terms-v1" ;
    gmeow:dictionaryVersion "1" ;
    gmeow:dictionaryStrategy gmeow:dictStrategyTermTable ;
    gmeow:dictionaryTargetLength 4096 ;
    gmeow:trainsOverCorpus gmeow:corpusTerms .

gmeow:payloadSchemaCells a gmeow:PayloadSchema ; gmeow:payloadSchemaId "cells-archive" ;
    gmeow:payloadSchemaMedium gmeow:mediumDist ;
    gmeow:payloadSchemaDictionary gmeow:dictCore .
gmeow:payloadSchemaSnapshot a gmeow:PayloadSchema ; gmeow:payloadSchemaId "gmeow:snapshot/wire" ;
    gmeow:payloadSchemaMedium gmeow:mediumBaseline .
# Registered but DELIBERATELY unassigned: the negative case for a rep that has a
# schema and no gmeow:payloadSchemaMedium.
gmeow:payloadSchemaOrphan a gmeow:PayloadSchema ; gmeow:payloadSchemaId "orphan-archive" .

gmeow:mediumDist a gmeow:ZstdDictMedium ;
    gmeow:mediumCodec gmeow:codecZstdRsyncable ;
    gmeow:mediumZstdLevel 12 ;
    gmeow:mediumSourceKind gmeow:mediumSourcePerRep ;
    gmeow:requiresReaderCapability "zstd-dictionary" , "zstd-rsyncable" ;
    gmeow:mediumDictionary gmeow:dictCore , gmeow:dictTerms .

gmeow:mediumBaseline a gmeow:ZstdDictMedium ;
    gmeow:mediumCodec gmeow:codecZstdRsyncable ;
    gmeow:mediumZstdLevel 12 ;
    gmeow:mediumSourceKind gmeow:mediumSourceWholeArtifact ;
    gmeow:requiresReaderCapability "zstd-rsyncable" .
{extra}
"#
    )
}

pub(crate) fn dataset(extra: &str) -> Arc<RdfDataset> {
    purrdf::parse_dataset(turtle(extra).as_bytes(), "text/turtle", None)
        .expect("the medium fixture parses as Turtle")
}
