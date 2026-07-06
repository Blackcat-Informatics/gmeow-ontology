// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Executed lens-law discharge for a `logic:Correspondence`.
//!
//! A correspondence is an asymmetric lens: a forward `get` leg (down-projection to an
//! external vocabulary) and an inverse `put` leg (the ingest up-lift). The prior
//! round-trip gate compared the two legs' `LegPath` *bodies* syntactically — a purely
//! textual inversion audit that a re-authored cell carrying an unrecoverable guard atom
//! could slip past (the `mapSiocTopic` failure mode). This module discharges the laws by
//! EXECUTION instead: it RUNS both SPARQL `CONSTRUCT` legs through the same native engine
//! the pipeline put loop uses (`purrdf::sparql::NativeSparqlEngine`, via the shared
//! [`crate::put_executor::run_construct`]) and compares the resulting atom sets. A verdict
//! is behavioural, not textual.
//!
//! Two laws are discharged whenever both legs already run:
//!
//! * [`CorrespondenceLaw::SectionLaw`] — `put ∘ get = id_S`. For each source seed `s`:
//!   run `get` over `s` → the forward image `v`; run `put` over `v` → the recovered source
//!   `s'`; the law holds on that seed iff `s' == s` (no spurious atom fabricated, no source
//!   atom dropped). Discharged iff every seed round-trips exactly; otherwise Violated with a
//!   [`Countermodel`] naming the failing seed and its spurious/missing atoms.
//! * [`CorrespondenceLaw::PutGet`] — `get ∘ put = id_V` on the forward image. For each seed:
//!   `get(put(v)) == v`. Both legs already run for the section check, so this is computed for
//!   free from the same executions.
//!
//! ## Why the seed corpus is branch-covering (the load-bearing move)
//!
//! A single happy-path seed is a *test*, not a *proof*: a `put` atom that fabricates only on
//! inputs the seed never exercises would round-trip cleanly and pass. So [`derive_seeds`]
//! synthesises one seed per `UNION` branch of the `get` leg's `WHERE` clause — instantiating
//! every positive triple pattern of that branch with fresh, deterministic per-variable IRIs
//! (`http://seed.example/vN`) — PLUS one combined seed unioning all branches. Every guard
//! atom and every variable position of `get` is therefore exercised at least once. A `put`
//! branch that fabricates an atom keyed to a specific `get` branch's data is forced to fire
//! under that branch's dedicated seed, where its fabricated atom is not among the seed's
//! source atoms — so the round-trip inequality surfaces it. The combined seed additionally
//! exercises cross-branch interference. Nothing here reads the clock or a random source; the
//! seed IRIs vary only by a deterministic index, so a verdict (and its countermodel bytes)
//! are reproducible.

use std::collections::BTreeSet;

use gmeow_logic_compile::ir::{
    CorrespondenceLaw, DischargeCondition, DischargeVerdict, LawClaimIr, MorphismClass,
};
use purrdf::sparql::NativeSparqlEngine;
use purrdf::{RdfQuad, RdfTerm, parse_dataset};

use crate::put_executor::run_construct;
use crate::up_projection_corpus::dump_nt;

/// A comparable source/target atom: subject, predicate, object as canonical strings. IRIs are
/// verbatim, blank nodes `_:label`, literals `"lex"` (datatype/lang folded into the lexical
/// rendering the engine returns). This is the granularity every law comparison works over.
pub type Atom = (String, String, String);

/// A synthesised source graph exercising one branch (or the union of branches) of the `get`
/// leg. `label` is a deterministic identifier used in countermodels and for reproducibility;
/// `atoms` are the seed's source triples (all positions IRIs, from the branch's positive
/// patterns instantiated with fresh per-variable IRIs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedGraph {
    /// Deterministic branch label, e.g. `branch-0` or `combined`.
    pub label: String,
    /// The seed's source atoms.
    pub atoms: Vec<Atom>,
}

impl SeedGraph {
    /// Serialise the seed to N-Triples for the engine (every position is an IRI).
    fn to_ntriples(&self) -> String {
        let mut out = String::new();
        for (s, p, o) in &self.atoms {
            out.push_str(&format!("<{s}> <{p}> <{o}> .\n"));
        }
        out
    }
}

/// A refutation of a lens law on one seed: the spurious atoms `put ∘ get` fabricated that the
/// source never carried, and the missing atoms it dropped. For a non-executable leg the atom
/// lists are empty and `reason` carries the engine error — never a silent pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Countermodel {
    /// The seed on which the law failed.
    pub seed_label: String,
    /// Human-readable summary (also carries any engine error).
    pub reason: String,
    /// Atoms present after the round-trip but absent from the source (fabricated).
    pub spurious: Vec<Atom>,
    /// Atoms present in the source but lost by the round-trip (dropped).
    pub missing: Vec<Atom>,
}

