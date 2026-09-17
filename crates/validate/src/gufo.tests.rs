// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::parse_dataset;
use std::sync::Arc;

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

fn cfg() -> GufoConfig {
    GufoConfig {
        namespace: NS.to_owned(),
    }
}

fn store_from(ttl: &str) -> Arc<RdfDataset> {
    parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
}

const PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix gufo: <http://purl.org/nemo/gufo#> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n";

#[test]
fn missing_stereotype_is_flagged() {
    let store = store_from(&format!("{PREFIXES}gmeow:Bare a owl:Class .\n"));
    let problems = exactly_one_stereotype(&store, &cfg());
    assert!(
        problems
            .iter()
            .any(|p| p.message.contains("carries no stereotype"))
    );
}

#[test]
fn conflicting_stereotypes_are_flagged() {
    let store = store_from(&format!(
        "{PREFIXES}gmeow:TwoFaced a owl:Class , gufo:Kind , gufo:Role .\n"
    ));
    let problems = exactly_one_stereotype(&store, &cfg());
    assert!(
        problems
            .iter()
            .any(|p| p.message.contains("conflicting stereotypes"))
    );
}

#[test]
fn kind_under_kind_is_flagged_mixiden() {
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:Animal a owl:Class , gufo:Kind .\n\
             gmeow:Dog a owl:Class , gufo:Kind ; rdfs:subClassOf gmeow:Animal .\n"
    ));
    let problems = identity_overlap(&store, &cfg());
    assert!(
        problems
            .iter()
            .any(|p| p.message.contains("MixIden") && p.message.contains("gmeow:Dog"))
    );
}

#[test]
fn free_role_is_flagged() {
    let store = store_from(&format!(
        "{PREFIXES}gmeow:Wanderer a owl:Class , gufo:Role .\n"
    ));
    let problems = anti_rigidity_discipline(&store, &cfg());
    assert!(problems.iter().any(|p| p.message.contains("FreeRole")));
}

#[test]
fn rigid_under_anti_rigid_is_flagged_mixrig() {
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:Student a owl:Class , gufo:Role .\n\
             gmeow:HonorsStudent a owl:Class , gufo:SubKind ; rdfs:subClassOf gmeow:Student .\n"
    ));
    let problems = anti_rigidity_discipline(&store, &cfg());
    assert!(problems.iter().any(|p| {
        p.message.contains("MixRig")
            && p.message.contains("gmeow:HonorsStudent")
            && p.message.contains("gmeow:Student")
    }));
}

#[test]
fn under_mediated_relator_is_flagged_relcomp() {
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:LonelyBond a owl:Class , gufo:Kind ; rdfs:subClassOf gufo:Relator .\n\
             gmeow:bondParty a owl:ObjectProperty , owl:FunctionalProperty ;\n\
               rdfs:domain gmeow:LonelyBond ; rdfs:range gmeow:Person .\n"
    ));
    let problems = relator_mediation(&store, &cfg());
    assert!(
        problems
            .iter()
            .any(|p| p.message.contains("RelComp") && p.message.contains("gmeow:LonelyBond"))
    );
}

#[test]
fn relator_finding_has_discipline_code_and_location() {
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:LonelyBond a owl:Class , gufo:Kind ; rdfs:subClassOf gufo:Relator .\n\
             gmeow:bondParty a owl:ObjectProperty , owl:FunctionalProperty ;\n\
               rdfs:domain gmeow:LonelyBond ; rdfs:range gmeow:Person .\n"
    ));
    let problems = relator_mediation(&store, &cfg());
    let finding = problems
        .iter()
        .find(|p| p.message.contains("gmeow:LonelyBond"))
        .expect("under-mediated relator finding must be present");
    assert_eq!(finding.code, "discipline/relator-mediation");
    assert!(
        finding.locations.iter().any(|loc| loc
            .logical
            .as_deref()
            .is_some_and(|l| l.contains("LonelyBond"))),
        "finding must carry a logical location for LonelyBond"
    );
}

