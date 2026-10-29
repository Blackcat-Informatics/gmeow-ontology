// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Strict template-only native-cell selection. Admission precedes any vocabulary
//! filtering; a selector cannot hide simply because its target is not written on
//! the authoring surface.

use super::get_leg::ProjectionCell;
use super::grounding_catalog::{
    GroundingCatalog, LiteralProjectionSupport, SELECTED_TARGET, error,
};
use crate::ingest::{DslTerm, DslView};

const GM: &str = "https://blackcatinformatics.ca/gmeow/";

/// One admitted template occurrence and its complete selected native support.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SelectedTemplate {
    pub profile: String,
    pub template_index: usize,
    pub subject_variable: String,
    pub object_variable: String,
    pub support: LiteralProjectionSupport,
}

impl SelectedTemplate {
    /// Retain the selected template occurrence and complete native support.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub(crate) fn write_cbor<W: std::io::Write + ?Sized>(
        &self,
        out: &mut W,
    ) -> std::io::Result<()> {
        use purrdf_lex::cbor::head;

        head::write_head(out, head::MAP, 5)?;
        head::write_text(out, "profile")?;
        head::write_text(out, &self.profile)?;
        head::write_text(out, "template_index")?;
        head::write_head(out, head::UNSIGNED, self.template_index as u64)?;
        head::write_text(out, "subject_variable")?;
        head::write_text(out, &self.subject_variable)?;
        head::write_text(out, "object_variable")?;
        head::write_text(out, &self.object_variable)?;
        head::write_text(out, "support")?;
        self.support.write_cbor(out)?;
        Ok(())
    }
}

fn one(view: &DslView, node: &DslTerm, local: &str) -> gmeow_errors::Result<DslTerm> {
    let values = view.objects_of_term(node, &format!("{GM}{local}"));
    match values.as_slice() {
        [value] => Ok(value.clone()),
        _ => Err(error(format!(
            "selected template {node:?} requires exactly one gmeow:{local}"
        ))),
    }
}

fn variable(view: &DslView, node: &DslTerm, local: &str) -> gmeow_errors::Result<String> {
    match one(view, node, local)? {
        DslTerm::Literal {
            lexical_form,
            datatype,
            language: None,
            direction: None,
        } if !lexical_form.is_empty()
            && (datatype.is_empty() || datatype == "http://www.w3.org/2001/XMLSchema#string") =>
        {
            Ok(lexical_form)
        }
        _ => Err(error(format!(
            "selected template {node:?} gmeow:{local} requires a nonempty untagged string variable"
        ))),
    }
}

fn source_property(
    view: &DslView,
    cell: &str,
    subject: &str,
    object: &str,
) -> gmeow_errors::Result<String> {
    let pattern = one(view, &DslTerm::Iri(cell.to_owned()), "hasMappingPattern")?;
    let head = one(view, &pattern, "atom")?;
    let mut witnesses = Vec::new();
    for node in view.rdf_list_strict(&head)? {
        if !view
            .objects_of_term(&node, &format!("{GM}optionalGroup"))
            .is_empty()
        {
            continue;
        }
        let mentions = |field: &str, wanted: &str| {
            view.objects_of_term(&node, &format!("{GM}{field}"))
                .iter()
                .any(|value| crate::ingest::literal_lexical(value) == Some(wanted))
        };
        if !mentions("subjectVar", subject) || !mentions("objectVar", object) {
            continue;
        }
        // A selected witness never inherits first-value or alias fallback from
        // the legacy get-leg reader. Validate all alternatives and every value.
        variable(view, &node, "subjectVar")?;
        variable(view, &node, "objectVar")?;
        for field in [
            "tSubj",
            "tObj",
            "tPred",
            "tPredFromCorrespondence",
            "predicateVar",
            "path",
            "pathAlts",
            "objectValue",
            "tObjValue",
            "objectLiteral",
        ] {
            if !view
                .objects_of_term(&node, &format!("{GM}{field}"))
                .is_empty()
            {
                return Err(error(format!(
                    "selected mandatory source atom {node:?} conflicts with gmeow:{field}"
                )));
            }
        }
        let optional = view.objects_of_term(&node, &format!("{GM}optional"));
        match optional.as_slice() {
            [] => {}
            [
                DslTerm::Literal {
                    lexical_form,
                    datatype,
                    language: None,
                    direction: None,
                },
            ] if datatype == "http://www.w3.org/2001/XMLSchema#boolean"
                && (lexical_form == "false" || lexical_form == "0") => {}
            _ => {
                return Err(error(format!(
                    "selected source atom {node:?} must be mandatory; optional requires one valid false xsd:boolean when present"
                )));
            }
        }
        let DslTerm::Iri(predicate) = one(view, &node, "predicate")? else {
            return Err(error(format!(
                "selected source atom {node:?} requires exactly one IRI predicate"
            )));
        };
        witnesses.push(predicate);
    }
    match witnesses.as_slice() {
        [predicate] => Ok(predicate.clone()),
        _ => Err(error(format!(
            "selected template in <{cell}> requires exactly one strict mandatory flat source atom for ?{subject} and ?{object}"
        ))),
    }
}

