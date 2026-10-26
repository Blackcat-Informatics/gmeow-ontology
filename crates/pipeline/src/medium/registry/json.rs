// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Typed registry transport only; semantic RDF admission stays with the registry.

use super::{
    CorpusDef, CorpusSelector, DictSelection, DictionaryDef, DictionaryStrategy, MediumDef,
    MediumRegistry, MediumSourceKind, RepAssignment, SchemaDef, TrainingSplitDef,
};
use crate::medium::source_observation::SourceMediumRegistry;
use gmeow_errors::json::{read_signed, read_unit_name, read_unsigned, read_variant};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

::purrdf_lex::json_record!(impl ToJson for SourceMediumRegistry {
    "source_path" => source_path, "source_digest" => source_digest,
    "registry" => registry, "producer_media" => producer_media,
});
::purrdf_lex::json_record!(impl ToJson for MediumRegistry {
    "dictionaries" => dictionaries, "dictionary_by_id" => dictionary_by_id,
    "corpora" => corpora, "split" => split, "schemas" => schemas,
    "schema_by_rep" => schema_by_rep, "media" => media, "assignment" => assignment,
});
::purrdf_lex::json_record!(impl ToJson for DictionaryDef {
    "iri" => iri, "id" => id, "version" => version, "strategy" => strategy,
    "target_length" => target_length, "corpus" => corpus,
});
::purrdf_lex::json_record!(impl ToJson for CorpusDef {
    "iri" => iri, "selectors" => selectors,
});
::purrdf_lex::json_record!(impl ToJson for TrainingSplitDef {
    "iri" => iri, "stride" => stride, "offset" => offset,
});
::purrdf_lex::json_record!(impl ToJson for SchemaDef {
    "iri" => iri, "rep" => rep,
});
::purrdf_lex::json_record!(impl ToJson for MediumDef {
    "iri" => iri, "codec" => codec, "zstd_level" => zstd_level,
    "source_kind" => source_kind, "dictionaries" => dictionaries,
    "reader_capabilities" => reader_capabilities,
});
::purrdf_lex::json_record!(impl ToJson for RepAssignment {
    "schema" => schema, "medium" => medium, "dictionary" => dictionary,
});

impl FromJson for SourceMediumRegistry {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SourceMediumRegistry")?;
        Ok(Self {
            source_path: fields.required("source_path")?,
            source_digest: fields.required("source_digest")?,
            registry: fields.required("registry")?,
            producer_media: fields.required("producer_media")?,
        })
    }
}
impl FromJson for MediumRegistry {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct MediumRegistry")?;
        Ok(Self {
            dictionaries: fields.required("dictionaries")?,
            dictionary_by_id: fields.required("dictionary_by_id")?,
            corpora: fields.required("corpora")?,
            split: fields.optional("split")?,
            schemas: fields.required("schemas")?,
            schema_by_rep: fields.required("schema_by_rep")?,
            media: fields.required("media")?,
            assignment: fields.required("assignment")?,
        })
    }
}
impl FromJson for DictionaryDef {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct DictionaryDef")?;
        Ok(Self {
            iri: fields.required("iri")?,
            id: fields.required("id")?,
            version: fields.required("version")?,
            strategy: fields.required("strategy")?,
            target_length: fields.required_with("target_length", |value| {
                read_unsigned::<usize>(value, "usize")
            })?,
            corpus: fields.required("corpus")?,
        })
    }
}
impl FromJson for CorpusDef {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CorpusDef")?;
        Ok(Self {
            iri: fields.required("iri")?,
            selectors: fields.required("selectors")?,
        })
    }
}
impl FromJson for TrainingSplitDef {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct TrainingSplitDef")?;
        Ok(Self {
            iri: fields.required("iri")?,
            stride: fields.required_with("stride", |value| read_unsigned::<u64>(value, "u64"))?,
            offset: fields.required_with("offset", |value| read_unsigned::<u64>(value, "u64"))?,
        })
    }
}
impl FromJson for SchemaDef {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SchemaDef")?;
        Ok(Self {
            iri: fields.required("iri")?,
            rep: fields.required("rep")?,
        })
    }
}
impl FromJson for MediumDef {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct MediumDef")?;
        Ok(Self {
            iri: fields.required("iri")?,
            codec: fields.required("codec")?,
            zstd_level: fields.required_with("zstd_level", |value| read_signed(value, "i32"))?,
            source_kind: fields.required("source_kind")?,
            dictionaries: fields.required("dictionaries")?,
            reader_capabilities: fields.required("reader_capabilities")?,
        })
    }
}
impl FromJson for RepAssignment {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct RepAssignment")?;
        Ok(Self {
            schema: fields.required("schema")?,
            medium: fields.required("medium")?,
            dictionary: fields.required("dictionary")?,
        })
    }
}