/// The outcome of discharging one law over a seed corpus: the verdict, and — when Violated —
/// the first (lexically-least seed label) countermodel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DischargeOutcome {
    /// The verdict for the law.
    pub verdict: DischargeVerdict,
    /// Present iff `verdict == ObligationViolated`.
    pub countermodel: Option<Countermodel>,
}

/// Canonical comparison string for an RDF term (IRIs verbatim, blanks `_:id`, literals `"lex"`,
/// and an RDF-star quoted triple rendered RECURSIVELY as `<< s p o >>` — never a collapsing
/// placeholder). The recursion keeps the rendering injective over term kind, so two DISTINCT
/// quoted triples never compare equal in the atom sets a law verdict is computed from. This is a
/// *comparison* key only: the inter-leg carrier is serialised from the typed [`RdfQuad`]s via the
/// canonical purrdf N-Triples serializer ([`dump_nt`]), so faithful round-tripping never depends
/// on this rendering.
fn term_str(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(iri) => iri.clone(),
        RdfTerm::BlankNode(id) => format!("_:{id}"),
        RdfTerm::Literal(lit) => format!("\"{}\"", lit.lexical_form),
        RdfTerm::Triple(t) => format!(
            "<< {} {} {} >>",
            term_str(&t.subject),
            t.predicate,
            term_str(&t.object)
        ),
    }
}

/// The comparable [`Atom`] key of a constructed quad — subject/object by term kind, predicate
/// verbatim. Used to compare the recovered/reprojected graph against the source and to name
/// countermodel atoms; the quad itself is what gets serialised between legs.
fn quad_atom(quad: &RdfQuad) -> Atom {
    (
        term_str(&quad.subject),
        quad.predicate.clone(),
        term_str(&quad.object),
    )
}

/// Run a `CONSTRUCT` over an N-Triples source graph, returning the constructed default-graph
/// quads as TYPED [`RdfQuad`]s. Threading the typed terms (rather than re-rendering to `Atom`
/// strings here) is what lets the caller serialise the inter-leg carrier faithfully — a literal,
/// blank node, or RDF-star quoted triple in the constructed graph keeps its term kind all the way
/// to [`dump_nt`], instead of being flattened into a malformed `<...>`-wrapped IRI. Any
/// parse/engine failure is surfaced as `Err` (hard-fail) — never dropped.
fn run_leg(
    engine: &NativeSparqlEngine,
    source_nt: &str,
    query: &str,
) -> Result<Vec<RdfQuad>, String> {
    let dataset = parse_dataset(source_nt.as_bytes(), "application/n-triples", None)
        .map_err(|e| format!("failed to parse round-trip source graph: {e}"))?;
    run_construct(engine, &dataset, query)
}

/// Serialise the typed constructed quads back to an N-Triples graph for the next leg, via the
/// canonical purrdf serializer. Every term kind — IRI, literal (lexical form + datatype/lang),
/// blank node, and RDF-star quoted triple — is emitted in correct N-Triples syntax, so the next
/// leg's parser accepts the carrier instead of choking on a hand-built `<literal>`. A
/// serialization failure is surfaced as `Err` (hard-fail) — never a silent drop.
fn quads_to_ntriples(quads: &[RdfQuad]) -> Result<String, String> {
    dump_nt(quads)
}

/// Discharge [`CorrespondenceLaw::SectionLaw`] (`put ∘ get = id_S`) by EXECUTION over `seeds`.
///
/// For each seed: run `get_rq` → forward image; run `put_rq` over the forward image →
/// recovered source; the seed round-trips iff `recovered == source`. The verdict is
/// [`DischargeVerdict::ObligationDischarged`] iff every seed round-trips exactly, else
/// [`DischargeVerdict::ObligationViolated`] with the countermodel for the lexically-least
/// failing seed (deterministic). An empty corpus yields [`DischargeVerdict::ObligationUnknown`]
/// — an unchecked law is never "proved absent". A non-executable leg is Violated, never a pass.
pub fn discharge_section_law(get_rq: &str, put_rq: &str, seeds: &[SeedGraph]) -> DischargeOutcome {
    if seeds.is_empty() {
        return DischargeOutcome {
            verdict: DischargeVerdict::ObligationUnknown,
            countermodel: None,
        };
    }
    let engine = NativeSparqlEngine::new();
    // Deterministic order: the lexically-least failing seed wins, so the countermodel bytes are
    // stable regardless of input ordering.
    let mut ordered: Vec<&SeedGraph> = seeds.iter().collect();
    ordered.sort_by(|a, b| a.label.cmp(&b.label));

    for seed in ordered {
        let source: BTreeSet<Atom> = seed.atoms.iter().cloned().collect();
        let forward = match run_leg(&engine, &seed.to_ntriples(), get_rq) {
            Ok(v) => v,
            Err(e) => return violated(seed, format!("get leg is not executable: {e}")),
        };
        let forward_nt = match quads_to_ntriples(&forward) {
            Ok(nt) => nt,
            Err(e) => return violated(seed, format!("forward image is not serialisable: {e}")),
        };
        let recovered: BTreeSet<Atom> = match run_leg(&engine, &forward_nt, put_rq) {
            Ok(v) => v.iter().map(quad_atom).collect(),
            Err(e) => return violated(seed, format!("put leg is not executable: {e}")),
        };
        if recovered != source {
            let spurious: Vec<Atom> = recovered.difference(&source).cloned().collect();
            let missing: Vec<Atom> = source.difference(&recovered).cloned().collect();
            return DischargeOutcome {
                verdict: DischargeVerdict::ObligationViolated,
                countermodel: Some(Countermodel {
                    seed_label: seed.label.clone(),
                    reason: format!(
                        "put∘get did not recover the source on seed `{}`: {} spurious, {} missing",
                        seed.label,
                        spurious.len(),
                        missing.len()
                    ),
                    spurious,
                    missing,
                }),
            };
        }
    }
    DischargeOutcome {
        verdict: DischargeVerdict::ObligationDischarged,
        countermodel: None,
    }
}

