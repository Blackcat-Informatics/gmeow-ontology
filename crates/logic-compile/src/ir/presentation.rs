// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Portable presentation declarations. Lowering is not a merge or rewrite proof.
//! Bodies and named records are stored once; references contain no local TermIds.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::{Formula, LogicModality, PreservationKind, ReasoningContract};

/// Complete native identity at a persistence boundary, including blank scope.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct PresentationTerm(#[serde(with = "crate::term_serde")] pub purrdf::TermValue);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct PresentationSourceRef {
    pub term: PresentationTerm,
    pub graph: Option<PresentationTerm>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum PresentationSymbolRole {
    Individual,
    Relation(usize),
    VariadicRelation,
    Function(usize),
    VariadicFunction,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum PresentationSign {
    Positive,
    Negative,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum PresentationSentenceKind {
    Axiom,
    Caveat,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PresentationContextIr {
    pub world: String,
    pub standpoint: Option<String>,
    pub time: Option<String>,
    pub path: Option<String>,
    pub module: Option<String>,
    pub modality: LogicModality,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PresentationSymbolIr {
    pub context: String,
    pub names: BTreeSet<PresentationTerm>,
    pub roles: BTreeSet<PresentationSymbolRole>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PresentationSentenceIr {
    pub formula: String,
    pub context: String,
    pub bindings: BTreeMap<String, String>,
    pub sign: PresentationSign,
    pub kind: PresentationSentenceKind,
    pub evidence: String,
}

/// The semantic evidence definition. Its original provenance/complement remains
/// in the separately authenticated native source publication, not copied here.
/// A detached declaration cannot fabricate that publication or an execution proof.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PresentationEvidenceIr {
    pub origin: PresentationTerm,
    pub preservation: BTreeSet<PreservationKind>,
    pub unsupported_constructs: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FinitePresentationIr {
    pub contract: String,
    pub contexts: BTreeSet<String>,
    pub symbols: BTreeSet<String>,
    pub sentences: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PresentationMapIr {
    pub source: String,
    pub target: String,
    pub images: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PresentationMergeIr {
    pub left: String,
    pub right: String,
}

/// Complete selected definitions, with shared formulas and no execution witnesses.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct PresentationDefinitions {
    pub formulas: BTreeMap<String, Arc<Formula>>,
    pub contracts: BTreeMap<String, Arc<ReasoningContract>>,
    pub contexts: BTreeMap<String, PresentationContextIr>,
    pub symbols: BTreeMap<String, PresentationSymbolIr>,
    pub sentences: BTreeMap<String, PresentationSentenceIr>,
    pub evidence: BTreeMap<String, PresentationEvidenceIr>,
    pub presentations: BTreeMap<String, FinitePresentationIr>,
    pub maps: BTreeMap<String, PresentationMapIr>,
    pub merges: BTreeMap<String, PresentationMergeIr>,
}

/// A selected source operation cannot disappear when its lowering fails.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum PresentationProgramIr {
    #[default]
    Empty,
    Lowered(Arc<PresentationDefinitions>),
    Refused {
        roots: BTreeSet<PresentationSourceRef>,
        focus: Option<String>,
        detail: String,
    },
}

/// Explicit framing, shared across every declaration field. This is an identity
/// operation over typed values, not JSON/RDF serialization of an intermediate.
struct Key(String);

impl Key {
    fn field(&mut self, value: &str) {
        use std::fmt::Write as _;
        write!(self.0, "{}:{value}", value.len()).expect("writing a String");
    }
    fn optional(&mut self, value: Option<&str>) {
        match value {
            Some(value) => {
                self.field("some");
                self.field(value);
            }
            None => self.field("none"),
        }
    }
    fn names<'a>(&mut self, values: impl ExactSizeIterator<Item = &'a String>) {
        self.field(&values.len().to_string());
        for value in values {
            self.field(value);
        }
    }
    fn map(&mut self, values: &BTreeMap<String, String>) {
        self.field(&values.len().to_string());
        for (source, target) in values {
            self.field(source);
            self.field(target);
        }
    }
    fn term(&mut self, value: &PresentationTerm) {
        self.field(&gmeow_term_arena::engine::native_term_key(&value.0));
    }
}

impl PresentationProgramIr {
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn canonical_key(&self) -> String {
        let mut key = Key("presentation-program-v1;".into());
        match self {
            Self::Empty => key.field("empty"),
            Self::Refused {
                roots,
                focus,
                detail,
            } => {
                key.field("refused");
                key.field(&roots.len().to_string());
                for root in roots {
                    key.term(&root.term);
                    match &root.graph {
                        Some(graph) => {
                            key.field("named");
                            key.term(graph);
                        }
                        None => key.field("default"),
                    }
                }
                key.optional(focus.as_deref());
                key.field(detail);
            }
            Self::Lowered(program) => {
                key.field("lowered");
                program.write_key(&mut key);
            }
        }
        key.0
    }
}

impl PresentationDefinitions {
    fn write_key(&self, key: &mut Key) {
        key.field("formulas");
        key.field(&self.formulas.len().to_string());
        for (name, formula) in &self.formulas {
            key.field(name);
            key.field(formula.content_key().as_str());
        }
        key.field("contracts");
        key.field(&self.contracts.len().to_string());
        for (name, contract) in &self.contracts {
            key.field(name);
            key.field(&contract.content_key());
        }
        key.field("contexts");
        key.field(&self.contexts.len().to_string());
        for (name, context) in &self.contexts {
            key.field(name);
            key.field(&context.world);
            for value in [
                &context.standpoint,
                &context.time,
                &context.path,
                &context.module,
            ] {
                key.optional(value.as_deref());
            }
            key.field(context.modality.as_str());
        }
        key.field("symbols");
        key.field(&self.symbols.len().to_string());
        for (name, symbol) in &self.symbols {
            key.field(name);
            key.field(&symbol.context);
            key.field(&symbol.names.len().to_string());
            for name in &symbol.names {
                key.term(name);
            }
            key.field(&symbol.roles.len().to_string());
            for role in &symbol.roles {
                match role {
                    PresentationSymbolRole::Individual => key.field("individual"),
                    PresentationSymbolRole::Relation(arity) => {
                        key.field("relation");
                        key.field(&arity.to_string());
                    }
                    PresentationSymbolRole::Function(arity) => {
                        key.field("function");
                        key.field(&arity.to_string());
                    }
                    PresentationSymbolRole::VariadicRelation => key.field("variadic-relation"),
                    PresentationSymbolRole::VariadicFunction => key.field("variadic-function"),
                }
            }
        }
        key.field("sentences");
        key.field(&self.sentences.len().to_string());
        for (name, sentence) in &self.sentences {
            key.field(name);
            key.field(&sentence.formula);
            key.field(&sentence.context);
            key.map(&sentence.bindings);
            key.field(match sentence.sign {
                PresentationSign::Positive => "positive",
                PresentationSign::Negative => "negative",
            });
            key.field(match sentence.kind {
                PresentationSentenceKind::Axiom => "axiom",
                PresentationSentenceKind::Caveat => "caveat",
            });
            key.field(&sentence.evidence);
        }
        key.field("evidence");
        key.field(&self.evidence.len().to_string());
        for (name, evidence) in &self.evidence {
            key.field(name);
            key.term(&evidence.origin);
            key.field(&evidence.preservation.len().to_string());
            for kind in &evidence.preservation {
                key.field(kind.as_str());
            }
            key.names(evidence.unsupported_constructs.iter());
        }
        key.field("presentations");
        key.field(&self.presentations.len().to_string());
        for (name, presentation) in &self.presentations {
            key.field(name);
            key.field(&presentation.contract);
            for names in [
                &presentation.contexts,
                &presentation.symbols,
                &presentation.sentences,
            ] {
                key.names(names.iter());
            }
        }
        key.field("maps");
        key.field(&self.maps.len().to_string());
        for (name, map) in &self.maps {
            key.field(name);
            key.field(&map.source);
            key.field(&map.target);
            key.map(&map.images);
        }
        key.field("merges");
        key.field(&self.merges.len().to_string());
        for (name, merge) in &self.merges {
            key.field(name);
            key.field(&merge.left);
            key.field(&merge.right);
        }
    }
}