/// Resolve every selector against actual template-list membership and a direct
/// mandatory source atom. The resulting GetLegAtom keeps the same variables;
/// native runtime admission remains required for each matched source value.
pub(crate) fn resolve(
    view: &DslView,
    catalog: &GroundingCatalog,
    cells: &mut [ProjectionCell],
) -> gmeow_errors::Result<()> {
    let authored = view.quads_with_predicate(SELECTED_TARGET);
    let mut visited = Vec::new();
    for cell in cells {
        let nodes = view.objects_of(&cell.iri, &format!("{GM}hasBinding"));
        for (resolved, binding_node) in cell.bindings.iter_mut().zip(nodes) {
            let super::get_leg::ProjectionBinding {
                source: binding,
                selected_templates,
            } = resolved;
            let heads = view.objects_of_term(&binding_node, &format!("{GM}templateAtoms"));
            if heads.len() > 1 {
                return Err(error(format!("{}: multiple templateAtoms heads", cell.iri)));
            }
            let Some(head) = heads.first() else {
                continue;
            };
            // Strict membership is required when any selector is in this source;
            // selected syntax never inherits the legacy lenient list reader.
            let members = if authored.is_empty() {
                view.rdf_list(Some(head))
            } else {
                view.rdf_list_strict(head)?
            };
            if members.len() != binding.template_atoms.len() {
                return Err(error(format!(
                    "{}: template membership does not match parsed atoms",
                    cell.iri
                )));
            }
            for (index, (node, atom)) in members.iter().zip(&mut binding.template_atoms).enumerate()
            {
                if view.objects_of_term(node, SELECTED_TARGET).is_empty() {
                    continue;
                }
                if !matches!(node, DslTerm::Iri(_) | DslTerm::Blank { .. }) {
                    return Err(error(
                        "selected template member must be a named or blank resource",
                    ));
                }
                if visited.contains(node) {
                    return Err(error(format!(
                        "selected template {node:?} has multiple memberships"
                    )));
                }
                visited.push(node.clone());
                let DslTerm::Iri(alias) = one(view, node, "tPredFromCorrespondence")? else {
                    return Err(error(format!(
                        "selected template {node:?} requires a named native correspondence IRI"
                    )));
                };
                for field in [
                    "tPred",
                    "predicate",
                    "predicateVar",
                    "path",
                    "pathAlts",
                    "subjectVar",
                    "objectVar",
                    "tObjValue",
                    "objectValue",
                    "objectLiteral",
                    "optional",
                ] {
                    if !view
                        .objects_of_term(node, &format!("{GM}{field}"))
                        .is_empty()
                    {
                        return Err(error(format!(
                            "selected template {node:?} conflicts with gmeow:{field}"
                        )));
                    }
                }
                let subject = variable(view, node, "tSubj")?;
                let object = variable(view, node, "tObj")?;
                if binding.transform.is_some() {
                    return Err(error(format!(
                        "selected literal template {node:?} cannot declare a value transform"
                    )));
                }
                if cell
                    .pattern
                    .binds
                    .iter()
                    .chain(&cell.pattern.mints)
                    .any(|bind| bind.var == subject || bind.var == object)
                {
                    return Err(error(format!(
                        "selected template {node:?} cannot rebind or mint source variables"
                    )));
                }
                let source_property = source_property(view, &cell.iri, &subject, &object)?;
                let support = catalog.literal_support(&alias, &source_property)?;
                atom.predicate = Some(support.target_property.clone());
                selected_templates.push(SelectedTemplate {
                    profile: binding.profile.clone(),
                    template_index: index,
                    subject_variable: subject,
                    object_variable: object,
                    support,
                });
            }
        }
    }
    for (node, _) in authored {
        if !visited.contains(&node) {
            return Err(error(format!(
                "selected correspondence field on {node:?} is outside a unique admitted template membership"
            )));
        }
    }
    Ok(())
}
