// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Lower selected presentation declarations once, retaining shared owned syntax.
//! Native merge admission and original-source evidence binding remain independent.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::{RdfDataset, TermId, TermRef, TermValue};

use super::{
    Diagnostic, OwnerDisposition, OwnerFamily, OwnerLowering, SourceEdgeRole, SourceNode,
    SourceUnitKind, StructuralSourceGraph,
};
use crate::ir::presentation::*;
use crate::ir::{Formula, LOGIC_NAMESPACE, LogicModality, LogicProgram, PreservationKind};

mod reader;
#[cfg(test)]
mod tests;

const MAX_SOURCE_WORK: usize = 1_048_576;
const MAX_SOURCE_MERGES: usize = 4096;

fn family(kind: &SourceUnitKind) -> Option<OwnerFamily> {
    Some(match kind {
        SourceUnitKind::FinitePresentation => OwnerFamily::FinitePresentation,
        SourceUnitKind::PresentationContext => OwnerFamily::PresentationContext,
        SourceUnitKind::PresentationSymbol => OwnerFamily::PresentationSymbol,
        SourceUnitKind::PresentationSentence => OwnerFamily::PresentationSentence,
        SourceUnitKind::PresentationEvidence => OwnerFamily::PresentationEvidence,
        SourceUnitKind::PresentationMap => OwnerFamily::PresentationMap,
        SourceUnitKind::PresentationMerge => OwnerFamily::PresentationMerge,
        _ => return None,
    })
}

pub(super) fn owns_kind(kind: &SourceUnitKind) -> bool {
    family(kind).is_some()
        || matches!(
            kind,
            SourceUnitKind::PresentationBinding | SourceUnitKind::ScopedBlankReference
        )
}

fn error(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Ir {
        detail: message.into(),
    })
}

fn add_size(
    total: &mut usize,
    amount: usize,
    limit: usize,
    name: &str,
) -> gmeow_errors::Result<()> {
    *total = total
        .checked_add(amount)
        .ok_or_else(|| error(format!("finite presentation {name} overflow")))?;
    if *total > limit {
        return Err(error(format!("finite presentation {name} limit exceeded")));
    }
    Ok(())
}

pub(super) fn extract(
    dataset: &RdfDataset,
    graph: &StructuralSourceGraph,
    program: &LogicProgram,
    formulas: &BTreeMap<SourceNode, Arc<Formula>>,
    diagnostics: &mut Vec<Diagnostic>,
    owners: &mut Vec<OwnerLowering>,
) -> PresentationProgramIr {
    let roots: Vec<_> = graph
        .units()
        .flat_map(|unit| {
            unit.declared_kinds
                .union(&unit.required_kinds)
                .filter_map(|kind| family(kind).map(|family| (unit.node, family)))
        })
        .collect();
    if roots.is_empty() {
        return PresentationProgramIr::Empty;
    }
    let mut reader = Reader {
        dataset,
        graph,
        program,
        formulas,
        owners,
        data: PresentationDefinitions::default(),
        predicates: BTreeMap::new(),
        checked_scope: BTreeSet::new(),
        checked_syntax: BTreeSet::new(),
        work: 0,
    };
    let lowered = reader.read(&roots);
    match lowered {
        Ok(()) => {
            let data = reader.data;
            for (family, names) in [
                (
                    OwnerFamily::FinitePresentation,
                    data.presentations.keys().collect::<Vec<_>>(),
                ),
                (
                    OwnerFamily::PresentationContext,
                    data.contexts.keys().collect(),
                ),
                (
                    OwnerFamily::PresentationSymbol,
                    data.symbols.keys().collect(),
                ),
                (
                    OwnerFamily::PresentationSentence,
                    data.sentences.keys().collect(),
                ),
                (
                    OwnerFamily::PresentationEvidence,
                    data.evidence.keys().collect(),
                ),
                (OwnerFamily::PresentationMap, data.maps.keys().collect()),
                (OwnerFamily::PresentationMerge, data.merges.keys().collect()),
            ] {
                for (index, iri) in names.into_iter().enumerate() {
                    owners.push(OwnerLowering {
                        source: SourceNode {
                            term: dataset.term_id_by_iri(iri).expect("original declaration"),
                            graph: None,
                        },
                        family,
                        disposition: OwnerDisposition::Emitted { index },
                        diagnostics: Vec::new(),
                    });
                }
            }
            PresentationProgramIr::Lowered(Arc::new(data))
        }
        Err(failure) => {
            let detail = failure.message().to_owned();
            let focus = failure
                .inner()
                .source_ctx
                .focus
                .as_ref()
                .map(|focus| focus.0.clone());
            let diagnostic = diagnostics.len();
            diagnostics.push(Diagnostic::error(
                "MALFORMED_PRESENTATION",
                &detail,
                focus.clone(),
            ));
            let roots = roots
                .into_iter()
                .map(|(source, family)| {
                    owners.push(OwnerLowering {
                        source,
                        family,
                        disposition: OwnerDisposition::Rejected,
                        diagnostics: vec![diagnostic],
                    });
                    PresentationSourceRef {
                        term: PresentationTerm(dataset.term_value(source.term)),
                        graph: source
                            .graph
                            .map(|graph| PresentationTerm(dataset.term_value(graph))),
                    }
                })
                .collect();
            PresentationProgramIr::Refused {
                roots,
                focus,
                detail,
            }
        }
    }
}

