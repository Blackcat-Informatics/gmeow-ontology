// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! OWL 2 RL/RDF deductive closure, computed by purrdf's `entail` chase.
//!
//! This is the Docker/Java-free entailment authority for the RL lane. The closure
//! is purrdf's OWL 2 RL materialization ([`purrdf::entail::materialize`] with
//! [`purrdf::entail::Materialization::OwlRl`]) — the full 78-rule calculus of
//! OWL 2 Profiles §4.3 Tables 4–9 — surfaced through the [`RlClosure`]/[`RlTriple`]
//! contract this module's consumers depend on. The retired native rule table only
//! ever fired a sound 32-rule subset; the cutover keeps the public shape and widens
//! the entailments to the whole profile.
//!
//! # Evaluation ceilings are external and honestly hard-failed
//!
//! purrdf's chase runs under fixed evaluation ceilings (`MAX_JOIN_STEPS`,
//! `MAX_STORED_FACTS`, `MAX_TERM_ARENA_BYTES` in the datalog engine) that are
//! upstream `pub const`s — not tunable from this repository. The in-repo obligation
//! is therefore honesty, not raising them: whenever [`purrdf::entail::materialize`]
//! (or [`purrdf::entail::explain_conclusion`]) exhausts a ceiling, the refusal
//! (`MatchBudget`, `Evaluate(EvalError::BudgetExhausted)`, `Chase(ChaseError::BudgetExhausted)`)
//! is propagated verbatim as a hard [`gmeow_errors`] error through `?`, never
//! swallowed and never presented as a complete-looking partial closure. Raising a
//! ceiling to make a hard instance answerable is an upstream-purrdf change; the honest
//! in-repo response to exhaustion is to fail, not to truncate silently.
//!
//! # The RDF 1.2 world ⇔ graph mapping (the encode/decode boundary)
//!
//! Every reasoning fact is world-scoped. The world of a triple is the RDF 1.2
//! quad's fourth position — its named graph. This module folds that axis at exactly
//! two points, and the two are inverses:
//!
//! * **encode** ([`lower_edb_for_rl`]): the input EDB is lowered into the dataset the
//!   chase closes. A named IRI graph becomes its own world; a default or blank-node
//!   graph folds to the [`DEFAULT_WORLD`] named graph, so an un-named graph still
//!   closes in one world. Because everything is emitted into a NAMED graph, the
//!   default graph the chase closes is empty, so each world closes against itself
//!   alone and two worlds never mix — the same per-world independence the native
//!   encoder gave with its explicit `?w` thread.
//! * **decode** ([`rl_closure`]): each closure quad's graph slot is read back to the
//!   world string — a named IRI graph to its IRI, anything else to [`DEFAULT_WORLD`].
//!
//! # `is_edb` and rule attribution
//!
//! purrdf's OWL 2 RL closure carries the asserted quads (the seed is copied into the
//! result) alongside every derived one. A closure quad present in the lowered EDB is
//! `is_edb` (asserted); the rest are derived. `is_edb` is decided eagerly and cheaply
//! (set membership against the lowered EDB).
//!
//! The firing rule of a derived triple is attributed by re-explaining the conclusion
//! over the retained lowered EDB ([`RlClosure::rule_name`] → [`rule_name_for_conclusion`]).
//! This is computed ON DEMAND, not eagerly, because purrdf exposes no BULK per-triple
//! provenance: [`purrdf::entail::materialize`]'s report gives only aggregate
//! `rules_fired()` counts, and [`purrdf::entail::explain_conclusion`] re-runs the whole
//! closure fixpoint for each conclusion. Attributing every derived triple eagerly is
//! therefore `O(derived × closure)` — measured at ~27s for a two-module scoped closure
//! whose materialization alone is ~0.1s — so a caller pays that cost only for the
//! triples it actually asks about. OWL 2 RL has no existential heads, so every derived
//! conclusion has a checkable derivation and the attribution never refuses.

use crate::facts::{SKOLEM_PREFIX, skolem_iri};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermValue};

/// Wrap a reasoning-driver condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn reason_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

