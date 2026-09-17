// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers for the owning production module.

use super::*;

/// The declarative SHACL node-kind a result column's [`Kind`] maps to.
#[cfg(test)]
pub(super) fn column_node_kind(kind: Kind) -> ShaclNodeKind {
    match kind {
        Kind::Iri => ShaclNodeKind::Iri,
        Kind::Literal => ShaclNodeKind::Literal,
        Kind::BlankNode => ShaclNodeKind::BlankNode,
    }
}

/// Map ONE result column onto the equivalent DECLARATIVE property shape the canonical
/// [`ValidationShapeIr`] model would use — WITHOUT changing the emitted `sh:sparql` TTL.
///
/// * `kind` → a `sh:nodeKind` component (`NodeKindShacl`),
/// * a `Literal` column with a pinned `datatype` → an additional `sh:datatype` component,
/// * `required` → a `sh:minCount 1` obligation (with an OWL-restriction provenance so the
///   constraint model accepts the cardinality; a non-required column carries no cardinality).
///
/// This captures the SAME contract semantics as the procedural `sh:sparql` column
/// constraint; it is the declarative peer of the procedural projection, never emitted.
#[cfg(test)]
pub(super) fn column_contract_property(col: &Column) -> gmeow_errors::Result<PropertyConstraintIr> {
    let mut components = vec![ConstraintComponent::NodeKindShacl(column_node_kind(
        col.kind,
    ))];
    if col.kind == Kind::Literal
        && let Some(dt) = &col.datatype
    {
        components.push(ConstraintComponent::Datatype(dt.clone()));
    }
    let (min, prov) = if col.required {
        (Some(1), Some(ConstraintProvenance::OwlRestriction))
    } else {
        (None, None)
    };
    PropertyConstraintIr::new(
        format!("{GMEOW_NS}resultColumn/{}", col.var),
        min,
        None,
        prov,
        components,
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Transform {
            message: e.to_string(),
        })
    })
}

/// Wrap [`column_contract_property`] in a well-formed [`ValidationShapeIr`] value-keyed on
/// the cell variable — the declarative shape whose content is identical to the procedural
/// `sh:sparql` column constraint. Proves subsumption of the column contract, not a byte form.
#[cfg(test)]
pub(super) fn column_contract(
    shape_iri: &str,
    col: &Column,
) -> gmeow_errors::Result<ValidationShapeIr> {
    let property = column_contract_property(col)?;
    ValidationShapeIr::new(
        format!("{shape_iri}/resultColumn/{}", col.var),
        ShapeTarget::ValueKeyed {
            predicate: format!("{GMEOW_NS}cellVar"),
            value: col.var.clone(),
        },
        vec![property],
        None,
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Transform {
            message: e.to_string(),
        })
    })
}