/// Discharge [`CorrespondenceLaw::PutGet`] (`get ∘ put = id_V` on the forward image) by
/// EXECUTION over `seeds`. For each seed: `v = get(seed)`; `s' = put(v)`; `v' = get(s')`; the
/// law holds iff `v' == v`. Same verdict discipline as [`discharge_section_law`].
pub fn discharge_put_get_law(get_rq: &str, put_rq: &str, seeds: &[SeedGraph]) -> DischargeOutcome {
    if seeds.is_empty() {
        return DischargeOutcome {
            verdict: DischargeVerdict::ObligationUnknown,
            countermodel: None,
        };
    }
    let engine = NativeSparqlEngine::new();
    let mut ordered: Vec<&SeedGraph> = seeds.iter().collect();
    ordered.sort_by(|a, b| a.label.cmp(&b.label));

    for seed in ordered {
        let view_quads = match run_leg(&engine, &seed.to_ntriples(), get_rq) {
            Ok(v) => v,
            Err(e) => return violated(seed, format!("get leg is not executable: {e}")),
        };
        let view: BTreeSet<Atom> = view_quads.iter().map(quad_atom).collect();
        let view_nt = match quads_to_ntriples(&view_quads) {
            Ok(nt) => nt,
            Err(e) => return violated(seed, format!("forward image is not serialisable: {e}")),
        };
        let recovered_quads = match run_leg(&engine, &view_nt, put_rq) {
            Ok(v) => v,
            Err(e) => return violated(seed, format!("put leg is not executable: {e}")),
        };
        let recovered_nt = match quads_to_ntriples(&recovered_quads) {
            Ok(nt) => nt,
            Err(e) => return violated(seed, format!("recovered graph is not serialisable: {e}")),
        };
        let reprojected: BTreeSet<Atom> = match run_leg(&engine, &recovered_nt, get_rq) {
            Ok(v) => v.iter().map(quad_atom).collect(),
            Err(e) => return violated(seed, format!("get leg is not executable: {e}")),
        };
        if reprojected != view {
            let spurious: Vec<Atom> = reprojected.difference(&view).cloned().collect();
            let missing: Vec<Atom> = view.difference(&reprojected).cloned().collect();
            return DischargeOutcome {
                verdict: DischargeVerdict::ObligationViolated,
                countermodel: Some(Countermodel {
                    seed_label: seed.label.clone(),
                    reason: format!(
                        "get∘put did not preserve the view on seed `{}`: {} spurious, {} missing",
                        seed.label,
                        spurious.len(),
                        missing.len()
                    ),
                    spurious,
                    missing,
                }),
            };
        }
    }
    DischargeOutcome {
        verdict: DischargeVerdict::ObligationDischarged,
        countermodel: None,
    }
}

/// Build a Violated outcome carrying a non-executability countermodel.
fn violated(seed: &SeedGraph, reason: String) -> DischargeOutcome {
    DischargeOutcome {
        verdict: DischargeVerdict::ObligationViolated,
        countermodel: Some(Countermodel {
            seed_label: seed.label.clone(),
            reason,
            spurious: Vec::new(),
            missing: Vec::new(),
        }),
    }
}

/// The `PREFIX name: <iri>` bindings declared at the head of a SPARQL query, plus the fixed
/// `rdf:type` shorthand `a`.
fn parse_prefixes(query: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in query.lines() {
        let t = line.trim();
        let lower = t.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("prefix ") {
            // Re-slice the original (case-preserving) text after the keyword.
            let rest = t[t.len() - rest.len()..].trim();
            if let Some(colon) = rest.find(':') {
                let name = rest[..colon].trim().to_owned();
                let after = rest[colon + 1..].trim();
                if let (Some(lt), Some(gt)) = (after.find('<'), after.find('>')) {
                    let iri = after[lt + 1..gt].to_owned();
                    out.push((name, iri));
                }
            }
        }
    }
    out
}

