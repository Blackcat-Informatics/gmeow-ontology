// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Means–end refinement (RQ2/RQ3/RQ10) as a NATIVE derivation, not a search beside the
//! engine.
//!
//! # What runs where
//!
//! The expansion is eleven authored `logic:Rule`s in
//! `slices/grounding/logic/module.ttl` — the ordered-carrier walk
//! (`logic:methodYieldCell`, `logic:methodStep`), the one-step expansion
//! (`logic:refinementExpands`), its transitive closure (`logic:refinementReaches`), the
//! roster (`logic:refinementCandidateMethod`), and the four typed rejections. This module
//! selects exactly those rules out of the embedded module, opens a
//! [`ReasoningSession`] over them, and applies the caller's world as ONE budgeted
//! transaction. Every roster row, every reached subtask and every rejection in
//! [`RefineReport`] is a row of that session's closure, carrying the firing rule and the
//! premises the chase recorded.
//!
//! Nothing here decides what a candidate is. The predecessor of this module was a
//! breadth-first search over `BTreeMap`s that never touched the engine: its roster was a
//! list Rust built, its budget was a counter Rust kept, and "why is this candidate here"
//! had no answer the graph could give. That is the shape this branch's own
//! counter-examples ban elsewhere, and it is gone.
//!
//! # The two things Rust still does, and why neither is search
//!
//! 1. **Carrier validation.** `logic:methodYields` is ONE `rdf:List`, and a method
//!    naming two lists, an empty list, or a chain that never reaches `rdf:nil` denotes no
//!    sequence at all. That is a malformed REQUEST, so it is refused as
//!    [`OperationOutcome::Invalid`] before any derivation runs — where the predecessor
//!    silently dropped the method, turning a typo into a plan quietly one step short.
//! 2. **Reading the order back.** The chase derives `logic:methodStep` as a SET, because
//!    that is what a join needs. The ORDER lives in the `rdf:List` the author wrote, and
//!    rendering a candidate walks that list. Walking a chain of `rdf:first`/`rdf:rest` is
//!    reading a carrier, not choosing among alternatives.
//!
//! # Why a candidate is a METHOD
//!
//! [`RefineCandidate`] is an applicable `logic:DecompositionMethod` — the reusable schema
//! `logic:RefinementCandidateSet` is defined over — and not a fully linearized total plan.
//! Enumerating linearizations is the cross-product of every task's alternatives: it is
//! exponential in the method set, it is not expressible as binary Datalog without the
//! engine inventing a node per linearization, and it is precisely what let the
//! predecessor's unbounded queue exhaust memory before its expansion budget tripped. The
//! method roster carries the same information — each alternative is named, and
//! `logic:refinementReaches` states what each expands into — in space linear in the
//! method set.
//!
//! # Boundedness
//!
//! The budget is the session delta's `max_steps`: a committed-derivation budget the
//! engine's own governor enforces over the whole transaction. A cut returns
//! [`OperationOutcome::Incomplete`] and the session state does NOT advance, so there is no
//! roster to mistake for a closed one. The predecessor budgeted only expansions while
//! leaving its queue unbounded, which is the failure this replaces.
//!
//! # Outcomes
//!
//! [`RefineReport::outcome`] is the shipped six-way [`OperationOutcome`] — the same fold
//! `logic:OperationOutcome` names in the module and the session façade returns. There is
//! no private refinement status: a cancellation and a malformed request were
//! unrepresentable in the three-valued fold this replaces, and both are ordinary values
//! here.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, TermRef, TermValue};

use crate::annotation::AnnotationContract;
use crate::runtime::{
    IntegrityFault, OperationOutcome, ReasoningSession, SessionDelta, UnsupportedFragment,
};

/// The `logic:` namespace every predicate below lives in.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// `rdf:first` — the head cell of an RDF list.
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
/// `rdf:rest` — the tail cell of an RDF list.
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
/// `rdf:nil` — the empty list, and the only legal terminator of a well-formed chain.
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
/// `rdf:type`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The scratch world the caller's quads are promoted into. The session reasons over
/// named-graph worlds only, and refinement input is ordinarily default-graph Turtle, so a
/// file carrying methods would otherwise derive nothing — the least useful failure
/// available, because it looks exactly like "no method matched". Never leaks: it exists
/// only inside this module's transient session.
const REFINE_WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/refinement-search";

/// The refinement operation's own identity, and the ONLY content of the session's
/// authorized EDB.
///
/// The session is opened over the episode DECLARATION — what is being refined, and under
/// which declared `logic:SearchFragment` — and the caller's world arrives as the delta.
/// That split is the honest one: the operation's identity is what the session is pinned
/// to, and the world is what it reasons over.
const REFINE_OPERATION: &str =
    "https://blackcatinformatics.ca/gmeow/graph/refinement-search#operation";

/// The three `logic:SearchFragment` members a refinement may be admitted under. A
/// `--fragment` naming anything else is a malformed request, not a fragment the engine
/// silently ignores.
const SEARCH_FRAGMENTS: [&str; 3] = [
    "https://blackcatinformatics.ca/logic/FragmentTotallyOrdered",
    "https://blackcatinformatics.ca/logic/FragmentAcyclicMethod",
    "https://blackcatinformatics.ca/logic/FragmentBoundedDepth",
];

/// The fragment forbidding decomposition cycles.
const FRAGMENT_ACYCLIC: &str = "https://blackcatinformatics.ca/logic/FragmentAcyclicMethod";

/// The head predicates of the authored means–end rules — the selection criterion for the
/// sub-program this module drives.
///
/// The set is CLOSED under the rules' bodies: `logic:refinementReaches` is the only
/// derived predicate any of these rules reads, and it is a member, so selecting on head
/// predicate cannot silently drop a rule another one depends on. Every other body atom
/// names an asserted property.
const MEANS_END_PREDICATES: [&str; 9] = [
    "https://blackcatinformatics.ca/logic/methodStep",
    "https://blackcatinformatics.ca/logic/methodYieldCell",
    "https://blackcatinformatics.ca/logic/refinementCandidateMethod",
    "https://blackcatinformatics.ca/logic/refinementExpands",
    "https://blackcatinformatics.ca/logic/refinementReaches",
    "https://blackcatinformatics.ca/logic/refinementRejectedOnApproval",
    "https://blackcatinformatics.ca/logic/refinementRejectedOnCapability",
    "https://blackcatinformatics.ca/logic/refinementRejectedOnPrecondition",
    "https://blackcatinformatics.ca/logic/refinementRejectedOnResource",
];

/// The authored pin-derivation rules, selected by IRI rather than by head predicate.
///
/// By IRI because one of them heads `rdf:type`, and selecting THAT predicate would drag in
/// every unrelated rule in the module that concludes a type — turning the refinement's
/// sub-program into an arbitrary slice of the whole ontology. The six are closed under
/// their own bodies: every body atom names an asserted property, and none reads another pin
/// rule's conclusion, so selecting them by name cannot drop a rule one of them depends on.
const PIN_RULES: [&str; 6] = [
    "ruleEpisodeSelectsItsAuthorizedPin",
    "rulePinAuthorityFromEstablishingProof",
    "rulePinDigestFromMethodDigest",
    "rulePinInstantiatesAuthorizedMethod",
    "rulePinnedStepSequenceFromMethod",
    "rulePinnedSubgraphFromAuthorizedCandidate",
];

/// The pin predicates the closure is read back through, once the rules above have fired.
const PIN_INSTANTIATES_METHOD: &str = "https://blackcatinformatics.ca/logic/pinInstantiatesMethod";
/// `logic:pinnedStepSequence` — derived to the HEAD CELL of the method's yielded list.
const PINNED_STEP_SEQUENCE: &str = "https://blackcatinformatics.ca/logic/pinnedStepSequence";
/// `logic:pinAuthority` — derived from the proof that established the candidate.
const PIN_AUTHORITY: &str = "https://blackcatinformatics.ca/logic/pinAuthority";
/// `logic:selectedPin` — derived from the episode that produced the roster.
const SELECTED_PIN: &str = "https://blackcatinformatics.ca/logic/selectedPin";

