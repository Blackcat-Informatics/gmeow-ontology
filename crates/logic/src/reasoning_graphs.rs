// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The dependency-free named-graph boundary of the object-level reasoning EDB.
//!
//! The pipeline assembles these named worlds plus the authored default world; bundle
//! readers and lower-layer coherence tests consume the same set. Keeping the identifiers in
//! the reasoning crate prevents the producer and its gate-teeth proof from silently drifting.
//! [`crate::reasoning_graphs::project_object_level_edb`] is the executable twin of that boundary: given a FULL
//! shipped bundle/snapshot dataset (every named graph GMEOW ships), it projects out
//! exactly the object-level EDB shape — the single authority `crates/pipeline` (build
//! time) and `crates/validate` (the `validate --deep` / `verify --deep` CLI deep-semantic
//! pass, re-deriving reasoning from an arbitrary shipped `.gts` file) both delegate to,
//! so neither can silently drift from the other's notion of "object-level".

use std::collections::HashSet;
use std::sync::Arc;

use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm};

/// The RDF 1.2 statement layer admitted to object-level reasoning.
pub const GRAPH_STATEMENTS: &str = "https://blackcatinformatics.ca/gmeow/graph/statements";
/// The vendored import closure admitted to object-level reasoning.
pub const GRAPH_IMPORTS: &str = "https://blackcatinformatics.ca/gmeow/graph/imports";
/// The canonical compiled `logic:` program admitted to object-level reasoning.
pub const GRAPH_LOGIC: &str = "https://blackcatinformatics.ca/gmeow/graph/logic";
/// The compiled relational-core lowering admitted to object-level reasoning.
pub const GRAPH_RELATIONAL_CORE: &str =
    "https://blackcatinformatics.ca/gmeow/graph/relational-core";

/// Demonstrator world witnessing the **jointly-acyclic** chase-termination class in the
/// shipped bundle (its per-world certificate is `chase.certificate.jointly-acyclic`).
pub const GRAPH_DEMO_JOINTLY_ACYCLIC: &str =
    "https://blackcatinformatics.ca/gmeow/graph/demo/jointly-acyclic";
/// Demonstrator world witnessing the **super-weakly-acyclic** chase-termination class.
pub const GRAPH_DEMO_SUPER_WEAKLY_ACYCLIC: &str =
    "https://blackcatinformatics.ca/gmeow/graph/demo/super-weakly-acyclic";
/// Demonstrator world witnessing the self-hosted **model-summarizing-acyclic** class.
pub const GRAPH_DEMO_MODEL_SUMMARIZING: &str =
    "https://blackcatinformatics.ca/gmeow/graph/demo/model-summarizing-acyclic";

/// EVERY slice's positive-demonstrator ABox corpus — every
/// `slices/<group>/<slice>/examples/*.ttl` file in the repository, parsed and unioned by
/// `stage-source-load` (`crates/pipeline/src/stages/source_load.rs`) into this one named
/// world. Admitted to object-level reasoning so each slice's authored worked examples
/// actually reach the shipped bundle's reasoned closure, and every reasoned-graph gate has
/// a real witness to decide over instead of running vacuously against an EDB that carries
/// no slice's demonstrators at all. The corpus is authored source, so it loads with the
/// rest of the authored sources rather than off a computed-graph producer.
pub const GRAPH_EXAMPLES: &str = "https://blackcatinformatics.ca/gmeow/graph/examples";

/// Every named graph admitted to the object-level reasoning EDB. The default graph is also
/// admitted, but has no IRI and therefore is not represented in this list.
pub const OBJECT_LEVEL_NAMED_GRAPHS: [&str; 8] = [
    GRAPH_STATEMENTS,
    GRAPH_IMPORTS,
    GRAPH_LOGIC,
    GRAPH_RELATIONAL_CORE,
    GRAPH_DEMO_JOINTLY_ACYCLIC,
    GRAPH_DEMO_SUPER_WEAKLY_ACYCLIC,
    GRAPH_DEMO_MODEL_SUMMARIZING,
    GRAPH_EXAMPLES,
];

