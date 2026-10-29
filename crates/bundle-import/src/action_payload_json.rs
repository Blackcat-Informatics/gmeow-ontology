// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete native receipt payloads; hydration preserves stored fields verbatim.

use super::{
    BundleFixtureSelector, CorpusArtifactPayload, CorpusArtifactPublication,
    TestFixtureSelectorEnvelope,
};
use gmeow_errors::json::read_unsigned;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record! { impl ToJson for CorpusArtifactPayload {
    "schema_version" => schema_version,
    "name" => name,
    "source_sha256" => source_sha256,
}}
impl FromJson for CorpusArtifactPayload {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CorpusArtifactPayload")?;
        let result = Self {
            schema_version: fields
                .required_with("schema_version", |value| read_unsigned::<u32>(value, "u32"))?,
            name: fields.required("name")?,
            source_sha256: fields.required("source_sha256")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

::purrdf_lex::json_record! { impl ToJson for CorpusArtifactPublication {
    "schema_version" => schema_version,
    "action_key" => action_key,
    "receipt_digest" => receipt_digest,
    "build_fingerprint" => build_fingerprint,
    "artifact_producer_fingerprint" => artifact_producer_fingerprint,
    "name" => name,
    "source_sha256" => source_sha256,
    "product_digest" => product_digest,
    "product_bytes" => product_bytes,
}}

impl FromJson for CorpusArtifactPublication {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CorpusArtifactPublication")?;
        let result = Self {
            schema_version: fields
                .required_with("schema_version", |value| read_unsigned::<u32>(value, "u32"))?,
            action_key: fields.required("action_key")?,
            receipt_digest: fields.required("receipt_digest")?,
            build_fingerprint: fields.required("build_fingerprint")?,
            artifact_producer_fingerprint: fields.required("artifact_producer_fingerprint")?,
            name: fields.required("name")?,
            source_sha256: fields.required("source_sha256")?,
            product_digest: fields.required("product_digest")?,
            product_bytes: fields
                .required_with("product_bytes", |value| read_unsigned::<u64>(value, "u64"))?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

::purrdf_lex::json_record! { impl ToJson for BundleFixtureSelector {
    "schema_version" => schema_version,
    "receipt_digest" => receipt_digest,
    "receipt" => receipt,
    "corpus_artifacts" => corpus_artifacts,
}}

impl FromJson for BundleFixtureSelector {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct BundleFixtureSelector")?;
        let result = Self {
            schema_version: fields
                .required_with("schema_version", |value| read_unsigned::<u32>(value, "u32"))?,
            receipt_digest: fields.required("receipt_digest")?,
            receipt: fields.required("receipt")?,
            corpus_artifacts: fields.required("corpus_artifacts")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

impl FromJson for TestFixtureSelectorEnvelope {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct TestFixtureSelectorEnvelope")?;
        let result = Self {
            bundle_import: fields.required("bundle_import")?,
        };
        // This record projects its own fields from the shared selector root.
        Ok(result)
    }
}
