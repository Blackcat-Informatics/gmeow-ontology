// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native statement behind a scoped claim, and whether its carrier is evidence only.
//!
//! The compiler lowers a scoped reifier or `rdf:Statement` node to one axiom per
//! scope and keeps the carrier node as the axiom's source. This module answers the
//! reverse question for a consumer that needs the exact RDF terms: which statement
//! of the prepared dataset is that axiom, and may it be read as a statement of the
//! theory. It reuses the compiler's own lowering to decide, so no second reading of
//! a claim exists, and it fails closed twice over:
//!
//! * a carrier resolves only when everything said about it is on the evidence
//!   allow-list of [`evidence_only`]; any other statement about the carrier, and
//!   any mention of it in another carried statement, may place, deny or qualify
//!   the claim, so the claim is not resolved;
//! * a carrier resolves only when exactly one statement it holds lowers to the
//!   axiom and no term of that statement is a typed term carrier.

use std::collections::BTreeMap;

use purrdf::{RdfDataset, TermId, TermRef};

use super::{
    AxiomSource, LogicAxiom, Node, PreparedLogicSource, RDF_OBJECT, RDF_PREDICATE, RDF_REIFIES,
    RDF_STATEMENT, RDF_SUBJECT, RDF_TYPE, SourceNode, Subject, emission::CompiledSource,
    formula_reader::read_atomic_carrier, logic_iri, native_axiom, native_quoted_axiom,
    term_as_subject, term_str,
};
use crate::graphutil::{default_graph_pattern, iri_of, node_of, source_graph_pattern, subject_of};
use purrdf::dataset_view::GraphMatch;

/// The statement a scoped claim carries, as term identities of the prepared dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimStatement {
    /// The carrier is evidence only, exactly one statement it holds lowers to the
    /// axiom, and the axiom states that statement's own RDF terms.
    Resolved {
        subject: TermId,
        predicate: TermId,
        object: TermId,
    },
    /// Something other than evidence is said about `carrier`. `predicate` is the
    /// first such predicate IRI in sorted order, `graph` the named graph that
    /// statement is in (none for the default graph), and `statements` is how many
    /// statements about the carrier are outside the evidence allow-list.
    Qualified {
        carrier: String,
        predicate: String,
        graph: Option<String>,
        statements: usize,
    },
    /// `carrier` occurs inside a triple term, as the predicate of a statement, or as
    /// the subject, predicate or object of a classic statement node: someone says
    /// something about the claim. `by` is the first node, in sorted order, whose
    /// statement mentions it, `graph` the named graph of that statement (none for
    /// the default graph), and `statements` is how many mentions there are.
    Mentioned {
        carrier: String,
        by: String,
        graph: Option<String>,
        statements: usize,
    },
    /// `carrier` names a graph, so the claim's node also stands for a context.
    GraphName { carrier: String },
    /// No carrier, a carrier outside the default graph, or not exactly one carried
    /// statement that the compiler lowered to the axiom.
    Unresolved,
    /// The axiom states a term its carrier rewrote (a typed term carrier in a term
    /// position), so the carried RDF terms are not the terms of the axiom.
    Mismatch,
}

/// THE rule for a statement about a claim's carrier: is it evidence that selects
/// no context?
///
/// The allow-list is closed and short. The reification structure itself is allowed:
/// `rdf:reifies`, and for the classic form `rdf:type rdf:Statement`, `rdf:subject`,
/// `rdf:predicate` and `rdf:object`. Beyond structure only `logic:provenance` (who
/// reported the claim) and `logic:confidence` (how sure the reporter is) are
/// allowed. Every other predicate keeps the claim out, known or unknown: a
/// standpoint, a stance or denial, a time, a modality (including `logic:modality
/// logic:none`), a module, an attribution, a second type, or an annotation this
/// reader has never heard of may each change where or whether the claim holds.
///
/// The rule has more arms, applied by [`ClaimResolver`] because they concern
/// statements the carrier is not the subject of. A statement whose object is the
/// carrier keeps the claim out. So does any mention of the carrier: inside a triple
/// term at any depth, as the predicate of a statement, or as the subject, predicate
/// or object of a classic statement node. A mention counts whoever makes it,
/// whether that other node is itself read, left unread, or a bare quotation, so no
/// fixpoint is needed: a claim that is mentioned is out. A carrier that names a
/// graph is out too.
///
/// Every arm reads every graph of the source, not only the default graph. What is
/// said about a carrier in a named graph can only keep its claim out: an
/// allow-listed statement there is not evidence, because the compiler reads a
/// claim's scope from the default graph alone, and it does not block either.
fn evidence_only(predicate: &str, object: &Node) -> bool {
    match predicate {
        RDF_REIFIES | RDF_SUBJECT | RDF_PREDICATE | RDF_OBJECT => true,
        RDF_TYPE => matches!(object, Node::Iri(class) if class == RDF_STATEMENT),
        _ => predicate == logic_iri("provenance") || predicate == logic_iri("confidence"),
    }
}

