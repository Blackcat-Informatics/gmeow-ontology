// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bind already lowered declarations to their original immutable source owner.
//! No RDF field extraction, parsing or formula reconstruction occurs here.

use super::*;
use gmeow_logic_compile::frontend::CompiledTheory;
use gmeow_logic_compile::ir::presentation::{PresentationDefinitions, PresentationProgramIr};

mod report;
#[cfg(test)]
mod tests;

/// Executed declarations and their exact immutable source publication. The native
/// witnesses admit reuse; their terminal report cannot be hydrated as authority.
#[derive(Debug)]
pub struct SourceMerges {
    source: Arc<CompiledTheory>,
    merges: BTreeMap<String, CheckedPushout>,
    presentations: BTreeMap<usize, String>,
    bodies: BTreeMap<String, Arc<SentenceBody>>,
    evidence: BTreeMap<String, Arc<PresentationEvidence>>,
}

impl SourceMerges {
    pub fn source(&self) -> &Arc<CompiledTheory> {
        &self.source
    }
    pub fn merges(&self) -> &BTreeMap<String, CheckedPushout> {
        &self.merges
    }
}

struct SourceSentence {
    body: Arc<SentenceBody>,
    context: String,
    bindings: Vec<String>,
    sign: SentenceSign,
    kind: SentenceKind,
    evidence: Arc<PresentationEvidence>,
}

struct SelectedPresentation {
    value: Arc<FinitePresentation>,
    symbols: BTreeMap<String, usize>,
}

/// Execute all selections in the source's own lowered IR. A detached or restored
/// declaration is data, not this source publication or a native merge witness.
pub fn execute(
    source: Arc<CompiledTheory>,
    limits: PresentationLimits,
) -> gmeow_errors::Result<SourceMerges> {
    let definitions = match &source.program().presentations {
        PresentationProgramIr::Empty => {
            return Ok(SourceMerges {
                source,
                merges: BTreeMap::new(),
                presentations: BTreeMap::new(),
                bodies: BTreeMap::new(),
                evidence: BTreeMap::new(),
            });
        }
        PresentationProgramIr::Lowered(definitions) => Arc::clone(definitions),
        PresentationProgramIr::Refused { focus, detail, .. } => {
            let failure = error(detail);
            return Err(match focus {
                Some(focus) => failure.with_focus(focus.clone()),
                None => failure,
            });
        }
    };
    bounded(
        definitions.merges.len(),
        limits.max_contexts,
        "source merge roots",
    )?;
    let mut reader = Reader {
        source: Arc::clone(&source),
        definitions: Arc::clone(&definitions),
        limits,
        work: 0,
        sentences: BTreeMap::new(),
        presentations: BTreeMap::new(),
        maps: BTreeMap::new(),
        contexts: BTreeMap::new(),
        bodies: BTreeMap::new(),
        evidence: BTreeMap::new(),
    };
    let mut merges = BTreeMap::new();
    for (name, declaration) in &definitions.merges {
        let left = reader.map(&declaration.left)?;
        let right = reader.map(&declaration.right)?;
        let result = CheckedPushout::build(left, right, limits)
            .map_err(|failure| failure.with_focus(name.clone()))?;
        merges.insert(name.clone(), result);
    }
    let presentations = reader
        .presentations
        .into_iter()
        .map(|(name, presentation)| (Arc::as_ptr(&presentation.value) as usize, name))
        .collect();
    Ok(SourceMerges {
        source,
        merges,
        presentations,
        bodies: reader.bodies,
        evidence: reader.evidence,
    })
}

struct Reader {
    source: Arc<CompiledTheory>,
    definitions: Arc<PresentationDefinitions>,
    limits: PresentationLimits,
    work: usize,
    sentences: BTreeMap<String, Arc<SourceSentence>>,
    presentations: BTreeMap<String, Arc<SelectedPresentation>>,
    maps: BTreeMap<String, PresentationMap>,
    contexts: BTreeMap<String, PresentationContext>,
    bodies: BTreeMap<String, Arc<SentenceBody>>,
    evidence: BTreeMap<String, Arc<PresentationEvidence>>,
}