/// One typed reason a step is not freely executable, mirroring the shipped
/// `logic:RejectionKind` vocabulary one for one.
///
/// The kind is the derived PREDICATE, so a rejection can never arrive untyped: there is
/// no predicate for "rejected, reason unrecorded".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RejectionKind {
    /// `logic:RejectedPrecondition` — the situation the action gate judged did not hold.
    Precondition,
    /// `logic:RejectedCapability` — no capability available to the deployment supplies it.
    Capability,
    /// `logic:RejectedResource` — the means to exercise the capability is claimed.
    Resource,
    /// `logic:RejectedApproval` — an authorization exists and has not been detached.
    Approval,
}

impl RejectionKind {
    /// The `logic:RejectionKind` individual this variant denotes.
    #[must_use]
    pub fn as_iri(self) -> &'static str {
        match self {
            RejectionKind::Precondition => {
                "https://blackcatinformatics.ca/logic/RejectedPrecondition"
            }
            RejectionKind::Capability => "https://blackcatinformatics.ca/logic/RejectedCapability",
            RejectionKind::Resource => "https://blackcatinformatics.ca/logic/RejectedResource",
            RejectionKind::Approval => "https://blackcatinformatics.ca/logic/RejectedApproval",
        }
    }

    /// The derived predicate that publishes this kind.
    fn predicate(self) -> &'static str {
        match self {
            RejectionKind::Precondition => {
                "https://blackcatinformatics.ca/logic/refinementRejectedOnPrecondition"
            }
            RejectionKind::Capability => {
                "https://blackcatinformatics.ca/logic/refinementRejectedOnCapability"
            }
            RejectionKind::Resource => {
                "https://blackcatinformatics.ca/logic/refinementRejectedOnResource"
            }
            RejectionKind::Approval => {
                "https://blackcatinformatics.ca/logic/refinementRejectedOnApproval"
            }
        }
    }

    /// Every kind, in the order a report lists them.
    fn all() -> [RejectionKind; 4] {
        [
            RejectionKind::Precondition,
            RejectionKind::Capability,
            RejectionKind::Resource,
            RejectionKind::Approval,
        ]
    }
}

/// The chase's own witness for one derived fact: which authored rule concluded it, and
/// from which premises.
///
/// Taken verbatim from the session's per-fact provenance rather than reconstructed here.
/// A refinement that assembled its own explanation would be asserting a derivation the
/// engine never made, which is the failure mode a proof witness exists to close.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofWitness {
    /// The authored `logic:Rule` IRI whose firing concluded the fact.
    pub rule_iri: String,
    /// The antecedent `(subject, predicate, object)` premises of the winning firing.
    pub premises: Vec<(String, String, String)>,
    /// The minimal proof height of the selected derivation (`0` for an asserted leaf).
    pub proof_height: u32,
}

/// One derived member of the roster: a `logic:DecompositionMethod` that decomposes the
/// refined task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefineCandidate {
    /// The task this candidate refines.
    pub task: String,
    /// The method's IRI.
    pub method: String,
    /// The subtasks the method yields, in the AUTHORED `rdf:List` order.
    pub steps: Vec<String>,
    /// The steps of [`Self::steps`] that are themselves decomposed further — the roster's
    /// open frontier. Read off the derived `logic:refinementCandidateMethod` relation, so
    /// a step is open because the reasoner found a method for it.
    pub open_steps: Vec<String>,
    /// The chase witness for `logic:refinementCandidateMethod(task, method)`.
    pub witness: ProofWitness,
}

/// One derived, typed rejection standing on a step the roster reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefineRejection {
    /// The step the rejection stands on.
    pub step: String,
    /// The typed reason.
    pub kind: RejectionKind,
    /// The thing the rejection names — the missing capability, the unmet precondition,
    /// the claiming lease, the undetached approval's entry. A rejection whose reason
    /// cannot be named is an opinion, so this is never absent.
    pub witness_iri: String,
    /// The chase witness for the rejection fact itself.
    pub witness: ProofWitness,
}

/// One DERIVED `logic:PinnedExecutableSubgraph`: the commitment an authorized candidate
/// becomes, read entirely out of the closure.
///
/// Nothing here is asserted by the caller. The input carries a roster, the provenance edge
/// from each candidate back to the method that produced it, and a
/// `logic:AuthorizationProof` naming the candidate it licenses; the pin's type, its
/// instantiated method, its frozen sequence, its content address and its authority are all
/// concluded by the six authored pin rules. That is the half of "validate the selected
/// refinement AND pin its executable subgraph" that had no derivation at all: a pin could
/// previously only be hand-authored, so the three laws reading one were laws about an
/// author's self-consistency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefinePin {
    /// The episode the pin was selected by (`logic:selectedPin`, derived).
    pub episode: String,
    /// The pin — the roster candidate the authorization turned into a commitment.
    pub pin: String,
    /// The method the pin instantiates (`logic:pinInstantiatesMethod`, derived).
    pub method: String,
    /// The frozen steps, in the AUTHORED `rdf:List` order of the method's
    /// `logic:methodYields` — the same carrier read the roster rows use, because
    /// `logic:pinnedStepSequence` derives to the list HEAD and the order lives in the list.
    pub steps: Vec<String>,
    /// The content address (`logic:pinDigest`, derived from the method's own digest).
    pub digest: String,
    /// The proof that licensed it (`logic:pinAuthority`, derived).
    pub authority: String,
    /// The chase witness for `logic:pinnedStepSequence(pin, cell)` — the derivation that
    /// froze the content, which is the one an approval binds against.
    pub witness: ProofWitness,
}

/// The result of one bounded, chase-derived refinement.
#[derive(Debug)]
pub struct RefineReport {
    /// The refined task.
    pub task: String,
    /// The declared `logic:SearchFragment` the refinement ran under.
    pub fragment: String,
    /// The roster: every candidate method for the refined task AND for every task the
    /// expansion reaches, in `(task, method)` order.
    pub candidates: Vec<RefineCandidate>,
    /// Every typed rejection standing on the refined task or on a step the expansion
    /// reaches, in `(step, kind)` order.
    pub rejections: Vec<RefineRejection>,
    /// Every subtask the refined task decomposes into, at any depth
    /// (`logic:refinementReaches`).
    pub reached: Vec<String>,
    /// Every DERIVED pin the closure carries, in pin IRI order. Empty when no candidate in
    /// scope was authorized — which is the informative absence `logic:selectedPin`'s own
    /// definition names, not a missing feature.
    pub pins: Vec<RefinePin>,
    /// The tasks that reach THEMSELVES — decomposition cycles, named concretely. Empty
    /// for a well-formed method set; non-empty is what puts the method set outside
    /// `logic:FragmentAcyclicMethod`.
    pub cycles: Vec<String>,
    /// The engine's typed outcome for the derivation. The candidates may be read as a
    /// CLOSED roster only under [`OperationOutcome::Applied`].
    pub outcome: OperationOutcome,
}

impl RefineReport {
    /// Whether the roster may be presented as closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        matches!(self.outcome, OperationOutcome::Applied { .. })
    }
}

/// Build a refinement diagnostic.
fn refine_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

/// Refuse the request as malformed, naming what is wrong with it.
fn invalid(task: &str, fragment: &str, detail: impl Into<String>) -> RefineReport {
    RefineReport {
        task: task.to_owned(),
        fragment: fragment.to_owned(),
        candidates: Vec::new(),
        rejections: Vec::new(),
        reached: Vec::new(),
        cycles: Vec::new(),
        pins: Vec::new(),
        outcome: OperationOutcome::Invalid {
            fault: IntegrityFault::MalformedRequest {
                detail: detail.into(),
            },
        },
    }
}

