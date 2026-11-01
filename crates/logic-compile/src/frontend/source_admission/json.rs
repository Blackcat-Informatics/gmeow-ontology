// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Named observational records never restore the compiler's live admission token.

use super::{
    AdmittedSemanticUnit, DocumentSelection, FofSemanticProfile, PropertyCharacteristic,
    RootSelection, SemanticUnitAdmission, SemanticUnitDisposition, SemanticUnitKind,
    SourceAdmission, SourceAdmissionStatus, SourceAssertion, SourceClassDefinition,
    SourceContractDeclaration, SourceGraphSelection, SourceSelection,
};
use crate::ir::Formula;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record, ToJson, Within},
};
use std::collections::BTreeSet;

::purrdf_lex::json_record!(impl ToJson for FofSemanticProfile {
    "id" => id,
    "graph" => graph,
    "world" => world,
    "equality" => equality,
    "blank_nodes" => blank_nodes,
    "free_variables" => free_variables,
    "context" => context,
    "contracts" => contracts,
});

impl FromJson for FofSemanticProfile {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct FofSemanticProfile")?;
        Ok(Self {
            id: fields.required("id")?,
            graph: fields.required("graph")?,
            world: fields.required("world")?,
            equality: fields.required("equality")?,
            blank_nodes: fields.required("blank_nodes")?,
            free_variables: fields.required("free_variables")?,
            context: fields.required("context")?,
            contracts: fields.required("contracts")?,
        })
    }
}

impl ToJson for SourceAdmissionStatus {
    fn to_json(&self) -> Value {
        match self {
            Self::Complete => "Complete",
            Self::Blocked => "Blocked",
            Self::Malformed => "Malformed",
            Self::Empty => "Empty",
        }
        .to_json()
    }
}

impl FromJson for SourceAdmissionStatus {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "SourceAdmissionStatus")? {
            "Complete" => Ok(Self::Complete),
            "Blocked" => Ok(Self::Blocked),
            "Malformed" => Ok(Self::Malformed),
            "Empty" => Ok(Self::Empty),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Complete", "Blocked", "Malformed", "Empty"],
            )),
        }
    }
}

impl FromJson for SourceGraphSelection {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "SourceGraphSelection")? {
            "DefaultGraph" => Ok(Self::DefaultGraph),
            name => Err(DecodeError::unknown_variant(name, &["DefaultGraph"])),
        }
    }
}

// Both document and root selection carry the same exact-set payload. Unit
// variants stay with their distinct semantic owners below.
fn exact_selection(value: &Value, expected: &str) -> Result<Option<BTreeSet<String>>, DecodeError> {
    if value
        .as_object()
        .is_some_and(|object| object.get("Exact").is_some())
    {
        let (_, payload) =
            gmeow_errors::json::read_variant(value, expected, "one selection variant")?;
        BTreeSet::<String>::from_json(payload)
            .map(Some)
            .map_err(|error| error.within("Exact"))
    } else {
        Ok(None)
    }
}

impl FromJson for DocumentSelection {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        if let Some(documents) = exact_selection(value, "DocumentSelection")? {
            return Ok(Self::Exact(documents));
        }
        match gmeow_errors::json::read_unit_name(value, "DocumentSelection")? {
            "AnonymousSource" => Ok(Self::AnonymousSource),
            name => Err(DecodeError::unknown_variant(
                name,
                &["AnonymousSource", "Exact"],
            )),
        }
    }
}

impl FromJson for RootSelection {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        if let Some(roots) = exact_selection(value, "RootSelection")? {
            return Ok(Self::Exact(roots));
        }
        match gmeow_errors::json::read_unit_name(value, "RootSelection")? {
            "None" => Ok(Self::None),
            "All" => Ok(Self::All),
            name => Err(DecodeError::unknown_variant(
                name,
                &["None", "All", "Exact"],
            )),
        }
    }
}

impl FromJson for SourceSelection {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SourceSelection")?;
        Ok(Self {
            graph: fields.required("graph")?,
            documents: fields.required("documents")?,
            formulas: fields.required("formulas")?,
            rules: fields.required("rules")?,
            owners: fields.required("owners")?,
        })
    }
}

impl FromJson for SemanticUnitKind {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "SemanticUnitKind")? {
            "Assertion" => Ok(Self::Assertion),
            "Formula" => Ok(Self::Formula),
            "Rule" => Ok(Self::Rule),
            "PropertyCharacteristic" => Ok(Self::PropertyCharacteristic),
            "ClassDefinition" => Ok(Self::ClassDefinition),
            "Import" => Ok(Self::Import),
            "StructuralSyntax" => Ok(Self::StructuralSyntax),
            "Quotation" => Ok(Self::Quotation),
            "Diagnostic" => Ok(Self::Diagnostic),
            "Owner" => Ok(Self::Owner),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "Assertion",
                    "Formula",
                    "Rule",
                    "PropertyCharacteristic",
                    "ClassDefinition",
                    "Import",
                    "StructuralSyntax",
                    "Quotation",
                    "Diagnostic",
                    "Owner",
                ],
            )),
        }
    }
}

impl FromJson for SourceAssertion {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_statement_fields(value, "struct SourceAssertion").map(
            |(subject, predicate, object)| SourceAssertion {
                subject,
                predicate,
                object,
            },
        )
    }
}

impl FromJson for PropertyCharacteristic {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct PropertyCharacteristic")?;
        Ok(Self {
            property: fields.required("property")?,
            characteristic: fields.required("characteristic")?,
        })
    }
}

impl FromJson for SourceClassDefinition {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SourceClassDefinition")?;
        Ok(Self {
            formula: fields.required_with("formula", Formula::from_observation_json)?,
            constants: fields.required("constants")?,
        })
    }
}

