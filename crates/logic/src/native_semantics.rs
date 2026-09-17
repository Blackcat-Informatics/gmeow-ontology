// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Role-sensitive native interpretation of the declared grounding vocabulary.
//!
//! This is an execution contract, not an RDF rewrite. Stored terms and positive
//! premises remain exact. Only an operator position, or a constant in its declared
//! marker position, can use the paired presentation spelling. Variables always
//! bind the original term, including when that term later occurs as ordinary data.

use gmeow_logic_compile::ir::{ContextualScope, LogicModality, LogicProgram};

/// The selected interpretation is part of preparation and execution identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum SemanticVocabulary {
    /// Exact predicate and term identity, used by ordinary relational templates.
    #[default]
    Exact,
    /// Canonical logic operators and their explicitly declared grounding views.
    GroundedLogicV1,
}

const INSTANCE: &str = "https://blackcatinformatics.ca/logic/instanceOf";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

#[derive(Clone, Copy)]
struct SymbolPair {
    canonical: &'static str,
    alternate: &'static str,
}

struct Resolver {
    predicates: std::collections::BTreeMap<&'static str, SymbolPair>,
    markers: std::collections::BTreeMap<&'static str, SymbolPair>,
}

fn resolver() -> &'static Resolver {
    static RESOLVER: std::sync::LazyLock<Resolver> = std::sync::LazyLock::new(|| {
        let mut result = Resolver {
            predicates: Default::default(),
            markers: Default::default(),
        };
        let insert = |map: &mut std::collections::BTreeMap<_, _>, canonical, projected| {
            map.insert(
                canonical,
                SymbolPair {
                    canonical,
                    alternate: projected,
                },
            );
            map.insert(
                projected,
                SymbolPair {
                    canonical,
                    alternate: canonical,
                },
            );
        };
        insert(&mut result.predicates, INSTANCE, TYPE);
        for &(canonical, projected) in crate::reason::calculus_vocabulary() {
            if gmeow_ns::owl_view_of_predicate(canonical) == Some(projected) {
                insert(&mut result.predicates, canonical, projected);
            }
            if gmeow_ns::owl_view_of_type_marker(canonical) == Some(projected) {
                insert(&mut result.markers, canonical, projected);
            }
        }
        result
    });
    &RESOLVER
}

impl SemanticVocabulary {
    /// Canonical operator key. No namespace substitution or data-term aliasing.
    pub(crate) fn predicate(self, iri: &str) -> &str {
        if self == Self::Exact {
            return iri;
        }
        resolver()
            .predicates
            .get(iri)
            .map_or(iri, |pair| pair.canonical)
    }

    /// At most one other declared spelling; the original spelling is never lost.
    pub(crate) fn alternate_predicate(self, iri: &str) -> Option<&'static str> {
        if self == Self::Exact {
            return None;
        }
        resolver().predicates.get(iri).map(|pair| pair.alternate)
    }

    /// Constant object markers only. Bound variables and quoted/data occurrences
    /// never call this operation. Class-valued positions admit Thing/Nothing only.
    pub(crate) fn alternate_marker(self, predicate: &str, iri: &str) -> Option<&'static str> {
        if self == Self::Exact {
            return None;
        }
        let pair = resolver().markers.get(iri)?;
        let type_position = self.predicate(predicate) == INSTANCE;
        let class_position = gmeow_ns::is_class_position_predicate(predicate)
            && matches!(
                pair.canonical,
                "https://blackcatinformatics.ca/logic/Thing"
                    | "https://blackcatinformatics.ca/logic/Nothing"
            );
        (type_position || class_position).then_some(pair.alternate)
    }

    /// Deliberately coarser symbol quotient for termination analysis only. Collapsing
    /// the finite declared spelling pairs can add joins, never remove a concrete
    /// firing. This quotient must never be used to rewrite execution bindings.
    pub(crate) fn analysis_symbol(self, iri: &str) -> &str {
        let predicate = self.predicate(iri);
        if predicate != iri || self == Self::Exact {
            return predicate;
        }
        resolver()
            .markers
            .get(iri)
            .map_or(iri, |pair| pair.canonical)
    }

    pub(crate) fn abstract_term(self, term: &mut crate::rule_ir::EvalTerm) {
        match term {
            crate::rule_ir::EvalTerm::ConstNamed(iri)
            | crate::rule_ir::EvalTerm::ConstLit(purrdf::TermValue::Iri(iri)) => {
                *iri = self.analysis_symbol(iri).to_owned();
            }
            _ => {}
        }
    }

    pub(crate) fn abstract_atom(self, atom: &mut crate::rule_ir::EvalAtom) {
        atom.predicate = self.predicate(&atom.predicate).to_owned();
        self.abstract_term(&mut atom.subject);
        self.abstract_term(&mut atom.object);
    }

    /// Conservative vocabulary inventory for effect selection, never a term rewrite.
    pub(crate) fn possible_spellings(self, iri: &str) -> impl Iterator<Item = &str> {
        std::iter::once(iri)
            .chain(self.alternate_predicate(iri))
            .chain(self.alternate_marker(TYPE, iri))
    }
}

