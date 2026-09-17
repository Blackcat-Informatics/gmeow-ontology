// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Finite indexed, evidence-decorated presentations and checked presentation maps.
//!
//! A map preserves declared sentences syntactically, modulo the canonical IR's
//! alpha/connective normalization. It is not an entailment oracle. Sign, context,
//! caveat ownership and the exact native evidence publication remain separate.
//! Formula bodies are analyzed once and shared; transport changes symbol bindings.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub use gmeow_logic_compile::ir::presentation::{
    PresentationSentenceKind as SentenceKind, PresentationSign as SentenceSign,
    PresentationSymbolRole as SymbolRole,
};
use gmeow_logic_compile::ir::{ContentKey, LogicModality, ReasoningContract};
use purrdf::{RdfDataset, TermValue};

use crate::result::{PreservationClaim, ResultContext};

mod syntax;
pub use syntax::SentenceBody;
mod pushout;
pub mod source;
pub use pushout::CheckedPushout;

#[cfg(test)]
mod tests;

fn error(message: impl Into<String>) -> gmeow_errors::Diag {
    super::exec_error(message)
}

/// Deterministic admission/work limits. Exceeding a limit refuses the selected
/// operation; it never truncates a signature, sentence or evidence record.
#[derive(Debug, Clone, Copy)]
pub struct PresentationLimits {
    pub max_contexts: usize,
    pub max_symbols: usize,
    pub max_sentences: usize,
    pub max_binding_slots: usize,
    pub max_formula_nodes: usize,
    pub max_formula_bytes: usize,
    pub max_formula_depth: usize,
    pub max_key_bytes: usize,
}

impl Default for PresentationLimits {
    fn default() -> Self {
        Self {
            max_contexts: 4096,
            max_symbols: 65_536,
            max_sentences: 65_536,
            max_binding_slots: 1_048_576,
            max_formula_nodes: 65_536,
            max_formula_bytes: 4 * 1024 * 1024,
            max_formula_depth: 128,
            max_key_bytes: 32 * 1024 * 1024,
        }
    }
}

fn bounded(size: usize, maximum: usize, name: &str) -> gmeow_errors::Result<()> {
    if size > maximum {
        return Err(error(format!("finite presentation {name} limit exceeded")));
    }
    Ok(())
}

fn add_size(
    total: &mut usize,
    size: usize,
    maximum: usize,
    name: &str,
) -> gmeow_errors::Result<()> {
    *total = total
        .checked_add(size)
        .ok_or_else(|| error(format!("finite presentation {name} overflow")))?;
    bounded(*total, maximum, name)
}

/// Index values are fixed by a map. Absence of a standpoint is an unspecified
/// index, never a universal scope. The module and modality are also preserved.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PresentationContext {
    pub answer: ResultContext,
    pub module: Option<String>,
    pub modality: LogicModality,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct ContextIdentity<'a> {
    world: &'a str,
    standpoint: Option<&'a str>,
    time: Option<&'a str>,
    path: Option<&'a str>,
    attributed: Option<&'a str>,
    module: Option<&'a str>,
    modality: &'static str,
}

fn context_identity(context: &PresentationContext) -> ContextIdentity<'_> {
    ContextIdentity {
        world: &context.answer.world,
        standpoint: context.answer.standpoint.as_deref(),
        time: context.answer.time.as_deref(),
        path: context.answer.path.as_deref(),
        attributed: context.answer.attributed.as_deref(),
        module: context.module.as_deref(),
        modality: context.modality.as_str(),
    }
}

/// One context-indexed signature generator. Its vector position is its local
/// identity. Historical names are co-equal metadata, not global identifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationSymbol {
    pub context: usize,
    pub names: BTreeSet<TermValue>,
    pub roles: BTreeSet<SymbolRole>,
}

/// A native evidence publication retained by its actual immutable owner.
/// Source-owned evidence borrows the compiler's canonical dataset and original
/// occurrence records without copying either the corpus or its term dictionary.
#[derive(Debug, Clone)]
pub enum EvidenceDataset {
    Native(Arc<RdfDataset>),
    Source(Arc<gmeow_logic_compile::frontend::CompiledTheory>),
}

impl EvidenceDataset {
    pub fn dataset(&self) -> &RdfDataset {
        match self {
            Self::Native(dataset) => dataset,
            Self::Source(theory) => theory.source().dataset(),
        }
    }
    pub fn same_publication(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Native(first), Self::Native(second)) => Arc::ptr_eq(first, second),
            (Self::Source(first), Self::Source(second)) => Arc::ptr_eq(first, second),
            _ => false,
        }
    }
}

/// Exact original evidence publication. Presentation maps carry its native
/// datasets unchanged with the symbol bindings, preserving historical provenance,
/// graph catalogues, RDF 1.2 statements and opaque sidecars without materialization.
#[derive(Debug)]
pub struct PresentationEvidence {
    origin: TermValue,
    provenance: EvidenceDataset,
    complement: EvidenceDataset,
    loss: PreservationClaim,
}