struct Reader<'a> {
    dataset: &'a RdfDataset,
    graph: &'a StructuralSourceGraph,
    program: &'a LogicProgram,
    formulas: &'a BTreeMap<SourceNode, Arc<Formula>>,
    owners: &'a [OwnerLowering],
    data: PresentationDefinitions,
    predicates: BTreeMap<&'static str, Option<TermId>>,
    checked_scope: BTreeSet<TermId>,
    checked_syntax: BTreeSet<TermId>,
    work: usize,
}

impl Reader<'_> {
    fn read(&mut self, roots: &[(SourceNode, OwnerFamily)]) -> gmeow_errors::Result<()> {
        if roots.len() > MAX_SOURCE_WORK
            || roots
                .iter()
                .filter(|(_, family)| *family == OwnerFamily::PresentationMerge)
                .count()
                > MAX_SOURCE_MERGES
        {
            return Err(error(
                "finite presentation source merge roots limit exceeded",
            ));
        }
        let mut named_roots = BTreeMap::new();
        for (root, family) in roots {
            if root.graph.is_some() {
                return Err(self.error(
                    root.term,
                    "presentation declaration is outside the default source graph",
                ));
            }
            if named_roots
                .insert(self.named(root.term)?, (root.term, family))
                .is_some()
            {
                return Err(self.error(
                    root.term,
                    "presentation declaration has conflicting structural roles",
                ));
            }
        }
        for (name, (node, family)) in named_roots {
            match family {
                OwnerFamily::FinitePresentation => {
                    self.presentation(node)?;
                }
                OwnerFamily::PresentationContext => {
                    self.context(node)?;
                }
                OwnerFamily::PresentationSymbol => {
                    self.symbol(node)?;
                }
                OwnerFamily::PresentationSentence => {
                    self.sentence(node)?;
                }
                OwnerFamily::PresentationEvidence => {
                    self.evidence(node)?;
                }
                OwnerFamily::PresentationMap => {
                    self.map(node)?;
                }
                OwnerFamily::PresentationMerge => {
                    self.typed(node, "PresentationMerge")?;
                    let left = self.one(node, "mergeLeft")?;
                    let right = self.one(node, "mergeRight")?;
                    let left = self.map(left)?;
                    let right = self.map(right)?;
                    self.data
                        .merges
                        .insert(name, PresentationMergeIr { left, right });
                }
                _ => unreachable!("presentation root family"),
            }
        }
        Ok(())
    }

    fn context(&mut self, node: TermId) -> gmeow_errors::Result<String> {
        let name = self.named(node)?;
        if self.data.contexts.contains_key(&name) {
            return Ok(name);
        }
        self.typed(node, "PresentationContext")?;
        let world = self.name_value(node, "presentationWorld")?;
        let standpoint = self.optional_name(node, "presentationStandpoint")?;
        let time = self.optional_name(node, "presentationTime")?;
        let path = self.optional_name(node, "presentationPath")?;
        let module = self.optional_name(node, "presentationModule")?;
        let mode = self.one(node, "presentationModality")?;
        let mode = self.string(mode, "presentationModality")?;
        let modality = LogicModality::from_str_value(&mode)
            .ok_or_else(|| self.error(node, "unknown presentation modality"))?;
        self.data.contexts.insert(
            name.clone(),
            PresentationContextIr {
                world,
                standpoint,
                time,
                path,
                module,
                modality,
            },
        );
        Ok(name)
    }

    fn string(&self, node: TermId, property: &str) -> gmeow_errors::Result<String> {
        match self.dataset.term_value(node) {
            TermValue::Literal {
                lexical_form,
                datatype,
                language: None,
                direction: None,
            } if datatype == "http://www.w3.org/2001/XMLSchema#string" => Ok(lexical_form),
            _ => Err(self.error(node, &format!("{property} requires an untagged xsd:string"))),
        }
    }

    fn contract(&mut self, node: TermId) -> gmeow_errors::Result<String> {
        let name = self.named(node)?;
        if self.data.contracts.contains_key(&name) {
            return Ok(name);
        }
        let owner = self
            .owners
            .iter()
            .find(|owner| {
                owner.source
                    == SourceNode {
                        term: node,
                        graph: None,
                    }
                    && owner.family == OwnerFamily::Contract
            })
            .ok_or_else(|| {
                self.error(node, "presentation contract has no original compiler owner")
            })?;
        if !owner.diagnostics.is_empty() {
            return Err(self.error(
                node,
                "presentation contract carries unresolved extraction diagnostics",
            ));
        }
        let OwnerDisposition::Emitted { index } = owner.disposition else {
            return Err(self.error(node, "presentation contract was not emitted"));
        };
        self.data.contracts.insert(
            name.clone(),
            Arc::new(self.program.contracts[index].clone()),
        );
        Ok(name)
    }

    fn scalar(&self, node: TermId) -> gmeow_errors::Result<purrdf::xsd::XsdValue> {
        let TermValue::Literal {
            lexical_form,
            datatype,
            language: None,
            direction: None,
        } = self.dataset.term_value(node)
        else {
            return Err(self.error(node, "presentation role requires a typed scalar literal"));
        };
        purrdf::xsd::parse_by_iri(&lexical_form, &datatype)
            .map_err(|failure| error(format!("presentation scalar: {failure}")))?
            .ok_or_else(|| self.error(node, "unknown presentation scalar datatype"))
    }

    fn symbol(&mut self, node: TermId) -> gmeow_errors::Result<String> {
        let name = self.named(node)?;
        if self.data.symbols.contains_key(&name) {
            return Ok(name);
        }
        self.typed(node, "PresentationSymbol")?;
        let context = self.one(node, "symbolContext")?;
        let context = self.context(context)?;
        let names = self
            .values(node, "symbolName")?
            .into_iter()
            .map(|value| self.resource(value))
            .collect::<gmeow_errors::Result<_>>()?;
        let mut roles = BTreeSet::new();
        for (property, role) in [
            ("individualSymbolRole", PresentationSymbolRole::Individual),
            (
                "variadicRelationSymbolRole",
                PresentationSymbolRole::VariadicRelation,
            ),
            (
                "variadicFunctionSymbolRole",
                PresentationSymbolRole::VariadicFunction,
            ),
        ] {
            let values = self.values(node, property)?;
            if values.len() > 1 {
                return Err(self.error(node, "duplicate symbol role selector"));
            }
            for value in values {
                if !matches!(self.scalar(value)?, purrdf::xsd::XsdValue::Boolean(true)) {
                    return Err(self.error(node, "selected symbol role requires boolean true"));
                }
                roles.insert(role);
            }
        }
        for (property, function) in [
            ("relationSymbolArity", false),
            ("functionSymbolArity", true),
        ] {
            for value in self.values(node, property)? {
                let purrdf::xsd::XsdValue::Integer { value, .. } = self.scalar(value)? else {
                    return Err(self.error(node, "symbol arity requires an integer-family literal"));
                };
                let arity = usize::try_from(value).map_err(|_| {
                    self.error(node, "symbol arity must fit a nonnegative native index")
                })?;
                roles.insert(if function {
                    PresentationSymbolRole::Function(arity)
                } else {
                    PresentationSymbolRole::Relation(arity)
                });
            }
        }
        if roles.is_empty() {
            return Err(self.error(node, "presentation symbol has no declared role"));
        }
        self.data.symbols.insert(
            name.clone(),
            PresentationSymbolIr {
                context,
                names,
                roles,
            },
        );
        Ok(name)
    }

    fn formula(&mut self, node: TermId) -> gmeow_errors::Result<String> {
        let name = self.named(node)?;
        if self.data.formulas.contains_key(&name) {
            return Ok(name);
        }
        self.typed(node, "Formula")?;
        self.admit_syntax(node)?;
        let formula = self
            .formulas
            .get(&SourceNode {
                term: node,
                graph: None,
            })
            .ok_or_else(|| {
                self.error(
                    node,
                    "presentation formula has no successful source-owned compilation",
                )
            })?;
        self.data.formulas.insert(name.clone(), Arc::clone(formula));
        Ok(name)
    }

    fn evidence(&mut self, node: TermId) -> gmeow_errors::Result<String> {
        let name = self.named(node)?;
        if self.data.evidence.contains_key(&name) {
            return Ok(name);
        }
        self.typed(node, "PresentationEvidence")?;
        let origin = self.one(node, "evidenceOrigin")?;
        let origin = self.resource(origin)?;
        let mut preservation = BTreeSet::new();
        for value in self.values(node, "preservationKind")? {
            let value = self.named(value)?;
            preservation.insert(
                value
                    .strip_prefix(LOGIC_NAMESPACE)
                    .and_then(PreservationKind::from_local)
                    .ok_or_else(|| self.error(node, "unknown presentation preservation kind"))?,
            );
        }
        if preservation.is_empty() {
            return Err(self.error(
                node,
                "presentation evidence requires an explicit preservation judgment",
            ));
        }
        let mut unsupported_constructs = BTreeSet::new();
        for value in self.values(node, "unsupportedConstruct")? {
            let value = self.string(value, "unsupportedConstruct")?;
            if value.trim().is_empty() {
                return Err(self.error(node, "unsupportedConstruct requires a nonempty xsd:string"));
            }
            unsupported_constructs.insert(value);
        }
        self.data.evidence.insert(
            name.clone(),
            PresentationEvidenceIr {
                origin,
                preservation,
                unsupported_constructs,
            },
        );
        Ok(name)
    }

    /// Historical native blank identity is metadata, not a new generator or a
    /// certificate. A typed reference survives serialization and source relabeling.
    fn resource(&mut self, node: TermId) -> gmeow_errors::Result<PresentationTerm> {
        let source = SourceNode {
            term: node,
            graph: None,
        };
        let reference = self.graph.unit(source).is_some_and(|unit| {
            unit.declared_kinds
                .contains(&SourceUnitKind::ScopedBlankReference)
        });
        if reference {
            self.admit_scope(node)?;
            let label = self.one(node, "sourceBlankLabel")?;
            let label = self.string(label, "sourceBlankLabel")?;
            if label.is_empty() {
                return Err(self.error(node, "source blank label must be nonempty"));
            }
            let scope = self.one(node, "sourceBlankScope")?;
            let purrdf::xsd::XsdValue::Integer { value, .. } = self.scalar(scope)? else {
                return Err(self.error(node, "source blank scope must be an integer"));
            };
            let scope = u32::try_from(value)
                .map_err(|_| self.error(node, "source blank scope must fit a u32"))?;
            return Ok(PresentationTerm(TermValue::Blank {
                label,
                scope: purrdf::BlankScope(scope),
            }));
        }
        match self.dataset.term_value(node) {
            value @ (TermValue::Iri(_) | TermValue::Blank { .. }) => Ok(PresentationTerm(value)),
            _ => Err(self.error(node, "presentation resource must be an IRI or scoped blank")),
        }
    }

    fn sentence(&mut self, node: TermId) -> gmeow_errors::Result<String> {
        let name = self.named(node)?;
        if self.data.sentences.contains_key(&name) {
            return Ok(name);
        }
        self.typed(node, "PresentationSentence")?;
        let formula = self.one(node, "presentationFormula")?;
        let formula = self.formula(formula)?;
        let context = self.one(node, "sentenceContext")?;
        let context = self.context(context)?;
        let sign = match self
            .name_value(node, "sentenceSign")?
            .strip_prefix(LOGIC_NAMESPACE)
        {
            Some("PositiveSupport") => PresentationSign::Positive,
            Some("NegativeSupport") => PresentationSign::Negative,
            _ => return Err(self.error(node, "unknown sentence support sign")),
        };
        let kind = match self
            .name_value(node, "sentenceKind")?
            .strip_prefix(LOGIC_NAMESPACE)
        {
            Some("PresentationAxiom") => PresentationSentenceKind::Axiom,
            Some("PresentationCaveat") => PresentationSentenceKind::Caveat,
            _ => return Err(self.error(node, "unknown sentence kind")),
        };
        let evidence = self.one(node, "sentenceEvidence")?;
        let evidence = self.evidence(evidence)?;
        let mut bindings = BTreeMap::new();
        for binding in self.values(node, "sentenceBinding")? {
            self.admit_scope(binding)?;
            let symbol_name = self.name_value(binding, "bindingName")?;
            let symbol = self.one(binding, "bindingSymbol")?;
            let symbol = self.symbol(symbol)?;
            if bindings.insert(symbol_name, symbol).is_some() {
                return Err(self.error(node, "duplicate sentence symbol binding"));
            }
        }
        self.data.sentences.insert(
            name.clone(),
            PresentationSentenceIr {
                formula,
                context,
                bindings,
                sign,
                kind,
                evidence,
            },
        );
        Ok(name)
    }

    fn presentation(&mut self, node: TermId) -> gmeow_errors::Result<String> {
        let name = self.named(node)?;
        if self.data.presentations.contains_key(&name) {
            return Ok(name);
        }
        self.typed(node, "FinitePresentation")?;
        let contract = self.one(node, "presentationContract")?;
        let contract = self.contract(contract)?;
        let mut contexts = BTreeSet::new();
        for context in self.values(node, "presentationContext")? {
            contexts.insert(self.context(context)?);
        }
        let mut symbols = BTreeSet::new();
        for symbol in self.values(node, "presentationSymbol")? {
            symbols.insert(self.symbol(symbol)?);
        }
        let mut sentences = BTreeSet::new();
        for sentence in self.values(node, "presentationSentence")? {
            sentences.insert(self.sentence(sentence)?);
        }
        self.data.presentations.insert(
            name.clone(),
            FinitePresentationIr {
                contract,
                contexts,
                symbols,
                sentences,
            },
        );
        Ok(name)
    }

    fn map(&mut self, node: TermId) -> gmeow_errors::Result<String> {
        let name = self.named(node)?;
        if self.data.maps.contains_key(&name) {
            return Ok(name);
        }
        self.typed(node, "PresentationMap")?;
        let source = self.one(node, "mapSource")?;
        let source = self.presentation(source)?;
        let target = self.one(node, "mapTarget")?;
        let target = self.presentation(target)?;
        let mut images = BTreeMap::new();
        for binding in self.values(node, "generatorBinding")? {
            self.admit_scope(binding)?;
            let from = self.name_value(binding, "bindingSource")?;
            let to = self.name_value(binding, "bindingTarget")?;
            if images.insert(from, to).is_some() {
                return Err(self.error(node, "duplicate generator image"));
            }
        }
        self.data.maps.insert(
            name.clone(),
            PresentationMapIr {
                source,
                target,
                images,
            },
        );
        Ok(name)
    }
}