/// Whether a named graph belongs to the object-level reasoning EDB.
pub fn is_object_level_named_graph(iri: &str) -> bool {
    OBJECT_LEVEL_NAMED_GRAPHS.contains(&iri)
}

/// `rdf:type` and the canonical `logic:GroundingCorrespondence` class IRI — used to recognize
/// the meta-level grounding-correspondence records that are excluded from object-level closure.
const RDF_TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const LOGIC_GROUNDING_CORRESPONDENCE_IRI: &str =
    "https://blackcatinformatics.ca/logic/GroundingCorrespondence";
/// The two correspondence-DEFINING predicates: a `logic:GroundingCorrespondence` names its two
/// related terms through these, and no production object-level record uses them.
const LOGIC_SOURCE_ENDPOINT_IRI: &str = "https://blackcatinformatics.ca/logic/sourceEndpoint";
const LOGIC_TARGET_ENDPOINT_IRI: &str = "https://blackcatinformatics.ca/logic/targetEndpoint";

/// Every grounding-correspondence subject in `snapshot` (in ANY graph): the meta-level grounding
/// correspondences whose triples are re-projected into `graph/correspondence-laws` and therefore
/// must not enter the object-level reasoning EDB even when authored inline in an object-level
/// graph (a slice `module.ttl` body).
///
/// A record is identified by its `rdf:type logic:GroundingCorrespondence` OR — robustly against
/// graph placement — by carrying one of the correspondence-defining endpoint predicates
/// (`logic:sourceEndpoint`/`logic:targetEndpoint`). The type alone is not enough for the
/// build-time twin: the compile stage projects the `rdf:type` + morphism metadata into
/// `graph/correspondence-laws` (a meta graph absent from a narrowed object-level union) while the
/// raw endpoint triples remain in an admitted graph, so keying on the type over the narrowed union
/// finds NO subjects and the endpoints — whose objects are the out-of-fragment
/// `logic:inverseFunctionalProperty` / `logic:oneOf` referents — leak into the EDB and make
/// reason-verify honestly-but-uselessly withhold. Keying on the endpoint predicates catches the
/// record wherever its endpoint triples land.
pub fn grounding_correspondence_subjects(snapshot: &RdfDataset) -> HashSet<String> {
    let mut subjects = HashSet::new();
    for quad in snapshot.owned_quads() {
        let RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        let identifies_correspondence = if quad.predicate == RDF_TYPE_IRI {
            matches!(&quad.object, RdfTerm::Iri(o) if o == LOGIC_GROUNDING_CORRESPONDENCE_IRI)
        } else {
            quad.predicate == LOGIC_SOURCE_ENDPOINT_IRI
                || quad.predicate == LOGIC_TARGET_ENDPOINT_IRI
        };
        if identifies_correspondence {
            subjects.insert(subject.clone());
        }
    }
    subjects
}

/// True iff `quad`'s subject is a `logic:GroundingCorrespondence` record — a meta-level
/// correspondence datum excluded from the object-level reasoning EDB.
pub fn is_correspondence_quad(quad: &RdfQuad, correspondence_subjects: &HashSet<String>) -> bool {
    matches!(&quad.subject, RdfTerm::Iri(s) if correspondence_subjects.contains(s))
}

/// Return `dataset` with every `logic:GroundingCorrespondence` record's triples removed. This
/// is the SHARED meta-level exclusion the two object-level-EDB twins apply so they cannot
/// drift: `project_object_level_edb` (the snapshot authority) applies it inline while
/// projecting the admitted graphs, and `crates/pipeline`'s `assemble_object_level_edb` (the
/// build-time twin) applies it to the assembled object-level union. A correspondence's
/// `logic:sourceEndpoint`/`logic:targetEndpoint` referents are correspondence data (preserved
/// in `graph/correspondence-laws`), not production object-level class expressions.
pub fn exclude_grounding_correspondences(
    dataset: &RdfDataset,
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let correspondence_subjects = grounding_correspondence_subjects(dataset);
    let mut builder = RdfDatasetBuilder::new();
    for quad in dataset.owned_quads() {
        if !is_correspondence_quad(&quad, &correspondence_subjects) {
            builder.push_owned_quad(&quad);
        }
    }
    for reifier in dataset.owned_reifiers() {
        builder.push_owned_reifier(&reifier);
    }
    for annotation in dataset.owned_annotations() {
        builder.push_owned_annotation(&annotation);
    }
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Reason {
            detail: format!("freeze grounding-correspondence exclusion: {e}"),
        })
    })
}