/// Surface an engine failure verbatim rather than folding it into an empty roster.
fn engine_failure(task: &str, fragment: &str, diagnostic: gmeow_errors::Diag) -> RefineReport {
    RefineReport {
        task: task.to_owned(),
        fragment: fragment.to_owned(),
        candidates: Vec::new(),
        rejections: Vec::new(),
        reached: Vec::new(),
        cycles: Vec::new(),
        pins: Vec::new(),
        outcome: OperationOutcome::EngineFailure { diagnostic },
    }
}

/// The means–end sub-program, compiled once per process from the embedded
/// `logic/module.ttl`.
///
/// The embedded module is a fixed compile-time asset whose rule census is pinned by this
/// module's own tests, so a failure here is an authoring/build bug rather than a runtime
/// condition a caller could recover from — the same loud failure
/// [`super::compiled_law_report`] makes for the constraint half of the same file.
fn means_end_program() -> &'static gmeow_logic_compile::ir::LogicProgram {
    static PROGRAM: OnceLock<gmeow_logic_compile::ir::LogicProgram> = OnceLock::new();
    PROGRAM.get_or_init(|| {
        build_means_end_program().unwrap_or_else(|e| {
            panic!(
                "means–end refinement: failed to compile the embedded logic/module.ttl \
                 means–end rules: {e}"
            )
        })
    })
}

/// Parse the embedded module and keep exactly the rules whose head is one of
/// [`MEANS_END_PREDICATES`].
///
/// The result carries NO axioms, formulas or constraints. That is load-bearing rather
/// than tidy: `ReasoningSession` certifies a program as incrementally maintainable only
/// when `program.rules` fully captures its forward-derivable semantics, so a program
/// dragging the module's `logic:Formula` half along would be routed to a full rebuild and
/// never reach the `Applied` outcome a complete refinement must report.
///
/// # Errors
///
/// Returns `Err` if the embedded Turtle fails to parse or the `logic:` frontend cannot
/// compile it, or if the selection is empty — an empty rule set would derive nothing while
/// reporting a clean, closed, empty roster, which is the exact silence this module exists
/// to prevent.
fn build_means_end_program() -> gmeow_errors::Result<gmeow_logic_compile::ir::LogicProgram> {
    let source = purrdf::parse_dataset(super::LOGIC_MODULE_TTL.as_bytes(), "text/turtle", None)
        .map_err(|e| refine_err(format!("parse the embedded logic/module.ttl: {e}")))?;
    let (program, _diagnostics) = gmeow_logic_compile::frontend::parse_logic_dataset(
        source.as_ref(),
        Some(super::LOGIC_MODULE_SOURCE_IRI.to_owned()),
    )
    .map_err(|e| {
        refine_err(format!(
            "compile the embedded logic/module.ttl into a LogicProgram: {e}"
        ))
    })?;
    let selected: Vec<gmeow_logic_compile::ir::LogicRule> = program
        .rules
        .iter()
        .filter(|rule| {
            MEANS_END_PREDICATES.contains(&rule.head.predicate.as_str())
                || rule.scope.provenance.as_deref().is_some_and(|iri| {
                    PIN_RULES
                        .iter()
                        .any(|name| iri.ends_with(&format!("/{name}")))
                })
        })
        .cloned()
        .collect();
    if selected.is_empty() {
        return Err(refine_err(
            "means–end refinement: no authored logic:Rule heads a means–end predicate, so \
             the refinement would derive nothing while reporting a closed empty roster"
                .to_owned(),
        ));
    }
    Ok(gmeow_logic_compile::ir::LogicProgram::new(
        Vec::new(),
        selected,
        Vec::new(),
        Some(super::LOGIC_MODULE_SOURCE_IRI.to_owned()),
    ))
}

/// Every `(subject, predicate, object)` IRI-or-literal triple of `input`, as owned
/// strings — the carrier view the `rdf:List` validation and the order read work over.
fn input_triples(input: &RdfDataset) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for quad in input.quads() {
        let (TermRef::Iri(subject), TermRef::Iri(predicate)) =
            (input.resolve(quad.s), input.resolve(quad.p))
        else {
            continue;
        };
        let object = match input.resolve(quad.o) {
            TermRef::Iri(iri) => iri.to_owned(),
            TermRef::Literal { lexical, .. } => lexical.to_owned(),
            _ => continue,
        };
        out.push((subject.to_owned(), predicate.to_owned(), object));
    }
    out
}

/// Promote every IRI-subject quad of `input` into the single [`REFINE_WORLD`].
///
/// # Errors
///
/// Returns `Err` if the promoted dataset fails its freeze-time structural contract.
fn promote(input: &RdfDataset) -> gmeow_errors::Result<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let world = builder.intern_iri(REFINE_WORLD);
    for quad in input.quads() {
        let (TermRef::Iri(subject), TermRef::Iri(predicate)) =
            (input.resolve(quad.s), input.resolve(quad.p))
        else {
            continue;
        };
        let (subject, predicate) = (subject.to_owned(), predicate.to_owned());
        let object = match input.resolve(quad.o) {
            TermRef::Iri(iri) => {
                let iri = iri.to_owned();
                builder.intern_iri(&iri)
            }
            TermRef::Literal {
                lexical, language, ..
            } => {
                let literal = RdfLiteral {
                    lexical_form: lexical.to_owned(),
                    datatype: None,
                    language: language.map(str::to_owned),
                    direction: None,
                };
                builder.intern_literal(literal)
            }
            _ => continue,
        };
        let s = builder.intern_iri(&subject);
        let p = builder.intern_iri(&predicate);
        builder.push_quad(s, p, object, Some(world));
    }
    own(builder)
}

/// The session's authorized EDB: the refinement episode's own declaration.
///
/// # Errors
///
/// Returns `Err` if the seed dataset fails its freeze-time structural contract.
fn seed_edb(task: &str, fragment: &str) -> gmeow_errors::Result<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let world = builder.intern_iri(REFINE_WORLD);
    let operation = builder.intern_iri(REFINE_OPERATION);
    for (predicate, object) in [
        (RDF_TYPE, format!("{LOGIC_NS}RefinementEpisode")),
        (&format!("{LOGIC_NS}refinesStep"), task.to_owned()),
        (&format!("{LOGIC_NS}searchFragment"), fragment.to_owned()),
    ] {
        let p = builder.intern_iri(predicate);
        let o = builder.intern_iri(&object);
        builder.push_quad(operation, p, o, Some(world));
    }
    own(builder)
}

/// Freeze a builder into an OWNED dataset (the session façade takes owned datasets).
///
/// # Errors
///
/// Returns `Err` if the freeze fails, or if the freshly-frozen dataset is unexpectedly
/// shared — an internal-invariant failure, never a degraded success.
fn own(builder: RdfDatasetBuilder) -> gmeow_errors::Result<RdfDataset> {
    let frozen = builder
        .freeze()
        .map_err(|e| refine_err(format!("freeze the refinement world: {e}")))?;
    std::sync::Arc::try_unwrap(frozen).map_err(|_| {
        refine_err(
            "means–end refinement: a freshly-frozen refinement dataset was unexpectedly shared"
                .to_owned(),
        )
    })
}

/// Walk the `rdf:List` rooted at `head` into its members, in list order.
///
/// # Errors
///
/// Returns `Err` for a MALFORMED chain — a cell missing `rdf:first` or `rdf:rest`, or one
/// that loops back on itself — rather than the prefix it managed to read. A truncated
/// prefix is a different, shorter plan, and returning one would let a broken list silently
/// delete every step after the break.
fn list_members(
    head: &str,
    first: &BTreeMap<&str, &str>,
    rest: &BTreeMap<&str, &str>,
) -> gmeow_errors::Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut cell = head;
    while cell != RDF_NIL {
        if !seen.insert(cell) {
            return Err(refine_err(format!(
                "the list cell <{cell}> is revisited, so the chain has no last element and \
                 denotes no sequence"
            )));
        }
        let Some(member) = first.get(cell) else {
            return Err(refine_err(format!(
                "the list cell <{cell}> carries no rdf:first"
            )));
        };
        out.push((*member).to_owned());
        let Some(next) = rest.get(cell) else {
            return Err(refine_err(format!(
                "the list cell <{cell}> carries no rdf:rest, so the chain never reaches rdf:nil"
            )));
        };
        cell = next;
    }
    Ok(out)
}