#[test]
fn well_formed_relator_passes() {
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:Bond a owl:Class , gufo:Kind ; rdfs:subClassOf gufo:Relator .\n\
             gmeow:bondLeft a owl:ObjectProperty , owl:FunctionalProperty ;\n\
               rdfs:domain gmeow:Bond ; rdfs:range gmeow:Person .\n\
             gmeow:bondRight a owl:ObjectProperty , owl:FunctionalProperty ;\n\
               rdfs:domain gmeow:Bond ; rdfs:range gmeow:Person .\n"
    ));
    assert!(relator_mediation(&store, &cfg()).is_empty());
}

#[test]
fn abstract_relator_base_is_exempt() {
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:AbstractBond a owl:Class , gufo:Kind ; rdfs:subClassOf gufo:Relator .\n\
             gmeow:ConcreteBond a owl:Class , gufo:SubKind ; rdfs:subClassOf gmeow:AbstractBond .\n"
    ));
    assert!(
        !relator_mediation(&store, &cfg())
            .iter()
            .any(|p| p.message.contains("gmeow:AbstractBond"))
    );
}

#[test]
fn disjoint_collection_is_parsed() {
    // owl:AllDisjointClasses with a 2-member owl:members collection.
    let store = store_from(&format!(
        "{PREFIXES}\
             [] a owl:AllDisjointClasses ; owl:members ( gmeow:A gmeow:B ) .\n"
    ));
    let sets = all_disjoint_member_sets(&store);
    assert_eq!(sets.len(), 1);
    let want: HashSet<String> = [format!("{NS}A"), format!("{NS}B")].into_iter().collect();
    assert_eq!(sets[0], want);
}

#[test]
fn coequal_bridge_is_flagged() {
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:axisA gmeow:coequalFacet true ; rdfs:range gmeow:RangeA .\n\
             gmeow:axisB gmeow:coequalFacet true ; rdfs:range gmeow:RangeB ;\n\
               rdfs:subPropertyOf gmeow:axisA .\n"
    ));
    let problems = coequal_facet_orthogonality(&store, &cfg());
    assert!(problems.iter().any(|p| p.message.contains("bridged")));
}

#[test]
fn frame_completeness_is_flagged() {
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:pointsFrame rdfs:subPropertyOf gmeow:hasReferenceFrame ;\n\
               rdfs:domain gmeow:Carrier .\n"
    ));
    let problems = frame_declaration_completeness(&store, &cfg());
    assert!(
        problems
            .iter()
            .any(|p| p.message.contains("gmeow:Carrier") && p.message.contains("P11"))
    );
}

#[test]
fn clean_graph_has_no_problems() {
    let store = store_from(&format!(
        "{PREFIXES}gmeow:Animal a owl:Class , gufo:Kind .\n"
    ));
    assert!(reasoning_invariants(&store, &cfg()).is_empty());
}

// ── logic: stereotype acceptance (owl/gUFO → logic: migration) ────────

#[test]
fn logic_kind_satisfies_stereotype_requirement() {
    // The canonical logic: form is accepted exactly as gufo: was — no
    // "carries no gUFO meta-class" for a class stereotyped a logic:Kind.
    let store = store_from(&format!(
        "{PREFIXES}gmeow:Animal a owl:Class , logic:Kind .\n"
    ));
    assert!(exactly_one_stereotype(&store, &cfg()).is_empty());
}

#[test]
fn logic_perdurant_rename_is_accepted() {
    // gufo:EventType / gufo:SituationType down-project to logic:Event /
    // logic:Situation; both are valid perdurant stereotypes.
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:Wedding a owl:Class , logic:Event .\n\
             gmeow:Marriage a owl:Class , logic:Situation .\n"
    ));
    assert!(exactly_one_stereotype(&store, &cfg()).is_empty());
}

#[test]
fn logic_sortal_under_logic_kind_passes_mixiden() {
    // A logic:SubKind that traces to exactly one logic:Kind is well-formed.
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:Animal a owl:Class , logic:Kind .\n\
             gmeow:Dog a owl:Class , logic:SubKind ; rdfs:subClassOf gmeow:Animal .\n"
    ));
    assert!(identity_overlap(&store, &cfg()).is_empty());
}

#[test]
fn logic_relator_is_mediation_checked() {
    // An under-mediated logic:Relator is flagged exactly like a gufo:Relator.
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:LonelyBond a owl:Class , logic:Kind ; rdfs:subClassOf logic:Relator .\n\
             gmeow:bondParty a owl:ObjectProperty , owl:FunctionalProperty ;\n\
               rdfs:domain gmeow:LonelyBond ; rdfs:range gmeow:Person .\n"
    ));
    assert!(
        relator_mediation(&store, &cfg())
            .iter()
            .any(|p| p.message.contains("RelComp") && p.message.contains("gmeow:LonelyBond"))
    );
}

