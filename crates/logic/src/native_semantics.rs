// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Role-sensitive native interpretation of the declared grounding vocabulary.
//!
//! This is an execution contract, not an RDF rewrite. Stored terms and positive
//! premises remain exact. Only an operator position, or a constant in its declared
//! marker position, can use the paired presentation spelling. Variables always
//! bind the original term, including when that term later occurs as ordinary data.

use gmeow_logic_compile::ir::{ContextualScope, LogicProgram, ScopeMembership, TheorySelection};

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

    /// Conservative vocabulary inventory for effect selection, never a term rewrite.
    pub(crate) fn possible_spellings(self, iri: &str) -> impl Iterator<Item = &str> {
        std::iter::once(iri)
            .chain(self.alternate_predicate(iri))
            .chain(self.alternate_marker(TYPE, iri))
    }
}

/// The construct of the source program a retained scope belongs to.
///
/// Its text form (`axiom/3`, `rule/0/body/1`) is the one shown in a refusal and the
/// one committed to by execution identity, so the two never diverge.
#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(into = "String", try_from = "String")]
pub(crate) enum SourceOwner {
    Axiom(usize),
    Rule(usize),
    Head(usize),
    Body(usize, usize),
}

/// The debug form is the quoted text form, exactly what the owner printed when it
/// was a string. It MUST NOT change: the witness contract hashes the debug text of
/// the whole program admission, so any other rendering would move the witness
/// contract, the joint input identity and every minted witness IRI of every
/// existing program.
impl std::fmt::Debug for SourceOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.to_string(), f)
    }
}

impl std::fmt::Display for SourceOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Axiom(index) => write!(f, "axiom/{index}"),
            Self::Rule(index) => write!(f, "rule/{index}"),
            Self::Head(index) => write!(f, "rule/{index}/head"),
            Self::Body(index, atom) => write!(f, "rule/{index}/body/{atom}"),
        }
    }
}

impl From<SourceOwner> for String {
    fn from(owner: SourceOwner) -> Self {
        owner.to_string()
    }
}

/// A serialized owner that is not one of the four text forms.
#[derive(Debug)]
pub(crate) struct UnknownSourceOwner(String);

impl std::fmt::Display for UnknownSourceOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown source scope owner {:?}", self.0)
    }
}

impl std::error::Error for UnknownSourceOwner {}

impl TryFrom<String> for SourceOwner {
    type Error = UnknownSourceOwner;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let index = |part: &str| part.parse::<usize>().ok();
        let parts: Vec<&str> = text.split('/').collect();
        let owner = match parts[..] {
            ["axiom", axiom] => index(axiom).map(Self::Axiom),
            ["rule", rule] => index(rule).map(Self::Rule),
            ["rule", rule, "head"] => index(rule).map(Self::Head),
            ["rule", rule, "body", atom] => index(rule)
                .zip(index(atom))
                .map(|(rule, atom)| Self::Body(rule, atom)),
            _ => None,
        };
        owner.ok_or(UnknownSourceOwner(text))
    }
}

/// Source scope retained before the relational lowerer removes scope annotations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct SourceScope {
    pub(crate) owner: SourceOwner,
    pub(crate) scope: ContextualScope,
}

/// Neither profile claims complete source-theory execution or coverage.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) enum SourceAdmissionProfile {
    /// Bare-program execution: the program's rules and formulas run as world-local
    /// templates and no source-owned axiom is read. Any context coordinate on any
    /// retained scope refuses the program.
    WorldLocalTemplatesV1,
    /// Execution of one selected theory. Every scoped axiom is accounted for by the
    /// claim ingress ledger of the same selection before execution: read as a fact
    /// of the selected world, or named as not read. An axiom's scope is therefore no
    /// reason to refuse; a rule, head or body-atom scope outside the theory still is.
    SelectedTheoryV1 { theory: TheorySelection },
}

/// Bare-program execution is explicitly a world-local template operation. It does
/// not admit a CompiledTheory or promote source-owned axioms into global facts.
/// The selected-theory profile is the one place a scoped source axiom can become a
/// fact, and only through its claim ingress ledger.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct ProgramAdmission {
    pub(crate) profile: SourceAdmissionProfile,
    pub(crate) source_iri: Option<String>,
    pub(crate) scopes: Vec<SourceScope>,
}

impl ProgramAdmission {
    pub(crate) fn capture(program: &LogicProgram, profile: SourceAdmissionProfile) -> Self {
        let mut scopes = Vec::new();
        for (index, axiom) in program.axioms.iter().enumerate() {
            scopes.push(SourceScope {
                owner: SourceOwner::Axiom(index),
                scope: axiom.scope.clone(),
            });
        }
        for (index, rule) in program.rules.iter().enumerate() {
            scopes.push(SourceScope {
                owner: SourceOwner::Rule(index),
                scope: rule.scope.clone(),
            });
            scopes.push(SourceScope {
                owner: SourceOwner::Head(index),
                scope: rule.head.scope.clone(),
            });
            for (body_index, atom) in rule.body.iter().enumerate() {
                scopes.push(SourceScope {
                    owner: SourceOwner::Body(index, body_index),
                    scope: atom.scope.clone(),
                });
            }
        }
        Self {
            profile,
            source_iri: program.source_iri.clone(),
            scopes,
        }
    }

    /// Refuse semantics this profile cannot execute before starting any world.
    /// Confidence and provenance remain source evidence; neither selects a world.
    pub(crate) fn admit(&self) -> gmeow_errors::Result<()> {
        let (name, unsupported) = match &self.profile {
            SourceAdmissionProfile::WorldLocalTemplatesV1 => (
                "world-local template",
                self.outside(|source| !source.scope.selects_no_context()),
            ),
            SourceAdmissionProfile::SelectedTheoryV1 { theory } => (
                "selected-theory",
                self.outside(|source| {
                    !matches!(source.owner, SourceOwner::Axiom(_))
                        && !matches!(
                            theory.membership(&source.scope),
                            ScopeMembership::InTheory { .. }
                        )
                }),
            ),
        };
        if unsupported.is_empty() {
            return Ok(());
        }
        Err(gmeow_errors::Diag::of_kind(crate::error::NativeRefusal {
            reason: format!(
                "{name} admission refuses unexecuted source scope from {:?}",
                self.source_iri
            ),
            gaps: unsupported,
        }))
    }

    fn outside(&self, refused: impl Fn(&SourceScope) -> bool) -> Vec<String> {
        self.scopes
            .iter()
            .filter(|source| refused(source))
            .map(|source| format!("{}: {:?}", source.owner, source.scope))
            .collect()
    }
}