/// The authored, ORDERED step sequence of every `logic:DecompositionMethod` in `rows`.
///
/// This is carrier validation and carrier reading, deliberately kept out of the
/// derivation: the chase concludes WHICH methods decompose WHAT, and this says what each
/// method's list literally is.
///
/// # Errors
///
/// Returns `Err` naming why a method's `logic:methodYields` carrier is malformed — two
/// lists, an empty list, a broken or cyclic chain. Each is a request the refinement
/// refuses rather than silently narrows.
fn method_orders(
    rows: &[(String, String, String)],
) -> gmeow_errors::Result<BTreeMap<String, Vec<String>>> {
    let yields_p = format!("{LOGIC_NS}methodYields");

    let mut yields: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut first: BTreeMap<&str, &str> = BTreeMap::new();
    let mut rest: BTreeMap<&str, &str> = BTreeMap::new();
    for (subject, predicate, object) in rows {
        if *predicate == yields_p {
            yields.entry(subject).or_default().push(object);
        } else if predicate == RDF_FIRST {
            first.insert(subject, object);
        } else if predicate == RDF_REST {
            rest.insert(subject, object);
        }
    }

    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (method, heads) in yields {
        let mut heads = heads;
        heads.sort_unstable();
        heads.dedup();
        if heads.len() > 1 {
            // Single-valued by definition. Picking one of two candidate sequences by read
            // order is exactly the arbitrary choice the ordered carrier abolished.
            return Err(refine_err(format!(
                "<{method}> names {} logic:methodYields lists; the property is single-valued, \
                 so which sequence the method yields is undetermined",
                heads.len()
            )));
        }
        let head = heads[0];
        let steps = list_members(head, &first, &rest)
            .map_err(|why| refine_err(format!("<{method}> logic:methodYields <{head}>: {why}")))?;
        if steps.is_empty() {
            return Err(refine_err(format!(
                "<{method}> logic:methodYields the empty list; 'this method reduces the task to \
                 nothing' is a claim an author must make explicitly, never one a reader invents"
            )));
        }
        out.insert(method.to_owned(), steps);
    }
    Ok(out)
}

/// The `<iri>` display surface the chase provenance renders an IRI as.
fn displayed(iri: &str) -> String {
    format!("<{iri}>")
}

/// Index the session's full-closure provenance by the fact it witnesses.
fn witness_index(session: &ReasoningSession) -> BTreeMap<(String, String, String), ProofWitness> {
    let mut index = BTreeMap::new();
    for derivation in session.provenance() {
        index
            .entry((
                derivation.subject.clone(),
                derivation.predicate.clone(),
                derivation.object.clone(),
            ))
            .or_insert_with(|| ProofWitness {
                rule_iri: derivation.rule_iri.clone(),
                premises: derivation.premises.clone(),
                proof_height: derivation.proof_height,
            });
    }
    index
}

