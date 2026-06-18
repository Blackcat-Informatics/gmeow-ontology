// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native DL consistency / unsatisfiability over the Nemo chase.
//!
//! Builds on the fixed EL calculus ([`crate::reason::el::EL_RULES`]) by adding
//! the clash-detection rules that decide consistency on the EL Horn fragment:
//! an individual forced into `owl:Nothing` witnesses an inconsistency, and a
//! class that subsumes two disjoint classes is unsatisfiable. On that fragment
//! this subsumes HermiT; constructs beyond it (negation, existential/universal
//! restrictions, cardinality, nominals, …) are *not* expressible in the
//! predicate-as-symbol ternary encoding, so consistency for axioms using them
//! is decided by the HermiT oracle only — never silently assumed consistent.
//! Those constructs are named in [`DlVerdict::gaps`].
//!
//! # Distinction
//!
//! An unsatisfiable but *unpopulated* class does **not** make the ontology
//! inconsistent: it is merely a class that can have no members. Only an
//! individual actually forced into `owl:Nothing` is an inconsistency. The
//! verdict keeps both surfaces separate ([`DlVerdict::unsatisfiable_classes`]
//! vs [`DlVerdict::inconsistencies`]).

use crate::reason::el::EL_RULES;
use crate::reason::InferredAxiom;
use gmeow_rdf::{RdfLoss, RdfStore, RdfTerm};

// ── OWL/RDF IRI constants ──────────────────────────────────────────────────────

const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

// Beyond-EL construct IRIs scanned for in the input edb to populate `gaps`.
const OWL_COMPLEMENT_OF: &str = "http://www.w3.org/2002/07/owl#complementOf";
const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const OWL_ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
const OWL_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#cardinality";
const OWL_MIN_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minCardinality";
const OWL_MAX_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
const OWL_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#qualifiedCardinality";
const OWL_MIN_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minQualifiedCardinality";
const OWL_MAX_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
const OWL_DISJOINT_UNION_OF: &str = "http://www.w3.org/2002/07/owl#disjointUnionOf";
const OWL_ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const OWL_HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";

/// The clash-detection rules layered on top of [`EL_RULES`], in the
/// world-scoped ternary gmeow encoding. Full IRIs in angle brackets; `?w`
/// threads the world. Assembled with [`EL_RULES`] into [`dl_rules`].
const DL_EXTRA_RULES: &str = r#"
#[name("dl:individual-clash")]
<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>(?i,<http://www.w3.org/2002/07/owl#Nothing>,?w) :- <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>(?i,?c1,?w), <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>(?i,?c2,?w), <http://www.w3.org/2002/07/owl#disjointWith>(?c1,?c2,?w) .
#[name("dl:unsatisfiable-class")]
<http://www.w3.org/2000/01/rdf-schema#subClassOf>(?c,<http://www.w3.org/2002/07/owl#Nothing>,?w) :- <http://www.w3.org/2000/01/rdf-schema#subClassOf>(?c,?d,?w), <http://www.w3.org/2000/01/rdf-schema#subClassOf>(?c,?e,?w), <http://www.w3.org/2002/07/owl#disjointWith>(?d,?e,?w) .
#[name("dl:nothing-membership")]
<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>(?i,<http://www.w3.org/2002/07/owl#Nothing>,?w) :- <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>(?i,?c,?w), <http://www.w3.org/2000/01/rdf-schema#subClassOf>(?c,<http://www.w3.org/2002/07/owl#Nothing>,?w) .
"#;

/// Assemble the full DL rule set: the fixed EL calculus plus the
/// clash-detection rules. Built at runtime from [`EL_RULES`] +
/// [`DL_EXTRA_RULES`] to avoid duplicating the EL rule text.
///
/// `pub(crate)` so the single-chase combined entry point
/// ([`crate::reason::reason_all`]) can run this exact rule set once and derive
/// both the subsumption closure and the consistency verdict from the same
/// `Vec<InferredAxiom>`.
pub(crate) fn dl_rules() -> String {
    format!("{EL_RULES}\n{DL_EXTRA_RULES}")
}

/// A class proven unsatisfiable: it subsumes two disjoint classes, so it can
/// have no members. Unsatisfiability alone does *not* make the ontology
/// inconsistent — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsatClass {
    pub class: String,
    pub world: String,
    pub premises: Vec<(String, String, String)>,
}

/// An individual forced into `owl:Nothing`: a witness that the ontology is
/// inconsistent in `world`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InconsistencyWitness {
    pub individual: String,
    pub world: String,
    pub premises: Vec<(String, String, String)>,
}

/// The verdict of a native DL consistency run.
///
/// `consistent` is `false` iff at least one [`InconsistencyWitness`] was found.
/// `unsatisfiable_classes` lists provably empty classes (which do *not* on
/// their own make the ontology inconsistent). `gaps` names the beyond-EL
/// constructs found in the input whose consistency this native check does not
/// decide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlVerdict {
    pub consistent: bool,
    pub unsatisfiable_classes: Vec<UnsatClass>,
    pub inconsistencies: Vec<InconsistencyWitness>,
    pub gaps: Vec<RdfLoss>,
}