/// The `xsd:string` datatype IRI — the datatype a plain literal carries once purrdf
/// has expanded it, and the one an N-Triples object form leaves implicit.
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// The sentinel world IRI a default-graph (un-named) triple is closed under.
///
/// A default or blank-node graph carries no world of its own, so its triples fold to
/// this single named world; a named IRI graph keeps its own IRI. The value is the
/// same one the retired native encoder used, so a downstream consumer folding the
/// closure back into an un-named graph still recognizes it.
pub const DEFAULT_WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/rl-default";

/// One triple in the RL closure, decoded from a closure quad.
///
/// `subject`/`predicate` are bare IRI strings (a blank-node subject is rendered as
/// its skolem IRI); `object` is the N-Triples object form (`<iri>`, a skolem
/// `<iri>` for a blank node, or a quoted literal `"v"` / `"v"@lang` / `"v"^^<dt>`);
/// `world` is the named-graph IRI (or [`DEFAULT_WORLD`]).
/// `is_edb` distinguishes asserted facts (`true`) from rule-derived ones.
///
/// The firing rule of a derived triple is NOT stored here — it is attributed on
/// demand by [`RlClosure::rule_name`]; see the module docs for why eager attribution
/// is `O(derived × closure)` under purrdf's public API.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RlTriple {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub world: String,
    pub is_edb: bool,
}

/// The result of an OWL 2 RL closure run: every asserted + derived triple.
///
/// Also retains the lowered EDB and the materialized closure so [`Self::rule_name`]
/// can attribute a derived triple's firing rule on demand. Those two carriers are
/// reasoner state, not closure content, so [`PartialEq`]/[`Eq`] compare only
/// [`Self::triples`]: two closures with equal triples are equal.
#[derive(Debug, Clone)]
pub struct RlClosure {
    pub triples: Vec<RlTriple>,
    /// The lowered EDB the closure was reasoned from, for on-demand attribution.
    /// `None` for a hand-constructed closure (e.g. a render fixture).
    edb: Option<std::sync::Arc<RdfDataset>>,
    /// The materialized closure, for recovering a derived triple's typed terms during
    /// attribution. `None` for a hand-constructed closure.
    closure: Option<std::sync::Arc<RdfDataset>>,
}

impl PartialEq for RlClosure {
    fn eq(&self, other: &Self) -> bool {
        self.triples == other.triples
    }
}

impl Eq for RlClosure {}