/// Every `(subject, object)` IRI pair the closure carries under `predicate`.
fn closure_pairs(session: &ReasoningSession, predicate: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = session
        .facts()
        .rows
        .iter()
        .filter(|row| row.predicate == predicate)
        .filter_map(|row| match (row.args.first(), row.args.get(1)) {
            (Some(TermValue::Iri(subject)), Some(TermValue::Iri(object))) => {
                Some((subject.clone(), object.clone()))
            }
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The `logic:methodDigest` each method carries, read off the input carrier.
///
/// A CARRIER READ, in the same sense as [`method_orders`] — and for a sharper reason. The
/// incremental session's EDB is IRI-object only (`crate::reason::build_edb_facts` drops
/// every literal-valued quad before a fact is built), so a datatype property is invisible
/// to this lane's engine by construction. `logic:rulePinDigestFromMethodDigest` is
/// therefore authored, compiled and selected here, and CONCLUDES on the full-reasoner lane
/// where literals survive; on this lane its premise never arrives.
///
/// Reading the value here rather than inventing one is what keeps that honest: the digest a
/// pin carries is the method version's own, byte for byte, and if the method carries none
/// then neither does the pin — which is why [`collect_pins`] reports no pin at all rather
/// than an unaddressed one.
fn method_digests(rows: &[(String, String, String)]) -> BTreeMap<String, String> {
    let digest_p = format!("{LOGIC_NS}methodDigest");
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for (subject, predicate, object) in rows {
        if *predicate == digest_p {
            out.insert(subject.clone(), object.clone());
        }
    }
    out
}

/// Assemble the derived pins out of the settled closure.
///
/// A pin is reported only when ALL FIVE of its derived triples are present. That is not
/// defensiveness: the completeness law requires the method, the sequence and the digest
/// together, so a partial row would be a pin the kernel is about to condemn, and printing
/// it as a commitment would tell an operator that something was frozen when nothing was.
/// The commonest cause is a method carrying no `logic:methodDigest` — an honest gap in the
/// method, reported as "no pin", never as a pin with a fabricated address.
fn collect_pins(
    session: &ReasoningSession,
    scope: &BTreeSet<String>,
    orders: &BTreeMap<String, Vec<String>>,
    digests: &BTreeMap<String, String>,
    witnesses: &BTreeMap<(String, String, String), ProofWitness>,
) -> Vec<RefinePin> {
    let methods: BTreeMap<String, String> = closure_pairs(session, PIN_INSTANTIATES_METHOD)
        .into_iter()
        .collect();
    let sequences: BTreeMap<String, String> = closure_pairs(session, PINNED_STEP_SEQUENCE)
        .into_iter()
        .collect();
    let authorities: BTreeMap<String, String> =
        closure_pairs(session, PIN_AUTHORITY).into_iter().collect();

    let mut pins: Vec<RefinePin> = Vec::new();
    for (episode, pin) in closure_pairs(session, SELECTED_PIN) {
        let (Some(method), Some(cell), Some(authority)) = (
            methods.get(&pin),
            sequences.get(&pin),
            authorities.get(&pin),
        ) else {
            continue;
        };
        let Some(digest) = digests.get(method) else {
            continue;
        };
        // In scope through the METHOD's task: a pin is this refinement's business when the
        // method it instantiates decomposes something the expansion reached.
        let steps = orders.get(method).cloned().unwrap_or_default();
        if !steps.iter().any(|step| scope.contains(step)) && !scope.contains(method) {
            continue;
        }
        let witness = witnesses
            .get(&(
                displayed(&pin),
                PINNED_STEP_SEQUENCE.to_owned(),
                displayed(cell),
            ))
            .cloned()
            .unwrap_or_else(|| ProofWitness {
                rule_iri: crate::provenance::ASSERT_RULE_IRI.to_owned(),
                premises: Vec::new(),
                proof_height: 0,
            });
        pins.push(RefinePin {
            episode,
            pin,
            method: method.clone(),
            steps,
            digest: digest.clone(),
            authority: authority.clone(),
            witness,
        });
    }
    pins.sort_by(|a, b| (&a.pin, &a.episode).cmp(&(&b.pin, &b.episode)));
    pins
}

/// Run a bounded means–end refinement of `task` over the method set carried by `input`.
///
/// `fragment` is the declared `logic:SearchFragment` IRI and `budget` is the
/// committed-derivation budget the engine's governor enforces.
///
/// Never panics and never returns a partial roster wearing a complete one's clothes: the
/// roster is populated only under [`OperationOutcome::Applied`], and every other outcome
/// carries the typed reason it is not.
#[must_use]
pub fn refine(input: &RdfDataset, task: &str, fragment: &str, budget: u32) -> RefineReport {
    if !SEARCH_FRAGMENTS.contains(&fragment) {
        return invalid(
            task,
            fragment,
            format!(
                "<{fragment}> is not one of the three declared logic:SearchFragment members \
                 ({}); a refinement admitted under an unknown fragment has no completeness \
                 claim to make",
                SEARCH_FRAGMENTS.join(", ")
            ),
        );
    }

    let rows = input_triples(input);
    if !rows
        .iter()
        .any(|(subject, _, object)| subject == task || object == task)
    {
        // A typo'd task IRI must not read as "no decomposition exists". That answer is
        // both wrong and reassuring, which is the worst pair available.
        return invalid(
            task,
            fragment,
            format!("<{task}> does not occur in the input, so there is nothing to refine"),
        );
    }
    let orders = match method_orders(&rows) {
        Ok(orders) => orders,
        Err(why) => {
            return invalid(
                task,
                fragment,
                format!("a logic:methodYields carrier is malformed: {why}"),
            );
        }
    };

    let edb = match seed_edb(task, fragment) {
        Ok(edb) => edb,
        Err(diagnostic) => return engine_failure(task, fragment, diagnostic),
    };
    let additions = match promote(input) {
        Ok(additions) => additions,
        Err(diagnostic) => return engine_failure(task, fragment, diagnostic),
    };

    let contract = gmeow_logic_compile::ir::ReasoningContract::new();
    let annotation = AnnotationContract::exact();
    let mut session =
        match ReasoningSession::open(&edb, means_end_program(), &contract, &annotation) {
            Ok(session) => session,
            Err(diagnostic) => return engine_failure(task, fragment, diagnostic),
        };

    let delta = match SessionDelta::new(
        session.identity().data_generation.clone(),
        session.head().to_owned(),
        additions,
        Vec::new(),
        Some(u64::from(budget)),
    ) {
        Ok(delta) => delta,
        Err(diagnostic) => return engine_failure(task, fragment, diagnostic),
    };

    let outcome = session.apply(&delta);
    if !matches!(outcome, OperationOutcome::Applied { .. }) {
        // The session commits only on a complete run, so there is no closure to read and
        // no roster to mistake for a closed one.
        return RefineReport {
            task: task.to_owned(),
            fragment: fragment.to_owned(),
            candidates: Vec::new(),
            rejections: Vec::new(),
            reached: Vec::new(),
            cycles: Vec::new(),
            pins: Vec::new(),
            outcome,
        };
    }

    collect(
        &session,
        task,
        fragment,
        &orders,
        &method_digests(&rows),
        outcome,
    )
}

/// Project the settled closure into the report.
fn collect(
    session: &ReasoningSession,
    task: &str,
    fragment: &str,
    orders: &BTreeMap<String, Vec<String>>,
    digests: &BTreeMap<String, String>,
    outcome: OperationOutcome,
) -> RefineReport {
    let witnesses = witness_index(session);
    let reaches = closure_pairs(session, &format!("{LOGIC_NS}refinementReaches"));
    let candidate_pairs = closure_pairs(session, &format!("{LOGIC_NS}refinementCandidateMethod"));

    // The refined task plus everything its expansion reaches — the scope every roster row
    // and every rejection below is filtered to, so a refinement of one step never reports
    // a rejection standing on an unrelated part of the graph.
    let mut scope: BTreeSet<String> = BTreeSet::new();
    scope.insert(task.to_owned());
    for (from, to) in &reaches {
        if from == task {
            scope.insert(to.clone());
        }
    }
    let reached: Vec<String> = scope.iter().filter(|step| *step != task).cloned().collect();

    // A task that reaches ITSELF closes a decomposition loop. Reported for the refined
    // task and everything it reaches, because a cycle further down still means no
    // expansion of the root terminates.
    let cycles: Vec<String> = reaches
        .iter()
        .filter(|(from, to)| from == to && scope.contains(from))
        .map(|(from, _)| from.clone())
        .collect();

    let open: BTreeSet<&str> = candidate_pairs
        .iter()
        .map(|(step, _)| step.as_str())
        .collect();

    let candidate_predicate = format!("{LOGIC_NS}refinementCandidateMethod");
    let mut candidates: Vec<RefineCandidate> = Vec::new();
    for (candidate_task, method) in &candidate_pairs {
        if !scope.contains(candidate_task) {
            continue;
        }
        let steps = orders.get(method).cloned().unwrap_or_default();
        let open_steps: Vec<String> = steps
            .iter()
            .filter(|step| open.contains(step.as_str()))
            .cloned()
            .collect();
        let witness = witnesses
            .get(&(
                displayed(candidate_task),
                candidate_predicate.clone(),
                displayed(method),
            ))
            .cloned()
            .unwrap_or_else(|| ProofWitness {
                rule_iri: crate::provenance::ASSERT_RULE_IRI.to_owned(),
                premises: Vec::new(),
                proof_height: 0,
            });
        candidates.push(RefineCandidate {
            task: candidate_task.clone(),
            method: method.clone(),
            steps,
            open_steps,
            witness,
        });
    }

    let mut rejections: Vec<RefineRejection> = Vec::new();
    for kind in RejectionKind::all() {
        let predicate = kind.predicate().to_owned();
        for (step, witness_iri) in closure_pairs(session, &predicate) {
            if !scope.contains(&step) {
                continue;
            }
            let witness = witnesses
                .get(&(displayed(&step), predicate.clone(), displayed(&witness_iri)))
                .cloned()
                .unwrap_or_else(|| ProofWitness {
                    rule_iri: crate::provenance::ASSERT_RULE_IRI.to_owned(),
                    premises: Vec::new(),
                    proof_height: 0,
                });
            rejections.push(RefineRejection {
                step,
                kind,
                witness_iri,
                witness,
            });
        }
    }
    rejections
        .sort_by(|a, b| (&a.step, a.kind, &a.witness_iri).cmp(&(&b.step, b.kind, &b.witness_iri)));

    // Under the acyclic fragment a cycle is an OUT-OF-FRAGMENT refusal, not a thin
    // result and not a budget problem: no budget would fix it, and reporting it as one
    // sends an operator to buy compute for a problem compute cannot solve. The derivation
    // itself completed — a transitive closure over a cyclic graph is finite — so the
    // refusal is about the METHOD SET, which is why it is decided here and not by the
    // engine's own outcome.
    let outcome = if fragment == FRAGMENT_ACYCLIC && !cycles.is_empty() {
        OperationOutcome::UnsupportedFragment {
            kind: UnsupportedFragment::MethodSetOutsideSearchFragment,
        }
    } else {
        outcome
    };
    let closed = matches!(outcome, OperationOutcome::Applied { .. });
    let pins = collect_pins(session, &scope, orders, digests, &witnesses);

    RefineReport {
        task: task.to_owned(),
        fragment: fragment.to_owned(),
        candidates: if closed { candidates } else { Vec::new() },
        rejections: if closed { rejections } else { Vec::new() },
        reached: if closed { reached } else { Vec::new() },
        pins: if closed { pins } else { Vec::new() },
        cycles,
        outcome,
    }
}

#[cfg(test)]
mod tests {
    use super::{MEANS_END_PREDICATES, OperationOutcome, RejectionKind, means_end_program, refine};

    /// The eleven authored means–end rules, by IRI local name.
    ///
    /// Spelled out rather than counted, because a count alone would stay green if one rule
    /// silently dropped out of the module and an unrelated one arrived. This is the census
    /// the module doc's claim rests on: the expansion an operator reads is exactly the
    /// authored rule set, and nothing supplements it in Rust.
    const MEANS_END_RULES: [&str; 11] = [
        "ruleMethodStep",
        "ruleMethodYieldCellHead",
        "ruleMethodYieldCellRest",
        "ruleRefinementCandidateMethod",
        "ruleRefinementExpands",
        "ruleRefinementReachesBase",
        "ruleRefinementReachesTransitive",
        "ruleRefinementRejectedOnApproval",
        "ruleRefinementRejectedOnCapability",
        "ruleRefinementRejectedOnPrecondition",
        "ruleRefinementRejectedOnResource",
    ];

    fn dataset(turtle: &str) -> std::sync::Arc<purrdf::RdfDataset> {
        purrdf::parse_dataset(turtle.as_bytes(), "text/turtle", None).expect("parse fixture")
    }

    const ACYCLIC: &str = "https://blackcatinformatics.ca/logic/FragmentAcyclicMethod";
    const NS: &str = "https://blackcatinformatics.ca/gmeow/refinetest/";

    /// A five-step ordered method whose sequence is neither alphabetical nor
    /// reverse-alphabetical, so a reader that sorted it would fail visibly.
    const ORDERED: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/refinetest/> .
e:m a logic:DecompositionMethod ; logic:methodDecomposes e:ocr ; logic:methodYields e:c1 .
e:c1 rdf:first e:inspect ; rdf:rest e:c2 .
e:c2 rdf:first e:prepare ; rdf:rest e:c3 .
e:c3 rdf:first e:extract ; rdf:rest e:c4 .
e:c4 rdf:first e:verify  ; rdf:rest e:c5 .
e:c5 rdf:first e:store   ; rdf:rest rdf:nil .
"#;

    #[test]
    fn every_authored_means_end_rule_is_selected_into_the_program() {
        let program = means_end_program();
        let selected: Vec<&str> = program
            .rules
            .iter()
            .filter_map(|rule| rule.scope.provenance.as_deref())
            .collect();
        let missing: Vec<&str> = MEANS_END_RULES
            .iter()
            .copied()
            .filter(|name| {
                !selected
                    .iter()
                    .any(|iri| iri.ends_with(&format!("/{name}")))
            })
            .collect();
        assert!(
            missing.is_empty(),
            "these authored means–end rules are not in the sub-program the refinement runs, so \
             the expansion silently does less than the module says: {missing:?}"
        );
    }

    /// The six pin-derivation rules reach the sub-program the refinement runs.
    ///
    /// Selected by IRI rather than head predicate, so a rename in the module would silently
    /// drop one and the refinement would report a roster with no commitment — which reads
    /// exactly like "nothing was authorized".
    #[test]
    fn every_authored_pin_rule_is_selected_into_the_program() {
        let program = means_end_program();
        let selected: Vec<&str> = program
            .rules
            .iter()
            .filter_map(|rule| rule.scope.provenance.as_deref())
            .collect();
        let missing: Vec<&str> = super::PIN_RULES
            .iter()
            .copied()
            .filter(|name| {
                !selected
                    .iter()
                    .any(|iri| iri.ends_with(&format!("/{name}")))
            })
            .collect();
        assert!(
            missing.is_empty(),
            "these authored pin rules are not in the sub-program, so an authorized candidate \
             would produce no commitment while the roster looked complete: {missing:?}; \
             selected: {selected:?}"
        );
    }

    /// Every selected rule's head is one of the declared means–end predicates, and every
    /// declared predicate is headed by some rule.
    ///
    /// The second half is the one that matters: a predicate this module READS but no rule
    /// DERIVES would make the corresponding column of every report permanently empty while
    /// the refinement reported a clean, closed roster.
    #[test]
    fn the_declared_means_end_predicates_are_exactly_the_derived_ones() {
        let program = means_end_program();
        let headed: std::collections::BTreeSet<&str> = program
            .rules
            .iter()
            .map(|rule| rule.head.predicate.as_str())
            .collect();
        let underived: Vec<&str> = MEANS_END_PREDICATES
            .iter()
            .copied()
            .filter(|predicate| !headed.contains(predicate))
            .collect();
        assert!(
            underived.is_empty(),
            "these means–end predicates are read but never derived, so the report's \
             corresponding column is permanently empty: {underived:?}"
        );
    }

    /// The whole point: the authored order survives the derivation.
    #[test]
    fn a_methodised_task_yields_its_authored_sequence_in_order() {
        let input = dataset(ORDERED);
        let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 10_000);
        assert!(
            report.is_closed(),
            "a well-formed method set must settle: {:?}",
            report.outcome
        );
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(
            report.candidates[0].steps,
            vec![
                format!("{NS}inspect"),
                format!("{NS}prepare"),
                format!("{NS}extract"),
                format!("{NS}verify"),
                format!("{NS}store"),
            ],
            "the list order IS the plan; alphabetised, this one verifies before it extracts"
        );
    }

    /// Every roster row carries the rule that concluded it — the property the predecessor
    /// could not have, because nothing concluded anything.
    #[test]
    fn a_candidate_carries_the_authored_rule_that_derived_it() {
        let input = dataset(ORDERED);
        let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 10_000);
        let witness = &report.candidates[0].witness;
        assert!(
            witness.rule_iri.ends_with("/ruleRefinementCandidateMethod"),
            "a roster row must name the authored rule that concluded it, got {}",
            witness.rule_iri
        );
        assert!(
            !witness.premises.is_empty(),
            "a proof witness with no premises explains nothing"
        );
    }

    #[test]
    fn two_methods_for_one_task_are_two_alternatives() {
        let input = dataset(&format!(
            r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <{NS}> .
e:fast a logic:DecompositionMethod ; logic:methodDecomposes e:ocr ; logic:methodYields e:f1 .
e:f1 rdf:first e:quick ; rdf:rest rdf:nil .
e:thorough a logic:DecompositionMethod ; logic:methodDecomposes e:ocr ; logic:methodYields e:t1 .
e:t1 rdf:first e:extract ; rdf:rest e:t2 .
e:t2 rdf:first e:verify ; rdf:rest rdf:nil .
"#
        ));
        let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 10_000);
        assert!(report.is_closed());
        assert_eq!(
            report.candidates.len(),
            2,
            "a roster that dropped one would be picking a plan on the operator's behalf"
        );
    }

    #[test]
    fn a_nested_method_set_reaches_the_deep_steps_and_marks_the_open_one() {
        let input = dataset(&format!(
            r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <{NS}> .
e:top a logic:DecompositionMethod ; logic:methodDecomposes e:ingest ; logic:methodYields e:p1 .
e:p1 rdf:first e:ocr ; rdf:rest e:p2 .
e:p2 rdf:first e:store ; rdf:rest rdf:nil .
e:sub a logic:DecompositionMethod ; logic:methodDecomposes e:ocr ; logic:methodYields e:s1 .
e:s1 rdf:first e:extract ; rdf:rest rdf:nil .
"#
        ));
        let report = refine(input.as_ref(), &format!("{NS}ingest"), ACYCLIC, 10_000);
        assert!(report.is_closed());
        assert!(
            report.reached.contains(&format!("{NS}extract")),
            "the transitive expansion must reach past the first level: {:?}",
            report.reached
        );
        let top = report
            .candidates
            .iter()
            .find(|c| c.method == format!("{NS}top"))
            .expect("the top method is a candidate");
        assert_eq!(
            top.open_steps,
            vec![format!("{NS}ocr")],
            "a step with a method of its own is OPEN, and a roster that hid that would present \
             an abstract task as executable work"
        );
    }

    /// A cycle is an out-of-fragment refusal, and it names the task that closes the loop.
    #[test]
    fn a_method_cycle_is_out_of_fragment_and_names_the_looping_task() {
        let input = dataset(&format!(
            r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <{NS}> .
e:a a logic:DecompositionMethod ; logic:methodDecomposes e:t ; logic:methodYields e:a1 .
e:a1 rdf:first e:u ; rdf:rest rdf:nil .
e:b a logic:DecompositionMethod ; logic:methodDecomposes e:u ; logic:methodYields e:b1 .
e:b1 rdf:first e:t ; rdf:rest rdf:nil .
"#
        ));
        let report = refine(input.as_ref(), &format!("{NS}t"), ACYCLIC, 10_000);
        assert!(
            matches!(report.outcome, OperationOutcome::UnsupportedFragment { .. }),
            "a cyclic method set is out-of-fragment, never a budget problem: {:?}",
            report.outcome
        );
        assert!(
            report.cycles.contains(&format!("{NS}t")),
            "the refusal must name WHICH task closes the loop: {:?}",
            report.cycles
        );
        assert!(
            report.candidates.is_empty(),
            "an out-of-fragment refusal must not also publish a roster"
        );
    }

    /// A budget cut must never present as a closed roster.
    #[test]
    fn a_budget_cut_is_incomplete_and_never_closed() {
        let input = dataset(ORDERED);
        let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 1);
        assert!(
            matches!(report.outcome, OperationOutcome::Incomplete { .. }),
            "a one-derivation budget cannot settle a five-step method: {:?}",
            report.outcome
        );
        assert!(!report.is_closed());
        assert!(
            report.candidates.is_empty(),
            "the engine commits only on a complete run, so a cut leaves no roster to \
             misread as closed"
        );
    }

    #[test]
    fn a_malformed_yields_chain_is_an_invalid_request_not_a_quietly_shorter_plan() {
        let input = dataset(&format!(
            r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <{NS}> .
e:m a logic:DecompositionMethod ; logic:methodDecomposes e:t ; logic:methodYields e:c1 .
e:c1 rdf:first e:s1 ; rdf:rest e:c2 .
e:c2 rdf:first e:s2 .
"#
        ));
        let report = refine(input.as_ref(), &format!("{NS}t"), ACYCLIC, 10_000);
        assert!(
            matches!(report.outcome, OperationOutcome::Invalid { .. }),
            "a chain that never reaches rdf:nil denotes no sequence: {:?}",
            report.outcome
        );
    }

    #[test]
    fn a_method_naming_two_yields_lists_is_an_invalid_request() {
        let input = dataset(&format!(
            r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <{NS}> .
e:m a logic:DecompositionMethod ; logic:methodDecomposes e:t ;
    logic:methodYields e:a1 , e:b1 .
e:a1 rdf:first e:s1 ; rdf:rest rdf:nil .
e:b1 rdf:first e:s2 ; rdf:rest rdf:nil .
"#
        ));
        let report = refine(input.as_ref(), &format!("{NS}t"), ACYCLIC, 10_000);
        assert!(matches!(report.outcome, OperationOutcome::Invalid { .. }));
    }

    #[test]
    fn a_task_the_input_never_mentions_is_an_invalid_request_not_an_empty_roster() {
        let input = dataset(ORDERED);
        let report = refine(input.as_ref(), &format!("{NS}nosuchtask"), ACYCLIC, 10_000);
        assert!(
            matches!(report.outcome, OperationOutcome::Invalid { .. }),
            "a typo'd task must not read as 'no decomposition exists': {:?}",
            report.outcome
        );
    }

    #[test]
    fn an_undeclared_search_fragment_is_an_invalid_request() {
        let input = dataset(ORDERED);
        let report = refine(
            input.as_ref(),
            &format!("{NS}ocr"),
            "https://example.org/NotAFragment",
            10_000,
        );
        assert!(matches!(report.outcome, OperationOutcome::Invalid { .. }));
    }

    /// The capability rejection names the MISSING CAPABILITY, which is what makes the
    /// refusal actionable rather than merely honest.
    #[test]
    fn an_operational_capability_gap_rejects_the_step_and_names_the_capability() {
        let input = dataset(&format!(
            r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix e:     <{NS}> .
e:gap a logic:OperationalCapabilityGap ; logic:gapBlockedStep e:ocr .
e:proposal a logic:CapabilityGapProposal ;
    logic:proposalBlockedStep e:ocr ;
    logic:proposalMissingCapability e:ocrCapability .
"#
        ));
        let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 10_000);
        assert!(report.is_closed(), "{:?}", report.outcome);
        let rejection = report
            .rejections
            .iter()
            .find(|r| r.kind == RejectionKind::Capability)
            .expect("the capability gap must reject the step it blocks");
        assert_eq!(rejection.witness_iri, format!("{NS}ocrCapability"));
        assert!(
            rejection
                .witness
                .rule_iri
                .ends_with("/ruleRefinementRejectedOnCapability")
        );
    }

    #[test]
    fn an_undetached_approval_rejects_the_step_it_gates() {
        let input = dataset(&format!(
            r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix e:     <{NS}> .
e:entry a logic:FrontierEntry ;
    logic:entryAction e:publish ;
    logic:entryAxisWitness logic:StepReady , logic:ApprovalCreated .
"#
        ));
        let report = refine(input.as_ref(), &format!("{NS}publish"), ACYCLIC, 10_000);
        assert!(report.is_closed(), "{:?}", report.outcome);
        let rejection = report
            .rejections
            .iter()
            .find(|r| r.kind == RejectionKind::Approval)
            .expect("a created-but-undetached approval gates the step");
        assert_eq!(rejection.witness_iri, format!("{NS}entry"));
    }

    #[test]
    fn an_awaited_resource_under_a_lease_rejects_the_step_on_resource() {
        let input = dataset(&format!(
            r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix e:     <{NS}> .
e:entry a logic:FrontierEntry ;
    logic:entryAction e:closeValve ;
    logic:entryAxisWitness logic:StepWaiting ;
    logic:entryAwaits e:valve104 .
e:valveLease a logic:ResourceLease ; logic:leaseScope e:valve104 .
"#
        ));
        let report = refine(input.as_ref(), &format!("{NS}closeValve"), ACYCLIC, 10_000);
        assert!(report.is_closed(), "{:?}", report.outcome);
        let rejection = report
            .rejections
            .iter()
            .find(|r| r.kind == RejectionKind::Resource)
            .expect("a lease over the awaited resource is what stands in the way");
        assert_eq!(rejection.witness_iri, format!("{NS}valveLease"));
    }

    #[test]
    fn a_denied_action_gate_rejects_the_step_on_its_precondition() {
        let input = dataset(&format!(
            r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix e:     <{NS}> .
e:extract a logic:ActionSchema ; logic:precondition e:pagesRasterised .
e:probe a logic:GateProbe ;
    logic:probesSchema e:extract ;
    logic:gateVerdict logic:GateDenied .
"#
        ));
        let report = refine(input.as_ref(), &format!("{NS}extract"), ACYCLIC, 10_000);
        assert!(report.is_closed(), "{:?}", report.outcome);
        let rejection = report
            .rejections
            .iter()
            .find(|r| r.kind == RejectionKind::Precondition)
            .expect("a denied gate rejects on the precondition it judged");
        assert_eq!(rejection.witness_iri, format!("{NS}pagesRasterised"));
    }

    /// All four typed reasons are reachable on one graph, which is what "per-candidate
    /// precondition / capability / resource / approval reasons" actually requires.
    #[test]
    fn all_four_rejection_kinds_are_derivable_over_one_expansion() {
        let input = dataset(&format!(
            r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <{NS}> .
e:m a logic:DecompositionMethod ; logic:methodDecomposes e:root ; logic:methodYields e:c1 .
e:c1 rdf:first e:needsCapability ; rdf:rest e:c2 .
e:c2 rdf:first e:needsApproval   ; rdf:rest e:c3 .
e:c3 rdf:first e:needsResource   ; rdf:rest e:c4 .
e:c4 rdf:first e:needsPrecondition ; rdf:rest rdf:nil .

e:gap a logic:OperationalCapabilityGap ; logic:gapBlockedStep e:needsCapability .
e:proposal a logic:CapabilityGapProposal ;
    logic:proposalBlockedStep e:needsCapability ;
    logic:proposalMissingCapability e:ocrCapability .

e:approvalEntry a logic:FrontierEntry ;
    logic:entryAction e:needsApproval ;
    logic:entryAxisWitness logic:ApprovalCreated .

e:resourceEntry a logic:FrontierEntry ;
    logic:entryAction e:needsResource ;
    logic:entryAwaits e:valve104 .
e:valveLease a logic:ResourceLease ; logic:leaseScope e:valve104 .

e:needsPrecondition a logic:ActionSchema ; logic:precondition e:pagesRasterised .
e:probe a logic:GateProbe ;
    logic:probesSchema e:needsPrecondition ;
    logic:gateVerdict logic:GateDenied .
"#
        ));
        let report = refine(input.as_ref(), &format!("{NS}root"), ACYCLIC, 100_000);
        assert!(report.is_closed(), "{:?}", report.outcome);
        let kinds: std::collections::BTreeSet<RejectionKind> =
            report.rejections.iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds.len(),
            4,
            "all four typed reasons must be derivable over one expansion, got {kinds:?}"
        );
        for rejection in &report.rejections {
            assert!(
                !rejection.witness.rule_iri.is_empty() && !rejection.witness.premises.is_empty(),
                "every rejection must carry the chase premises that established it: {rejection:?}"
            );
        }
    }

    // ── The pin half: an authorized candidate becomes a commitment ON THE CHASE ────────
    //
    // Three laws read a logic:PinnedExecutableSubgraph and nothing in the repository could
    // produce one: `logic:rule…Pin…` matched no rule, and `gmeow logic refine` emitted no
    // pin, so a pin could only ever be hand-authored — the same defect the kernel condemns
    // for frontier labels, at the other end of the same episode. These two tests are the
    // red/green pair for the six rules that close it.

    /// A roster whose candidate carries its method and its licensing proof — every input
    /// the pin derivation needs. Nothing here IS a pin: no `logic:PinnedExecutableSubgraph`
    /// type, no `logic:pinnedStepSequence`, no `logic:pinDigest`, no `logic:pinAuthority`,
    /// no `logic:selectedPin`. All five are derived.
    const AUTHORIZED_CANDIDATE: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <https://blackcatinformatics.ca/gmeow/refinetest/> .