/// Whether `graph` (a quad's/reifier's/annotation's graph-name term — `None` for the
/// true default graph) is admitted to the object-level reasoning EDB: the default graph
/// always is; a named graph is iff [`is_object_level_named_graph`] admits its IRI; a
/// blank-node-labelled graph name never is (every object-level world is IRI-named).
fn admitted_graph(graph: &Option<RdfTerm>) -> bool {
    match graph {
        None => true,
        Some(RdfTerm::Iri(iri)) => is_object_level_named_graph(iri),
        Some(_) => false,
    }
}

/// Remove correspondence-owned recovery evidence from an otherwise object-level dataset.
///
/// A recovery case is executable meta-language: its formula seeds a source graph for the
/// correspondence executor, but the formula tree is not an ontology ABox to saturate. The
/// compiled `graph/correspondence` projection is already excluded from the reasoning EDB; this
/// function applies the same boundary to the canonical source envelope that remains in the
/// default graph. Besides avoiding false ontology facts, doing so keeps RDFC-1.0 labels for
/// unrelated ontology blank nodes stable when recovery evidence grows.
///
/// Traversal follows only ownership links. In particular, `logic:relation` and
/// `logic:termIri` are deliberately not followed: their objects are ontology vocabulary terms,
/// not nodes owned by the recovery case.
pub fn without_recovery_case_envelopes(
    dataset: &RdfDataset,
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    const RECOVERY_CASE: &str = "https://blackcatinformatics.ca/logic/recoveryCase";
    const OWNERSHIP_LINKS: [&str; 11] = [
        "https://blackcatinformatics.ca/logic/recoveryTransform",
        "https://blackcatinformatics.ca/logic/not",
        "https://blackcatinformatics.ca/logic/and",
        "https://blackcatinformatics.ca/logic/or",
        "https://blackcatinformatics.ca/logic/antecedent",
        "https://blackcatinformatics.ca/logic/consequent",
        "https://blackcatinformatics.ca/logic/iff",
        "https://blackcatinformatics.ca/logic/forall",
        "https://blackcatinformatics.ca/logic/exists",
        "https://blackcatinformatics.ca/logic/argument",
        "https://blackcatinformatics.ca/logic/quantifiedVariable",
    ];

    fn resource(term: &RdfTerm) -> bool {
        matches!(term, RdfTerm::Iri(_) | RdfTerm::BlankNode(_))
    }

    let quads: Vec<RdfQuad> = dataset.owned_quads().collect();
    let reifiers: Vec<purrdf::RdfReifier> = dataset.owned_reifiers().collect();
    let mut owned: HashSet<(Option<RdfTerm>, RdfTerm)> = quads
        .iter()
        .filter(|quad| quad.predicate == RECOVERY_CASE)
        .filter(|quad| resource(&quad.object))
        .map(|quad| (quad.graph_name.clone(), quad.object.clone()))
        .collect();

    loop {
        let before = owned.len();
        for quad in &quads {
            if owned.contains(&(quad.graph_name.clone(), quad.subject.clone()))
                && OWNERSHIP_LINKS.contains(&quad.predicate.as_str())
                && resource(&quad.object)
            {
                owned.insert((quad.graph_name.clone(), quad.object.clone()));
            }
        }
        // A reifier binds a name to a triple occurrence: when the reified statement
        // touches recovery-owned territory (its subject or object is owned), the
        // reifier's own identity is recovery-owned too. Folding that into the SAME
        // fixed-point closure (rather than a one-shot pass below) makes pruning
        // transitive across nested reification: RDF 1.2 allows annotating an
        // annotation by reifying its `~reifier` triple (`<<~r1 :note "x">> :certainty
        // 0.9 .`), so an outer reifier whose statement subject/object IS an inner
        // pruned reifier's identity becomes owned here too, and on the next
        // iteration any annotation keyed on THAT outer reifier is caught below.
        for reifier in &reifiers {
            if resource(&reifier.reifier)
                && !owned.contains(&(reifier.graph.clone(), reifier.reifier.clone()))
                && (owned.contains(&(reifier.graph.clone(), reifier.statement.subject.clone()))
                    || owned.contains(&(reifier.graph.clone(), reifier.statement.object.clone())))
            {
                owned.insert((reifier.graph.clone(), reifier.reifier.clone()));
            }
        }
        if owned.len() == before {
            break;
        }
    }

    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        let recovery_owned = quad.predicate == RECOVERY_CASE
            || owned.contains(&(quad.graph_name.clone(), quad.subject.clone()));
        if !recovery_owned {
            builder.push_owned_quad(&quad);
        }
    }
    for reifier in reifiers {
        let recovery_owned = owned.contains(&(reifier.graph.clone(), reifier.reifier.clone()))
            || owned.contains(&(reifier.graph.clone(), reifier.statement.subject.clone()))
            || owned.contains(&(reifier.graph.clone(), reifier.statement.object.clone()));
        if !recovery_owned {
            builder.push_owned_reifier(&reifier);
        }
    }
    // Every annotation quad keyed on a pruned reifier is dropped here too: `owned`
    // now includes every reifier identity pruned above (directly, or transitively
    // through nested reification of an annotation triple), so no dangling RDF-star
    // metadata can reference a reifier that no longer exists in the EDB.
    for annotation in dataset.owned_annotations() {
        if !owned.contains(&(annotation.graph.clone(), annotation.reifier.clone())) {
            builder.push_owned_annotation(&annotation);
        }
    }
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Reason {
            detail: format!("prune recovery-case envelopes: {e}"),
        })
    })
}

