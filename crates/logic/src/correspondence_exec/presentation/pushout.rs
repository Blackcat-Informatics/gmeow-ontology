// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Construction and independent checking of the finite presentation pushout.

use super::*;

/// A checked square in the finite decorated-presentation category. Construction
/// and checking are separate: checking establishes exact intersection, coverage
/// and the minimal transported sentence/name sets, not merely commutativity.
/// It proves no unrestricted optimizer rewrite or arbitrary theory equivalence.
#[derive(Debug)]
pub struct CheckedPushout {
    left: PresentationMap,
    right: PresentationMap,
    left_injection: PresentationMap,
    right_injection: PresentationMap,
    engine_descriptor: String,
}

impl CheckedPushout {
    pub fn build(
        left: PresentationMap,
        right: PresentationMap,
        limits: PresentationLimits,
    ) -> gmeow_errors::Result<Self> {
        admit_span(&left, &right)?;
        let a = &left.target;
        let b = &right.target;
        let total = a
            .symbols
            .len()
            .checked_add(b.symbols.len())
            .ok_or_else(|| error("pushout signature size overflow"))?;
        bounded(total, limits.max_symbols, "pushout generator work")?;
        let sentence_total = a
            .sentences
            .len()
            .checked_add(b.sentences.len())
            .ok_or_else(|| error("pushout sentence size overflow"))?;
        bounded(
            sentence_total,
            limits.max_sentences,
            "pushout sentence work",
        )?;
        let mut parents: Vec<_> = (0..total).collect();
        // These are the only identifications made by the construction. In
        // particular, equal spelling never causes a union outside the apex.
        for (&first, &second) in left.symbols.iter().zip(&right.symbols) {
            let first = root(&mut parents, first);
            let second = root(&mut parents, a.symbols.len() + second);
            parents[first.max(second)] = first.min(second);
        }
        bounded(a.contexts.len(), limits.max_contexts, "pushout contexts")?;
        let mut context_indices: BTreeMap<_, _> = a
            .contexts
            .iter()
            .enumerate()
            .map(|(index, context)| (context_identity(context), index))
            .collect();
        let mut contexts = a.contexts.clone();
        let first_contexts: Vec<_> = (0..contexts.len()).collect();
        let mut second_contexts = Vec::with_capacity(b.contexts.len());
        for context in &b.contexts {
            let index = if let Some(&index) = context_indices.get(&context_identity(context)) {
                index
            } else {
                bounded(contexts.len() + 1, limits.max_contexts, "pushout contexts")?;
                context_indices.insert(context_identity(context), contexts.len());
                contexts.push(context.clone());
                contexts.len() - 1
            };
            second_contexts.push(index);
        }
        let mut symbols: Vec<PresentationSymbol> = Vec::new();
        let mut roots = BTreeMap::new();
        let mut images = Vec::with_capacity(total);
        for (offset, (presentation, context_images)) in
            [(a, &first_contexts), (b, &second_contexts)]
                .into_iter()
                .enumerate()
        {
            let base = if offset == 0 { 0 } else { a.symbols.len() };
            for (index, symbol) in presentation.symbols.iter().enumerate() {
                let representative = root(&mut parents, base + index);
                let image = *roots.entry(representative).or_insert(symbols.len());
                if image == symbols.len() {
                    let mut symbol = symbol.clone();
                    symbol.context = context_images[symbol.context];
                    symbols.push(symbol);
                } else {
                    symbols[image].names.extend(symbol.names.iter().cloned());
                }
                images.push(image);
            }
        }
        let first = &images[..a.symbols.len()];
        let second = &images[a.symbols.len()..];
        let sentences = a
            .sentences
            .iter()
            .map(|sentence| sentence.transported(&first_contexts, first))
            .chain(
                b.sentences
                    .iter()
                    .map(|sentence| sentence.transported(&second_contexts, second)),
            )
            .collect();
        let result = FinitePresentation::new(
            contexts,
            symbols,
            sentences,
            Arc::clone(&a.contract),
            limits,
        )?;
        let left_injection =
            PresentationMap::new(Arc::clone(a), Arc::clone(&result), first.to_vec())?;
        let right_injection = PresentationMap::new(Arc::clone(b), result, second.to_vec())?;
        Self::check(left, right, left_injection, right_injection)
    }

