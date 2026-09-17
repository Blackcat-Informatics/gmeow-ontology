// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-owned native existential definitions. The selected immutable-definition
//! profile retains the original record grammar and admits no runtime grammar writer.

use crate::physical::StatementPattern;
use crate::physical::{ExistentialRule, SourceExistentialRule, WitnessPolicy};
use crate::rule_ir::{EvalAtom, EvalTerm, Fact, FactKey};
use purrdf::TermValue;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

pub(crate) const PROFILE: &str = "native-edb-immutable-definitions-v1";
const NS: &str = "https://blackcatinformatics.ca/gmeow/logic/existential#";
const RULE_CLASS: &str = "https://blackcatinformatics.ca/gmeow/logic/existential#ExistentialRule";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const INSTANCE: &str = "https://blackcatinformatics.ca/logic/instanceOf";
const STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

fn vocabulary(local: &str) -> String {
    format!("{NS}{local}")
}

pub(crate) fn refusal(source: &str, world: &str, detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::NativeCoverage {
        profile: PROFILE.to_owned(),
        source: source.to_owned(),
        world: world.to_owned(),
        detail: detail.into(),
    })
}

/// Exact definition patterns. Ordinary class inference is not a grammar mutation:
/// only the exact declaration marker, or an actual record field, is protected.
pub(crate) fn grammar_patterns() -> Vec<StatementPattern> {
    let mut patterns: Vec<_> = ["body", "head", "s", "p", "o"]
        .into_iter()
        .map(|local| StatementPattern::relation(Some(&vocabulary(local)), None))
        .collect();
    patterns.push(StatementPattern::relation(Some(TYPE), Some(RULE_CLASS)));
    patterns.push(StatementPattern::relation(Some(INSTANCE), Some(RULE_CLASS)));
    patterns
}

fn grammar(predicate: &str, object: &TermValue) -> bool {
    matches!(predicate, TYPE | INSTANCE) && object.as_iri() == Some(RULE_CLASS)
        || predicate
            .strip_prefix(NS)
            .is_some_and(|local| matches!(local, "body" | "head" | "s" | "p" | "o"))
}

/// Collected during the native input traversal, before any term lowering. Empty
/// source graphs retain ownership but never acquire implicit rule declarations.
#[derive(Clone, Default)]
pub(crate) struct Collector {
    worlds: BTreeMap<String, (Option<TermValue>, BTreeMap<FactKey, Fact>)>,
}

impl Collector {
    pub(super) fn observe(
        &mut self,
        world: &str,
        graph: &Option<TermValue>,
        subject: &TermValue,
        predicate: &str,
        object: &TermValue,
    ) {
        if !grammar(predicate, object) {
            return;
        }
        let statement = Fact {
            subject: subject.clone(),
            predicate: predicate.to_owned(),
            object: object.clone(),
        };
        self.worlds
            .entry(world.to_owned())
            .or_insert_with(|| (graph.clone(), BTreeMap::new()))
            .1
            .entry(statement.key())
            .or_insert(statement);
    }