/// Project a FULL bundle/snapshot dataset (every named graph GMEOW ships, including
/// meta/report graphs such as `graph/documentation`, `graph/diagnostics`,
/// `graph/correspondence`) down to the object-level reasoning EDB: the true default
/// graph plus every [`OBJECT_LEVEL_NAMED_GRAPHS`] member, with recovery-case envelopes
/// pruned ([`without_recovery_case_envelopes`]).
///
/// This is the SHARED authority for "what counts as object-level" starting from a full
/// snapshot (as opposed to `crates/pipeline`'s `assemble_object_level_edb`, which
/// assembles the SAME boundary at build time directly from the individual producer
/// products, before a snapshot exists). `crates/pipeline`'s
/// `stages::carrier::snapshot_reasoning_edb` (the `gmeow-dev-cli` `--fresh` reasoning
/// lanes) and `crates/validate`'s deep-semantic pass (`gmeow validate --deep` / `gmeow
/// verify --deep`, re-deriving reasoning from an arbitrary shipped `.gts` file) both
/// delegate here, so a consumer's fresh reasoning over a shipped bundle sees
/// byte-identical worlds to the pipeline's own `stage-reason` — neither caller can
/// silently drift from the other's notion of "object-level".
pub fn project_object_level_edb(
    snapshot: &RdfDataset,
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    // Grounding correspondences are meta-level ontology content — shipped OUTSIDE object-level
    // closure and re-projected into `graph/correspondence-laws` (docs/GROUNDING.md, CONSTITUTION
    // Principle 17). A `logic:GroundingCorrespondence` record authored in an object-level graph
    // (e.g. a slice `module.ttl` body) therefore must NOT enter the object-level reasoning EDB:
    // its `logic:sourceEndpoint`/`logic:targetEndpoint` referents (e.g. `logic:oneOf`,
    // `logic:inverseFunctionalProperty`) are correspondence data, not production class
    // expressions, and admitting them makes the native DL path honestly-but-uselessly withhold
    // (reason-verify refuses on the out-of-fragment nominals/inverse-functional it cannot decide).
    // The correspondence itself is preserved in `graph/correspondence-laws` (the mappings stage).
    let correspondence_subjects = grounding_correspondence_subjects(snapshot);
    let mut builder = RdfDatasetBuilder::new();
    for quad in snapshot.owned_quads() {
        if admitted_graph(&quad.graph_name)
            && !is_correspondence_quad(&quad, &correspondence_subjects)
        {
            builder.push_owned_quad(&quad);
        }
    }
    for reifier in snapshot.owned_reifiers() {
        if admitted_graph(&reifier.graph) {
            builder.push_owned_reifier(&reifier);
        }
    }
    for annotation in snapshot.owned_annotations() {
        if admitted_graph(&annotation.graph) {
            builder.push_owned_annotation(&annotation);
        }
    }
    let admitted = builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Reason {
            detail: format!("freeze snapshot object-level reasoning EDB: {e}"),
        })
    })?;
    without_recovery_case_envelopes(admitted.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_is_unique_and_excludes_meta_graphs() {
        let unique = OBJECT_LEVEL_NAMED_GRAPHS
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), OBJECT_LEVEL_NAMED_GRAPHS.len());
        assert!(!is_object_level_named_graph(
            "https://blackcatinformatics.ca/gmeow/graph/correspondence"
        ));
        assert!(!is_object_level_named_graph(
            "https://blackcatinformatics.ca/gmeow/graph/correspondence-laws"
        ));
        // The grounding seam registry asserts governance/policy data (which
        // cross-grounding reference channels are sanctioned), not object-level
        // axioms — excluded exactly like the correspondence-laws graph.
        assert!(!is_object_level_named_graph(
            "https://blackcatinformatics.ca/gmeow/graph/grounding-seams"
        ));
    }

    #[test]
    fn endpoint_predicates_identify_a_correspondence_without_its_type_triple() {
        // The build-time object-level-EDB twin sees a narrowed union in which the compile
        // stage has already projected the `rdf:type logic:GroundingCorrespondence` triple into
        // the meta `graph/correspondence-laws` graph, leaving only the raw endpoint triples in
        // an admitted graph. Keying on the endpoint predicates still identifies the record, so
        // its out-of-fragment `owl:InverseFunctionalProperty` referent never reaches the EDB
        // (the reason-verify regression these leaks caused).
        let ttl = concat!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n",
            "logic:corrIFP\n",
            "  logic:sourceEndpoint logic:inverseFunctionalProperty ;\n",
            "  logic:targetEndpoint <http://www.w3.org/2002/07/owl#InverseFunctionalProperty> .\n",
            "logic:Person a logic:Class .\n",
        );
        let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse ttl");

        let subjects = grounding_correspondence_subjects(&ds);
        assert!(
            subjects.contains("https://blackcatinformatics.ca/logic/corrIFP"),
            "an endpoint-only correspondence record must be detected: {subjects:?}"
        );

        let excluded = exclude_grounding_correspondences(&ds).expect("exclude correspondences");
        let leaks_ifp = excluded.owned_quads().any(|q| {
            matches!(&q.object, RdfTerm::Iri(o)
                if o == "http://www.w3.org/2002/07/owl#InverseFunctionalProperty")
        });
        assert!(
            !leaks_ifp,
            "the inverse-functional endpoint referent must be excluded from the object-level EDB"
        );
        let keeps_person = excluded.owned_quads().any(|q| {
            matches!(&q.subject, RdfTerm::Iri(s)
                if s == "https://blackcatinformatics.ca/logic/Person")
        });
        assert!(
            keeps_person,
            "ordinary object-level axioms (logic:Person a logic:Class) must be preserved"
        );
    }
}