/// Expand a `prefix:local` (or `<iri>`, or the `a` shorthand) token to a full IRI. Unknown
/// prefixes/bare tokens are returned verbatim so a malformed token later hard-fails at the
/// engine rather than being silently rewritten.
fn expand_iri(token: &str, prefixes: &[(String, String)]) -> Option<String> {
    let token = token.trim();
    if token == "a" {
        return Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned());
    }
    if token.starts_with('<') && token.ends_with('>') {
        return Some(token[1..token.len() - 1].to_owned());
    }
    if token.starts_with('?') || token.starts_with('$') {
        return None; // a variable, not an IRI
    }
    if let Some(colon) = token.find(':') {
        let pfx = &token[..colon];
        let local = &token[colon + 1..];
        for (name, iri) in prefixes {
            if name == pfx {
                return Some(format!("{iri}{local}"));
            }
        }
    }
    None
}

/// A single positive triple pattern of a `WHERE` branch: each position is either a variable
/// (`?x`) or a concrete IRI token.
#[derive(Debug, Clone)]
enum PatTerm {
    Var(String),
    Iri(String),
}

/// Split the `get` leg `WHERE` body into its top-level `UNION` branches, returning the raw
/// text of each `{ … }` group. Nested braces are balanced so a `FILTER NOT EXISTS { … }`
/// inside a branch stays with its branch.
fn split_union_branches(where_body: &str) -> Vec<String> {
    let bytes: Vec<char> = where_body.chars().collect();
    let mut branches = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '{' {
            let mut depth = 1;
            let start = i + 1;
            i += 1;
            while i < bytes.len() && depth > 0 {
                match bytes[i] {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                if depth == 0 {
                    break;
                }
                i += 1;
            }
            let group: String = bytes[start..i].iter().collect();
            branches.push(group);
        }
        i += 1;
    }
    branches
}

/// Extract the positive triple patterns of one branch, dropping any `FILTER … { … }` group
/// (guards are negative constraints; a covering seed simply omits their forbidden atoms). Each
/// pattern is three whitespace-separated tokens terminated by `.`.
fn branch_patterns(branch: &str) -> Vec<[PatTerm; 3]> {
    // Strip nested `{ … }` groups (FILTER NOT EXISTS bodies) so their patterns never leak in.
    let mut flat = String::new();
    let mut depth = 0;
    for ch in branch.chars() {
        match ch {
            '{' => depth += 1,
            '}' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            _ if depth == 0 => flat.push(ch),
            _ => {}
        }
    }
    // Drop any FILTER clause remnants (they cannot start a triple pattern we consume).
    let mut patterns = Vec::new();
    for stmt in flat.split('.') {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        let upper = stmt.to_ascii_uppercase();
        if upper.starts_with("FILTER") || upper.starts_with("BIND") || upper.starts_with("VALUES") {
            continue;
        }
        let toks: Vec<&str> = stmt.split_whitespace().collect();
        if toks.len() != 3 {
            continue; // only simple `s p o .` patterns participate in seed synthesis
        }
        let mk = |t: &str| -> PatTerm {
            if t.starts_with('?') || t.starts_with('$') {
                PatTerm::Var(t.to_owned())
            } else {
                PatTerm::Iri(t.to_owned())
            }
        };
        patterns.push([mk(toks[0]), mk(toks[1]), mk(toks[2])]);
    }
    patterns
}

/// Instantiate the positive patterns of one branch into concrete source atoms, assigning each
/// distinct variable a fresh deterministic IRI drawn from a running counter. Returns `None` if
/// any concrete term fails to expand to an IRI (a malformed/unsupported pattern) — that branch
/// contributes no seed rather than a fabricated one.
fn instantiate_branch(
    patterns: &[[PatTerm; 3]],
    prefixes: &[(String, String)],
    counter: &mut usize,
    var_iri: &mut std::collections::BTreeMap<String, String>,
) -> Option<Vec<Atom>> {
    let mut atoms = Vec::new();
    for pat in patterns {
        let mut resolved: [String; 3] = [String::new(), String::new(), String::new()];
        for (slot, term) in pat.iter().enumerate() {
            let iri = match term {
                PatTerm::Var(v) => var_iri
                    .entry(v.clone())
                    .or_insert_with(|| {
                        let iri = format!("http://seed.example/v{counter}");
                        *counter += 1;
                        iri
                    })
                    .clone(),
                PatTerm::Iri(tok) => expand_iri(tok, prefixes)?,
            };
            resolved[slot] = iri;
        }
        let [s, p, o] = resolved;
        atoms.push((s, p, o));
    }
    if atoms.is_empty() { None } else { Some(atoms) }
}