/// Source scope retained before the relational lowerer removes scope annotations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct SourceScope {
    pub(crate) owner: String,
    pub(crate) scope: ContextualScope,
}

/// This profile does not claim complete source-theory execution or coverage.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub(crate) enum SourceAdmissionProfile {
    WorldLocalTemplatesV1,
}

/// Bare-program execution is explicitly a world-local template operation. It does
/// not admit a CompiledTheory or promote source-owned axioms into global facts.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct ProgramAdmission {
    pub(crate) profile: SourceAdmissionProfile,
    pub(crate) source_iri: Option<String>,
    pub(crate) scopes: Vec<SourceScope>,
}

impl ProgramAdmission {
    pub(crate) fn capture(program: &LogicProgram) -> Self {
        let mut scopes = Vec::new();
        for (index, axiom) in program.axioms.iter().enumerate() {
            scopes.push(SourceScope {
                owner: format!("axiom/{index}"),
                scope: axiom.scope.clone(),
            });
        }
        for (index, rule) in program.rules.iter().enumerate() {
            scopes.push(SourceScope {
                owner: format!("rule/{index}"),
                scope: rule.scope.clone(),
            });
            scopes.push(SourceScope {
                owner: format!("rule/{index}/head"),
                scope: rule.head.scope.clone(),
            });
            for (body_index, atom) in rule.body.iter().enumerate() {
                scopes.push(SourceScope {
                    owner: format!("rule/{index}/body/{body_index}"),
                    scope: atom.scope.clone(),
                });
            }
        }
        Self {
            profile: SourceAdmissionProfile::WorldLocalTemplatesV1,
            source_iri: program.source_iri.clone(),
            scopes,
        }
    }

    /// Refuse semantics this profile cannot execute before starting any world.
    /// Confidence and provenance remain source evidence; neither selects a world.
    pub(crate) fn admit_world_local_template(&self) -> gmeow_errors::Result<()> {
        match self.profile {
            SourceAdmissionProfile::WorldLocalTemplatesV1 => {}
        }
        let unsupported: Vec<_> = self
            .scopes
            .iter()
            .filter(|source| {
                let scope = &source.scope;
                scope.standpoint.is_some()
                    || scope.time.is_some()
                    || scope.module.is_some()
                    || scope.modality != LogicModality::None
            })
            .map(|source| format!("{}: {:?}", source.owner, source.scope))
            .collect();
        if unsupported.is_empty() {
            return Ok(());
        }
        Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
            detail: format!(
                "world-local template admission refuses unexecuted source scope from {:?}: {}",
                self.source_iri,
                unsupported.join("; ")
            ),
        }))
    }
}