fn missing(name: &str, kind: &str) -> gmeow_errors::Diag {
    error(format!("missing lowered presentation {kind}")).with_focus(name.to_owned())
}

impl Reader {
    fn charge(&mut self, slots: usize) -> gmeow_errors::Result<()> {
        add_size(
            &mut self.work,
            slots,
            self.limits.max_binding_slots,
            "typed presentation admission work",
        )
    }

    fn context(&mut self, name: &str) -> gmeow_errors::Result<PresentationContext> {
        if let Some(context) = self.contexts.get(name) {
            return Ok(context.clone());
        }
        self.charge(1)?;
        let context = self
            .definitions
            .contexts
            .get(name)
            .ok_or_else(|| missing(name, "context"))?;
        let context = PresentationContext {
            answer: ResultContext {
                world: context.world.clone(),
                standpoint: context.standpoint.clone(),
                attributed: None,
                time: context.time.clone(),
                path: context.path.clone(),
            },
            module: context.module.clone(),
            modality: context.modality,
        };
        self.contexts.insert(name.to_owned(), context.clone());
        Ok(context)
    }

    fn body(&mut self, name: &str) -> gmeow_errors::Result<Arc<SentenceBody>> {
        if let Some(body) = self.bodies.get(name) {
            return Ok(Arc::clone(body));
        }
        self.charge(1)?;
        let formula = self
            .definitions
            .formulas
            .get(name)
            .ok_or_else(|| missing(name, "formula"))?;
        let body = SentenceBody::new(Arc::clone(formula), self.limits)?;
        self.bodies.insert(name.to_owned(), Arc::clone(&body));
        Ok(body)
    }

    fn evidence(&mut self, name: &str) -> gmeow_errors::Result<Arc<PresentationEvidence>> {
        if let Some(evidence) = self.evidence.get(name) {
            return Ok(Arc::clone(evidence));
        }
        self.charge(1)?;
        let definition = self
            .definitions
            .evidence
            .get(name)
            .ok_or_else(|| missing(name, "evidence"))?;
        let origin = definition.origin.0.clone();
        resource(&origin)?;
        let mut loss = PreservationClaim::default();
        if definition.preservation.is_empty() {
            return Err(error(
                "presentation evidence requires an explicit preservation judgment",
            ));
        }
        for kind in &definition.preservation {
            loss.insert(*kind)?;
        }
        loss.unsupported_constructs = definition.unsupported_constructs.clone();
        loss.validate()?;
        let publication = EvidenceDataset::Source(Arc::clone(&self.source));
        let evidence = Arc::new(PresentationEvidence {
            origin,
            provenance: publication.clone(),
            complement: publication,
            loss,
        });
        self.evidence.insert(name.to_owned(), Arc::clone(&evidence));
        Ok(evidence)
    }

    fn sentence(&mut self, name: &str) -> gmeow_errors::Result<Arc<SourceSentence>> {
        if let Some(sentence) = self.sentences.get(name) {
            return Ok(Arc::clone(sentence));
        }
        let definitions = Arc::clone(&self.definitions);
        let definition = definitions
            .sentences
            .get(name)
            .ok_or_else(|| missing(name, "sentence"))?;
        self.charge(1 + definition.bindings.len())?;
        let body = self.body(&definition.formula)?;
        let evidence = self.evidence(&definition.evidence)?;
        if body.symbols().len() != definition.bindings.len() {
            return Err(error("sentence binding is not total on formula symbols")
                .with_focus(name.to_owned()));
        }
        let bindings = body
            .symbols()
            .iter()
            .map(|(symbol, _)| {
                definition.bindings.get(symbol).cloned().ok_or_else(|| {
                    error("sentence binding is not total on formula symbols")
                        .with_focus(name.to_owned())
                })
            })
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        let sentence = Arc::new(SourceSentence {
            body,
            context: definition.context.clone(),
            bindings,
            sign: definition.sign,
            kind: definition.kind,
            evidence,
        });
        self.sentences
            .insert(name.to_owned(), Arc::clone(&sentence));
        Ok(sentence)
    }

