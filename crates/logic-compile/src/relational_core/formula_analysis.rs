// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bounded, shared clausification for canonical carriers and native policy execution.
//! Keys retain the complete authored syntax: alpha equality cannot substitute another
//! source's variable names or residue text. No serialized formula or dataset is retained.

use std::collections::{BTreeSet, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

use super::{Formula, RcRule};

const MAX_ENTRIES: usize = 64;
const MAX_ENTRY_BYTES: usize = 256 * 1024;
const MAX_RULES: usize = 128;

/// The complete result for one source formula, including every unsupported clause.
#[derive(Debug, serde::Serialize)]
pub struct FormulaAnalysis {
    pub rules: Vec<RcRule>,
    pub residue: Vec<String>,
}

/// Stream exact structural serialization into a digest and a bounded byte counter.
/// The bytes are neither stored nor parsed, and no semantic normalization occurs here.
#[derive(Default)]
struct Digest {
    hash: blake3::Hasher,
    bytes: usize,
}

impl std::io::Write for Digest {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.hash.update(bytes);
        self.bytes = self.bytes.saturating_add(bytes.len());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Cached {
    analysis: Arc<FormulaAnalysis>,
    retained: bool,
}

type Entry = Arc<OnceLock<Cached>>;
type Cache = VecDeque<([u8; 32], Entry)>;

fn compute(formula: &Formula) -> Arc<FormulaAnalysis> {
    let mut rules = Vec::new();
    let mut residue = BTreeSet::new();
    let normalized = super::skolemize(super::nnf(formula));
    super::lower_formula_top(&normalized, formula, &mut rules, &mut residue);
    Arc::new(FormulaAnalysis {
        rules,
        residue: residue.into_iter().collect(),
    })
}

/// Share one complete clausification while its bounded entry remains resident.
/// Concurrent users of the same entry wait on its initializer; different formulas
/// remain independent. Eviction or an oversized result causes exact recomputation.
pub fn analyze(formula: &Formula) -> Arc<FormulaAnalysis> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    analyze_cached(formula, CACHE.get_or_init(|| Mutex::new(Cache::new())))
}

fn analyze_cached(formula: &Formula, cache: &Mutex<Cache>) -> Arc<FormulaAnalysis> {
    let mut source = Digest::default();
    source.hash.update(b"gmeow-formula-analysis-v1\0");
    serde_json::to_writer(&mut source, formula).expect("typed formula digest is infallible");
    if source.bytes > MAX_ENTRY_BYTES {
        return compute(formula);
    }
    let key = *source.hash.finalize().as_bytes();
    let entry = {
        let mut cache = cache.lock().expect("formula analysis cache poisoned");
        if let Some(index) = cache.iter().position(|(candidate, _)| candidate == &key) {
            let pair = cache.remove(index).expect("located cache entry");
            let entry = Arc::clone(&pair.1);
            cache.push_back(pair);
            entry
        } else {
            if cache.len() == MAX_ENTRIES {
                cache.pop_front();
            }
            let entry = Arc::new(OnceLock::new());
            cache.push_back((key, Arc::clone(&entry)));
            entry
        }
    };
    let result = entry.get_or_init(|| {
        let analysis = compute(formula);
        let mut payload = Digest::default();
        serde_json::to_writer(&mut payload, analysis.as_ref())
            .expect("typed analysis sizing is infallible");
        let retained = payload.bytes <= MAX_ENTRY_BYTES && analysis.rules.len() <= MAX_RULES;
        Cached { analysis, retained }
    });
    if !result.retained {
        cache
            .lock()
            .expect("formula analysis cache poisoned")
            .retain(|(_, candidate)| !Arc::ptr_eq(candidate, &entry));
    }
    Arc::clone(&result.analysis)
}

#[path = "formula_analysis.tests.rs"]
#[cfg(test)]
mod tests;
