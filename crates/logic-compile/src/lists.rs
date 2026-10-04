// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The single RDF-list lowering shared by every list-valued `logic:` constructor.
//!
//! `logic:oneOf`, `logic:unionOf`, `logic:intersectionOf`, `logic:disjointUnionOf`,
//! `logic:members`, `logic:hasKey` and `logic:propertyChainAxiom` all take an RDF list
//! object. The IR carries every such list in exactly one encoding, for named and
//! anonymous owners alike:
//!
//! ```text
//! owner <constructor> cell₀ .
//! cellᵢ rdf:first memberᵢ ; rdf:rest cellᵢ₊₁ .      # the last cell's rest is rdf:nil
//! ```
//!
//! A blank source cell is minted as the deterministic, content-addressed IRI
//! `logic:list/<sha256_12(members)>/cell/<nnnn>` ([`list_base`] + [`cell_iri`]); the OWL
//! projection mints its own synthesized lists with the same [`cell_iri`] scheme, so the
//! two surfaces agree on one cell spelling. An IRI source cell is an authored name and is
//! kept verbatim (a pinned or content-addressed list must stay addressable), which is also
//! what makes the canonical RDF 1.2 projection re-lift to the identical IR.
//!
//! Set-valued constructors (every one except `propertyChainAxiom`) are sorted and
//! de-duplicated before minting, so authored order cannot change the cell identity and an
//! `owl:`- and `logic:`-authored list collide. `propertyChainAxiom` composes left to right,
//! so its authored order is the meaning and is preserved verbatim.
//!
//! A list that is not a finite, nil-terminated, non-branching chain of resource cells, or
//! that carries a member its constructor cannot admit, is a [`ListDefect`]. Callers turn a
//! defect into a diagnostic and drop the whole owning statement; no partial list and no
//! dangling constructor edge is ever emitted.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::RdfDataset;

use crate::frontend::{Diagnostic, Severity, SourceNode};
use crate::graphutil::{
    Node, Subject, default_graph_pattern, iri_of, nn, node_of, objects, sha256_12, subject_id,
    term_as_subject,
};
use crate::ir::{AtomicTerm, LOGIC_NAMESPACE};
use crate::restriction::LiftedTriple;

/// `rdf:first`.
pub const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
/// `rdf:rest`.
pub const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
/// `rdf:nil`.
pub const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const LOGIC_INSTANCE_OF: &str = "https://blackcatinformatics.ca/logic/instanceOf";

/// IRI prefix of a skolemized anonymous enumeration (`[ logic:oneOf ( … ) ]`).
pub const ENUMERATION_PREFIX: &str = "https://blackcatinformatics.ca/logic/enumeration/";
/// IRI prefix of a skolemized anonymous boolean class expression
/// (`[ logic:unionOf ( … ) ]`, `[ logic:complementOf C ]`, …).
pub const CLASS_EXPRESSION_PREFIX: &str = "https://blackcatinformatics.ca/logic/class-expression/";
/// IRI prefix of a skolemized anonymous n-ary axiom resource
/// (`[ a logic:AllDisjointClasses ; logic:members ( … ) ]`).
pub const NARY_AXIOM_PREFIX: &str = "https://blackcatinformatics.ca/logic/axiom/";

/// Whether `iri` names a compiler-minted anonymous class expression or n-ary axiom
/// resource. Its `logic:` typing is part of its content, not vocabulary self-description.
pub fn is_minted_owner(iri: &str) -> bool {
    [
        ENUMERATION_PREFIX,
        CLASS_EXPRESSION_PREFIX,
        NARY_AXIOM_PREFIX,
    ]
    .iter()
    .any(|prefix| iri.starts_with(prefix))
}

/// Whether member order carries meaning for a constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListOrder {
    /// The list denotes a set: members are sorted and de-duplicated before minting.
    Set,
    /// The list is a sequence: authored order and repetition are preserved.
    Sequence,
}

/// Which member terms a constructor admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMembers {
    /// Classes, properties or individuals only.
    Resources,
    /// A nominal/data enumeration (or an unregistered list): literals are also members.
    ResourcesOrLiterals,
}

/// One list-valued `logic:` constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListConstructor {
    /// The `logic:` local name.
    pub local: &'static str,
    /// Member-order semantics.
    pub order: ListOrder,
    /// Admitted member kinds.
    pub members: ListMembers,
}

