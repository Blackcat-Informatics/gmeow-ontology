// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native terminal projection of typed presentation declarations. These are
//! descriptions, never serialized native merge or optimization witnesses.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use purrdf::{BlankScope, RdfLiteral, TermId, TermValue};

use super::rdf::{TripleSink, emit_formula, project_contract, project_named_contract};
use super::{LOGIC_NS, OverclaimError, RDF_TYPE, XSD_NS};
use crate::ir::LogicProgram;
use crate::ir::presentation::*;

/// Keep original contract names without duplicating their entries in the global
/// registry. Equal contracts still have distinct source owners and occurrences.
pub(super) fn contracts(g: &mut TripleSink, program: &LogicProgram) -> Result<(), OverclaimError> {
    let mut names: BTreeMap<String, VecDeque<&str>> = BTreeMap::new();
    if let PresentationProgramIr::Lowered(data) = &program.presentations {
        for (name, contract) in &data.contracts {
            names
                .entry(contract.content_key())
                .or_default()
                .push_back(name);
        }
    }
    for (index, contract) in program.contracts.iter().enumerate() {
        if let Some(name) = names
            .get_mut(&contract.content_key())
            .and_then(VecDeque::pop_front)
        {
            project_named_contract(g, name, contract);
        } else {
            project_contract(g, index, contract);
        }
    }
    if names.values().any(|names| !names.is_empty()) {
        return Err(OverclaimError(
            "presentation contract lacks its compiler registry occurrence".into(),
        ));
    }
    Ok(())
}

pub(super) fn emit(
    g: &mut TripleSink,
    program: &PresentationProgramIr,
) -> Result<(), OverclaimError> {
    let data = match program {
        PresentationProgramIr::Empty => return Ok(()),
        PresentationProgramIr::Refused { detail, .. } => {
            return Err(OverclaimError(detail.clone()));
        }
        PresentationProgramIr::Lowered(data) => data,
    };
    // Shared bodies have exactly one named definition, independent of the number
    // of sentences and presentations referencing them.
    for (name, formula) in &data.formulas {
        emit_formula(g, name, formula);
    }
    let occupied = g
        .builder
        .blank_identities()
        .map(|(label, _)| label.to_owned())
        .collect();
    let mut e = Emitter {
        g,
        occupied,
        next: 0,
    };
    for (name, context) in &data.contexts {
        let node = e.record(name, "PresentationContext");
        e.iri(node, "presentationWorld", &context.world);
        e.literal(
            node,
            "presentationModality",
            RdfLiteral::simple(context.modality.as_str()),
        );
        for (property, value) in [
            ("presentationStandpoint", &context.standpoint),
            ("presentationTime", &context.time),
            ("presentationPath", &context.path),
            ("presentationModule", &context.module),
        ] {
            if let Some(value) = value {
                e.iri(node, property, value);
            }
        }
    }
    for (name, symbol) in &data.symbols {
        let node = e.record(name, "PresentationSymbol");
        e.iri(node, "symbolContext", &symbol.context);
        for name in &symbol.names {
            e.resource(node, "symbolName", name)?;
        }
        for role in &symbol.roles {
            let (property, value) = match role {
                PresentationSymbolRole::Individual => ("individualSymbolRole", boolean()),
                PresentationSymbolRole::VariadicRelation => {
                    ("variadicRelationSymbolRole", boolean())
                }
                PresentationSymbolRole::VariadicFunction => {
                    ("variadicFunctionSymbolRole", boolean())
                }
                PresentationSymbolRole::Relation(arity) => ("relationSymbolArity", integer(*arity)),
                PresentationSymbolRole::Function(arity) => ("functionSymbolArity", integer(*arity)),
            };
            e.literal(node, property, value);
        }
    }
    for (name, evidence) in &data.evidence {
        let node = e.record(name, "PresentationEvidence");
        e.resource(node, "evidenceOrigin", &evidence.origin)?;
        for kind in &evidence.preservation {
            e.iri(node, "preservationKind", &kind.iri());
        }
        for residue in &evidence.unsupported_constructs {
            e.literal(node, "unsupportedConstruct", RdfLiteral::simple(residue));
        }
    }
    for (name, sentence) in &data.sentences {
        let node = e.record(name, "PresentationSentence");
        e.iri(node, "presentationFormula", &sentence.formula);
        e.iri(node, "sentenceContext", &sentence.context);
        e.iri(node, "sentenceEvidence", &sentence.evidence);
        e.iri(
            node,
            "sentenceSign",
            &format!(
                "{LOGIC_NS}{}",
                match sentence.sign {
                    PresentationSign::Positive => "PositiveSupport",
                    PresentationSign::Negative => "NegativeSupport",
                }
            ),
        );
        e.iri(
            node,
            "sentenceKind",
            &format!(
                "{LOGIC_NS}{}",
                match sentence.kind {
                    PresentationSentenceKind::Axiom => "PresentationAxiom",
                    PresentationSentenceKind::Caveat => "PresentationCaveat",
                }
            ),
        );
        for (name, symbol) in &sentence.bindings {
            let binding = e.blank();
            e.link(node, "sentenceBinding", binding);
            e.iri(binding, "bindingName", name);
            e.iri(binding, "bindingSymbol", symbol);
        }
    }
    for (name, presentation) in &data.presentations {
        let node = e.record(name, "FinitePresentation");
        e.iri(node, "presentationContract", &presentation.contract);
        for (property, members) in [
            ("presentationContext", &presentation.contexts),
            ("presentationSymbol", &presentation.symbols),
            ("presentationSentence", &presentation.sentences),
        ] {
            for member in members {
                e.iri(node, property, member);
            }
        }
    }
    for (name, map) in &data.maps {
        let node = e.record(name, "PresentationMap");
        e.iri(node, "mapSource", &map.source);
        e.iri(node, "mapTarget", &map.target);
        for (from, to) in &map.images {
            let binding = e.blank();
            e.link(node, "generatorBinding", binding);
            e.iri(binding, "bindingSource", from);
            e.iri(binding, "bindingTarget", to);
        }
    }
    for (name, merge) in &data.merges {
        let node = e.record(name, "PresentationMerge");
        e.iri(node, "mergeLeft", &merge.left);
        e.iri(node, "mergeRight", &merge.right);
    }
    Ok(())
}