/// Strip a decoded Nemo object display form (`<iri>`) back to the bare IRI.
///
/// Derived/asserted object terms come through the chase decoder as their Nemo
/// display string; IRIs are wrapped in angle brackets. Non-IRI forms are
/// returned unchanged.
fn unwrap_iri(display: &str) -> &str {
    display
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(display)
}

/// Decide native DL consistency / unsatisfiability of `edb` via the Nemo chase.
///
/// Runs the full [`dl_rules`] set through the shared
/// [`crate::reason::run_reasoning`] machinery, then reads off the clash facts:
/// every `type(?i, owl:Nothing, ?w)` is an [`InconsistencyWitness`]; every
/// `subClassOf(?c, owl:Nothing, ?w)` (with `?c` not `owl:Nothing` itself) is an
/// [`UnsatClass`]. The verdict is consistent iff no inconsistency witness was
/// derived. Beyond-EL constructs present in the input are surfaced as `gaps`.
///
/// # Errors
///
/// Returns `Err(String)` if the source store cannot be loaded or the Nemo
/// chase fails to parse/validate/evaluate/decode.
pub fn dl_consistency(edb: &impl RdfStore) -> Result<DlVerdict, String> {
    let inferred: Vec<InferredAxiom> = crate::reason::run_reasoning(edb, &dl_rules())?;
    verdict_from_inferred(&inferred, edb)
}

/// Read off the [`DlVerdict`] from an already-computed [`dl_rules`] closure.
///
/// Pure over `inferred` for the clash scan (every `type(?i, owl:Nothing, ?w)` is
/// an [`InconsistencyWitness`]; every `subClassOf(?c, owl:Nothing, ?w)` with `?c`
/// not `owl:Nothing` is an [`UnsatClass`]); the `gaps` scan still walks `edb`
/// (the beyond-EL constructs are an *input* property, not a derived one). The
/// verdict is consistent iff no inconsistency witness was derived.
///
/// Factored out so the single-chase [`crate::reason::reason_all`] can reuse the
/// SAME `Vec<InferredAxiom>` it derives for the subsumption closure, running
/// Nemo once instead of twice. [`dl_consistency`] is the thin wrapper that runs
/// the chase then calls this — its behaviour is unchanged.
///
/// # Errors
///
/// Returns `Err(String)` if a quad cannot be read from `edb` during the gap scan.
pub(crate) fn verdict_from_inferred(
    inferred: &[InferredAxiom],
    edb: &impl RdfStore,
) -> Result<DlVerdict, String> {
    let mut inconsistencies: Vec<InconsistencyWitness> = Vec::new();
    let mut unsatisfiable_classes: Vec<UnsatClass> = Vec::new();

    for ax in inferred {
        let object_iri = unwrap_iri(&ax.object);
        // An individual forced into owl:Nothing — an inconsistency witness.
        if ax.predicate == RDF_TYPE && object_iri == OWL_NOTHING {
            inconsistencies.push(InconsistencyWitness {
                individual: ax.subject.clone(),
                world: ax.world.clone(),
                premises: ax.premises.clone(),
            });
        }
        // A class subsumed by owl:Nothing — an unsatisfiable (empty) class.
        // Exclude owl:Nothing ⊑ owl:Nothing (vacuously true, not informative).
        else if ax.predicate == RDFS_SUBCLASSOF
            && object_iri == OWL_NOTHING
            && unwrap_iri(&ax.subject) != OWL_NOTHING
            && ax.subject != OWL_NOTHING
        {
            unsatisfiable_classes.push(UnsatClass {
                class: ax.subject.clone(),
                world: ax.world.clone(),
                premises: ax.premises.clone(),
            });
        }
    }

    // Only a populated clash (an individual in owl:Nothing) makes the ontology
    // inconsistent; an unsatisfiable-but-unpopulated class does not.
    let consistent = inconsistencies.is_empty();

    let gaps = scan_gaps(edb)?;

    Ok(DlVerdict {
        consistent,
        unsatisfiable_classes,
        inconsistencies,
        gaps,
    })
}