/// A use of a carrier: the predicate or mentioning node, and the named graph.
type Use = (String, Option<String>);

/// What the source says about one claim carrier from outside it, in any graph.
#[derive(Debug, Default)]
struct CarrierUses {
    /// Predicates of statements whose object is the carrier.
    references: Vec<Use>,
    /// Nodes whose statement mentions the carrier.
    mentions: Vec<Use>,
    /// The carrier is the name of a graph.
    names_graph: bool,
}

/// Resolves the scoped claims of one compiled source.
///
/// Built once per source: it holds every outside use of a claim carrier, so
/// resolving every claim reads the dataset a single time.
#[derive(Debug)]
pub struct ClaimResolver<'source, S> {
    compiled: &'source CompiledSource<S>,
    uses: BTreeMap<TermId, CarrierUses>,
}

/// A node as a drop names it: its IRI, or `_:label` for a blank node.
fn node_name(store: &RdfDataset, node: TermId) -> String {
    match subject_of(store, node) {
        Subject::Iri(iri) => iri,
        Subject::Blank { label, .. } => format!("_:{label}"),
    }
}

/// Every term inside the triple term `term`, at any depth. Nothing for other terms.
fn triple_term_parts(store: &RdfDataset, term: TermId, parts: &mut Vec<TermId>) {
    let mut pending = vec![term];
    while let Some(term) = pending.pop() {
        if let TermRef::Triple { s, p, o } = store.resolve(term) {
            parts.extend([s, p, o]);
            pending.extend([s, p, o]);
        }
    }
}

impl<S: std::borrow::Borrow<PreparedLogicSource>> CompiledSource<S> {
    /// A resolver for the scoped claims of this source.
    #[must_use]
    pub fn claim_resolver(&self) -> ClaimResolver<'_, S> {
        let store = self.source().dataset();
        let mut uses: BTreeMap<TermId, CarrierUses> = self
            .axiom_sources()
            .iter()
            .flatten()
            .filter_map(|source| match source {
                AxiomSource::Reification(node) => Some((node.term, CarrierUses::default())),
                _ => None,
            })
            .collect();
        if !uses.is_empty() {
            for graph in store.named_graphs() {
                if let Some(carrier) = uses.get_mut(&graph) {
                    carrier.names_graph = true;
                }
            }
            let mut parts = Vec::new();
            for quad in source_graph_pattern(store, None, None, None, GraphMatch::Any) {
                let graph = quad.g.map(|graph| node_name(store, graph));
                if let Some(graph) = quad.g
                    && let Some(carrier) = uses.get_mut(&graph)
                {
                    carrier.names_graph = true;
                }
                if let Some(carrier) = uses.get_mut(&quad.o) {
                    let predicate = iri_of(store, quad.p);
                    if matches!(predicate.as_str(), RDF_SUBJECT | RDF_PREDICATE | RDF_OBJECT) {
                        carrier
                            .mentions
                            .push((node_name(store, quad.s), graph.clone()));
                    } else {
                        carrier
                            .references
                            .push((predicate.as_str().to_owned(), graph.clone()));
                    }
                }
                parts.clear();
                parts.push(quad.p);
                triple_term_parts(store, quad.o, &mut parts);
                for part in &parts {
                    if let Some(carrier) = uses.get_mut(part) {
                        carrier
                            .mentions
                            .push((node_name(store, quad.s), graph.clone()));
                    }
                }
            }
        }
        ClaimResolver {
            compiled: self,
            uses,
        }
    }
}

impl<S: std::borrow::Borrow<PreparedLogicSource>> ClaimResolver<'_, S> {
    /// The native statement of the scoped claim at `index` of `program().axioms`.
    ///
    /// Every reification source of the axiom must be an evidence-only carrier that
    /// nothing mentions, and all must resolve to the same statement. An axiom with
    /// any other kind of source, or with none, is unresolved.
    #[must_use]
    pub fn statement(&self, index: usize) -> ClaimStatement {
        let (Some(axiom), Some(sources)) = (
            self.compiled.program().axioms.get(index),
            self.compiled.axiom_sources().get(index),
        ) else {
            return ClaimStatement::Unresolved;
        };
        let store = self.compiled.source().dataset();
        let mut resolved = None;
        for source in sources {
            let AxiomSource::Reification(node) = source else {
                return ClaimStatement::Unresolved;
            };
            if node.graph.is_some() {
                return ClaimStatement::Unresolved;
            }
            if let Some(refused) = self.said_about(store, node.term) {
                return refused;
            }
            match carried_statement(store, *node, axiom) {
                statement @ ClaimStatement::Resolved { .. } => {
                    if resolved.as_ref().is_some_and(|prior| *prior != statement) {
                        return ClaimStatement::Unresolved;
                    }
                    resolved = Some(statement);
                }
                refused => return refused,
            }
        }
        resolved.unwrap_or(ClaimStatement::Unresolved)
    }

    /// What is said about `carrier` beyond evidence, in any graph, if anything: a
    /// statement it is the subject of that fails [`evidence_only`] or a statement it
    /// is the object of (`Qualified`), else a statement that mentions it
    /// (`Mentioned`), else its use as a graph name (`GraphName`).
    fn said_about(&self, store: &RdfDataset, carrier: TermId) -> Option<ClaimStatement> {
        let uses = self.uses.get(&carrier);
        let mut outside: Vec<Use> =
            source_graph_pattern(store, Some(carrier), None, None, GraphMatch::Any)
                .filter_map(|quad| {
                    let predicate = iri_of(store, quad.p);
                    (!evidence_only(predicate.as_str(), &node_of(store, quad.o))).then(|| {
                        (
                            predicate.as_str().to_owned(),
                            quad.g.map(|graph| node_name(store, graph)),
                        )
                    })
                })
                .chain(
                    uses.into_iter()
                        .flat_map(|uses| uses.references.iter().cloned()),
                )
                .collect();
        outside.sort();
        if let Some((predicate, graph)) = outside.first() {
            return Some(ClaimStatement::Qualified {
                carrier: node_name(store, carrier),
                predicate: predicate.clone(),
                graph: graph.clone(),
                statements: outside.len(),
            });
        }
        let uses = uses?;
        if let Some((by, graph)) = uses.mentions.iter().min() {
            return Some(ClaimStatement::Mentioned {
                carrier: node_name(store, carrier),
                by: by.clone(),
                graph: graph.clone(),
                statements: uses.mentions.len(),
            });
        }
        uses.names_graph.then(|| ClaimStatement::GraphName {
            carrier: node_name(store, carrier),
        })
    }
}