/// Every registered list-valued `logic:` constructor. Their objects MUST be lists.
pub const LIST_CONSTRUCTORS: &[ListConstructor] = &[
    ListConstructor {
        local: "oneOf",
        order: ListOrder::Set,
        members: ListMembers::ResourcesOrLiterals,
    },
    ListConstructor {
        local: "unionOf",
        order: ListOrder::Set,
        members: ListMembers::Resources,
    },
    ListConstructor {
        local: "intersectionOf",
        order: ListOrder::Set,
        members: ListMembers::Resources,
    },
    ListConstructor {
        local: "disjointUnionOf",
        order: ListOrder::Set,
        members: ListMembers::Resources,
    },
    ListConstructor {
        local: "members",
        order: ListOrder::Set,
        members: ListMembers::Resources,
    },
    ListConstructor {
        local: "hasKey",
        order: ListOrder::Set,
        members: ListMembers::Resources,
    },
    ListConstructor {
        local: "propertyChainAxiom",
        order: ListOrder::Sequence,
        members: ListMembers::Resources,
    },
];

/// The lowering applied to a list reached through an unregistered `logic:` predicate:
/// nothing is known about its meaning, so it is kept as authored.
pub const UNREGISTERED_LIST: ListConstructor = ListConstructor {
    local: "",
    order: ListOrder::Sequence,
    members: ListMembers::ResourcesOrLiterals,
};

/// The registered constructor spelled by `predicate`, if any.
pub fn constructor(predicate: &str) -> Option<&'static ListConstructor> {
    let local = predicate.strip_prefix(LOGIC_NAMESPACE)?;
    LIST_CONSTRUCTORS.iter().find(|ctor| ctor.local == local)
}

/// The content-addressed base of a minted list: a function of its member terms only.
pub fn list_base(members: &[AtomicTerm]) -> String {
    let key = crate::ir::atomic::frame("list", members.iter().map(AtomicTerm::key));
    format!("{LOGIC_NAMESPACE}list/{}", sha256_12(&key))
}

/// The minted IRI of cell `index` under `base` — the one cell spelling shared by the
/// IR lowering and the OWL projection's synthesized lists.
pub fn cell_iri(base: &str, index: usize) -> String {
    format!("{base}/cell/{index:04}")
}

/// Why a list could not be lowered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListDefect {
    /// The RDF list structure itself is not a finite, nil-terminated chain, or a member
    /// is of a kind the constructor cannot admit.
    Malformed(String),
    /// A blank member has no lifted class-expression identity to stand for it.
    UnresolvedMember(String),
}

impl ListDefect {
    /// The human-readable reason.
    pub fn reason(&self) -> &str {
        match self {
            Self::Malformed(why) | Self::UnresolvedMember(why) => why,
        }
    }
}

/// A walked list: each source cell paired with its `rdf:first` member, in order.
type WalkedCells = Vec<(Node, Node)>;

fn node_key(node: &Subject) -> String {
    match node {
        Subject::Iri(iri) => format!("I{iri}"),
        Subject::Blank { label, .. } => format!("B{label}"),
    }
}

/// Walk an `rdf:first`/`rdf:rest` chain from `head`. A literal cell, a cell without
/// exactly one `rdf:first` and one `rdf:rest`, a revisited cell, and an `rdf:nil`
/// carrying list fields are each a [`ListDefect::Malformed`], never a truncation.
pub(crate) fn walk(store: &RdfDataset, head: &Node) -> Result<WalkedCells, ListDefect> {
    let first = nn(RDF_FIRST);
    let rest = nn(RDF_REST);
    let malformed = |why: &str| Err(ListDefect::Malformed(why.to_owned()));
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut cursor = head.clone();
    loop {
        let Some(cell) = term_as_subject(&cursor) else {
            return malformed("a list cell is a literal, not a resource");
        };
        let firsts = objects(store, &cell, &first);
        let rests = objects(store, &cell, &rest);
        if matches!(&cell, Subject::Iri(iri) if iri == RDF_NIL) {
            if !firsts.is_empty() || !rests.is_empty() {
                return malformed("rdf:nil carries rdf:first/rdf:rest fields");
            }
            return Ok(out);
        }
        if !seen.insert(node_key(&cell)) {
            return malformed("the list is cyclic");
        }
        let member = match firsts.as_slice() {
            [] => return malformed("a list cell has no rdf:first"),
            [only] => only.clone(),
            _ => return malformed("a list cell branches: it has more than one rdf:first"),
        };
        let next = match rests.as_slice() {
            [] => return malformed("the list is not nil-terminated (a cell has no rdf:rest)"),
            [only] => only.clone(),
            _ => return malformed("a list cell branches: it has more than one rdf:rest"),
        };
        out.push((cursor, member));
        cursor = next;
    }
}