impl RlClosure {
    /// Render the full closure as a deterministic N-Triples document.
    ///
    /// A skolemized blank-node IRI (`{SKOLEM_PREFIX}…`) is mapped back to an
    /// N-Triples blank-node label so a source blank node round-trips as a blank
    /// node; every other subject/predicate is a NamedNode and the object is already
    /// in N-Triples object form (`<iri>` or a quoted literal). The world axis is
    /// dropped (default-graph N-Triples); lines are de-duplicated and sorted for a
    /// byte-stable result.
    #[must_use]
    pub fn to_ntriples(&self) -> String {
        let mut lines: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for t in &self.triples {
            let s = render_nt_resource(&t.subject);
            let p = render_nt_resource(&t.predicate);
            let o = render_nt_object(&t.object);
            lines.insert(format!("{s} {p} {o} ."));
        }
        let mut out = String::new();
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    /// The OWL 2 RL specification rule name credited with deriving `triple`, or `None`
    /// for an asserted (EDB) triple.
    ///
    /// Computed ON DEMAND via purrdf's [`purrdf::entail::explain_conclusion`] over the
    /// retained lowered EDB — see the module docs for why eager attribution is
    /// impractical under purrdf's public API. Deterministic: the specification-table-first
    /// rule the derivation cites (the single head rule is not exposed on purrdf's public
    /// `ChaseProof` surface).
    ///
    /// # Errors
    ///
    /// A [`Reason`](crate::error::Reason) diagnostic if this closure carries no reasoner
    /// input (a hand-constructed closure), if `triple` is not a member of it, or if purrdf
    /// refuses to explain a derived triple (impossible for OWL 2 RL, which has no
    /// existential heads).
    pub fn rule_name(&self, triple: &RlTriple) -> gmeow_errors::Result<Option<String>> {
        if triple.is_edb {
            return Ok(None);
        }
        let (Some(edb), Some(closure)) = (self.edb.as_ref(), self.closure.as_ref()) else {
            return Err(reason_err(format!(
                "RL closure carries no reasoner input, so the firing rule of derived triple \
                 {} {} {} cannot be attributed",
                triple.subject, triple.predicate, triple.object
            )));
        };
        // Recover the conclusion's typed terms from the materialized closure: a blank
        // node's original identity is not recoverable from its rendered skolem string, so
        // attribution re-explains over the typed terms the closure still holds.
        for quad in closure.quads() {
            let subj_value = closure.term_value(quad.s);
            let pred_value = closure.term_value(quad.p);
            let obj_value = closure.term_value(quad.o);
            let (Some(subject), Some(predicate), Some(object)) = (
                render_subject_value(&subj_value),
                pred_value.as_iri().map(str::to_owned),
                render_object_value(&obj_value),
            ) else {
                continue;
            };
            let graph_value = quad.g.map(|g| closure.term_value(g));
            let world = world_string_of(graph_value.as_ref());
            if subject == triple.subject
                && predicate == triple.predicate
                && object == triple.object
                && world == triple.world
            {
                return rule_name_for_conclusion(
                    edb.as_ref(),
                    graph_value.as_ref(),
                    &subj_value,
                    &pred_value,
                    &obj_value,
                );
            }
        }
        Err(reason_err(format!(
            "derived triple {} {} {} (world {}) is not a member of this RL closure",
            triple.subject, triple.predicate, triple.object, triple.world
        )))
    }
}

/// Render an engine subject/predicate IRI (bare) as an N-Triples term: a skolem
/// IRI becomes a blank-node label, every other value a NamedNode.
fn render_nt_resource(value: &str) -> String {
    if let Some(tail) = value.strip_prefix(SKOLEM_PREFIX) {
        format!("_:{}", skolem_label(tail))
    } else {
        format!("<{value}>")
    }
}

/// Render an engine object term (already N-Triples display form) for re-parse. An
/// IRI object that is a skolem IRI is rewritten to a blank-node label; literals
/// (already valid N-Triples literals) pass through verbatim.
fn render_nt_object(obj_nt: &str) -> String {
    if let Some(inner) = obj_nt.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
        if let Some(tail) = inner.strip_prefix(SKOLEM_PREFIX) {
            return format!("_:{}", skolem_label(tail));
        }
        return obj_nt.to_owned();
    }
    // Literal (`"v"`, `"v"@lang`, `"v"^^<dt>`) — already valid N-Triples.
    obj_nt.to_owned()
}

/// Derive a syntactically-valid N-Triples blank-node label from a skolem tail
/// (the identifier after [`SKOLEM_PREFIX`]): prefix `b`, replace any character not
/// permitted in a label with `_`.
fn skolem_label(tail: &str) -> String {
    let mut label = String::with_capacity(tail.len() + 1);
    label.push('b');
    for ch in tail.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            label.push(ch);
        } else {
            label.push('_');
        }
    }
    label
}

/// Render a literal's parts as its N-Triples object form (`"v"`, `"v"@lang`,
/// `"v"^^<dt>`) — the form a downstream N-Triples parser reads back losslessly.
fn literal_nt(lexical_form: &str, datatype: &str, language: Option<&str>) -> String {
    // N-Triples requires escaping `\`, `"`, newline, CR, and tab in the value.
    let escaped = lexical_form
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    if let Some(lang) = language {
        format!("\"{escaped}\"@{lang}")
    } else if datatype == XSD_STRING {
        format!("\"{escaped}\"")
    } else {
        format!("\"{escaped}\"^^<{datatype}>")
    }
}

/// Render a closure subject term to the bare-string form [`RlTriple::subject`] holds:
/// an IRI verbatim, a blank node as its skolem IRI (so [`RlClosure::to_ntriples`]
/// re-derives a blank-node label). A literal or triple-term subject has no
/// standard-RDF form and drops the row — a literal can never be a triple subject and
/// a triple term is unsupported in this lane, exactly the rows the native authority
/// dropped.
fn render_subject_value(term: &TermValue) -> Option<String> {
    match term {
        TermValue::Iri(iri) => Some(iri.clone()),
        TermValue::Blank { label, .. } => Some(skolem_iri(label)),
        TermValue::Literal { .. } | TermValue::Triple { .. } => None,
    }
}