    /// Check an independently supplied candidate square. This checker does not
    /// trust the construction's union-find partition, names or sentence inventory.
    pub fn check(
        left: PresentationMap,
        right: PresentationMap,
        left_injection: PresentationMap,
        right_injection: PresentationMap,
    ) -> gmeow_errors::Result<Self> {
        admit_span(&left, &right)?;
        if !Arc::ptr_eq(&left.target, &left_injection.source)
            || !Arc::ptr_eq(&right.target, &right_injection.source)
            || !Arc::ptr_eq(&left_injection.target, &right_injection.target)
        {
            return Err(error(
                "pushout square is rebound to a different presentation",
            ));
        }
        if !left_injection.is_embedding() || !right_injection.is_embedding() {
            return Err(error("pushout injections must be embeddings"));
        }
        let output = &left_injection.target;
        let mut pairs = BTreeSet::new();
        for (&a, &b) in left.symbols.iter().zip(&right.symbols) {
            if left_injection.symbols[a] != right_injection.symbols[b] {
                return Err(error("pushout square does not commute on its apex"));
            }
            pairs.insert((a, b));
        }
        let mut origins = vec![(None, None); output.symbols.len()];
        for (index, &image) in left_injection.symbols.iter().enumerate() {
            origins[image].0 = Some(index);
        }
        for (index, &image) in right_injection.symbols.iter().enumerate() {
            origins[image].1 = Some(index);
        }
        for (symbol, (a, b)) in output.symbols.iter().zip(origins) {
            if a.is_none() && b.is_none() {
                return Err(error(
                    "pushout injections do not cover every result generator",
                ));
            }
            if let (Some(a), Some(b)) = (a, b)
                && !pairs.contains(&(a, b))
            {
                return Err(error(
                    "pushout identifies generators outside the declared apex",
                ));
            }
            let mut names = BTreeSet::new();
            if let Some(index) = a {
                names.extend(left.target.symbols[index].names.iter().cloned());
            }
            if let Some(index) = b {
                names.extend(right.target.symbols[index].names.iter().cloned());
            }
            if names != symbol.names {
                return Err(error(
                    "pushout result contains unsupported generator name evidence",
                ));
            }
        }
        let mut context_coverage = vec![false; output.contexts.len()];
        for &index in left_injection
            .contexts
            .iter()
            .chain(&right_injection.contexts)
        {
            context_coverage[index] = true;
        }
        if context_coverage.contains(&false) {
            return Err(error(
                "pushout contains a context not supplied by its inputs",
            ));
        }
        let mut sentences = BTreeSet::new();
        for injection in [&left_injection, &right_injection] {
            for sentence in &injection.source.sentences {
                let bindings: Vec<_> = sentence
                    .bindings
                    .iter()
                    .map(|&symbol| injection.symbols[symbol])
                    .collect();
                sentences.insert(sentence.key(injection.contexts[sentence.context], &bindings));
            }
        }
        if sentences != output.keys {
            return Err(error(
                "pushout result must contain exactly the transported signed sentences and evidence",
            ));
        }
        Ok(Self {
            left,
            right,
            left_injection,
            right_injection,
            engine_descriptor: crate::runtime::EngineContract::current().descriptor_hash,
        })
    }

    pub fn output(&self) -> &Arc<FinitePresentation> {
        &self.left_injection.target
    }
    /// The admitted embeddings from the exact shared apex into each input.
    pub fn left_embedding(&self) -> &PresentationMap {
        &self.left
    }
    pub fn right_embedding(&self) -> &PresentationMap {
        &self.right
    }
    pub fn left_injection(&self) -> &PresentationMap {
        &self.left_injection
    }
    pub fn right_injection(&self) -> &PresentationMap {
        &self.right_injection
    }
    pub fn engine_descriptor(&self) -> &str {
        &self.engine_descriptor
    }

    fn admit_cocone(
        &self,
        first: &PresentationMap,
        second: &PresentationMap,
    ) -> gmeow_errors::Result<()> {
        if !Arc::ptr_eq(&first.source, &self.left.target)
            || !Arc::ptr_eq(&second.source, &self.right.target)
            || !Arc::ptr_eq(&first.target, &second.target)
        {
            return Err(error(
                "factorization cocone is rebound to different presentations",
            ));
        }
        for (&a, &b) in self.left.symbols.iter().zip(&self.right.symbols) {
            if first.symbols[a] != second.symbols[b] {
                return Err(error("factorization cocone disagrees on the common apex"));
            }
        }
        Ok(())
    }

    /// Construct the induced map for ANY admitted compatible cocone. The checked
    /// coverage/intersection properties make the assignment total and independent
    /// of which input supplied a generator; the map checker verifies its sentences.
    pub fn factor(
        &self,
        first: &PresentationMap,
        second: &PresentationMap,
    ) -> gmeow_errors::Result<PresentationMap> {
        self.admit_cocone(first, second)?;
        let mut images = vec![None; self.output().symbols.len()];
        for (injection, map) in [
            (&self.left_injection, first),
            (&self.right_injection, second),
        ] {
            for (&generator, &image) in injection.symbols.iter().zip(&map.symbols) {
                if images[generator].is_some_and(|previous| previous != image) {
                    return Err(error("factorization has incompatible generator images"));
                }
                images[generator] = Some(image);
            }
        }
        let images = images
            .into_iter()
            .map(|image| image.ok_or_else(|| error("factorization lacks a result generator")))
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        let map =
            PresentationMap::new(Arc::clone(self.output()), Arc::clone(&first.target), images)?;
        self.check_factorization(first, second, &map)?;
        Ok(map)
    }

    /// Check a candidate mediator. Agreement on BOTH injections determines every
    /// output generator by joint surjectivity, proving uniqueness in this category.
    /// No bounded sample of formulas, models or cocones grants this witness.
    pub fn check_factorization(
        &self,
        first: &PresentationMap,
        second: &PresentationMap,
        candidate: &PresentationMap,
    ) -> gmeow_errors::Result<()> {
        self.admit_cocone(first, second)?;
        if !Arc::ptr_eq(&candidate.source, self.output())
            || !Arc::ptr_eq(&candidate.target, &first.target)
        {
            return Err(error("factorization mediator has different endpoints"));
        }
        for (injection, map) in [
            (&self.left_injection, first),
            (&self.right_injection, second),
        ] {
            for (&generator, &expected) in injection.symbols.iter().zip(&map.symbols) {
                if candidate.symbols[generator] != expected {
                    return Err(error(
                        "factorization mediator does not commute with both injections",
                    ));
                }
            }
        }
        Ok(())
    }
}

fn admit_span(left: &PresentationMap, right: &PresentationMap) -> gmeow_errors::Result<()> {
    if !Arc::ptr_eq(&left.source, &right.source) {
        return Err(error("pushout requires one exact common apex publication"));
    }
    if !left.is_embedding() || !right.is_embedding() {
        return Err(error("pushout span requires typed embeddings"));
    }
    Ok(())
}

fn root(parents: &mut [usize], mut index: usize) -> usize {
    while parents[index] != index {
        parents[index] = parents[parents[index]];
        index = parents[index];
    }
    index
}
