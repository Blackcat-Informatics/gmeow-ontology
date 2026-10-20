// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Admission of canonical callable signatures. Identity and position are authored,
//! never inferred from predicate sorting or minted by a projection dialect.

use super::*;
use crate::graphutil::{Node, node_of};
use crate::ingest::DslTerm;
use purrdf::dataset_view::{DatasetView, GraphMatch};
use purrdf::{TermRef, TermValue};

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const CALLABLE: &str = "https://blackcatinformatics.ca/logic/Callable";
const SIGNATURE: &str = "https://blackcatinformatics.ca/logic/CallableSignature";
const PARAMETER: &str = "https://blackcatinformatics.ca/logic/CallParameter";
const RESULT: &str = "https://blackcatinformatics.ca/logic/CallResult";
const METADATA: &[&str] = &[
    RDFS_LABEL,
    SKOS_DEFINITION,
    "http://www.w3.org/2000/01/rdf-schema#isDefinedBy",
    "http://www.w3.org/2000/01/rdf-schema#seeAlso",
    "http://purl.org/dc/terms/description",
    "http://www.w3.org/2004/02/skos/core#example",
    "https://blackcatinformatics.ca/gmeow/avoidWhen",
    "https://blackcatinformatics.ca/gmeow/howToUse",
    "https://blackcatinformatics.ca/gmeow/useWhen",
    "https://blackcatinformatics.ca/gmeow/graphBoxRole",
];

pub(super) struct Signatures {
    pub functions: Vec<FnFunction>,
    pub params: Vec<FnParam>,
    pub subjects: BTreeSet<String>,
    pub annotations: Vec<RdfQuad>,
}

fn refusal(subject: &str, detail: impl std::fmt::Display) -> Diag {
    Diag::of_kind(crate::error::Fno {
        detail: format!("canonical callable {subject}: {detail}"),
    })
}

fn check_fields(
    view: &DslView<'_>,
    subject: &str,
    kind: &str,
    fields: &[&str],
) -> gmeow_errors::Result<()> {
    let ds = view.dataset();
    let id = ds
        .term_id_by_iri(subject)
        .ok_or_else(|| refusal(subject, "missing declaration"))?;
    for quad in ds.quads_for_pattern(Some(id), None, None, GraphMatch::Default) {
        let TermRef::Iri(predicate) = ds.resolve(quad.p) else {
            return Err(refusal(subject, "a field predicate is not an IRI"));
        };
        if predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" {
            if ds.resolve(quad.o) != TermRef::Iri(kind)
                && !(kind == SIGNATURE && ds.resolve(quad.o) == TermRef::Iri(CALLABLE))
            {
                return Err(refusal(
                    subject,
                    "an additional source type has no FnO lowering",
                ));
            }
        } else if METADATA.contains(&predicate) {
            let object = ds.resolve(quad.o);
            let valid = if matches!(
                predicate,
                "http://www.w3.org/2000/01/rdf-schema#isDefinedBy"
                    | "http://www.w3.org/2000/01/rdf-schema#seeAlso"
                    | "https://blackcatinformatics.ca/gmeow/graphBoxRole"
            ) {
                matches!(object, TermRef::Iri(_))
            } else {
                matches!(object, TermRef::Literal { lexical, .. } if !lexical.is_empty())
            };
            if !valid {
                return Err(refusal(
                    subject,
                    format!("metadata {predicate} has an invalid value kind"),
                ));
            }
        } else if !fields
            .iter()
            .any(|field| predicate.strip_prefix(LOGIC) == Some(*field))
        {
            return Err(refusal(
                subject,
                format!("field {predicate} has no selected FnO lowering"),
            ));
        }
    }
    Ok(())
}

fn scalar(view: &DslView<'_>, subject: &str, local: &str) -> gmeow_errors::Result<DslTerm> {
    let values = view.objects_of(subject, &format!("{LOGIC}{local}"));
    match values.as_slice() {
        [value] => Ok(value.clone()),
        _ => Err(refusal(
            subject,
            format!("{local} requires exactly one value"),
        )),
    }
}

fn iri(view: &DslView<'_>, subject: &str, local: &str) -> gmeow_errors::Result<String> {
    match scalar(view, subject, local)? {
        DslTerm::Iri(value) => Ok(value),
        _ => Err(refusal(subject, format!("{local} requires an IRI"))),
    }
}