/// Render a closure object term to the N-Triples object form [`RlTriple::object`]
/// holds: `<iri>` for an IRI, `<skolem>` for a blank node (so
/// [`RlClosure::to_ntriples`] re-derives a blank-node label), the quoted literal form
/// for a literal. A triple-term object is unsupported and drops the row.
fn render_object_value(term: &TermValue) -> Option<String> {
    match term {
        TermValue::Iri(iri) => Some(format!("<{iri}>")),
        TermValue::Blank { label, .. } => Some(format!("<{}>", skolem_iri(label))),
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } => Some(literal_nt(lexical_form, datatype, language.as_deref())),
        TermValue::Triple { .. } => None,
    }
}

/// The world an RDF 1.2 quad's fourth-position graph slot denotes: a named IRI graph
/// is its own world; a default or blank-node graph folds to [`DEFAULT_WORLD`], the
/// single world an un-named graph closes in — the decode inverse of
/// [`lower_edb_for_rl`]'s encode fold.
fn world_string_of(graph: Option<&TermValue>) -> String {
    match graph {
        Some(TermValue::Iri(iri)) => iri.clone(),
        _ => DEFAULT_WORLD.to_owned(),
    }
}

/// Lower the input EDB into the [`RdfDataset`] the OWL 2 RL chase closes.
///
/// Two boundary transforms over assertions from all native RDF tables:
///
/// * **canonical → W3C spelling.** Every quad is emitted under each predicate spelling
///   [`super::edb_predicate_spellings`] yields, while an IRI object is normalized through
///   [`super::calculus_term`]. A canonical `logic:subClassOf` / `logic:subPropertyOf`
///   predicate is therefore also asserted under the `rdfs:` spelling, and canonical
///   class/property markers in object position reach the `owl:`/`rdfs:` constants the
///   fixed RL rules match. The authored predicate edge is kept too: predicate lowering
///   adds its W3C view rather than replacing authored data.
/// * **world ⇔ graph** (see the module docs): a named IRI graph is its own world; a
///   default or blank-node graph folds to the [`DEFAULT_WORLD`] named graph.
///
/// A triple-term subject or object is skipped: it has no place in this lane's encoding
/// and never appears in an RL fixture. `Ok(None)` means the lowered EDB is empty (no
/// quad survived), for which the closure is empty and no chase is run.
///
/// # Errors
///
/// A [`Reason`](crate::error::Reason) diagnostic if the lowered dataset cannot be
/// frozen.
fn lower_edb_for_rl(edb: &RdfDataset) -> gmeow_errors::Result<Option<std::sync::Arc<RdfDataset>>> {
    let mut builder = RdfDatasetBuilder::new();
    let mut pushed = false;
    for quad in purrdf::native_quads::flat_rdf_quads(edb) {
        if matches!(&quad.subject, RdfTerm::Triple(_)) || matches!(&quad.object, RdfTerm::Triple(_))
        {
            continue;
        }
        let object = match &quad.object {
            RdfTerm::Iri(iri) => RdfTerm::iri(super::calculus_term(iri)),
            other => other.clone(),
        };
        let world = match &quad.graph_name {
            Some(RdfTerm::Iri(iri)) => iri.clone(),
            _ => DEFAULT_WORLD.to_owned(),
        };
        for predicate in super::edb_predicate_spellings(&quad.predicate) {
            let lowered = RdfQuad::new(quad.subject.clone(), predicate, object.clone())
                .in_graph(RdfTerm::iri(world.clone()));
            builder.push_owned_quad(&lowered);
            pushed = true;
        }
    }
    if !pushed {
        return Ok(None);
    }
    builder
        .freeze()
        .map(Some)
        .map_err(|e| reason_err(format!("freeze lowered RL EDB: {e}")))
}