fn boolean() -> RdfLiteral {
    RdfLiteral::typed("true", format!("{XSD_NS}boolean"))
}
fn integer(value: usize) -> RdfLiteral {
    RdfLiteral::typed(value.to_string(), format!("{XSD_NS}integer"))
}

/// Anonymous transport structures cannot collide with authored record IRIs.
/// The original blank names are metadata inside explicit scoped references.
struct Emitter<'a> {
    g: &'a mut TripleSink,
    occupied: BTreeSet<String>,
    next: usize,
}

impl Emitter<'_> {
    fn blank(&mut self) -> TermId {
        loop {
            let label = format!("gmeow-presentation-{}", self.next);
            self.next += 1;
            if self.occupied.insert(label.clone()) {
                return self.g.builder.intern_blank(&label, BlankScope::DEFAULT);
            }
        }
    }
    fn record(&mut self, name: &str, class: &str) -> TermId {
        let node = self.g.builder.intern_iri(name);
        self.type_node(node, class);
        node
    }
    fn type_node(&mut self, node: TermId, class: &str) {
        let predicate = self.g.builder.intern_iri(RDF_TYPE);
        let class = self.g.builder.intern_iri(&format!("{LOGIC_NS}{class}"));
        self.g.builder.push_quad(node, predicate, class, None);
    }
    fn link(&mut self, node: TermId, property: &str, object: TermId) {
        let predicate = self.g.builder.intern_iri(&format!("{LOGIC_NS}{property}"));
        self.g.builder.push_quad(node, predicate, object, None);
    }
    fn iri(&mut self, node: TermId, property: &str, object: &str) {
        let object = self.g.builder.intern_iri(object);
        self.link(node, property, object);
    }
    fn literal(&mut self, node: TermId, property: &str, value: RdfLiteral) {
        let object = self.g.builder.intern_literal(value);
        self.link(node, property, object);
    }
    fn resource(
        &mut self,
        node: TermId,
        property: &str,
        value: &PresentationTerm,
    ) -> Result<(), OverclaimError> {
        match &value.0 {
            TermValue::Iri(iri) => self.iri(node, property, iri),
            TermValue::Blank { label, scope } => {
                let reference = self.blank();
                self.type_node(reference, "ScopedBlankReference");
                self.literal(reference, "sourceBlankLabel", RdfLiteral::simple(label));
                self.literal(reference, "sourceBlankScope", integer(scope.0 as usize));
                self.link(node, property, reference);
            }
            _ => {
                return Err(OverclaimError(
                    "presentation resource is not an IRI or scoped blank".into(),
                ));
            }
        }
        Ok(())
    }
}
