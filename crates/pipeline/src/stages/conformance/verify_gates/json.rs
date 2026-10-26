// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete native verification observations, including fallible report metadata
//! projections. Cache readers use the shared diagnostic envelope profile.

use super::{Observations, RepairObservation, SourceObservation};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl Observations {
    pub(super) fn json_value(&self) -> Result<Value, DecodeError> {
        let mut sources = Object::new();
        for (name, source) in &self.sources {
            sources.insert(name, source.json_value()?);
        }
        Ok(Object::new()
            .with("producer", self.producer.to_json())
            .with("query_digests", self.query_digests.to_json())
            .with("query_set_digest", self.query_set_digest.as_str())
            .with("law_source_digests", self.law_source_digests.to_json())
            .with("sources", sources)
            .into())
    }
}

impl SourceObservation {
    pub(super) fn json_value(&self) -> Result<Value, DecodeError> {
        let mut repairs = Object::new();
        for (name, repair) in &self.repairs {
            repairs.insert(name, repair.json_value()?);
        }
        Ok(Object::new()
            .with("source_path", self.source_path.as_str())
            .with("source_digest", self.source_digest.as_str())
            .with("original", self.original.json_value()?)
            .with("repairs", repairs)
            .into())
    }
}

impl RepairObservation {
    fn json_value(&self) -> Result<Value, DecodeError> {
        Ok(Object::new()
            .with("mutation_digest", self.mutation_digest.as_str())
            .with("removed_rows", self.removed_rows.to_json())
            .with("inserted_rows", self.inserted_rows.to_json())
            .with("report", self.report.json_value()?)
            .into())
    }
}

impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "verification observations")?;
        Ok(Self {
            producer: fields.required("producer")?,
            query_digests: fields.required("query_digests")?,
            query_set_digest: fields.required("query_set_digest")?,
            law_source_digests: fields.required("law_source_digests")?,
            sources: fields.required("sources")?,
        })
    }
}

impl FromJson for SourceObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "verification source observation")?;
        Ok(Self {
            source_path: fields.required("source_path")?,
            source_digest: fields.required("source_digest")?,
            original: fields.required("original")?,
            repairs: fields.required("repairs")?,
        })
    }
}

impl FromJson for RepairObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "verification repair observation")?;
        Ok(Self {
            mutation_digest: fields.required("mutation_digest")?,
            removed_rows: fields.required_with("removed_rows", |value| {
                gmeow_errors::json::read_unsigned(value, "usize removed row count")
            })?,
            inserted_rows: fields.required_with("inserted_rows", |value| {
                gmeow_errors::json::read_unsigned(value, "usize inserted row count")
            })?,
            report: fields.required("report")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_action_cache::{ActionStore, STORE_FORMAT_VERSION, StoreLimits};

    #[test]
    fn verification_observation_cache_preserves_nested_reports_without_recomputation() {
        // gmeow-test-input: synthetic-only
        let deep = format!(
            "{}1e400{}",
            "[".repeat(gmeow_errors::METADATA_JSON_DEPTH),
            "]".repeat(gmeow_errors::METADATA_JSON_DEPTH)
        );
        let report = gmeow_errors::Report::from_json_slice(
            format!(
                r#"{{"schema":"{}","tool":"verify","metadata":{{"deep":{deep}}}}}"#,
                gmeow_errors::REPORT_JSON_SCHEMA
            )
            .as_bytes(),
        )
        .unwrap();
        let observed = SourceObservation {
            source_path: "authored/scene.ttl".into(),
            source_digest: "authored-source".into(),
            original: report.clone(),
            repairs: std::collections::BTreeMap::from([(
                "repair".into(),
                RepairObservation {
                    mutation_digest: "authored-mutation".into(),
                    removed_rows: 1,
                    inserted_rows: 2,
                    report,
                },
            )]),
        };
        let directory = tempfile::tempdir().unwrap();
        let store = ActionStore::open(
            directory.path(),
            STORE_FORMAT_VERSION,
            StoreLimits::default(),
        )
        .unwrap();
        let encoded = crate::stages::conformance::execution::cached_bytes(
            &store,
            "authored-verify-report-v2",
            Vec::new(),
            || Ok(purrdf_lex::json::write_compact(&observed.json_value().unwrap()).into_bytes()),
        )
        .unwrap();
        let hit = crate::stages::conformance::execution::cached_bytes(
            &store,
            "authored-verify-report-v2",
            Vec::new(),
            || panic!("an authenticated observation hit must not recompute the scene"),
        )
        .unwrap();
        assert_eq!(hit, encoded);
        let restored: SourceObservation =
            gmeow_errors::json::read_diagnostic_document(&hit).unwrap();
        assert_eq!(restored.source_path, observed.source_path);
        assert_eq!(restored.source_digest, observed.source_digest);
        assert_eq!(restored.original, observed.original);
        assert_eq!(
            restored.repairs["repair"].report,
            observed.repairs["repair"].report
        );
        assert_eq!(
            restored.repairs["repair"].mutation_digest,
            "authored-mutation"
        );
        assert_eq!(
            (
                restored.repairs["repair"].removed_rows,
                restored.repairs["repair"].inserted_rows
            ),
            (1, 2)
        );
        let aggregate = Observations {
            producer: crate::cache::BuildIdentity::current(),
            query_digests: std::collections::BTreeMap::from([("query".into(), "digest".into())]),
            query_set_digest: "query-set".into(),
            law_source_digests: std::collections::BTreeMap::from([(
                "law".into(),
                "law-digest".into(),
            )]),
            sources: std::collections::BTreeMap::from([("scene".into(), restored)]),
        };
        let value = aggregate.json_value().unwrap();
        let decoded: Observations = gmeow_errors::json::read_diagnostic_document(
            purrdf_lex::json::write_compact(&value).as_bytes(),
        )
        .unwrap();
        assert!(decoded.json_value().unwrap().same_text(&value));
    }
}