impl FromJson for SourceContractDeclaration {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SourceContractDeclaration")?;
        Ok(Self {
            configuration: fields.required("configuration")?,
            facts: fields.required("facts")?,
        })
    }
}

impl FromJson for AdmittedSemanticUnit {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) = gmeow_errors::json::read_variant(
            value,
            "AdmittedSemanticUnit",
            "one admitted unit variant",
        )?;
        let decode = || -> Result<Self, DecodeError> {
            match tag {
                "Assertion" => SourceAssertion::from_json(payload).map(Self::Assertion),
                "Formula" => {
                    let mut fields =
                        Record::new(payload, "struct variant AdmittedSemanticUnit::Formula")?;
                    Ok(Self::Formula {
                        index: fields.required_with("index", |value| {
                            gmeow_errors::json::read_unsigned::<usize>(
                                value,
                                "an unsigned source index",
                            )
                        })?,
                    })
                }
                "Rule" => {
                    let mut fields =
                        Record::new(payload, "struct variant AdmittedSemanticUnit::Rule")?;
                    Ok(Self::Rule {
                        index: fields.required_with("index", |value| {
                            gmeow_errors::json::read_unsigned::<usize>(
                                value,
                                "an unsigned source index",
                            )
                        })?,
                    })
                }
                "PropertyCharacteristic" => {
                    PropertyCharacteristic::from_json(payload).map(Self::PropertyCharacteristic)
                }
                "ClassDefinition" => {
                    SourceClassDefinition::from_json(payload).map(Self::ClassDefinition)
                }
                name => Err(DecodeError::unknown_variant(
                    name,
                    &[
                        "Assertion",
                        "Formula",
                        "Rule",
                        "PropertyCharacteristic",
                        "ClassDefinition",
                    ],
                )),
            }
        };
        decode().map_err(|error| error.within(tag))
    }
}

impl FromJson for SemanticUnitDisposition {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) = gmeow_errors::json::read_variant(
            value,
            "SemanticUnitDisposition",
            "one disposition variant",
        )?;
        let decode = || -> Result<Self, DecodeError> {
            match tag {
                "Admitted" => AdmittedSemanticUnit::from_json(payload).map(Self::Admitted),
                "ConfigurationDeclaration" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant SemanticUnitDisposition::ConfigurationDeclaration",
                    )?;
                    Ok(Self::ConfigurationDeclaration {
                        declaration: fields.required("declaration")?,
                        reason: fields.required("reason")?,
                    })
                }
                "StructuralSupport" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant SemanticUnitDisposition::StructuralSupport",
                    )?;
                    Ok(Self::StructuralSupport {
                        detail: fields.required("detail")?,
                    })
                }
                "OutsideSelection" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant SemanticUnitDisposition::OutsideSelection",
                    )?;
                    Ok(Self::OutsideSelection {
                        reason: fields.required("reason")?,
                    })
                }
                "QuotedUnasserted" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant SemanticUnitDisposition::QuotedUnasserted",
                    )?;
                    Ok(Self::QuotedUnasserted {
                        detail: fields.required("detail")?,
                    })
                }
                "Blocked" => {
                    let mut fields =
                        Record::new(payload, "struct variant SemanticUnitDisposition::Blocked")?;
                    Ok(Self::Blocked {
                        code: fields.required("code")?,
                        reason: fields.required("reason")?,
                    })
                }
                "Malformed" => {
                    let mut fields =
                        Record::new(payload, "struct variant SemanticUnitDisposition::Malformed")?;
                    Ok(Self::Malformed {
                        code: fields.required("code")?,
                        reason: fields.required("reason")?,
                    })
                }
                "Observed" => {
                    let mut fields =
                        Record::new(payload, "struct variant SemanticUnitDisposition::Observed")?;
                    Ok(Self::Observed {
                        detail: fields.required("detail")?,
                    })
                }
                name => Err(DecodeError::unknown_variant(
                    name,
                    &[
                        "Admitted",
                        "ConfigurationDeclaration",
                        "StructuralSupport",
                        "OutsideSelection",
                        "QuotedUnasserted",
                        "Blocked",
                        "Malformed",
                        "Observed",
                    ],
                )),
            }
        };
        decode().map_err(|error| error.within(tag))
    }
}

::purrdf_lex::json_record!(impl ToJson for SemanticUnitAdmission {
    "id" => id,
    "kind" => kind,
    "anchor" => anchor,
    "documents" => documents,
    "disposition" => disposition,
});

impl FromJson for SemanticUnitAdmission {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SemanticUnitAdmission")?;
        Ok(Self {
            id: fields.required("id")?,
            kind: fields.required("kind")?,
            anchor: fields.required("anchor")?,
            documents: fields.required("documents")?,
            disposition: fields.required("disposition")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for SourceAdmission {
    "profile" => profile,
    "source_digest" => source_digest,
    "program_digest" => program_digest,
    "selection" => selection,
    "selection_digest" => selection_digest,
    "status" => status,
    "units" => units,
    "declared_classes" => declared_classes,
    "declared_builtins" => declared_builtins,
});

impl FromJson for SourceAdmission {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SourceAdmission")?;
        Ok(Self {
            profile: fields.required("profile")?,
            source_digest: fields.required("source_digest")?,
            program_digest: fields.required("program_digest")?,
            selection: fields.required("selection")?,
            selection_digest: fields.required("selection_digest")?,
            status: fields.required("status")?,
            units: fields.required("units")?,
            declared_classes: fields.required("declared_classes")?,
            declared_builtins: fields.required("declared_builtins")?,
            // Even injected input for this skipped field is ignored, never read.
            checked_evidence: None,
        })
    }
}
