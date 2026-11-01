// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native registry observation transport; source policy and authentication are separate.

use super::{
    CatalogFamily, CatalogRegistry, NativeKernelEvidence, RegistryObservations, ResidueExemption,
};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for CatalogFamily {
    fn to_json(&self) -> Value {
        Object::new()
            .with("iri", self.iri.to_json())
            .with("name", self.name.to_json())
            .with("namespaces", self.namespaces.to_json())
            .with("owners", self.owners.to_json())
            .with("minimum", self.minimum)
            .into()
    }
}

impl ToJson for ResidueExemption {
    fn to_json(&self) -> Value {
        Object::new()
            .with("iri", self.iri.to_json())
            .with("family_iri", self.family_iri.to_json())
            .with("rationale", self.rationale.to_json())
            .with("row_ceiling", self.row_ceiling)
            .into()
    }
}

impl ToJson for CatalogRegistry {
    fn to_json(&self) -> Value {
        Object::new()
            .with("families", self.families.to_json())
            .with("exemptions", self.exemptions.to_json())
            .with(
                "native_kernel_families",
                self.native_kernel_families.to_json(),
            )
            .into()
    }
}

impl ToJson for RegistryObservations {
    fn to_json(&self) -> Value {
        Object::new()
            .with("registry", self.registry.to_json())
            .with("guarded_namespaces", self.guarded_namespaces.to_json())
            .with(
                "native_kernel_evidence",
                self.native_kernel_evidence.to_json(),
            )
            .into()
    }
}

impl FromJson for CatalogFamily {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct CatalogFamily")?;
        Ok(Self {
            iri: record.required("iri")?,
            name: record.required("name")?,
            namespaces: record.required("namespaces")?,
            owners: record.required("owners")?,
            minimum: record.required_with("minimum", |value| {
                crate::json_counts::read_usize(value, "usize")
            })?,
        })
    }
}

impl FromJson for ResidueExemption {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct ResidueExemption")?;
        Ok(Self {
            iri: record.required("iri")?,
            family_iri: record.required("family_iri")?,
            rationale: record.required("rationale")?,
            row_ceiling: record.required_with("row_ceiling", |value| {
                crate::json_counts::read_usize(value, "usize")
            })?,
        })
    }
}

impl FromJson for CatalogRegistry {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct CatalogRegistry")?;
        Ok(Self {
            families: record.required("families")?,
            exemptions: record.required("exemptions")?,
            native_kernel_families: record.required("native_kernel_families")?,
        })
    }
}

impl FromJson for RegistryObservations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct RegistryObservations")?;
        Ok(Self {
            registry: record.required("registry")?,
            guarded_namespaces: record.required("guarded_namespaces")?,
            native_kernel_evidence: record.required("native_kernel_evidence")?,
        })
    }
}

impl ToJson for NativeKernelEvidence {
    fn to_json(&self) -> Value {
        Object::new()
            .with("family", self.family.to_json())
            .with("correspondence", self.correspondence.to_json())
            .with("semantic_key", self.semantic_key.to_json())
            .with("source", self.source.to_json())
            .with("target", self.target.to_json())
            .with("source_owner", self.source_owner.to_json())
            .with("source_module_path", self.source_module_path.to_json())
            .with("source_module_digest", self.source_module_digest.to_json())
            .with("artifact_owner", self.artifact_owner.to_json())
            .with("artifact_path", self.artifact_path.to_json())
            .with("artifact_digest", self.artifact_digest.to_json())
            .into()
    }
}

impl FromJson for NativeKernelEvidence {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct NativeKernelEvidence")?;
        Ok(Self {
            family: record.required("family")?,
            correspondence: record.required("correspondence")?,
            semantic_key: record.required("semantic_key")?,
            source: record.required("source")?,
            target: record.required("target")?,
            source_owner: record.required("source_owner")?,
            source_module_path: record.required("source_module_path")?,
            source_module_digest: record.required("source_module_digest")?,
            artifact_owner: record.required("artifact_owner")?,
            artifact_path: record.required("artifact_path")?,
            artifact_digest: record.required("artifact_digest")?,
        })
    }
}

/// Decode already-authenticated bytes without reading sources or selecting fixtures.
/// Keep native refusal pointers and causes in the actual consumer's primary error.
pub(super) fn decode_observations(bytes: &[u8]) -> gmeow_errors::Result<RegistryObservations> {
    purrdf_lex::json::record::from_slice(bytes).map_err(|cause| {
        gmeow_errors::Diag::of_kind(crate::error::RecordedInput {
            operation: "decode authenticated catalog-family observations",
            cause: std::sync::Arc::new(cause),
        })
    })
}

#[cfg(test)]
mod tests;