/// Derive the branch-covering seed corpus from the `get` leg query text.
///
/// One seed per top-level `UNION` branch of the `WHERE` clause (each instantiating that
/// branch's positive triple patterns with fresh per-variable IRIs), plus one `combined` seed
/// unioning every branch's atoms. Variables are numbered from a single running counter so
/// every seed atom is distinct and the whole corpus is deterministic (no clock, no RNG). See
/// the module docs for why this exercises every guard atom / variable position and is thus a
/// proof rather than a spot-check.
pub fn derive_seeds(get_rq: &str) -> Vec<SeedGraph> {
    let prefixes = parse_prefixes(get_rq);
    // Isolate the WHERE body: everything between the first `WHERE {` and the matching close.
    let Some(where_body) = extract_where_body(get_rq) else {
        return Vec::new();
    };
    let branches = split_union_branches(&where_body);

    let mut counter = 0usize;
    let mut seeds = Vec::new();
    let mut combined: Vec<Atom> = Vec::new();
    for (idx, branch) in branches.iter().enumerate() {
        let patterns = branch_patterns(branch);
        // Each branch gets its OWN variable namespace so per-branch seeds never accidentally
        // join through a shared variable; the running counter keeps IRIs globally distinct.
        let mut var_iri = std::collections::BTreeMap::new();
        if let Some(atoms) = instantiate_branch(&patterns, &prefixes, &mut counter, &mut var_iri) {
            combined.extend(atoms.iter().cloned());
            seeds.push(SeedGraph {
                label: format!("branch-{idx}"),
                atoms,
            });
        }
    }
    if !combined.is_empty() {
        // De-duplicate while preserving determinism.
        let set: BTreeSet<Atom> = combined.into_iter().collect();
        seeds.push(SeedGraph {
            label: "combined".to_owned(),
            atoms: set.into_iter().collect(),
        });
    }
    seeds
}

/// Return the balanced text between the first top-level `WHERE {` and its matching `}`.
fn extract_where_body(query: &str) -> Option<String> {
    let lower = query.to_ascii_lowercase();
    let where_pos = lower.find("where")?;
    let after = &query[where_pos + "where".len()..];
    let open = after.find('{')?;
    let chars: Vec<char> = after[open + 1..].chars().collect();
    let mut depth = 1;
    let mut body = String::new();
    for ch in chars {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(body);
                }
            }
            _ => {}
        }
        body.push(ch);
    }
    None
}

/// Whether the rung permits claiming [`CorrespondenceLaw::SectionLaw`] (`put ∘ get = id_S`).
/// A section requires an injective `get`, so only the injective rungs qualify — matched via
/// [`MorphismClass::is_injective_rung`] (never the derived `Ord`, which is reverse-strength).
fn section_law_claimable(rung: MorphismClass) -> bool {
    rung.is_injective_rung()
}

/// Whether the rung permits claiming [`CorrespondenceLaw::PutGet`] (`get ∘ put = id_V`):
/// well-behaved-lens and up — the same injective-rung set.
fn put_get_claimable(rung: MorphismClass) -> bool {
    rung.is_injective_rung()
}

/// Fold a [`DischargeOutcome`] into a [`LawClaimIr`] for `law`. A Discharged/Violated verdict
/// was established over the executed bounded corpus, so it carries
/// [`DischargeCondition::DischargeBoundedCorpus`]; an Unknown (empty corpus) carries no
/// condition (it is not yet checkable).
fn claim_from(law: CorrespondenceLaw, outcome: &DischargeOutcome) -> LawClaimIr {
    let condition = match outcome.verdict {
        DischargeVerdict::ObligationUnknown => None,
        _ => Some(DischargeCondition::DischargeBoundedCorpus),
    };
    LawClaimIr {
        law,
        verdict: outcome.verdict,
        condition,
    }
}

/// The public entry: discharge every law the `rung` permits by EXECUTION and return the
/// resulting [`LawClaimIr`]s. Seeds are derived branch-covering from `get_rq`; both legs run;
/// each permitted law becomes one claim. The caller attaches these to the
/// `Correspondence` and mints the RDF — this service produces only the typed claims.
pub fn discharge_laws(get_rq: &str, put_rq: &str, rung: MorphismClass) -> Vec<LawClaimIr> {
    let seeds = derive_seeds(get_rq);
    let mut claims = Vec::new();
    if section_law_claimable(rung) {
        let outcome = discharge_section_law(get_rq, put_rq, &seeds);
        claims.push(claim_from(CorrespondenceLaw::SectionLaw, &outcome));
    }
    if put_get_claimable(rung) {
        let outcome = discharge_put_get_law(get_rq, put_rq, &seeds);
        claims.push(claim_from(CorrespondenceLaw::PutGet, &outcome));
    }
    claims
}