/// The one statement carried by `node` that the compiler lowered to `axiom`.
///
/// The comparison is on the whole axiom, so a claim the compiler lowered as negated
/// or as anything but an object-level statement never matches a carried statement.
fn carried_statement(store: &RdfDataset, node: SourceNode, axiom: &LogicAxiom) -> ClaimStatement {
    let carrier = subject_of(store, node.term);
    let mut matched: Vec<(TermId, TermId, TermId)> = Vec::new();
    let mut rewritten = false;
    let mut keep = |statement: (TermId, TermId, TermId)| {
        if !matched.contains(&statement) {
            matched.push(statement);
        }
    };
    // A typed term carrier in a term position makes the axiom state the carried
    // term's value, not the RDF term itself.
    let plain =
        |resource: &Subject| matches!(read_atomic_carrier(store, &carrier, resource), Ok(None));

    // RDF 1.2 carrier: `node rdf:reifies <<( s p o )>>`.
    if let Some(reifies) = store.term_id_by_iri(RDF_REIFIES) {
        for quad in default_graph_pattern(store, Some(node.term), Some(reifies), None) {
            let TermRef::Triple { s, p, o } = store.resolve(quad.o) else {
                continue;
            };
            let Node::Triple(triple) = node_of(store, quad.o) else {
                continue;
            };
            let lowered = native_quoted_axiom(store, &carrier, &triple, false, axiom.scope.clone());
            if lowered.ok().as_ref() != Some(axiom) {
                continue;
            }
            if plain(&triple.subject)
                && plain(&Subject::Iri(triple.predicate.as_str().to_owned()))
                && term_as_subject(&triple.object).is_none_or(|object| plain(&object))
            {
                keep((s, p, o));
            } else {
                rewritten = true;
            }
        }
    }

    // Classic carrier: an `rdf:Statement` node with one subject, predicate and object.
    let typed = store
        .term_id_by_iri(RDF_TYPE)
        .zip(store.term_id_by_iri(RDF_STATEMENT))
        .is_some_and(|(kind, statement)| {
            default_graph_pattern(store, Some(node.term), Some(kind), Some(statement))
                .next()
                .is_some()
        });
    if typed {
        let [subjects, predicates, objects] =
            [RDF_SUBJECT, RDF_PREDICATE, RDF_OBJECT].map(|part| {
                let mut values: Vec<TermId> = Vec::new();
                if let Some(part) = store.term_id_by_iri(part) {
                    for quad in default_graph_pattern(store, Some(node.term), Some(part), None) {
                        if !values.contains(&quad.o) {
                            values.push(quad.o);
                        }
                    }
                }
                values
            });
        if subjects.len() > 1 || predicates.len() > 1 || objects.len() > 1 {
            return ClaimStatement::Unresolved;
        }
        if let (&[s], &[p], &[o]) = (&subjects[..], &predicates[..], &objects[..])
            && let subject @ (Node::Iri(_) | Node::Blank { .. }) = node_of(store, s)
            && let Node::Iri(predicate) = node_of(store, p)
            && let Ok(lowered) = native_axiom(
                term_str(&subject),
                predicate,
                &node_of(store, o),
                false,
                axiom.scope.clone(),
            )
            && lowered == *axiom
        {
            keep((s, p, o));
        }
    }

    match matched[..] {
        [(subject, predicate, object)] => ClaimStatement::Resolved {
            subject,
            predicate,
            object,
        },
        [] if rewritten => ClaimStatement::Mismatch,
        _ => ClaimStatement::Unresolved,
    }
}