/// Whether `node` is the head of an RDF list: `rdf:nil`, or a resource carrying
/// `rdf:first` or `rdf:rest`.
pub(crate) fn is_list_head(store: &RdfDataset, node: &Node) -> bool {
    match term_as_subject(node) {
        Some(Subject::Iri(iri)) if iri == RDF_NIL => true,
        Some(cell) => {
            !objects(store, &cell, &nn(RDF_FIRST)).is_empty()
                || !objects(store, &cell, &nn(RDF_REST)).is_empty()
        }
        None => false,
    }
}

/// A list lowered to its single IR encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoweredList {
    /// The IRI of the head cell — the object of the owning constructor edge.
    pub head: String,
    /// The members in emitted order (sorted + de-duplicated for a set constructor whose
    /// cells were minted; authored order otherwise).
    pub members: Vec<AtomicTerm>,
    /// Every `cell rdf:first member` / `cell rdf:rest next` triple, head first.
    pub cells: Vec<(String, &'static str, AtomicTerm)>,
}

/// Lower the list at `head` for `ctor`. `resolve` maps a non-literal member node to its
/// IR term (an IRI stays an IRI; a blank member must resolve to the content-addressed
/// identity of a lifted anonymous expression or the lowering fails).
pub(crate) fn lower(
    store: &RdfDataset,
    ctor: &ListConstructor,
    head: &Node,
    resolve: &mut dyn FnMut(&Node) -> Result<AtomicTerm, ListDefect>,
) -> Result<LoweredList, ListDefect> {
    let walked = walk(store, head)?;
    if walked.is_empty() {
        return Err(ListDefect::Malformed("the list is empty".to_owned()));
    }
    let mut members = Vec::with_capacity(walked.len());
    for (_, member) in &walked {
        let term = match member {
            Node::Lit(literal) => {
                if ctor.members == ListMembers::Resources {
                    return Err(ListDefect::Malformed(format!(
                        "a literal member {:?} is not admitted in a logic:{} list",
                        literal.lexical_form, ctor.local
                    )));
                }
                AtomicTerm::Literal(literal.clone())
            }
            Node::Triple(_) => {
                return Err(ListDefect::Malformed(
                    "a quoted-triple list member requires typed formula lowering".to_owned(),
                ));
            }
            Node::Iri(_) | Node::Blank { .. } => resolve(member)?,
        };
        members.push(term);
    }
    let authored_cells = walked.iter().any(|(cell, _)| matches!(cell, Node::Iri(_)));
    if !authored_cells && ctor.order == ListOrder::Set {
        members.sort();
        members.dedup();
    }
    let base = list_base(&members);
    let names: Vec<String> = if authored_cells {
        walked
            .iter()
            .enumerate()
            .map(|(index, (cell, _))| match cell {
                Node::Iri(iri) => iri.clone(),
                _ => cell_iri(&base, index),
            })
            .collect()
    } else {
        (0..members.len())
            .map(|index| cell_iri(&base, index))
            .collect()
    };
    let mut cells = Vec::with_capacity(members.len() * 2);
    for (index, member) in members.iter().enumerate() {
        let next = names.get(index + 1).map_or(RDF_NIL, String::as_str);
        cells.push((names[index].clone(), RDF_FIRST, member.clone()));
        cells.push((
            names[index].clone(),
            RDF_REST,
            AtomicTerm::Iri(next.to_owned()),
        ));
    }
    Ok(LoweredList {
        head: names[0].clone(),
        members,
        cells,
    })
}