    pub(crate) fn prepare(self) -> gmeow_errors::Result<PreparedSources> {
        let identity = crate::physical::metadata_identity(PROFILE, &self.worlds);
        if let Some(prepared) = source_cache()
            .lock()
            .map_err(|_| {
                refusal(
                    "<source cache>",
                    "<selected worlds>",
                    "source preparation cache poisoned",
                )
            })?
            .get(&identity)
        {
            return Ok(prepared);
        }
        let mut rules = Vec::new();
        let marker = TermValue::iri(RULE_CLASS);
        for (world, (graph, statements)) in self.worlds {
            let mut nodes: BTreeMap<TermValue, BTreeMap<String, BTreeSet<TermValue>>> =
                BTreeMap::new();
            for statement in statements.values() {
                nodes
                    .entry(statement.subject.clone())
                    .or_default()
                    .entry(statement.predicate.clone())
                    .or_default()
                    .insert(statement.object.clone());
                let source = crate::provenance::term_display(&statement.subject);
                if matches!(statement.predicate.strip_prefix(NS), Some("body" | "head"))
                    && !matches!(
                        statement.object,
                        TermValue::Iri(_) | TermValue::Blank { .. }
                    )
                {
                    return Err(refusal(
                        &source,
                        &world,
                        format!(
                            "logicx:{} value is not a resource",
                            statement.predicate.strip_prefix(NS).expect("field")
                        ),
                    ));
                }
            }
            // Identical asserted occurrences are one RDF statement; conflicting
            // slot values are malformed authoring and cannot choose a winner.
            for (node, fields) in &nodes {
                for local in ["s", "p", "o"] {
                    if fields
                        .get(&vocabulary(local))
                        .is_some_and(|values| values.len() > 1)
                    {
                        return Err(refusal(
                            &crate::provenance::term_display(node),
                            &world,
                            format!("more than one logicx:{local}"),
                        ));
                    }
                }
            }
            for (node, fields) in &nodes {
                if ![TYPE, INSTANCE]
                    .into_iter()
                    .any(|p| fields.get(p).is_some_and(|values| values.contains(&marker)))
                {
                    continue;
                }
                let source = crate::provenance::term_display(node);
                let rule_iri = super::subject_iri(&crate::facts::skolemize(node))?;
                let mut owned = BTreeSet::from([node.clone()]);
                let mut atoms = |local: &str| -> gmeow_errors::Result<Vec<EvalAtom>> {
                    let mut result = Vec::new();
                    for atom in fields.get(&vocabulary(local)).into_iter().flatten() {
                        owned.insert(atom.clone());
                        let atom_name = crate::provenance::term_display(atom);
                        let slot = |local: &str| -> gmeow_errors::Result<&TermValue> {
                            nodes
                                .get(atom)
                                .and_then(|fields| fields.get(&vocabulary(local)))
                                .and_then(|values| values.first())
                                .ok_or_else(|| {
                                    refusal(
                                        &source,
                                        &world,
                                        format!("atom {atom_name} is missing its logicx:{local}"),
                                    )
                                })
                        };
                        let subject = source_term(slot("s")?, true, &source, &world)?;
                        let TermValue::Iri(predicate) = slot("p")? else {
                            return Err(refusal(
                                &source,
                                &world,
                                format!("atom {atom_name} has non-IRI logicx:p"),
                            ));
                        };
                        let object = source_term(slot("o")?, false, &source, &world)?;
                        result.push(EvalAtom::positive(subject, predicate, object));
                    }
                    if result.is_empty() {
                        return Err(refusal(
                            &source,
                            &world,
                            format!(
                                "both body and head must be non-empty; no logicx:{local} atoms"
                            ),
                        ));
                    }
                    Ok(result)
                };
                let body = atoms("body")?;
                let head = atoms("head")?;
                let evidence: Vec<_> = statements
                    .values()
                    .filter(|statement| owned.contains(&statement.subject))
                    .cloned()
                    .collect();
                let premises = evidence.iter().map(execution_fact).collect();
                rules.push(SourceExistentialRule {
                    profile: PROFILE.to_owned(),
                    world: world.clone(),
                    graph: graph.clone(),
                    source: node.clone(),
                    evidence,
                    premises,
                    rule: ExistentialRule {
                        numeric: Vec::new(),
                        rule_iri,
                        body,
                        head,
                        distinct: Vec::new(),
                        witness_frontier: None,
                        witness_policy: WitnessPolicy::FrontierSkolem,
                    },
                });
            }
        }
        let prepared = PreparedSources {
            rules: Arc::new(rules),
            identity,
        };
        if prepared.cacheable() {
            source_cache()
                .lock()
                .map_err(|_| {
                    refusal(
                        "<source cache>",
                        "<selected worlds>",
                        "source preparation cache poisoned",
                    )
                })?
                .insert(prepared.clone());
        }
        Ok(prepared)
    }
}

