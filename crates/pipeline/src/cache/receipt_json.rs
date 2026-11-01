// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete native receipt payloads; hydration preserves stored fields verbatim.

use super::{RawInputDigest, ReceiptEntity, StageInputDigest, StageKeyContext, StageReceipt};
use gmeow_errors::json::read_unsigned;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record! { impl ToJson for StageInputDigest {
    "producer" => producer,
    "entity" => entity,
    "digest" => digest,
}}
impl FromJson for StageInputDigest {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct StageInputDigest")?;
        let result = Self {
            producer: fields.required("producer")?,
            entity: fields.required("entity")?,
            digest: fields.required("digest")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

::purrdf_lex::json_record! { impl ToJson for RawInputDigest {
    "path" => path,
    "digest" => digest,
}}
impl FromJson for RawInputDigest {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct RawInputDigest")?;
        let result = Self {
            path: fields.required("path")?,
            digest: fields.required("digest")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

::purrdf_lex::json_record! { impl ToJson for StageKeyContext {
    "schema_version" => schema_version,
    "stage_id" => stage_id,
    "impl_version" => impl_version,
    "codec" => codec,
    "build" => build,
    "upstream" => upstream,
    "raw_inputs" => raw_inputs,
    "dimensions" => dimensions,
}}
impl FromJson for StageKeyContext {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct StageKeyContext")?;
        let result = Self {
            schema_version: fields
                .required_with("schema_version", |value| read_unsigned::<u32>(value, "u32"))?,
            stage_id: fields.required("stage_id")?,
            impl_version: fields.required("impl_version")?,
            codec: fields.required("codec")?,
            build: fields.required("build")?,
            upstream: fields.required("upstream")?,
            raw_inputs: fields.required("raw_inputs")?,
            dimensions: fields.required("dimensions")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

::purrdf_lex::json_record! { impl ToJson for ReceiptEntity {
    "identity" => identity,
    "digest" => digest,
    "structural_count" => structural_count,
    "decoded_bytes" => decoded_bytes,
}}
impl FromJson for ReceiptEntity {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ReceiptEntity")?;
        let result = Self {
            identity: fields.required("identity")?,
            digest: fields.required("digest")?,
            structural_count: fields.required_with("structural_count", |value| {
                read_unsigned::<u64>(value, "u64")
            })?,
            decoded_bytes: fields
                .required_with("decoded_bytes", |value| read_unsigned::<u64>(value, "u64"))?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

::purrdf_lex::json_record! { impl ToJson for StageReceipt {
    "schema_version" => schema_version,
    "action_key" => action_key,
    "context" => context,
    "stability" => stability,
    "cache_disposition" => cache_disposition,
    "product_digest" => product_digest,
    "product_blob_digest" => product_blob_digest,
    "product_blob_bytes" => product_blob_bytes,
    "dataset_quads" => dataset_quads,
    "default_graph" => default_graph,
    "provenance" => provenance,
    "content_store" => content_store,
    "graphs" => graphs,
    "blob_representations" => blob_representations,
    "logical_artifacts" => logical_artifacts,
    "typed_handles" => typed_handles,
}}
impl FromJson for StageReceipt {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct StageReceipt")?;
        let result = Self {
            schema_version: fields
                .required_with("schema_version", |value| read_unsigned::<u32>(value, "u32"))?,
            action_key: fields.required("action_key")?,
            context: fields.required("context")?,
            stability: fields.required("stability")?,
            cache_disposition: fields.required("cache_disposition")?,
            product_digest: fields.required("product_digest")?,
            product_blob_digest: fields.required("product_blob_digest")?,
            product_blob_bytes: fields.required_with("product_blob_bytes", |value| {
                read_unsigned::<u64>(value, "u64")
            })?,
            dataset_quads: fields
                .required_with("dataset_quads", |value| read_unsigned::<u64>(value, "u64"))?,
            default_graph: fields.required("default_graph")?,
            provenance: fields.required("provenance")?,
            content_store: fields.required("content_store")?,
            graphs: fields.required("graphs")?,
            blob_representations: fields.required("blob_representations")?,
            logical_artifacts: fields.required("logical_artifacts")?,
            typed_handles: fields.required("typed_handles")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