/// A member resolver over already-known anonymous identities: IRIs pass through, a blank
/// member resolves through `skolems` (blank label → lifted IRI) or is unresolved.
pub(crate) fn resolve_known<'a>(
    skolems: &'a BTreeMap<String, String>,
) -> impl FnMut(&Node) -> Result<AtomicTerm, ListDefect> + 'a {
    move |member| match member {
        Node::Iri(iri) => Ok(AtomicTerm::Iri(iri.clone())),
        Node::Blank { label, .. } => skolems
            .get(label)
            .map(|iri| AtomicTerm::Iri(iri.clone()))
            .ok_or_else(|| {
                ListDefect::UnresolvedMember(format!(
                    "anonymous member {label:?} has no lifted class-expression identity"
                ))
            }),
        Node::Lit(literal) => Ok(AtomicTerm::Literal(literal.clone())),
        Node::Triple(_) => Err(ListDefect::Malformed(
            "a quoted-triple list member requires typed formula lowering".to_owned(),
        )),
    }
}

/// The `logic:` predicates that make a blank subject an anonymous owner lifted by
/// [`skolemize_anonymous_owners`]: every list constructor except `oneOf` (anonymous
/// enumerations are the enumeration skolemizer's), plus the unary `complementOf`.
fn is_anonymous_owner_predicate(predicate: &str) -> bool {
    predicate
        .strip_prefix(LOGIC_NAMESPACE)
        .is_some_and(|local| {
            local == "complementOf"
                || LIST_CONSTRUCTORS
                    .iter()
                    .any(|ctor| ctor.local == local && local != "oneOf")
        })
}