#[cfg(test)]
mod tests {
    use super::*;

    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
    const EX: &str = "http://example.org/";
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    fn repo_root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repo root resolves")
    }

    fn read_query(name: &str) -> String {
        let path = repo_root().join("generated").join("queries").join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    // ── The real committed SIOC fixture: the three CompleteOver cells round-trip exactly. ──
    #[test]
    fn sioc_section_law_discharged_on_the_complete_over_cells() {
        let get_rq = read_query("sioc.rq");
        let put_rq = read_query("sioc.put.rq");
        // The exact three recoverable source atoms (per the shipped CompleteOver up-lift).
        let seed = SeedGraph {
            label: "sioc-complete-over".to_owned(),
            atoms: vec![
                (
                    format!("{EX}t1"),
                    RDF_TYPE.to_owned(),
                    format!("{GMEOW}Thread"),
                ),
                (
                    format!("{EX}m1"),
                    format!("{GMEOW}partOfThread"),
                    format!("{EX}th1"),
                ),
                (
                    format!("{EX}r1"),
                    format!("{GMEOW}inReplyTo"),
                    format!("{EX}p1"),
                ),
            ],
        };
        let outcome = discharge_section_law(&get_rq, &put_rq, std::slice::from_ref(&seed));
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationDischarged,
            "the three SIOC CompleteOver cells must discharge the section law\n{outcome:#?}"
        );
        assert!(outcome.countermodel.is_none());
    }

    // ── Inter-leg carrier round-trips non-IRI terms (literal / blank node). ──
    //
    // The forward image of these correspondences carries a term that is NOT an IRI — a literal in
    // one case, a fresh blank node in the other. The seed is all-IRI (so its own serialization is
    // untouched); the non-IRI term appears ONLY in the carrier between the get and put legs. The
    // pre-fix `atoms_to_ntriples` blanket-wrapped every component in `<...>`, so it fed the put
    // leg a malformed line (`<...> <...> <"foo">` / `<...> <...> <_:b>`) that the N-Triples parser
    // REJECTS → `run_leg` returned `Err` → a spurious `ObligationViolated`. Threading typed quads
    // through the canonical serializer makes the carrier well-formed, so the seed round-trips and
    // the law discharges. These tests FAIL on the old all-IRI serializer and PASS now.

    // get mints a constant LITERAL object into the forward image; put matches that literal and
    // reconstructs the exact source atom. The literal lives only in the carrier.
    const LITERAL_GET: &str = "\
CONSTRUCT { ?s <http://ext.example/label> \"foo\" }
WHERE { ?s <http://src.example/p> ?o }";
    const LITERAL_PUT: &str = "\
CONSTRUCT { ?s <http://src.example/p> <http://o.example/y> }
WHERE { ?s <http://ext.example/label> \"foo\" }";

    #[test]
    fn literal_object_in_the_carrier_round_trips_and_discharges() {
        let seed = SeedGraph {
            label: "literal-carrier".to_owned(),
            atoms: vec![(
                "http://s.example/x".to_owned(),
                "http://src.example/p".to_owned(),
                "http://o.example/y".to_owned(),
            )],
        };
        let outcome = discharge_section_law(LITERAL_GET, LITERAL_PUT, std::slice::from_ref(&seed));
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationDischarged,
            "a literal in the inter-leg carrier must round-trip, not produce a false \
             ObligationViolated\n{outcome:#?}"
        );
        assert!(outcome.countermodel.is_none());
    }

    // A datatyped literal (the datatype must survive serialization → parse → match too).
    const TYPED_LITERAL_GET: &str = "\
CONSTRUCT { ?s <http://ext.example/n> \"42\"^^<http://www.w3.org/2001/XMLSchema#integer> }
WHERE { ?s <http://src.example/p> ?o }";
    const TYPED_LITERAL_PUT: &str = "\
CONSTRUCT { ?s <http://src.example/p> <http://o.example/y> }
WHERE { ?s <http://ext.example/n> \"42\"^^<http://www.w3.org/2001/XMLSchema#integer> }";

    #[test]
    fn datatyped_literal_in_the_carrier_round_trips_and_discharges() {
        let seed = SeedGraph {
            label: "typed-literal-carrier".to_owned(),
            atoms: vec![(
                "http://s.example/x".to_owned(),
                "http://src.example/p".to_owned(),
                "http://o.example/y".to_owned(),
            )],
        };
        let outcome = discharge_section_law(
            TYPED_LITERAL_GET,
            TYPED_LITERAL_PUT,
            std::slice::from_ref(&seed),
        );
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationDischarged,
            "a datatyped literal in the carrier must round-trip with its datatype intact\n{outcome:#?}"
        );
    }

    // get mints a fresh BLANK NODE that joins two forward-image triples; put joins on it and
    // reconstructs the source. The blank lives only in the carrier.
    const BLANK_GET: &str = "\
CONSTRUCT { ?s <http://ext.example/r> _:b . _:b <http://ext.example/v> ?o }
WHERE { ?s <http://src.example/p> ?o }";
    const BLANK_PUT: &str = "\
CONSTRUCT { ?s <http://src.example/p> ?o }
WHERE { ?s <http://ext.example/r> ?b . ?b <http://ext.example/v> ?o }";

    #[test]
    fn blank_node_in_the_carrier_round_trips_and_discharges() {
        let seed = SeedGraph {
            label: "blank-carrier".to_owned(),
            atoms: vec![(
                "http://s.example/x".to_owned(),
                "http://src.example/p".to_owned(),
                "http://o.example/y".to_owned(),
            )],
        };
        let outcome = discharge_section_law(BLANK_GET, BLANK_PUT, std::slice::from_ref(&seed));
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationDischarged,
            "a fresh blank node in the inter-leg carrier must round-trip, not produce a false \
             ObligationViolated\n{outcome:#?}"
        );
        assert!(outcome.countermodel.is_none());
    }

    // The quoted-triple comparison key must be injective: two DISTINCT RDF-star quoted triples
    // must NOT render to the same `Atom` string (the pre-fix `<<triple>>` placeholder collapsed
    // them, so a fabricated/dropped quoted-triple atom could hide in the set comparison).
    #[test]
    fn distinct_quoted_triples_do_not_collapse_to_equal_atoms() {
        use purrdf::{RdfTerm, RdfTriple};
        let qt1 = RdfTerm::triple(RdfTriple::new(
            RdfTerm::iri("http://s.example/a"),
            "http://p.example/rel",
            RdfTerm::iri("http://o.example/b"),
        ));
        let qt2 = RdfTerm::triple(RdfTriple::new(
            RdfTerm::iri("http://s.example/a"),
            "http://p.example/rel",
            RdfTerm::iri("http://o.example/c"),
        ));
        assert_ne!(
            term_str(&qt1),
            term_str(&qt2),
            "distinct quoted triples must render to distinct atom keys, not a collapsing placeholder"
        );
    }

    // A get leg with two independent branches; a put leg that recovers both AND fabricates a
    // type-guard atom whenever branch-2 data is present. A single happy-path seed touching only
    // branch-1 MISSES the fabrication; the branch-covering corpus CATCHES it. (AC2 integrity.)
    const FAB_GET: &str = "\
PREFIX src: <http://src.example/>
PREFIX ext: <http://ext.example/>
CONSTRUCT {
  ?a ext:p1 ?b .
  ?c ext:p2 ?d .
} WHERE {
  { ?a src:rel1 ?b . }
  UNION
  { ?c src:rel2 ?d . }
}";

    const FAB_PUT: &str = "\
PREFIX src: <http://src.example/>
PREFIX ext: <http://ext.example/>
CONSTRUCT {
  ?a src:rel1 ?b .
  ?c src:rel2 ?d .
  ?c a src:GuardType .
} WHERE {
  { ?a ext:p1 ?b . }
  UNION
  { ?c ext:p2 ?d . }
}";

    fn happy_path_branch1_seed() -> SeedGraph {
        SeedGraph {
            label: "happy".to_owned(),
            atoms: vec![(
                "http://seed.example/a".to_owned(),
                "http://src.example/rel1".to_owned(),
                "http://seed.example/b".to_owned(),
            )],
        }
    }

    #[test]
    fn single_happy_path_seed_misses_the_fabricated_guard_atom() {
        // Branch-1 only: the fabricating put branch (keyed on ext:p2) never fires, so the seed
        // round-trips cleanly — a lone happy-path seed would wrongly report the law discharged.
        let seed = happy_path_branch1_seed();
        let outcome = discharge_section_law(FAB_GET, FAB_PUT, std::slice::from_ref(&seed));
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationDischarged,
            "a single branch-1 seed MUST miss the branch-2 fabrication (that is the blind spot)\n{outcome:#?}"
        );
    }

    #[test]
    fn branch_covering_corpus_catches_the_fabricated_guard_atom() {
        let seeds = derive_seeds(FAB_GET);
        // The corpus must exercise branch-2 (and the combined seed): at least three seeds.
        assert!(
            seeds.len() >= 3,
            "expected one seed per branch plus combined, got {}: {seeds:#?}",
            seeds.len()
        );
        let outcome = discharge_section_law(FAB_GET, FAB_PUT, &seeds);
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationViolated,
            "the branch-covering corpus MUST catch the branch-2 fabrication\n{outcome:#?}"
        );
        let cm = outcome.countermodel.expect("a countermodel is present");
        // The spurious atom is the fabricated `?c a src:GuardType`.
        assert!(
            cm.spurious
                .iter()
                .any(|(_, p, o)| p == RDF_TYPE && o == "http://src.example/GuardType"),
            "the countermodel must name the fabricated GuardType atom\n{cm:#?}"
        );
        assert!(
            cm.missing.is_empty(),
            "nothing was dropped, only fabricated\n{cm:#?}"
        );
    }

    #[test]
    fn discharge_is_deterministic_in_verdict_and_countermodel_bytes() {
        let seeds = derive_seeds(FAB_GET);
        let a = discharge_section_law(FAB_GET, FAB_PUT, &seeds);
        let b = discharge_section_law(FAB_GET, FAB_PUT, &seeds);
        assert_eq!(
            a, b,
            "same inputs must yield an identical outcome (verdict + countermodel)"
        );
        // Countermodel bytes are stable across independent seed-derivation runs too.
        let seeds2 = derive_seeds(FAB_GET);
        assert_eq!(seeds, seeds2, "seed derivation must be deterministic");
        let c = discharge_section_law(FAB_GET, FAB_PUT, &seeds2);
        assert_eq!(a, c);
    }

    #[test]
    fn derive_seeds_is_branch_covering_with_fresh_distinct_iris() {
        let seeds = derive_seeds(FAB_GET);
        // branch-0, branch-1, combined.
        let labels: Vec<&str> = seeds.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["branch-0", "branch-1", "combined"],
            "{seeds:#?}"
        );
        // Each per-branch seed carries exactly its one positive pattern.
        assert_eq!(seeds[0].atoms.len(), 1);
        assert_eq!(seeds[1].atoms.len(), 1);
        // The combined seed unions both branches (two distinct atoms, distinct IRIs).
        assert_eq!(seeds[2].atoms.len(), 2, "{:#?}", seeds[2]);
        let all_iris: BTreeSet<&String> = seeds
            .iter()
            .flat_map(|s| s.atoms.iter().flat_map(|(a, _, c)| [a, c]))
            .collect();
        // v0..v3 across the two branches — all fresh and distinct, deterministic.
        assert!(all_iris.contains(&"http://seed.example/v0".to_owned()));
        assert!(all_iris.contains(&"http://seed.example/v3".to_owned()));
    }

    #[test]
    fn non_executable_leg_is_violated_never_a_silent_pass() {
        let seed = happy_path_branch1_seed();
        let broken = "this is not valid SPARQL {{{";
        let outcome = discharge_section_law(broken, FAB_PUT, std::slice::from_ref(&seed));
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationViolated,
            "a malformed get leg must hard-fail to Violated, never pass\n{outcome:#?}"
        );
        assert!(outcome.countermodel.is_some());
    }

    #[test]
    fn empty_corpus_is_unknown_not_discharged() {
        let outcome = discharge_section_law(FAB_GET, FAB_PUT, &[]);
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationUnknown,
            "an unchecked law is Unknown — never proved absent"
        );
    }

    #[test]
    fn discharge_laws_gates_claims_by_rung() {
        // BridgeView (floor) claims no injective law.
        let none = discharge_laws(FAB_GET, FAB_PUT, MorphismClass::BridgeView);
        assert!(
            none.is_empty(),
            "a non-injective rung claims no section/put-get law\n{none:#?}"
        );

        // SectionRetraction claims BOTH SectionLaw and PutGet; the fabrication makes them Violated.
        let claims = discharge_laws(FAB_GET, FAB_PUT, MorphismClass::SectionRetraction);
        let laws: BTreeSet<CorrespondenceLaw> = claims.iter().map(|c| c.law).collect();
        assert!(laws.contains(&CorrespondenceLaw::SectionLaw), "{claims:#?}");
        assert!(laws.contains(&CorrespondenceLaw::PutGet), "{claims:#?}");
        let section = claims
            .iter()
            .find(|c| c.law == CorrespondenceLaw::SectionLaw)
            .expect("section claim present");
        assert_eq!(
            section.verdict,
            DischargeVerdict::ObligationViolated,
            "{section:#?}"
        );
    }

    #[test]
    fn discharge_laws_on_real_sioc_produces_claims() {
        // The shipped SIOC get leg has lossy branches (mapSiocTopic), so the auto-derived
        // branch corpus does NOT globally discharge the section law — but the service must run
        // end-to-end on the real queries and return a claim per permitted law.
        let get_rq = read_query("sioc.rq");
        let put_rq = read_query("sioc.put.rq");
        let claims = discharge_laws(&get_rq, &put_rq, MorphismClass::SectionRetraction);
        assert_eq!(claims.len(), 2, "SectionLaw + PutGet\n{claims:#?}");
        for c in &claims {
            assert_ne!(
                c.verdict,
                DischargeVerdict::ObligationUnknown,
                "a non-empty SIOC corpus must yield a decided verdict\n{c:#?}"
            );
        }
    }
}