/// The OWL 2 RL specification rule name to credit a DERIVED closure triple to.
///
/// purrdf's [`purrdf::entail::explain_conclusion`] rebuilds the conclusion's
/// derivation over the same lowered EDB, and [`ChaseProof::rules`] returns every rule
/// that derivation cites, in specification-table order, deduplicated. The single head
/// rule is not exposed on the public `ChaseProof` surface, so the deterministic
/// representative used here is the specification-table-first cited rule. A conclusion
/// citing no rule is asserted rather than derived and carries no name.
///
/// [`ChaseProof::rules`]: purrdf::entail::ChaseProof::rules
///
/// # Errors
///
/// A [`Reason`](crate::error::Reason) diagnostic if purrdf refuses to explain a triple
/// the closure derived — for OWL 2 RL (no existential heads) that never happens, so a
/// refusal is a real defect surfaced rather than a wrong or absent attribution.
fn rule_name_for_conclusion(
    lowered: &RdfDataset,
    graph: Option<&TermValue>,
    subject: &TermValue,
    predicate: &TermValue,
    object: &TermValue,
) -> gmeow_errors::Result<Option<String>> {
    let proof = purrdf::entail::explain_conclusion(
        lowered,
        purrdf::entail::Regime::OwlRl,
        graph,
        subject,
        predicate,
        object,
    )
    .map_err(|e| {
        reason_err(format!(
            "OWL 2 RL rule attribution refused for a derived triple \
             ({subject:?} {predicate:?} {object:?}): {e}"
        ))
    })?;
    Ok(proof.rules().first().map(|rule| rule.as_str().to_owned()))
}

/// Compute the OWL 2 RL/RDF deductive closure of `edb`.
///
/// Lowers `edb` into the chase dataset ([`lower_edb_for_rl`]), runs purrdf's OWL 2 RL
/// materialization once, and decodes every closure quad back into an [`RlTriple`]
/// (asserted + derived). The closure is world-scoped: a derived triple carries the
/// world of the graph that produced it.
///
/// # Errors
///
/// Returns an error if purrdf's materialization refuses (an evaluation ceiling, an
/// inconsistency witness, or a build failure) or if the rule attribution of a derived
/// triple refuses.
pub fn rl_closure(edb: &RdfDataset) -> gmeow_errors::Result<RlClosure> {
    let Some(lowered) = lower_edb_for_rl(edb)? else {
        return Ok(RlClosure {
            triples: vec![],
            edb: None,
            closure: None,
        });
    };

    let (closure, _report) =
        purrdf::entail::materialize(lowered.as_ref(), purrdf::entail::Materialization::OwlRl)
            .map_err(|e| reason_err(format!("OWL 2 RL materialization refused: {e}")))?;

    // The asserted rows of the lowered EDB, keyed by the same rendered surfaces the
    // closure decode produces, so a closure row present in the EDB reads `is_edb`.
    let mut edb_rows: std::collections::HashSet<(String, String, String, String)> =
        std::collections::HashSet::new();
    for quad in lowered.quads() {
        let (Some(subject), Some(predicate), Some(object)) = (
            render_subject_value(&lowered.term_value(quad.s)),
            lowered.term_value(quad.p).as_iri().map(str::to_owned),
            render_object_value(&lowered.term_value(quad.o)),
        ) else {
            continue;
        };
        let world = world_string_of(quad.g.map(|g| lowered.term_value(g)).as_ref());
        edb_rows.insert((subject, predicate, object, world));
    }

    let mut triples: Vec<RlTriple> = Vec::new();
    for quad in closure.quads() {
        let subj_value = closure.term_value(quad.s);
        let pred_value = closure.term_value(quad.p);
        let obj_value = closure.term_value(quad.o);
        let (Some(subject), Some(predicate), Some(object)) = (
            render_subject_value(&subj_value),
            pred_value.as_iri().map(str::to_owned),
            render_object_value(&obj_value),
        ) else {
            continue;
        };
        let graph_value = quad.g.map(|g| closure.term_value(g));
        let world = world_string_of(graph_value.as_ref());
        let key = (subject, predicate, object, world);
        let is_edb = edb_rows.contains(&key);
        let (subject, predicate, object, world) = key;
        triples.push(RlTriple {
            subject,
            predicate,
            object,
            world,
            is_edb,
        });
    }
    Ok(RlClosure {
        triples,
        edb: Some(lowered),
        closure: Some(closure),
    })
}

#[path = "rl.tests.rs"]
#[cfg(test)]
mod tests;