fn source_term(
    value: &TermValue,
    subject: bool,
    source: &str,
    world: &str,
) -> gmeow_errors::Result<EvalTerm> {
    if let TermValue::Literal {
        lexical_form,
        datatype,
        language: None,
        direction: None,
    } = value
        && datatype == STRING
        && lexical_form.starts_with('?')
    {
        if lexical_form.len() == 1 || lexical_form.chars().any(char::is_whitespace) {
            return Err(refusal(source, world, "malformed variable carrier"));
        }
        return Ok(EvalTerm::var(lexical_form));
    }
    if subject && !matches!(value, TermValue::Iri(_) | TermValue::Blank { .. }) {
        return Err(refusal(
            source,
            world,
            "logicx:s constant must be an RDF resource",
        ));
    }
    Ok(match crate::facts::skolemize(value).as_ref() {
        TermValue::Iri(iri) => EvalTerm::named(iri),
        other => EvalTerm::ConstLit(other.clone()),
    })
}

fn execution_fact(source: &Fact) -> Fact {
    Fact {
        subject: crate::facts::skolemize(&source.subject).into_owned(),
        predicate: source.predicate.clone(),
        object: crate::facts::skolemize(&source.object).into_owned(),
    }
}

/// Only compact source definitions are retained. No dataset or execution result is
/// cached; all original graph/node/value identities participate in this digest.
#[derive(Debug, Clone)]
pub(crate) struct PreparedSources {
    pub(crate) rules: Arc<Vec<SourceExistentialRule>>,
    identity: [u8; 32],
}

impl PreparedSources {
    fn cacheable(&self) -> bool {
        struct Limit(usize);
        impl std::fmt::Write for Limit {
            fn write_str(&mut self, value: &str) -> std::fmt::Result {
                self.0 = self.0.saturating_add(value.len());
                if self.0 > 1024 * 1024 {
                    Err(std::fmt::Error)
                } else {
                    Ok(())
                }
            }
        }
        self.rules.len() <= 1024
            && self
                .rules
                .iter()
                .map(|source| source.rule.body.len() + source.rule.head.len())
                .sum::<usize>()
                <= 16 * 1024
            && write!(Limit(0), "{:?}", self.rules).is_ok()
    }

    pub(crate) fn contract(&self) -> crate::physical::DefinitionContract {
        crate::physical::DefinitionContract {
            profile: PROFILE.to_owned(),
            patterns: grammar_patterns(),
            owners: self
                .rules
                .iter()
                .map(|source| {
                    (
                        crate::provenance::term_display(&source.source),
                        source.world.clone(),
                    )
                })
                .collect(),
        }
    }
    pub(crate) fn identity(&self) -> [u8; 32] {
        self.identity
    }
}

/// A bounded syntactic preparation cache. Admission and source effects are always
/// checked against the selected complete native input; a cache hit certifies no
/// execution, scope, completion or rewrite.
#[derive(Default)]
struct SourceCache {
    entries: VecDeque<PreparedSources>,
}
impl SourceCache {
    fn get(&mut self, identity: &[u8; 32]) -> Option<PreparedSources> {
        let index = self
            .entries
            .iter()
            .position(|entry| &entry.identity == identity)?;
        let entry = self.entries.remove(index)?;
        self.entries.push_back(entry.clone());
        Some(entry)
    }
    fn insert(&mut self, prepared: PreparedSources) {
        if self.get(&prepared.identity).is_some() {
            return;
        }
        if self.entries.len() == 16 {
            self.entries.pop_front();
        }
        self.entries.push_back(prepared);
    }
}
fn source_cache() -> &'static Mutex<SourceCache> {
    static CACHE: std::sync::LazyLock<Mutex<SourceCache>> =
        std::sync::LazyLock::new(|| Mutex::new(SourceCache::default()));
    &CACHE
}

#[cfg(test)]
mod tests;