/// Every blank subject carrying an anonymous-owner predicate, minus the nodes another
/// skolemizer owns (`exclude`), keyed by label.
pub(crate) fn anonymous_owner_nodes(
    store: &RdfDataset,
    exclude: &BTreeSet<String>,
) -> BTreeMap<String, Subject> {
    let mut out = BTreeMap::new();
    for quad in crate::graphutil::default_graph_quads(store) {
        if let Subject::Blank { label, .. } = &quad.subject
            && is_anonymous_owner_predicate(quad.predicate.as_str())
            && !exclude.contains(label)
        {
            out.entry(label.clone())
                .or_insert_with(|| quad.subject.clone());
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OwnerState {
    InProgress,
    Failed,
}

struct OwnerSkolemizer<'a> {
    store: &'a RdfDataset,
    nodes: &'a BTreeMap<String, Subject>,
    skolems: &'a mut BTreeMap<String, String>,
    state: BTreeMap<String, OwnerState>,
    emitted: BTreeSet<String>,
    out: Vec<LiftedTriple>,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl OwnerSkolemizer<'_> {
    fn member(&mut self, node: &Node) -> Result<AtomicTerm, ListDefect> {
        match node {
            Node::Blank { label, .. } if self.nodes.contains_key(label) => {
                self.resolve(label).map(AtomicTerm::Iri).ok_or_else(|| {
                    ListDefect::UnresolvedMember(format!(
                        "anonymous member {label:?} could not be lifted"
                    ))
                })
            }
            _ => resolve_known(&*self.skolems)(node),
        }
    }

    fn fail(&mut self, label: &str, code: &str, why: &str) -> Option<String> {
        // A node is disclosed once, even when a cycle reaches it again.
        if self.state.insert(label.to_owned(), OwnerState::Failed) == Some(OwnerState::Failed) {
            return None;
        }
        self.diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            code: code.to_owned(),
            message: format!("anonymous class expression {label:?} is not lifted: {why}"),
            subject: Some(label.to_owned()),
        });
        None
    }

    fn resolve(&mut self, label: &str) -> Option<String> {
        if let Some(iri) = self.skolems.get(label) {
            return Some(iri.clone());
        }
        let node = self.nodes.get(label)?.clone();
        match self.state.get(label) {
            Some(OwnerState::Failed) => return None,
            Some(OwnerState::InProgress) => {
                return self.fail(label, "MALFORMED_LIST", "it contains itself (cyclic)");
            }
            None => {}
        }
        self.state.insert(label.to_owned(), OwnerState::InProgress);
        let source = SourceNode {
            term: subject_id(self.store, &node).expect("selected anonymous owner"),
            graph: None,
        };
        let rows: Vec<(String, Node)> =
            default_graph_pattern(self.store, Some(source.term), None, None)
                .map(|quad| {
                    (
                        iri_of(self.store, quad.p).as_str().to_owned(),
                        node_of(self.store, quad.o),
                    )
                })
                .collect();
        // (predicate, object) pairs re-emitted on the skolem, plus the list cells.
        let mut fields: Vec<(String, AtomicTerm)> = Vec::new();
        let mut cells: Vec<(String, &'static str, AtomicTerm)> = Vec::new();
        let mut key_types: Vec<String> = Vec::new();
        let mut is_nary_axiom = false;
        for (predicate, object) in rows {
            if predicate == RDF_TYPE || predicate == LOGIC_INSTANCE_OF {
                let Ok(class) = crate::graphutil::atomic_object(&object) else {
                    return self.fail(label, "MALFORMED_LIST", "its typing is a quoted triple");
                };
                key_types.push(crate::ir::atomic::frame(
                    "type",
                    [predicate.as_str(), &class.key()],
                ));
                if matches!(&object, Node::Iri(class) if class.starts_with(LOGIC_NAMESPACE)) {
                    fields.push((
                        predicate,
                        AtomicTerm::Iri(crate::graphutil::term_str(&object)),
                    ));
                }
                continue;
            }
            if !predicate.starts_with(LOGIC_NAMESPACE) {
                continue;
            }
            if let Some(ctor) = constructor(&predicate) {
                is_nary_axiom |= ctor.local == "members";
                let store = self.store;
                let lowered = lower(store, ctor, &object, &mut |m| self.member(m));
                match lowered {
                    Ok(list) => {
                        fields.push((predicate, AtomicTerm::Iri(list.head)));
                        cells.extend(list.cells);
                    }
                    Err(defect) => {
                        let code = match defect {
                            ListDefect::Malformed(_) => "MALFORMED_LIST",
                            ListDefect::UnresolvedMember(_) => {
                                "UNSUPPORTED_NESTED_CLASS_EXPRESSION"
                            }
                        };
                        return self.fail(
                            label,
                            code,
                            &format!("its logic:{} list: {}", ctor.local, defect.reason()),
                        );
                    }
                }
                continue;
            }
            match self.member(&object) {
                Ok(term) => fields.push((predicate, term)),
                Err(defect) => {
                    let code = match defect {
                        ListDefect::Malformed(_) => "MALFORMED_LIST",
                        ListDefect::UnresolvedMember(_) => "UNSUPPORTED_NESTED_CLASS_EXPRESSION",
                    };
                    return self.fail(
                        label,
                        code,
                        &format!("its {predicate} operand: {}", defect.reason()),
                    );
                }
            }
        }
        let mut key: Vec<String> = fields
            .iter()
            .map(|(predicate, term)| {
                crate::ir::atomic::frame("field", [predicate.as_str(), &term.key()])
            })
            .chain(key_types)
            .collect();
        key.sort();
        key.dedup();
        let prefix = if is_nary_axiom {
            NARY_AXIOM_PREFIX
        } else {
            CLASS_EXPRESSION_PREFIX
        };
        let iri = format!(
            "{prefix}{}",
            sha256_12(&crate::ir::atomic::frame("anonymous", key))
        );
        self.state.remove(label);
        self.skolems.insert(label.to_owned(), iri.clone());
        if self.emitted.insert(iri.clone()) {
            for (predicate, obj) in fields {
                self.out.push(LiftedTriple {
                    source,
                    subject: iri.clone(),
                    predicate,
                    obj,
                });
            }
            for (subject, predicate, obj) in cells {
                self.out.push(LiftedTriple {
                    source,
                    subject,
                    predicate: predicate.to_owned(),
                    obj,
                });
            }
        }
        Some(iri)
    }
}

/// Lift every anonymous boolean class expression / n-ary axiom resource in `nodes` to a
/// content-addressed IRI (`logic:class-expression/<hash>` or `logic:axiom/<hash>`) whose
/// key is its complete `logic:` content with nested anonymous operands and list members
/// resolved first. Records each lifted label in `skolems` so referencing edges are
/// redirected; a node that cannot be lifted is disclosed and left unresolved.
pub(crate) fn skolemize_anonymous_owners(
    store: &RdfDataset,
    nodes: &BTreeMap<String, Subject>,
    skolems: &mut BTreeMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<LiftedTriple> {
    let mut lifter = OwnerSkolemizer {
        store,
        nodes,
        skolems,
        state: BTreeMap::new(),
        emitted: BTreeSet::new(),
        out: Vec::new(),
        diagnostics,
    };
    for label in nodes.keys() {
        lifter.resolve(label);
    }
    lifter.out
}

#[path = "lists.tests.rs"]
#[cfg(test)]
mod tests;