/// Scan the input `edb` quads for beyond-EL OWL constructs and emit one
/// [`RdfLoss`] per distinct construct kind found.
///
/// Detection is by predicate-IRI presence (and object-IRI for restriction
/// fillers that ride `owl:onProperty`/`owl:someValuesFrom` patterns). Each loss
/// states precisely that the construct is beyond the EL Horn fragment in this
/// encoding and that its consistency is decided by the HermiT oracle only.
///
/// # Errors
///
/// Returns `Err(String)` if a quad cannot be read from the source store.
fn scan_gaps(edb: &impl RdfStore) -> Result<Vec<RdfLoss>, String> {
    // (predicate-or-object IRI, short construct name, gap-code suffix).
    const BEYOND_EL: &[(&str, &str, &str)] = &[
        (OWL_COMPLEMENT_OF, "owl:complementOf", "complementOf"),
        (OWL_SOME_VALUES_FROM, "owl:someValuesFrom", "someValuesFrom"),
        (OWL_ALL_VALUES_FROM, "owl:allValuesFrom", "allValuesFrom"),
        (OWL_CARDINALITY, "owl:cardinality", "cardinality"),
        (OWL_MIN_CARDINALITY, "owl:minCardinality", "minCardinality"),
        (OWL_MAX_CARDINALITY, "owl:maxCardinality", "maxCardinality"),
        (
            OWL_QUALIFIED_CARDINALITY,
            "owl:qualifiedCardinality",
            "qualifiedCardinality",
        ),
        (
            OWL_MIN_QUALIFIED_CARDINALITY,
            "owl:minQualifiedCardinality",
            "minQualifiedCardinality",
        ),
        (
            OWL_MAX_QUALIFIED_CARDINALITY,
            "owl:maxQualifiedCardinality",
            "maxQualifiedCardinality",
        ),
        (
            OWL_DISJOINT_UNION_OF,
            "owl:disjointUnionOf",
            "disjointUnionOf",
        ),
        (OWL_ONE_OF, "owl:oneOf", "oneOf"),
        (OWL_HAS_VALUE, "owl:hasValue", "hasValue"),
    ];

    // Materialize the predicate IRIs and object IRIs once; a quad-read error is
    // a hard failure (no-optionality doctrine — silently dropping a quad could
    // miss a beyond-EL construct).
    let mut present_iris: std::collections::HashSet<String> = std::collections::HashSet::new();
    for quad in edb.quads() {
        let quad = quad.map_err(|d| format!("dl gap-scan: cannot read quad: {d}"))?;
        present_iris.insert(quad.predicate);
        if let RdfTerm::Iri(o) = quad.object {
            present_iris.insert(o);
        }
    }

    let mut gaps: Vec<RdfLoss> = Vec::new();
    for &(iri, name, suffix) in BEYOND_EL {
        // A construct is present if its IRI appears as a predicate or object of
        // any quad in any graph (restriction fillers ride the object position).
        if !present_iris.contains(iri) {
            continue;
        }
        let message = format!(
            "{name} is beyond the EL Horn fragment in the predicate-as-symbol \
             encoding; consistency for axioms using it is decided by the HermiT \
             oracle only (named, not silently assumed consistent)."
        );
        gaps.push(RdfLoss::new(format!("reason.dl-gap.{suffix}"), message));
    }
    Ok(gaps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf::{RdfQuad, RdfTerm, VecRdfStore};

    const W: &str = "http://gmeow.example/w";
    const SUBCLASS: &str = RDFS_SUBCLASSOF;
    const TYPE: &str = RDF_TYPE;
    const DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";

    const A: &str = "http://gmeow.example/A";
    const B: &str = "http://gmeow.example/B";
    const C: &str = "http://gmeow.example/C";
    const X: &str = "http://gmeow.example/x";

    fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    }

    #[test]
    fn disjoint_superclasses_make_a_unsat_and_x_inconsistent() {
        // A ⊑ B, A ⊑ C, B disjointWith C, x : A
        // ⇒ A is unsatisfiable, and x is forced into owl:Nothing (inconsistent).
        let store = VecRdfStore::with_quads(vec![
            quad(A, SUBCLASS, B),
            quad(A, SUBCLASS, C),
            quad(B, DISJOINT, C),
            quad(X, TYPE, A),
        ]);
        let verdict = dl_consistency(&store).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "x forced into owl:Nothing must make the ontology inconsistent"
        );
        assert!(
            verdict.unsatisfiable_classes.iter().any(|u| u.class == A),
            "A must be reported unsatisfiable: {:?}",
            verdict.unsatisfiable_classes
        );
        let witness = verdict
            .inconsistencies
            .iter()
            .find(|w| w.individual == X)
            .expect("x must be an inconsistency witness");
        assert_eq!(witness.world, W, "witness carries its world IRI");
        assert!(
            !witness.premises.is_empty(),
            "derived inconsistency must carry antecedent premises"
        );
    }

    #[test]
    fn no_disjointness_is_consistent() {
        // A ⊑ B, x : A — no disjointness ⇒ consistent, no inconsistencies.
        let store = VecRdfStore::with_quads(vec![quad(A, SUBCLASS, B), quad(X, TYPE, A)]);
        let verdict = dl_consistency(&store).expect("dl consistency should succeed");

        assert!(verdict.consistent, "no clash ⇒ consistent");
        assert!(
            verdict.inconsistencies.is_empty(),
            "no individual should be forced into owl:Nothing"
        );
    }

    #[test]
    fn complement_of_is_named_as_a_gap() {
        // An owl:complementOf triple is beyond EL; it must be surfaced as a gap,
        // and the verdict must not silently claim consistency about it.
        let store = VecRdfStore::with_quads(vec![quad(A, super::OWL_COMPLEMENT_OF, B)]);
        let verdict = dl_consistency(&store).expect("dl consistency should succeed");

        assert!(
            verdict
                .gaps
                .iter()
                .any(|g| { g.code.contains("complementOf") && g.message.contains("complementOf") }),
            "owl:complementOf must be named in the gap surface: {:?}",
            verdict.gaps
        );
    }
}