impl PresentationEvidence {
    pub fn new(
        origin: TermValue,
        provenance: Arc<RdfDataset>,
        complement: Arc<RdfDataset>,
        loss: PreservationClaim,
    ) -> gmeow_errors::Result<Arc<Self>> {
        resource(&origin)?;
        loss.validate()?;
        Ok(Arc::new(Self {
            origin,
            provenance: EvidenceDataset::Native(provenance),
            complement: EvidenceDataset::Native(complement),
            loss,
        }))
    }
    pub fn origin(&self) -> &TermValue {
        &self.origin
    }
    pub fn provenance(&self) -> &EvidenceDataset {
        &self.provenance
    }
    pub fn complement(&self) -> &EvidenceDataset {
        &self.complement
    }
    pub fn loss(&self) -> &PreservationClaim {
        &self.loss
    }
}

fn resource(value: &TermValue) -> gmeow_errors::Result<()> {
    match value {
        TermValue::Iri(iri) => {
            super::algebra_iri(iri)?;
        }
        TermValue::Blank { .. } => {}
        _ => {
            return Err(error(
                "presentation identities must be native resource terms",
            ));
        }
    }
    Ok(())
}

/// A shared analyzed sentence with one total binding from its original symbol
/// names to the containing signature. Construction is checked by the presentation.
#[derive(Debug, Clone)]
pub struct PresentationSentence {
    pub body: Arc<SentenceBody>,
    pub bindings: Vec<usize>,
    pub context: usize,
    pub kind: SentenceKind,
    pub sign: SentenceSign,
    pub evidence: Arc<PresentationEvidence>,
}

// Invocation-local membership index only. Evidence addresses never enter a
// durable key, output order, serialized certificate or user-visible diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SentenceKey {
    context: usize,
    kind: SentenceKind,
    sign: SentenceSign,
    formula: ContentKey,
    evidence: usize,
}

impl PresentationSentence {
    fn key(&self, context: usize, bindings: &[usize]) -> SentenceKey {
        SentenceKey {
            context,
            kind: self.kind,
            sign: self.sign,
            formula: self.body.key(bindings),
            evidence: Arc::as_ptr(&self.evidence) as usize,
        }
    }
    fn transported(&self, contexts: &[usize], symbols: &[usize]) -> Self {
        Self {
            body: Arc::clone(&self.body),
            bindings: self
                .bindings
                .iter()
                .map(|&symbol| symbols[symbol])
                .collect(),
            context: contexts[self.context],
            kind: self.kind,
            sign: self.sign,
            evidence: Arc::clone(&self.evidence),
        }
    }
}

/// An admitted finite presentation. Data is immutable after construction, so
/// maps and pushout witnesses cannot be rebound to mutated inputs. The contract
/// is retained exactly; this constructor does not claim to execute its theory.
#[derive(Debug)]
pub struct FinitePresentation {
    contexts: Vec<PresentationContext>,
    symbols: Vec<PresentationSymbol>,
    sentences: Vec<PresentationSentence>,
    keys: BTreeSet<SentenceKey>,
    contract: Arc<ReasoningContract>,
}

impl FinitePresentation {
    pub fn new(
        contexts: Vec<PresentationContext>,
        symbols: Vec<PresentationSymbol>,
        sentences: Vec<PresentationSentence>,
        contract: Arc<ReasoningContract>,
        limits: PresentationLimits,
    ) -> gmeow_errors::Result<Arc<Self>> {
        bounded(contexts.len(), limits.max_contexts, "contexts")?;
        bounded(symbols.len(), limits.max_symbols, "symbols")?;
        bounded(sentences.len(), limits.max_sentences, "sentences")?;
        let mut context_keys = BTreeSet::new();
        for context in &contexts {
            super::algebra_iri(&context.answer.world)?;
            for iri in [
                &context.answer.standpoint,
                &context.answer.path,
                &context.module,
            ]
            .into_iter()
            .flatten()
            {
                super::algebra_iri(iri)?;
            }
            if !context_keys.insert(context_identity(context)) {
                return Err(error("duplicate presentation context"));
            }
        }
        drop(context_keys);
        let mut slots = 0;
        for symbol in &symbols {
            if symbol.context >= contexts.len()
                || symbol.names.is_empty()
                || symbol.roles.is_empty()
            {
                return Err(error("incomplete presentation signature generator"));
            }
            add_size(
                &mut slots,
                symbol.names.len(),
                limits.max_binding_slots,
                "bindings",
            )?;
            add_size(
                &mut slots,
                symbol.roles.len(),
                limits.max_binding_slots,
                "bindings",
            )?;
            for name in &symbol.names {
                resource(name)?;
            }
            if symbol.roles.contains(&SymbolRole::Function(0)) {
                return Err(error("a nullary function must use the individual role"));
            }
        }
        let mut keys = BTreeSet::new();
        let mut key_bytes = 0;
        let mut retained = Vec::with_capacity(sentences.len());
        for sentence in sentences {
            if sentence.context >= contexts.len()
                || sentence.bindings.len() != sentence.body.symbols.len()
            {
                return Err(error("incomplete presentation sentence binding"));
            }
            sentence.body.admit_limits(limits)?;
            add_size(
                &mut slots,
                sentence.bindings.len(),
                limits.max_binding_slots,
                "bindings",
            )?;
            for ((_, roles), &index) in sentence.body.symbols.iter().zip(&sentence.bindings) {
                let symbol = symbols
                    .get(index)
                    .ok_or_else(|| error("sentence binding is outside its signature"))?;
                if symbol.context != sentence.context || !roles.is_subset(&symbol.roles) {
                    return Err(error(
                        "sentence symbol roles or context disagree with its signature",
                    ));
                }
            }
            let key = sentence.key(sentence.context, &sentence.bindings);
            add_size(
                &mut key_bytes,
                key.formula.as_str().len(),
                limits.max_key_bytes,
                "canonical key bytes",
            )?;
            if keys.insert(key) {
                retained.push(sentence);
            }
        }
        Ok(Arc::new(Self {
            contexts,
            symbols,
            sentences: retained,
            keys,
            contract,
        }))
    }

