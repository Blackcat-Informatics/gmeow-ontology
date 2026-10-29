// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete action-key framing. Normalization belongs to the constructors;
//! this writer preserves every stored field, vector entry and explicit null.

use purrdf_lex::json::{Object, Value, record::ToJson};

use super::{ActionContext, ActionInput, FileKind, ProducerIdentity};

impl ToJson for ProducerIdentity {
    fn to_json(&self) -> Value {
        Object::new()
            .with("digest", self.digest.to_json())
            .with("toolchain", self.toolchain.to_json())
            .with("target", self.target.to_json())
            .with("profile", self.profile.to_json())
            .with("features", self.features.to_json())
            .into()
    }
}

impl ToJson for FileKind {
    fn to_json(&self) -> Value {
        match self {
            Self::File => "file",
            Self::Symlink => "symlink",
            Self::Aggregate => "aggregate",
        }
        .to_json()
    }
}

impl ToJson for ActionInput {
    fn to_json(&self) -> Value {
        match self {
            Self::Upstream {
                producer,
                entity,
                receipt_digest,
                product_digest,
            } => Object::new()
                .with("kind", "upstream".to_json())
                .with("producer", producer.to_json())
                .with("entity", entity.to_json())
                .with("receipt_digest", receipt_digest.to_json())
                .with("product_digest", product_digest.to_json())
                .into(),
            Self::Raw {
                logical_path,
                file_kind,
                executable,
                digest,
            } => Object::new()
                .with("kind", "raw".to_json())
                .with("logical_path", logical_path.to_json())
                .with("file_kind", file_kind.to_json())
                .with("executable", executable.to_json())
                .with("digest", digest.to_json())
                .into(),
        }
    }
}

impl ToJson for ActionContext {
    fn to_json(&self) -> Value {
        Object::new()
            .with("schema_version", self.schema_version.to_json())
            .with("domain", self.domain.to_json())
            .with("action", self.action.to_json())
            .with("implementation", self.implementation.to_json())
            .with("codec", self.codec.to_json())
            .with("inputs", self.inputs.to_json())
            .with("dimensions", self.dimensions.to_json())
            .into()
    }
}
