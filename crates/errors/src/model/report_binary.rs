// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Full-publication CBOR persistence. Human-readable Serde would give metadata
//! a second, quoted-document JSON meaning, so it is explicitly refused.

use super::Report;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser::SerializeMap};

impl Serialize for Report {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            return Err(serde::ser::Error::custom(
                "Report JSON requires the native diagnostics version 2 codec",
            ));
        }
        let count = 1
            + usize::from(!self.findings.is_empty())
            + usize::from(!self.rules.is_empty())
            + usize::from(!self.metadata.is_empty());
        let mut fields = serializer.serialize_map(Some(count))?;
        fields.serialize_entry("tool", &self.tool)?;
        if !self.findings.is_empty() {
            fields.serialize_entry("findings", &self.findings)?;
        }
        if !self.rules.is_empty() {
            fields.serialize_entry("rules", &self.rules)?;
        }
        if !self.metadata.is_empty() {
            fields.serialize_entry("metadata", &self.metadata)?;
        }
        fields.end()
    }
}

impl<'de> Deserialize<'de> for Report {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            return Err(de::Error::custom(
                "Report JSON requires the native diagnostics version 2 codec",
            ));
        }
        struct BinaryReport;
        impl<'de> de::Visitor<'de> for BinaryReport {
            type Value = Report;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("binary diagnostic report with distinct fields")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut fields: A) -> Result<Report, A::Error> {
                let mut tool = None;
                let mut findings = None;
                let mut rules = None;
                let mut metadata = None;
                while let Some(name) = fields.next_key::<String>()? {
                    match name.as_str() {
                        "tool" => {
                            if tool.is_some() {
                                return Err(de::Error::duplicate_field("tool"));
                            }
                            tool = Some(fields.next_value()?);
                        }
                        "findings" => {
                            if findings.is_some() {
                                return Err(de::Error::duplicate_field("findings"));
                            }
                            findings = Some(fields.next_value()?);
                        }
                        "rules" => {
                            if rules.is_some() {
                                return Err(de::Error::duplicate_field("rules"));
                            }
                            rules = Some(fields.next_value()?);
                        }
                        "metadata" => {
                            if metadata.is_some() {
                                return Err(de::Error::duplicate_field("metadata"));
                            }
                            metadata = Some(fields.next_value()?);
                        }
                        _ => {
                            let _ = fields.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(Report {
                    tool: tool.ok_or_else(|| de::Error::missing_field("tool"))?,
                    findings: findings.unwrap_or_default(),
                    rules: rules.unwrap_or_default(),
                    metadata: metadata.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_map(BinaryReport)
    }
}