    fn presentation(&mut self, name: &str) -> gmeow_errors::Result<Arc<SelectedPresentation>> {
        if let Some(presentation) = self.presentations.get(name) {
            return Ok(Arc::clone(presentation));
        }
        let definitions = Arc::clone(&self.definitions);
        let definition = definitions
            .presentations
            .get(name)
            .ok_or_else(|| missing(name, "definition"))?;
        self.charge(
            1 + definition.contexts.len() + definition.symbols.len() + definition.sentences.len(),
        )?;
        let contract = definitions
            .contracts
            .get(&definition.contract)
            .ok_or_else(|| missing(&definition.contract, "contract"))?;
        let mut contexts = Vec::new();
        let mut context_indices = BTreeMap::new();
        for context in &definition.contexts {
            context_indices.insert(context.as_str(), contexts.len());
            contexts.push(self.context(context)?);
        }
        let mut symbols = Vec::new();
        let mut symbol_indices = BTreeMap::new();
        for symbol_name in &definition.symbols {
            let symbol = definitions
                .symbols
                .get(symbol_name)
                .ok_or_else(|| missing(symbol_name, "symbol"))?;
            self.charge(1 + symbol.names.len() + symbol.roles.len())?;
            let context = *context_indices
                .get(symbol.context.as_str())
                .ok_or_else(|| {
                    error("symbol context is outside its presentation")
                        .with_focus(symbol_name.clone())
                })?;
            symbol_indices.insert(symbol_name.clone(), symbols.len());
            symbols.push(PresentationSymbol {
                context,
                names: symbol.names.iter().map(|name| name.0.clone()).collect(),
                roles: symbol.roles.clone(),
            });
        }
        let mut sentences = Vec::new();
        for sentence_name in &definition.sentences {
            let sentence = self.sentence(sentence_name)?;
            let context = *context_indices
                .get(sentence.context.as_str())
                .ok_or_else(|| {
                    error("sentence context is outside its presentation")
                        .with_focus(sentence_name.clone())
                })?;
            let bindings = sentence
                .bindings
                .iter()
                .map(|symbol| {
                    symbol_indices.get(symbol).copied().ok_or_else(|| {
                        error("sentence binding is outside its presentation signature")
                            .with_focus(sentence_name.clone())
                    })
                })
                .collect::<gmeow_errors::Result<Vec<_>>>()?;
            sentences.push(PresentationSentence {
                body: Arc::clone(&sentence.body),
                bindings,
                context,
                kind: sentence.kind,
                sign: sentence.sign,
                evidence: Arc::clone(&sentence.evidence),
            });
        }
        let value = FinitePresentation::new(
            contexts,
            symbols,
            sentences,
            Arc::clone(contract),
            self.limits,
        )?;
        let presentation = Arc::new(SelectedPresentation {
            value,
            symbols: symbol_indices,
        });
        self.presentations
            .insert(name.to_owned(), Arc::clone(&presentation));
        Ok(presentation)
    }

    fn map(&mut self, name: &str) -> gmeow_errors::Result<PresentationMap> {
        if let Some(map) = self.maps.get(name) {
            return Ok(map.clone());
        }
        let definitions = Arc::clone(&self.definitions);
        let definition = definitions
            .maps
            .get(name)
            .ok_or_else(|| missing(name, "map"))?;
        self.charge(1 + definition.images.len())?;
        let source = self.presentation(&definition.source)?;
        let target = self.presentation(&definition.target)?;
        let mut images = vec![None; source.symbols.len()];
        for (from, to) in &definition.images {
            let from = *source.symbols.get(from).ok_or_else(|| {
                error("map binding is outside its source signature").with_focus(name.to_owned())
            })?;
            let to = *target.symbols.get(to).ok_or_else(|| {
                error("map binding is outside its target signature").with_focus(name.to_owned())
            })?;
            images[from] = Some(to);
        }
        let images = images
            .into_iter()
            .map(|image| {
                image.ok_or_else(|| {
                    error("presentation map is not total").with_focus(name.to_owned())
                })
            })
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        let map =
            PresentationMap::new(Arc::clone(&source.value), Arc::clone(&target.value), images)?;
        self.maps.insert(name.to_owned(), map.clone());
        Ok(map)
    }
}