fn ordered_slots(
    view: &DslView<'_>,
    subject: &str,
    local: &str,
) -> gmeow_errors::Result<Vec<String>> {
    let head = scalar(view, subject, local)?;
    let ds = view.dataset();
    let id = ds
        .term_id_by_value(&head)
        .ok_or_else(|| refusal(subject, "missing list head"))?;
    crate::lists::walk(ds, &node_of(ds, id))
        .map_err(|error| refusal(subject, format!("{local}: {}", error.reason())))?
        .into_iter()
        .map(|(_, member)| match member {
            Node::Iri(iri) => Ok(iri),
            _ => Err(refusal(
                subject,
                format!("{local} requires named parameter identities"),
            )),
        })
        .collect()
}

fn required(view: &DslView<'_>, subject: &str) -> gmeow_errors::Result<bool> {
    match scalar(view, subject, "parameterRequired")? {
        DslTerm::Literal {
            lexical_form,
            datatype,
            language: None,
            direction: None,
        } if datatype == "http://www.w3.org/2001/XMLSchema#boolean" => {
            match lexical_form.as_str() {
                "true" | "1" => Ok(true),
                "false" | "0" => Ok(false),
                _ => Err(refusal(
                    subject,
                    "parameterRequired has an invalid boolean lexical form",
                )),
            }
        }
        _ => Err(refusal(
            subject,
            "parameterRequired requires an xsd:boolean",
        )),
    }
}

fn literal(view: &DslView<'_>, subject: &str, predicate: &str) -> gmeow_errors::Result<String> {
    view.object_literal(subject, predicate)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| refusal(subject, format!("missing nonempty literal {predicate}")))
}

impl Signatures {
    pub fn admit(view: &DslView<'_>) -> gmeow_errors::Result<Self> {
        let mut out = Self {
            functions: Vec::new(),
            params: Vec::new(),
            subjects: BTreeSet::new(),
            annotations: Vec::new(),
        };
        let mut results = BTreeMap::new();
        for subject in view.subjects_of_type(PARAMETER) {
            check_fields(
                view,
                &subject,
                PARAMETER,
                &[
                    "parameterPredicate",
                    "parameterValueType",
                    "parameterRequired",
                ],
            )?;
            out.subjects.insert(subject.clone());
            out.params.push(FnParam {
                predicate: Some(iri(view, &subject, "parameterPredicate")?),
                r#type: iri(view, &subject, "parameterValueType")?,
                required: required(view, &subject)?,
                label: Some(literal(view, &subject, RDFS_LABEL)?),
                description: Some(literal(view, &subject, SKOS_DEFINITION)?),
                iri: subject,
            });
        }
        for subject in view.subjects_of_type(RESULT) {
            check_fields(
                view,
                &subject,
                RESULT,
                &["parameterPredicate", "parameterValueType"],
            )?;
            if !out.subjects.insert(subject.clone()) {
                return Err(refusal(&subject, "a slot cannot be both input and output"));
            }
            results.insert(
                subject.clone(),
                FnOutput {
                    predicate: Some(iri(view, &subject, "parameterPredicate")?),
                    r#type: iri(view, &subject, "parameterValueType")?,
                    label: Some(literal(view, &subject, RDFS_LABEL)?),
                    description: Some(literal(view, &subject, SKOS_DEFINITION)?),
                    iri: subject,
                },
            );
        }
        let parameters: BTreeSet<_> = out.params.iter().map(|p| p.iri.clone()).collect();
        let mut used_parameters = BTreeSet::new();
        let mut used_results = BTreeSet::new();
        for subject in view.subjects_of_type(SIGNATURE) {
            check_fields(
                view,
                &subject,
                SIGNATURE,
                &["inputParameters", "outputParameters"],
            )?;
            if !out.subjects.insert(subject.clone()) {
                return Err(refusal(
                    &subject,
                    "a callable cannot also be an input or output slot",
                ));
            }
            let expects = ordered_slots(view, &subject, "inputParameters")?;
            for parameter in &expects {
                if !parameters.contains(parameter) {
                    return Err(refusal(
                        &subject,
                        format!("input {parameter} is not a declared CallParameter"),
                    ));
                }
                used_parameters.insert(parameter.clone());
            }
            let returns = ordered_slots(view, &subject, "outputParameters")?;
            let [result] = returns.as_slice() else {
                return Err(refusal(
                    &subject,
                    "the FnO profile requires exactly one output; multiple outputs are unsupported",
                ));
            };
            let output = results.get(result).ok_or_else(|| {
                refusal(
                    &subject,
                    format!("output {result} is not a declared CallResult"),
                )
            })?;
            used_results.insert(result.clone());
            out.functions.push(FnFunction {
                iri: subject.clone(),
                label: literal(view, &subject, RDFS_LABEL)?,
                description: Some(literal(view, &subject, SKOS_DEFINITION)?),
                kind_types: vec![SIGNATURE.to_owned(), CALLABLE.to_owned()],
                see_also: None,
                expects,
                output: output.clone(),
            });
        }
        if used_parameters != parameters || used_results != results.keys().cloned().collect() {
            return Err(refusal(
                "catalog",
                "orphan input/output declarations would be lost by the FnO projection",
            ));
        }
        out.annotations = annotations(view, &out.subjects)?;
        out.functions.sort_by(|a, b| a.iri.cmp(&b.iri));
        out.params.sort_by(|a, b| a.iri.cmp(&b.iri));
        Ok(out)
    }

