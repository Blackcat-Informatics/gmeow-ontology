// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Collection lowering retains each actual constructor before canonical deduplication.

use super::*;

/// Capture a constructor already read and validated by a native skolemizer.
pub(crate) fn collection_origins(
    store: &RdfDataset,
    owner: SourceNode,
    predicate: &str,
    head: &Node,
    field: CollectionField,
) -> Vec<AxiomSource> {
    let predicate = store
        .term_id_by_iri(predicate)
        .expect("a successfully read constructor predicate belongs to the source dictionary");
    let head = term_as_subject(head)
        .and_then(|head| subject_id(store, &head))
        .expect("a successfully read collection head is an interned resource");
    collection_sources(store, owner, predicate, head)
        .map(|source| AxiomSource::Collection { source, field })
        .collect()
}

/// Lower the constructor and cells with their exact physical owner evidence.
/// A malformed list rejects its whole constructor without emitting partial cells.
#[allow(clippy::too_many_arguments)]
pub(super) fn lift_list_statement(
    store: &RdfDataset,
    source_quad: purrdf::QuadIds,
    owner: &Subject,
    predicate: &str,
    ctor: &crate::lists::ListConstructor,
    head: &Node,
    skolems: &BTreeMap<String, String>,
    axioms: &mut Vec<AxiomEmission>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let owner_label = subject_str(owner);
    let list =
        match crate::lists::lower(store, ctor, head, &mut crate::lists::resolve_known(skolems)) {
            Ok(list) => list,
            Err(defect) => {
                let enumeration = ctor.local == restriction::ONE_OF_LOCAL;
                let code = match (&defect, enumeration) {
                    (crate::lists::ListDefect::Malformed(_), true) => "MALFORMED_ENUMERATION",
                    (crate::lists::ListDefect::Malformed(_), false) => "MALFORMED_LIST",
                    (crate::lists::ListDefect::UnresolvedMember(_), true) => {
                        "UNSUPPORTED_NESTED_ENUMERATION"
                    }
                    (crate::lists::ListDefect::UnresolvedMember(_), false) => {
                        "UNSUPPORTED_NESTED_CLASS_EXPRESSION"
                    }
                };
                let message = format!(
                    "{owner_label:?} <{predicate}> has an unusable RDF list ({}); not lifted",
                    defect.reason()
                );
                diagnostics.push(if subject_is_blank(owner) {
                    Diagnostic::warning(code, message, Some(owner_label))
                } else {
                    Diagnostic::error(code, message, Some(owner_label))
                });
                return;
            }
        };
    let sources: Vec<_> = collection_sources(
        store,
        SourceNode {
            term: source_quad.s,
            graph: source_quad.g,
        },
        source_quad.p,
        source_quad.o,
    )
    .collect();
    if sources.is_empty() {
        diagnostics.push(Diagnostic::error(
            "MISSING_COLLECTION_SOURCE",
            "lowered collection has no actual physical constructor source",
            Some(owner_label),
        ));
        return;
    }
    match LogicAxiom::new(
        owner_label.clone(),
        predicate,
        crate::ir::AtomicTerm::Iri(list.head),
        false,
        ContextualScope::default(),
    ) {
        Ok(axiom) => axioms.push(AxiomEmission {
            axiom,
            sources: sources
                .iter()
                .map(|&source| AxiomSource::Collection {
                    source,
                    field: CollectionField::Constructor,
                })
                .collect(),
        }),
        Err(error) => {
            diagnostics.push(Diagnostic::warning(
                "MALFORMED_AXIOM",
                error.message().to_owned(),
                Some(owner_label),
            ));
            return;
        }
    }
    for (subject, predicate, object) in list.cells {
        match LogicAxiom::new(
            subject.clone(),
            predicate,
            object,
            false,
            ContextualScope::default(),
        ) {
            Ok(axiom) => axioms.push(AxiomEmission {
                axiom,
                sources: sources
                    .iter()
                    .map(|&source| AxiomSource::Collection {
                        source,
                        field: CollectionField::Cell,
                    })
                    .collect(),
            }),
            Err(error) => diagnostics.push(Diagnostic::error(
                "MALFORMED_LIST",
                error.message().to_owned(),
                Some(subject),
            )),
        }
    }
}