/// The subsumption traversals see a CANONICAL `logic:subClassOf` edge — the
/// property the shared [`gmeow_ns::SUB_CLASS_OF`] definition guarantees, pinned
/// here so a future narrowing of it re-reds this crate too.
///
/// `proper_ancestors` (via `identity_overlap`) and `gmeow_subclasses` (via
/// `relator_mediation`) must both trace an edge authored with NO `rdfs:`
/// spelling anywhere; an `rdfs:`-only read would report both fixtures clean by
/// simply not seeing the hierarchy.
#[test]
fn canonical_logic_subclass_edges_are_traversed() {
    // MixIden: two logic:Kind ancestors reached ONLY over `logic:subClassOf`.
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:Animal a owl:Class , logic:Kind .\n\
             gmeow:Machine a owl:Class , logic:Kind .\n\
             gmeow:Cyborg a owl:Class , logic:SubKind ;\n\
               logic:subClassOf gmeow:Animal , gmeow:Machine .\n"
    ));
    assert!(
        identity_overlap(&store, &cfg())
            .iter()
            .any(|p| p.message.contains("MixIden") && p.message.contains("gmeow:Cyborg")),
        "proper_ancestors must traverse the canonical subsumption edge"
    );

    // RelComp: a two-level chain — AbstractBond specializes logic:Relator, and
    // LonelyBond specializes AbstractBond — with EVERY edge authored only over
    // the canonical `logic:subClassOf` spelling (no `rdfs:` anywhere). This
    // pins `gmeow_subclasses` (not just `proper_ancestors`): a mutation to
    // RDFS-only `gmeow_subclasses` REDS this exact fixture, because AbstractBond
    // would then look concrete (no subclass found) and wrongly earn its own
    // RelComp finding instead of being skipped as the abstract base. (Verified:
    // reverting `gmeow_subclasses` to `[gmeow_ns::RDFS_SUB_CLASS_OF]` alone kept
    // the ORIGINAL single-level fixture green — LonelyBond had no subclasses
    // either way, so it never exercised `gmeow_subclasses` at all — which is
    // exactly why this fixture was extended to a second level.)
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:AbstractBond a owl:Class , logic:Kind ; logic:subClassOf logic:Relator .\n\
             gmeow:LonelyBond a owl:Class , logic:Kind ; logic:subClassOf gmeow:AbstractBond .\n\
             gmeow:bondParty a owl:ObjectProperty , owl:FunctionalProperty ;\n\
               rdfs:domain gmeow:LonelyBond ; rdfs:range gmeow:Person .\n"
    ));

    let abstract_bond = format!("{NS}AbstractBond");
    let lonely_bond = format!("{NS}LonelyBond");
    assert!(
        gmeow_subclasses(&store, &abstract_bond).contains(&lonely_bond),
        "gmeow_subclasses must match LonelyBond as a subclass of AbstractBond over \
             the canonical logic:subClassOf edge"
    );

    let findings = relator_mediation(&store, &cfg());
    assert!(
        findings
            .iter()
            .any(|p| p.message.contains("RelComp") && p.message.contains("gmeow:LonelyBond")),
        "the concrete child LonelyBond must get the RelComp finding: {findings:?}"
    );
    assert!(
        !findings
            .iter()
            .any(|p| p.message.contains("gmeow:AbstractBond")),
        "the abstract base AbstractBond must NOT get its own finding — its concrete \
             subtype carries the mediation: {findings:?}"
    );
}

#[test]
fn mixed_namespace_double_stereotype_is_flagged() {
    // A class mid-migration carrying BOTH gufo:Kind and logic:Kind is two
    // stereotypes — the cardinality discipline still flags it.
    let store = store_from(&format!(
        "{PREFIXES}gmeow:Half a owl:Class , gufo:Kind , logic:Kind .\n"
    ));
    assert!(
        exactly_one_stereotype(&store, &cfg())
            .iter()
            .any(|p| p.message.contains("conflicting stereotypes"))
    );
}