    /// Source annotations carry their complete RDF literal identities. Replace the serializer
    /// convenience strings with these values before the common annotation/retag pass.
    pub fn restore_annotations(&self, quads: &mut Vec<RdfQuad>) {
        quads.retain(|quad| {
            !matches!(&quad.subject, RdfTerm::Iri(iri)
            if self.subjects.contains(iri) && METADATA.contains(&quad.predicate.as_str()))
        });
        quads.extend(self.annotations.iter().cloned());
    }
}

fn annotations(
    view: &DslView<'_>,
    subjects: &BTreeSet<String>,
) -> gmeow_errors::Result<Vec<RdfQuad>> {
    let mut annotations = Vec::new();
    for subject in subjects {
        let owners = view.objects_of(subject, METADATA[2]);
        if owners.is_empty() || owners.iter().any(|owner| owner.as_iri().is_none()) {
            return Err(refusal(
                subject,
                "rdfs:isDefinedBy requires a source owner IRI",
            ));
        }
        for predicate in METADATA {
            for value in view.objects_of(subject, predicate) {
                let object = match value {
                    TermValue::Iri(value) => RdfTerm::iri(value),
                    TermValue::Literal {
                        lexical_form,
                        datatype,
                        language,
                        direction,
                    } => RdfTerm::literal(RdfLiteral {
                        lexical_form,
                        datatype: Some(datatype),
                        language,
                        direction,
                    }),
                    _ => {
                        return Err(refusal(
                            subject,
                            format!("unsupported metadata term for {predicate}"),
                        ));
                    }
                };
                annotations.push(RdfQuad::new(
                    RdfTerm::iri(subject.clone()),
                    *predicate,
                    object,
                ));
            }
        }
    }
    Ok(annotations)
}

/// Operation declarations do not invent an ABI. The generated descriptor carries
/// exactly the declared identity and annotations, with no fabricated expects/returns.
pub(super) struct Declarations {
    pub subjects: BTreeSet<String>,
    pub quads: Vec<RdfQuad>,
}

impl Declarations {
    pub fn admit(view: &DslView<'_>) -> gmeow_errors::Result<Self> {
        let interfaces: BTreeSet<_> = view.subjects_of_type(SIGNATURE).into_iter().collect();
        let mut subjects = BTreeSet::new();
        for subject in view.subjects_of_type(CALLABLE) {
            if interfaces.contains(&subject) {
                continue;
            }
            check_fields(view, &subject, CALLABLE, &[])?;
            literal(view, &subject, RDFS_LABEL)?;
            literal(view, &subject, SKOS_DEFINITION)?;
            subjects.insert(subject);
        }
        let mut quads = annotations(view, &subjects)?;
        for subject in &subjects {
            quads.push(RdfQuad::new(
                RdfTerm::iri(subject.clone()),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                RdfTerm::iri("https://w3id.org/function/ontology#Function"),
            ));
        }
        Ok(Self { subjects, quads })
    }
}