e:byPagePartition a logic:DecompositionMethod ;
    logic:methodDecomposes e:ocr ;
    logic:methodYields e:c1 ;
    logic:methodDigest "b3:4f1d0a97c25e6b3810df7a49b6c02e5318da7c4095b2ae6d3417f08c25be9d61" .
e:c1 rdf:first e:inspect ; rdf:rest e:c2 .
e:c2 rdf:first e:extract ; rdf:rest e:c3 .
e:c3 rdf:first e:verify  ; rdf:rest rdf:nil .

e:episode a logic:RefinementEpisode ;
    logic:refinesStep e:ocr ;
    logic:searchFragment logic:FragmentAcyclicMethod ;
    logic:producedCandidateSet e:roster .
e:roster a logic:RefinementCandidateSet ;
    logic:refinementCandidate e:ocrByPagePartitionCandidate .
e:ocrByPagePartitionCandidate logic:candidateInstantiatesMethod e:byPagePartition .

e:ocrAuthProof a logic:AuthorizationProof ;
    logic:proofEstablishes e:ocrByPagePartitionCandidate .
"#;

    /// GREEN — the authorized candidate yields a complete pin, and every field of it is
    /// concluded by an authored rule the witness names.
    #[test]
    fn an_authorized_candidate_yields_a_derived_pin_on_the_chase() {
        let input = dataset(AUTHORIZED_CANDIDATE);
        let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 100_000);
        assert!(report.is_closed(), "{:?}", report.outcome);
        assert_eq!(
            report.pins.len(),
            1,
            "the authorized candidate must yield exactly one pin: {:?}",
            report.pins
        );
        let pin = &report.pins[0];
        assert_eq!(pin.pin, format!("{NS}ocrByPagePartitionCandidate"));
        assert_eq!(pin.episode, format!("{NS}episode"));
        assert_eq!(pin.method, format!("{NS}byPagePartition"));
        assert_eq!(
            pin.steps,
            vec![
                format!("{NS}inspect"),
                format!("{NS}extract"),
                format!("{NS}verify"),
            ],
            "the frozen sequence IS the method's yielded sequence, in the authored order — \
             which is what makes the steps-match-method law unviolatable by a derived pin"
        );
        assert_eq!(
            pin.digest, "b3:4f1d0a97c25e6b3810df7a49b6c02e5318da7c4095b2ae6d3417f08c25be9d61",
            "the content address is the method version's own; a derived pin never invents one"
        );
        assert_eq!(pin.authority, format!("{NS}ocrAuthProof"));
        assert!(
            pin.witness
                .rule_iri
                .ends_with("/rulePinnedStepSequenceFromMethod"),
            "the frozen content must name the authored rule that concluded it, got {}",
            pin.witness.rule_iri
        );
        assert!(
            !pin.witness.premises.is_empty(),
            "a pin whose derivation cites no premises is a pin nobody can check"
        );
    }

    /// RED — the SAME roster with the authorization withdrawn derives no pin at all.
    ///
    /// The only edit is the proof's type: the record still exists, still names the
    /// candidate, still sits in the same graph. Deleting it would make the scene pin-free
    /// for a reason that has nothing to do with authority — no record, no join, no pin —
    /// and would leave rules that fire on any roster equally well. What changes here is
    /// whether the thing that licensed the candidate IS an authorization proof, which is
    /// the whole content of "an AUTHORIZED candidate".
    #[test]
    fn an_unauthorized_candidate_yields_no_pin() {
        let unlicensed = AUTHORIZED_CANDIDATE.replace(
            "e:ocrAuthProof a logic:AuthorizationProof ;",
            "e:ocrAuthProof a logic:Advisory ;",
        );
        assert_ne!(
            unlicensed, AUTHORIZED_CANDIDATE,
            "the edit must actually change the fixture, or the red half proves nothing"
        );
        let input = dataset(&unlicensed);
        let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 100_000);
        assert!(report.is_closed(), "{:?}", report.outcome);
        assert!(
            report.pins.is_empty(),
            "advice may motivate a pin and never licenses one, so a candidate whose only \
             backing is a logic:Advisory must freeze nothing: {:?}",
            report.pins
        );
        assert!(
            !report.candidates.is_empty(),
            "the roster must still be there — otherwise the red half is 'the search found \
             nothing', which is a different claim entirely"
        );
    }

    /// A method with no content address yields NO pin, rather than an unaddressed one.
    ///
    /// The completeness law requires the digest, so a pin derived without one would be a
    /// commitment the kernel is about to condemn. Refusing to report it is the honest
    /// failure: 'content-addressed' is a claim about the METHOD, and a method that never
    /// made it cannot lend it to a pin.
    #[test]
    fn a_method_with_no_digest_yields_no_pin_rather_than_an_unaddressed_one() {
        let undigested = AUTHORIZED_CANDIDATE.replace(
            " ;\n    logic:methodDigest \"b3:4f1d0a97c25e6b3810df7a49b6c02e5318da7c4095b2ae6d3417f08c25be9d61\" .",
            " .",
        );
        assert_ne!(
            undigested, AUTHORIZED_CANDIDATE,
            "the edit must remove the digest"
        );
        let input = dataset(&undigested);
        let report = refine(input.as_ref(), &format!("{NS}ocr"), ACYCLIC, 100_000);
        assert!(report.is_closed(), "{:?}", report.outcome);
        assert!(
            report.pins.is_empty(),
            "an unaddressed fragment is not a pin, and reporting one would tell an operator \
             something was frozen when nothing was: {:?}",
            report.pins
        );
    }

    /// A rejection standing outside the refined task's expansion is not this refinement's
    /// business, and reporting it would attribute an unrelated blockage to this plan.
    #[test]
    fn a_rejection_outside_the_expansion_is_not_reported() {
        let input = dataset(&format!(
            r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix e:     <{NS}> .
e:m a logic:DecompositionMethod ; logic:methodDecomposes e:root ; logic:methodYields e:c1 .
e:c1 rdf:first e:innocent ; rdf:rest rdf:nil .
e:gap a logic:OperationalCapabilityGap ; logic:gapBlockedStep e:elsewhere .
e:proposal a logic:CapabilityGapProposal ;
    logic:proposalBlockedStep e:elsewhere ;
    logic:proposalMissingCapability e:someCapability .
"#
        ));
        let report = refine(input.as_ref(), &format!("{NS}root"), ACYCLIC, 100_000);
        assert!(report.is_closed(), "{:?}", report.outcome);
        assert!(
            report.rejections.is_empty(),
            "an unrelated step's blockage must not be attributed to this expansion: {:?}",
            report.rejections
        );
    }
}