    pub fn contexts(&self) -> &[PresentationContext] {
        &self.contexts
    }
    pub fn symbols(&self) -> &[PresentationSymbol] {
        &self.symbols
    }
    pub fn sentences(&self) -> &[PresentationSentence] {
        &self.sentences
    }
    pub fn contract(&self) -> &Arc<ReasoningContract> {
        &self.contract
    }
}

/// A total, typed, index-preserving map. It preserves every decorated signed
/// sentence and source name. General maps may identify generators; embeddings
/// used by the pushout span must additionally be injective.
#[derive(Debug, Clone)]
pub struct PresentationMap {
    source: Arc<FinitePresentation>,
    target: Arc<FinitePresentation>,
    contexts: Vec<usize>,
    symbols: Vec<usize>,
}

impl PresentationMap {
    pub fn new(
        source: Arc<FinitePresentation>,
        target: Arc<FinitePresentation>,
        symbols: Vec<usize>,
    ) -> gmeow_errors::Result<Self> {
        if source.contract != target.contract {
            return Err(error(
                "presentation maps cannot change the selected reasoning contract",
            ));
        }
        if symbols.len() != source.symbols.len() {
            return Err(error("presentation map must be total on generators"));
        }
        let contexts = {
            let indices: BTreeMap<_, _> = target
                .contexts
                .iter()
                .enumerate()
                .map(|(index, context)| (context_identity(context), index))
                .collect();
            source
                .contexts
                .iter()
                .map(|context| {
                    indices
                        .get(&context_identity(context))
                        .copied()
                        .ok_or_else(|| {
                            error("presentation map cannot erase or change a declared context")
                        })
                })
                .collect::<gmeow_errors::Result<Vec<_>>>()?
        };
        for (origin, &index) in source.symbols.iter().zip(&symbols) {
            let image = target
                .symbols
                .get(index)
                .ok_or_else(|| error("presentation map image is outside its target signature"))?;
            if image.context != contexts[origin.context]
                || image.roles != origin.roles
                || !origin.names.is_subset(&image.names)
            {
                return Err(error(
                    "presentation map does not preserve generator context, roles or source names",
                ));
            }
        }
        for sentence in &source.sentences {
            let bindings: Vec<_> = sentence
                .bindings
                .iter()
                .map(|&symbol| symbols[symbol])
                .collect();
            if !target
                .keys
                .contains(&sentence.key(contexts[sentence.context], &bindings))
            {
                return Err(error(
                    "presentation map loses a signed axiom, caveat or its exact evidence",
                ));
            }
        }
        Ok(Self {
            source,
            target,
            contexts,
            symbols,
        })
    }

    pub fn identity(presentation: Arc<FinitePresentation>) -> Self {
        Self {
            contexts: (0..presentation.contexts.len()).collect(),
            symbols: (0..presentation.symbols.len()).collect(),
            source: Arc::clone(&presentation),
            target: presentation,
        }
    }
    pub fn source(&self) -> &Arc<FinitePresentation> {
        &self.source
    }
    pub fn target(&self) -> &Arc<FinitePresentation> {
        &self.target
    }
    pub fn images(&self) -> &[usize] {
        &self.symbols
    }
    pub fn is_embedding(&self) -> bool {
        self.symbols.iter().copied().collect::<BTreeSet<_>>().len() == self.symbols.len()
    }
    pub fn then(&self, next: &Self) -> gmeow_errors::Result<Self> {
        if !Arc::ptr_eq(&self.target, &next.source) {
            return Err(error(
                "presentation map composition requires the exact intermediate publication",
            ));
        }
        Self::new(
            Arc::clone(&self.source),
            Arc::clone(&next.target),
            self.symbols
                .iter()
                .map(|&index| next.symbols[index])
                .collect(),
        )
    }
    pub fn agrees_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.source, &other.source)
            && Arc::ptr_eq(&self.target, &other.target)
            && self.symbols == other.symbols
    }
}
