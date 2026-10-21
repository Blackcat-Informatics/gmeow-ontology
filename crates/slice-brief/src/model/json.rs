// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Packet-owned native JSON contracts. Record members retain declaration order;
//! records ignore unknown fields while checking known fields for duplicates.

use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within as _},
};

use super::{
    Annotation, AuthoringPacket, ClosureEntry, CoveredTerm, GroundingAttribute, GroundingCell,
    GroundingMargins, ObjTerm, Triple,
};

::purrdf_lex::json_string_enum!(GroundingAttribute {
    En => "En",
    Fr => "Fr",
    Zh => "Zh",
    ExternalMapped => "ExternalMapped",
    Exemplar => "Exemplar",
});

impl FromJson for GroundingMargins {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct GroundingMargins")?;
        Ok(Self {
            fr_present: fields.required("fr_present")?,
            fr_absent: fields.required("fr_absent")?,
            zh_present: fields.required("zh_present")?,
            zh_absent: fields.required("zh_absent")?,
            external_mapped: fields.required("external_mapped")?,
            external_absent: fields.required("external_absent")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for GroundingMargins {
    "fr_present" => fr_present,
    "fr_absent" => fr_absent,
    "zh_present" => zh_present,
    "zh_absent" => zh_absent,
    "external_mapped" => external_mapped,
    "external_absent" => external_absent,
});

impl FromJson for ObjTerm {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut variant = Record::new(value, "externally tagged ObjTerm object")?;
        // Check known tag multiplicities before selecting the single variant.
        for name in ["Iri", "Blank", "Literal"] {
            variant.raw(name)?;
        }
        let object = value.as_object().expect("Record admitted an object");
        if object.len() != 1 {
            return Err(DecodeError::invalid_value(
                value,
                "exactly one ObjTerm variant member",
            ));
        }
        let (name, payload) = object.iter().next().expect("one variant member");
        match name.as_str() {
            "Iri" => String::from_json(payload)
                .map(Self::Iri)
                .map_err(|error| error.within("Iri")),
            "Blank" => String::from_json(payload)
                .map(Self::Blank)
                .map_err(|error| error.within("Blank")),
            "Literal" => literal_from_json(payload).map_err(|error| error.within("Literal")),
            other => Err(DecodeError::unknown_variant(
                other,
                &["Iri", "Blank", "Literal"],
            )),
        }
    }
}

/// Decode the literal variant with native duplicate checks and nested field locations.
fn literal_from_json(value: &Value) -> Result<ObjTerm, DecodeError> {
    let mut fields = Record::new(value, "struct ObjTerm::Literal")?;
    Ok(ObjTerm::Literal {
        lexical: fields.required("lexical")?,
        datatype: fields.required("datatype")?,
        language: fields.optional("language")?,
    })
}

impl ToJson for ObjTerm {
    fn to_json(&self) -> Value {
        Value::from(match self {
            Self::Iri(iri) => Object::new().with("Iri", iri.to_json()),
            Self::Blank(label) => Object::new().with("Blank", label.to_json()),
            Self::Literal {
                lexical,
                datatype,
                language,
            } => Object::new().with(
                "Literal",
                Value::from(
                    Object::new()
                        .with("lexical", lexical.to_json())
                        .with("datatype", datatype.to_json())
                        .with("language", language.to_json()),
                ),
            ),
        })
    }
}

impl FromJson for Triple {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Triple")?;
        Ok(Self {
            predicate: fields.required("predicate")?,
            object: fields.required("object")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for Triple {
    "predicate" => predicate,
    "object" => object,
});

impl FromJson for Annotation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Annotation")?;
        Ok(Self {
            predicate: fields.required("predicate")?,
            language: fields.optional("language")?,
            value: fields.required("value")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for Annotation {
    "predicate" => predicate,
    "language" => language,
    "value" => value,
});

impl FromJson for ClosureEntry {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ClosureEntry")?;
        Ok(Self {
            iri: fields.required("iri")?,
            label: fields.optional("label")?,
            definition: fields.optional("definition")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for ClosureEntry {
    "iri" => iri,
    "label" => label,
    "definition" => definition,
});

impl FromJson for CoveredTerm {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CoveredTerm")?;
        Ok(Self {
            iri: fields.required("iri")?,
            label: fields.optional("label")?,
            definition: fields.optional("definition")?,
            coat: fields.required("coat")?,
            axioms: fields.required("axioms")?,
            neighbors: fields.required("neighbors")?,
            closure: fields.required("closure")?,
            content_digest: fields.required("content_digest")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for CoveredTerm {
    "iri" => iri,
    "label" => label,
    "definition" => definition,
    "coat" => coat,
    "axioms" => axioms,
    "neighbors" => neighbors,
    "closure" => closure,
    "content_digest" => content_digest,
});

impl FromJson for GroundingCell {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct GroundingCell")?;
        Ok(Self {
            cell_iri: fields.required("cell_iri")?,
            term: fields.required("term")?,
            attribute: fields.required("attribute")?,
            present: fields.required("present")?,
            predicate: fields.optional("predicate")?,
            value: fields.optional("value")?,
            external_entity: fields.optional("external_entity")?,
            external_label: fields.optional("external_label")?,
            align_predicate: fields.optional("align_predicate")?,
            confidence: fields.optional("confidence")?,
            conflict: fields.required("conflict")?,
            conflict_with: fields.optional("conflict_with")?,
        })
    }
}

impl ToJson for GroundingCell {
    fn to_json(&self) -> Value {
        assert!(
            self.confidence.is_none_or(f64::is_finite),
            "authoring packet failed to serialize to JSON: grounding cell {} confidence must be finite",
            self.cell_iri
        );
        Value::from(
            Object::new()
                .with("cell_iri", self.cell_iri.to_json())
                .with("term", self.term.to_json())
                .with("attribute", self.attribute.to_json())
                .with("present", self.present)
                .with("predicate", self.predicate.to_json())
                .with("value", self.value.to_json())
                .with("external_entity", self.external_entity.to_json())
                .with("external_label", self.external_label.to_json())
                .with("align_predicate", self.align_predicate.to_json())
                .with("confidence", self.confidence.to_json())
                .with("conflict", self.conflict)
                .with("conflict_with", self.conflict_with.to_json()),
        )
    }
}

impl FromJson for AuthoringPacket {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct AuthoringPacket")?;
        Ok(Self {
            packet_iri: fields.required("packet_iri")?,
            source_slice: fields.required("source_slice")?,
            axis: fields.optional("axis")?,
            batch: fields.required("batch")?,
            digest: fields.required("digest")?,
            term_count: fields.required("term_count")?,
            exemplar_shortfall: fields.required("exemplar_shortfall")?,
            margins: fields.required("margins")?,
            terms: fields.required("terms")?,
            exemplars: fields.required("exemplars")?,
            grounding: fields.required("grounding")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for AuthoringPacket {
    "packet_iri" => packet_iri,
    "source_slice" => source_slice,
    "axis" => axis,
    "batch" => batch,
    "digest" => digest,
    "term_count" => term_count,
    "exemplar_shortfall" => exemplar_shortfall,
    "margins" => margins,
    "terms" => terms,
    "exemplars" => exemplars,
    "grounding" => grounding,
});