impl ToJson for DictionaryStrategy {
    fn to_json(&self) -> Value {
        Value::from(match self {
            Self::Trained => "Trained",
            Self::RawContent => "RawContent",
            Self::TermTable => "TermTable",
        })
    }
}
impl FromJson for DictionaryStrategy {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match read_unit_name(value, "enum DictionaryStrategy")? {
            "Trained" => Ok(Self::Trained),
            "RawContent" => Ok(Self::RawContent),
            "TermTable" => Ok(Self::TermTable),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Trained", "RawContent", "TermTable"],
            )),
        }
    }
}
impl ToJson for MediumSourceKind {
    fn to_json(&self) -> Value {
        Value::from(match self {
            Self::PerRep => "PerRep",
            Self::HeaderDict => "HeaderDict",
            Self::WholeArtifact => "WholeArtifact",
        })
    }
}
impl FromJson for MediumSourceKind {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match read_unit_name(value, "enum MediumSourceKind")? {
            "PerRep" => Ok(Self::PerRep),
            "HeaderDict" => Ok(Self::HeaderDict),
            "WholeArtifact" => Ok(Self::WholeArtifact),
            name => Err(DecodeError::unknown_variant(
                name,
                &["PerRep", "HeaderDict", "WholeArtifact"],
            )),
        }
    }
}
impl ToJson for DictSelection {
    fn to_json(&self) -> Value {
        match self {
            Self::Named(iri) => Object::new().with("Named", iri.to_json()).into(),
            Self::Baseline => Value::from("Baseline"),
        }
    }
}
impl FromJson for DictSelection {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        if value.as_object().is_some() {
            let (name, _) = read_variant(value, "enum DictSelection", "enum DictSelection")?;
            if name == "Named" {
                return Record::new(value, "Named variant")?
                    .required("Named")
                    .map(Self::Named);
            }
        }
        match read_unit_name(value, "enum DictSelection")? {
            "Baseline" => Ok(Self::Baseline),
            name => Err(DecodeError::unknown_variant(name, &["Named", "Baseline"])),
        }
    }
}
impl ToJson for CorpusSelector {
    fn to_json(&self) -> Value {
        let (name, text) = match self {
            Self::BlobRep(value) => ("BlobRep", value),
            Self::Graph(value) => ("Graph", value),
            Self::PathPrefix(value) => ("PathPrefix", value),
            Self::StageProduct(value) => ("StageProduct", value),
        };
        Object::new().with(name, text.to_json()).into()
    }
}
impl FromJson for CorpusSelector {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (name, _) = read_variant(value, "enum CorpusSelector", "enum CorpusSelector")?;
        let mut fields = Record::new(value, "enum CorpusSelector")?;
        match name {
            "BlobRep" => fields.required("BlobRep").map(Self::BlobRep),
            "Graph" => fields.required("Graph").map(Self::Graph),
            "PathPrefix" => fields.required("PathPrefix").map(Self::PathPrefix),
            "StageProduct" => fields.required("StageProduct").map(Self::StageProduct),
            name => Err(DecodeError::unknown_variant(
                name,
                &["BlobRep", "Graph", "PathPrefix", "StageProduct"],
            )),
        }
    }
}

#[cfg(test)]
mod tests;
