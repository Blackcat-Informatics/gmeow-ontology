// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! RDF-isomorphic projection back-ends: OWL-DL, OWL-EL, gUFO, canonical-RDF12.
//!
//! These build a wasm-clean triple set ([`RdfDatasetBuilder`]) and serialize to
//! Turtle through the native codec.  The conformance goldens compare these targets
//! by **graph isomorphism** (not bytes), so the serialization need only reproduce
//! the same triples.  The Python duplicate was retired in #727; this is the source
//! of truth.

use gmeow_rdf::{serialize_dataset, RdfDatasetBuilder, RdfLiteral, SerializeGraph};

use super::super::ir::{LogicModality, LogicProgram};
use super::{
    assert_no_overclaim, contract_drop_notes, generated_banner, is_modal_or_scoped, target_meta,
    OverclaimError, ProjectionResult, GMEOW_NS, LOGIC_NS, OWL_NS, RDFS_NS, RDF_NS, RDF_TYPE,
    XSD_NS,
};

const GUFO_NS: &str = "http://purl.org/nemo/gufo#";

fn rdfs(local: &str) -> String {
    format!("{RDFS_NS}{local}")
}
fn owl(local: &str) -> String {
    format!("{OWL_NS}{local}")
}
fn logic(local: &str) -> String {
    format!("{LOGIC_NS}{local}")
}

/// Resolve a facet *value* to its emitted IRI. The open facet-value vocabulary
/// (#767, Gap 3) admits values that are already full custom IRIs (not under the
/// `logic:` namespace); these must be emitted verbatim, not re-prefixed (which
/// would yield a corrupt `…/logic/https://…`). A bare local name is prefixed under
/// the `logic:` namespace. This is symmetric with the front-end storage convention
/// (`strip_prefix(LOGIC_NAMESPACE).unwrap_or(full_iri)`), so a custom IRI
/// round-trips identically.
fn facet_value_iri(value: &str) -> String {
    if value.starts_with("http://") || value.starts_with("https://") {
        value.to_owned()
    } else {
        logic(value)
    }
}

// --------------------------------------------------------------------------- //
// Projection-side mapping tables (the authoritative logic: → OWL/gUFO maps)
// --------------------------------------------------------------------------- //

/// logic: sort IRI → gUFO class IRI (the 37 faithful down-projection targets).
fn gufo_for_sort(obj: &str) -> Option<String> {
    let local = obj.strip_prefix(LOGIC_NS)?;
    let g = match local {
        "Kind" => "Kind",
        "SubKind" => "SubKind",
        "Phase" => "Phase",
        "Role" => "Role",
        "Category" => "Category",
        "Mixin" => "Mixin",
        "RoleMixin" => "RoleMixin",
        "PhaseMixin" => "PhaseMixin",
        "Relator" => "Relator",
        "Event" => "EventType",
        "Situation" => "SituationType",
        "Individual" => "Individual",
        "ConcreteIndividual" => "ConcreteIndividual",
        "AbstractIndividual" => "AbstractIndividual",
        "Endurant" => "Endurant",
        "Participation" => "Participation",
        "Object" => "Object",
        "Aspect" => "Aspect",
        "Quality" => "Quality",
        "QualityValue" => "QualityValue",
        "Collection" => "Collection",
        "FixedCollection" => "FixedCollection",
        "VariableCollection" => "VariableCollection",
        "Quantity" => "Quantity",
        "FunctionalComplex" => "FunctionalComplex",
        "Type" => "Type",
        "EndurantType" => "EndurantType",
        "RelationshipType" => "RelationshipType",
        "MaterialRelationshipType" => "MaterialRelationshipType",
        "ComparativeRelationshipType" => "ComparativeRelationshipType",
        "AbstractIndividualType" => "AbstractIndividualType",
        "ConcreteIndividualType" => "ConcreteIndividualType",
        "Sortal" => "Sortal",
        "NonSortal" => "NonSortal",
        "RigidType" => "RigidType",
        "AntiRigidType" => "AntiRigidType",
        "SemiRigidType" => "SemiRigidType",
        "NonRigidType" => "NonRigidType",
        _ => return None,
    };
    Some(format!("{GUFO_NS}{g}"))
}

/// logic: structural predicate IRI → OWL/RDFS predicate IRI.
fn owl_for_pred(pred: &str) -> Option<String> {
    let local = pred.strip_prefix(LOGIC_NS)?;
    Some(match local {
        "subClassOf" => rdfs("subClassOf"),
        "equivalentClass" => owl("equivalentClass"),
        "disjointWith" => owl("disjointWith"),
        "subPropertyOf" => rdfs("subPropertyOf"),
        "equivalentProperty" => owl("equivalentProperty"),
        "inverseOf" => owl("inverseOf"),
        "domain" => rdfs("domain"),
        "range" => rdfs("range"),
        _ => return None,
    })
}

/// logic: characteristic sort IRI → OWL characteristic-type IRI.
fn owl_for_char(obj: &str) -> Option<String> {
    let local = obj.strip_prefix(LOGIC_NS)?;
    Some(match local {
        "transitiveProperty" => owl("TransitiveProperty"),
        "symmetricProperty" => owl("SymmetricProperty"),
        "functionalProperty" => owl("FunctionalProperty"),
        "inverseFunctionalProperty" => owl("InverseFunctionalProperty"),
        _ => return None,
    })
}

fn is_el_safe_pred(pred: &str) -> bool {
    matches!(
        pred.strip_prefix(LOGIC_NS),
        Some("subClassOf" | "equivalentClass" | "subPropertyOf" | "domain" | "range")
    )
}

fn is_el_safe_char(obj: &str) -> bool {
    obj.strip_prefix(LOGIC_NS) == Some("transitiveProperty")
}

// --------------------------------------------------------------------------- //
// Triple sink + deterministic Turtle serialization
// --------------------------------------------------------------------------- //

/// Accumulates triples (default graph) and serializes them to deterministic
/// Turtle.  Only IRI subjects/predicates and IRI/Literal objects are used by any
/// projection; a well-formed program only ever supplies valid IRIs (the corpus is
/// the parity anchor), so triples are interned directly into the wasm-clean
/// [`RdfDatasetBuilder`].
#[derive(Default)]
pub(crate) struct TripleSink {
    builder: RdfDatasetBuilder,
}

impl TripleSink {
    pub(crate) fn add_iri(&mut self, s: &str, p: &str, o: &str) {
        let s = self.builder.intern_iri(s.to_owned());
        let p = self.builder.intern_iri(p.to_owned());
        let o = self.builder.intern_iri(o.to_owned());
        self.builder.push_quad(s, p, o, None);
    }

    pub(crate) fn add_lit(&mut self, s: &str, p: &str, lit: RdfLiteral) {
        let s = self.builder.intern_iri(s.to_owned());
        let p = self.builder.intern_iri(p.to_owned());
        let o = self.builder.intern_literal(lit);
        self.builder.push_quad(s, p, o, None);
    }

    /// Add a typed/plain object that may be an IRI or a literal.
    pub(crate) fn add_obj(&mut self, s: &str, p: &str, obj: &str, obj_is_literal: bool) {
        if obj_is_literal {
            self.add_lit(s, p, RdfLiteral::simple(obj));
        } else {
            self.add_iri(s, p, obj);
        }
    }

    /// Serialize to Turtle with a GENERATED banner.  The triple set is frozen into
    /// the `RdfDataset` IR and serialized through the native codec (#909), which
    /// emits canonical, deterministic Turtle — so no manual pre-sort is needed (the
    /// goldens compare by isomorphism either way). All projected quads live in the
    /// default graph, so `SerializeGraph::DefaultGraph` is the faithful selector.
    pub(crate) fn serialize(self, banner: &str) -> String {
        let body = self
            .builder
            .freeze()
            .ok()
            .and_then(|dataset| {
                serialize_dataset(
                    dataset.as_ref(),
                    "text/turtle",
                    SerializeGraph::DefaultGraph,
                )
                .ok()
            })
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_default();
        let body = format!("{}\n", body.trim_end_matches('\n'));
        format!("{banner}{body}")
    }
}

fn rdf_result(
    target: &str,
    sink: TripleSink,
    banner_label: &str,
    actual_drops: Vec<String>,
) -> Result<ProjectionResult, OverclaimError> {
    let (kind, cx, drops) = target_meta(target);
    assert_no_overclaim(target, kind, &actual_drops)?;
    let content = sink.serialize(&generated_banner(banner_label));
    Ok(ProjectionResult {
        target: target.to_owned(),
        content,
        is_rdf: true,
        preservation: kind,
        complexity: cx.to_owned(),
        lossy_drops: drops.into_iter().map(str::to_owned).collect(),
        actual_drops,
    })
}

// --------------------------------------------------------------------------- //
// Holon surface helper
// --------------------------------------------------------------------------- //

/// Emit the OWL skeleton for the holon vocabulary: class and property
/// declarations that survive the lossy OWL projection.  Called only when the
/// program actually uses `logic:properPartOf` (i.e. it has holon axioms).
///
/// Loss ledger rationale:
/// * `asymmetric` + `irreflexive` characteristics on `logic:properPartOf` are
///   not expressible in OWL 2 DL; only `owl:TransitiveProperty` survives.
/// * The five-place `logic:HolonicPosition` relation is projected as the unary
///   `logic:Holon` class; positional arity is dropped.
/// * `logic:WeakSupplementation` stays in the logic: layer; no OWL lowering.
fn emit_holon_surface(g: &mut TripleSink) {
    // Classes
    for cls in &["Holon", "HolonicPosition", "Holarchy"] {
        g.add_iri(&logic(cls), RDF_TYPE, &owl("Class"));
    }
    // Object properties
    for prop in &[
        "properPartOf",
        "isHolon",
        "positionEntity",
        "positionHolarchy",
        "positionContext",
        "positionInterval",
        "positionPath",
    ] {
        g.add_iri(&logic(prop), RDF_TYPE, &owl("ObjectProperty"));
    }
    // properPartOf also gets TransitiveProperty (transitivity survives;
    // asymmetric + irreflexive are the documented loss).
    g.add_iri(&logic("properPartOf"), RDF_TYPE, &owl("TransitiveProperty"));
    // Datatype property
    g.add_iri(&logic("holonicLevel"), RDF_TYPE, &owl("DatatypeProperty"));
}

// --------------------------------------------------------------------------- //
// OWL 2 DL
// --------------------------------------------------------------------------- //

/// Project to OWL 2 DL Turtle (`generated/owl/gmeow-dl.ttl`).
pub fn project_owl_dl(program: &LogicProgram) -> Result<ProjectionResult, OverclaimError> {
    let mut g = TripleSink::default();
    let mut actual_drops: Vec<String> = Vec::new();

    g.add_iri(
        &format!("{GMEOW_NS}owl/gmeow-dl"),
        RDF_TYPE,
        &owl("Ontology"),
    );

    // Detect holon usage before the axiom loop so we can emit the surface once.
    let uses_holons = program
        .axioms
        .iter()
        .any(|a| a.predicate == logic("properPartOf"));
    if uses_holons {
        emit_holon_surface(&mut g);
    }

    for axiom in &program.axioms {
        let pred = &axiom.predicate;
        let obj = &axiom.obj;
        if pred == RDF_TYPE {
            if let Some(gufo_type) = gufo_for_sort(obj) {
                g.add_iri(&axiom.subject, RDF_TYPE, &gufo_type);
                g.add_iri(&axiom.subject, RDF_TYPE, &owl("Class"));
                continue;
            }
            if let Some(owl_char) = owl_for_char(obj) {
                g.add_iri(&axiom.subject, RDF_TYPE, &owl_char);
                g.add_iri(&axiom.subject, RDF_TYPE, &owl("ObjectProperty"));
                continue;
            }
            if !axiom.obj_is_literal {
                g.add_iri(&axiom.subject, RDF_TYPE, obj);
            }
            continue;
        }
        // properPartOf edges survive as object-property assertions.
        if pred == &logic("properPartOf") {
            if !axiom.obj_is_literal {
                g.add_iri(&axiom.subject, &logic("properPartOf"), obj);
            }
            continue;
        }
        if let Some(owl_pred) = owl_for_pred(pred) {
            g.add_obj(&axiom.subject, &owl_pred, obj, axiom.obj_is_literal);
            continue;
        }
        if let Some(local) = pred.strip_prefix(LOGIC_NS) {
            actual_drops.push(format!(
                "logic:{local} on <{}> has no OWL DL equivalent",
                axiom.subject
            ));
        }
    }

    // Record holon-surface structural losses.
    if uses_holons {
        actual_drops.push(
            "logic:properPartOf strict-order characteristics (asymmetric + irreflexive) \
             cannot be declared in OWL 2 DL; only owl:TransitiveProperty is projected"
                .to_string(),
        );
        actual_drops.push(
            "the five-place logic:HolonicPosition relation is projected lossily as the \
             unary logic:Holon class; its positional arity (holarchy, context, interval, \
             path) is dropped"
                .to_string(),
        );
        actual_drops.push(
            "the logic:WeakSupplementation mereology axiom is not lowered to OWL and \
             stays in logic:"
                .to_string(),
        );
    }

    for rule in &program.rules {
        let head = &rule.head;
        if rule.body.len() == 1
            && owl_for_pred(&head.predicate).is_some()
            && !head.obj_is_literal
            && owl_for_pred(&rule.body[0].predicate).is_some()
        {
            let owl_head_pred = owl_for_pred(&head.predicate).unwrap();
            g.add_iri(&head.subject, &owl_head_pred, &head.obj);
            continue;
        }
        actual_drops.push(format!(
            "rule head <{}> {} not expressible in OWL DL (body complexity)",
            rule.head.subject,
            super::python_repr(&rule.head.predicate)
        ));
    }

    actual_drops.extend(contract_drop_notes(program, "OWL 2 DL"));
    rdf_result("owl-dl", g, "OWL 2 DL", actual_drops)
}

// --------------------------------------------------------------------------- //
// OWL 2 EL
// --------------------------------------------------------------------------- //

/// Project to OWL 2 EL Turtle (`generated/owl/gmeow-el.ttl`).
pub fn project_owl_el(program: &LogicProgram) -> Result<ProjectionResult, OverclaimError> {
    let mut g = TripleSink::default();
    let mut actual_drops: Vec<String> = Vec::new();

    g.add_iri(
        &format!("{GMEOW_NS}owl/gmeow-el"),
        RDF_TYPE,
        &owl("Ontology"),
    );

    // Detect holon usage before the axiom loop so we can emit the surface once.
    let uses_holons = program
        .axioms
        .iter()
        .any(|a| a.predicate == logic("properPartOf"));
    if uses_holons {
        emit_holon_surface(&mut g);
    }

    for axiom in &program.axioms {
        let pred = &axiom.predicate;
        let obj = &axiom.obj;
        if pred == RDF_TYPE {
            if let Some(gufo_type) = gufo_for_sort(obj) {
                g.add_iri(&axiom.subject, RDF_TYPE, &gufo_type);
                g.add_iri(&axiom.subject, RDF_TYPE, &owl("Class"));
                continue;
            }
            if is_el_safe_char(obj) {
                let owl_char = owl_for_char(obj).unwrap();
                g.add_iri(&axiom.subject, RDF_TYPE, &owl_char);
                g.add_iri(&axiom.subject, RDF_TYPE, &owl("ObjectProperty"));
                continue;
            }
            if let Some(local) = obj.strip_prefix(LOGIC_NS) {
                if owl_for_char(obj).is_some() {
                    actual_drops.push(format!(
                        "logic:{local} on <{}> is not EL-safe; dropped",
                        axiom.subject
                    ));
                    continue;
                }
            }
            if !axiom.obj_is_literal {
                g.add_iri(&axiom.subject, RDF_TYPE, obj);
            }
            continue;
        }
        // properPartOf edges survive as object-property assertions (transitivity
        // is EL-safe; asymmetric/irreflexive are the documented loss).
        if pred == &logic("properPartOf") {
            if !axiom.obj_is_literal {
                g.add_iri(&axiom.subject, &logic("properPartOf"), obj);
            }
            continue;
        }
        if is_el_safe_pred(pred) {
            let owl_pred = owl_for_pred(pred).unwrap();
            g.add_obj(&axiom.subject, &owl_pred, obj, axiom.obj_is_literal);
            continue;
        }
        if let Some(local) = pred.strip_prefix(LOGIC_NS) {
            if owl_for_pred(pred).is_some() {
                actual_drops.push(format!(
                    "logic:{local} on <{}> is not EL-safe; dropped",
                    axiom.subject
                ));
            } else {
                actual_drops.push(format!(
                    "logic:{local} on <{}> has no EL equivalent",
                    axiom.subject
                ));
            }
        }
    }

    // Record holon-surface structural losses.
    if uses_holons {
        actual_drops.push(
            "logic:properPartOf strict-order characteristics (asymmetric + irreflexive) \
             cannot be declared in OWL 2 EL; only owl:TransitiveProperty is projected"
                .to_string(),
        );
        actual_drops.push(
            "the five-place logic:HolonicPosition relation is projected lossily as the \
             unary logic:Holon class; its positional arity (holarchy, context, interval, \
             path) is dropped"
                .to_string(),
        );
        actual_drops.push(
            "the logic:WeakSupplementation mereology axiom is not lowered to OWL 2 EL \
             and stays in logic:"
                .to_string(),
        );
    }

    for rule in &program.rules {
        actual_drops.push(format!(
            "rule head <{}> dropped (EL has no rule surface)",
            rule.head.subject
        ));
    }

    actual_drops.extend(contract_drop_notes(program, "OWL 2 EL"));
    rdf_result("owl-el", g, "OWL 2 EL", actual_drops)
}

// --------------------------------------------------------------------------- //
// gUFO bridge
// --------------------------------------------------------------------------- //

/// Project to gUFO bridge Turtle (`generated/foundation/gufo.ttl`).
pub fn project_gufo(program: &LogicProgram) -> Result<ProjectionResult, OverclaimError> {
    let mut g = TripleSink::default();
    let mut actual_drops: Vec<String> = Vec::new();

    g.add_iri(
        &format!("{GMEOW_NS}foundation/gufo"),
        RDF_TYPE,
        &owl("Ontology"),
    );

    for axiom in &program.axioms {
        let pred = &axiom.predicate;
        let obj = &axiom.obj;
        if pred == RDF_TYPE {
            if let Some(gufo_type) = gufo_for_sort(obj) {
                g.add_iri(&axiom.subject, RDF_TYPE, &gufo_type);
                continue;
            }
            if let Some(local) = obj.strip_prefix(LOGIC_NS) {
                actual_drops.push(format!(
                    "rdf:type logic:{local} on <{}> has no gUFO equivalent",
                    axiom.subject
                ));
            }
            continue;
        }
        if pred == &logic("subClassOf") {
            if !axiom.obj_is_literal {
                g.add_iri(&axiom.subject, &rdfs("subClassOf"), obj);
            }
            continue;
        }
        if let Some(local) = pred.strip_prefix(LOGIC_NS) {
            actual_drops.push(format!(
                "logic:{local} on <{}> has no gUFO bridge equivalent",
                axiom.subject
            ));
        }
    }

    for rule in &program.rules {
        actual_drops.push(format!(
            "rule head <{}> dropped (gUFO bridge has no rule surface)",
            rule.head.subject
        ));
    }

    actual_drops.extend(contract_drop_notes(program, "the gUFO bridge"));
    rdf_result("gufo", g, "gUFO bridge", actual_drops)
}

// --------------------------------------------------------------------------- //
// Canonical RDF 1.2 (round-trippable)
// --------------------------------------------------------------------------- //

/// Project to canonical RDF 1.2 Turtle (`generated/logic/gmeow.logic.rdf12.ttl`).
pub fn project_canonical_rdf12(program: &LogicProgram) -> Result<ProjectionResult, OverclaimError> {
    let mut g = TripleSink::default();

    g.add_iri(
        &format!("{GMEOW_NS}logic/gmeow.logic.rdf12"),
        RDF_TYPE,
        &owl("Ontology"),
    );

    let rule_struct_preds = [
        logic("head"),
        logic("body"),
        logic("negatedBody"),
        logic("distinctBody"),
    ];

    // Axioms (skipping rule-structural predicates — re-emitted as Rule nodes).
    for axiom in &program.axioms {
        if rule_struct_preds.contains(&axiom.predicate) {
            continue;
        }
        g.add_obj(
            &axiom.subject,
            &axiom.predicate,
            &axiom.obj,
            axiom.obj_is_literal,
        );

        if is_modal_or_scoped(axiom) {
            let key_hash = sha256_12(&axiom.sort_key());
            let reifier = format!("{LOGIC_NS}reifier/{key_hash}");
            g.add_iri(&reifier, RDF_TYPE, &format!("{RDF_NS}Statement"));
            g.add_iri(&reifier, &format!("{RDF_NS}subject"), &axiom.subject);
            g.add_iri(&reifier, &format!("{RDF_NS}predicate"), &axiom.predicate);
            if axiom.obj_is_literal {
                g.add_lit(
                    &reifier,
                    &format!("{RDF_NS}object"),
                    RdfLiteral::simple(&axiom.obj),
                );
            } else {
                g.add_iri(&reifier, &format!("{RDF_NS}object"), &axiom.obj);
            }
            let scope = &axiom.scope;
            if let Some(sp) = &scope.standpoint {
                g.add_iri(&reifier, &logic("standpoint"), sp);
            }
            if let Some(t) = &scope.time {
                g.add_lit(&reifier, &logic("time"), RdfLiteral::simple(t));
            }
            if let Some(c) = scope.confidence {
                g.add_lit(&reifier, &logic("confidence"), decimal_literal(c));
            }
            if scope.modality != LogicModality::None {
                g.add_iri(
                    &reifier,
                    &logic("modality"),
                    &logic(scope.modality.as_str()),
                );
            }
            if let Some(p) = &scope.provenance {
                g.add_iri(&reifier, &logic("provenance"), p);
            }
        }
    }

    // Reasoning contracts (#767). LOSSLESS projection: every contract — whether
    // it carries a preset or only direct facets — is emitted in full as DIRECT
    // facet properties on its subject node, so a re-parse through
    // `extract_contracts` reconstructs the byte-identical `ReasoningContract`
    // (same `sort_key()`).  The values are emitted as plain `logic:<Value>` IRIs;
    // the parser routes them by the FACET PROPERTY (not the value's rdf:type), so
    // the projection need not (and does not) re-emit each value's facet-class type.
    for (idx, contract) in program.contracts.iter().enumerate() {
        project_contract(&mut g, idx, contract);
    }

    // Rules as logic:Rule nodes with classic reification for head/body.
    for (idx, rule) in program.rules.iter().enumerate() {
        let rule_id = format!("_{:06}", idx + 1);
        let rule_node = format!("{LOGIC_NS}rule/{rule_id}");
        g.add_iri(&rule_node, RDF_TYPE, &logic("Rule"));

        // Head.
        let head = &rule.head;
        let head_node = format!("{LOGIC_NS}rule/{rule_id}/head");
        g.add_iri(&rule_node, &logic("head"), &head_node);
        g.add_iri(&head_node, RDF_TYPE, &format!("{RDF_NS}Statement"));
        add_reified_term(&mut g, &head_node, "subject", &head.subject, false);
        g.add_iri(&head_node, &format!("{RDF_NS}predicate"), &head.predicate);
        add_reified_term(&mut g, &head_node, "object", &head.obj, head.obj_is_literal);

        // Body (positive then negated), each polarity sorted independently.
        let positive: Vec<_> = rule.body.iter().filter(|a| !a.negated).collect();
        let negated: Vec<_> = rule.body.iter().filter(|a| a.negated).collect();
        for (link_local, path_seg, atoms) in [
            ("body", "body", &positive),
            ("negatedBody", "negatedBody", &negated),
        ] {
            let mut sorted = atoms.clone();
            sorted.sort_by_cached_key(|a| a.sort_key());
            for (i, ba) in sorted.iter().enumerate() {
                let body_node = format!("{LOGIC_NS}rule/{rule_id}/{path_seg}/{i:04}");
                g.add_iri(&rule_node, &logic(link_local), &body_node);
                g.add_iri(&body_node, RDF_TYPE, &format!("{RDF_NS}Statement"));
                add_reified_term(&mut g, &body_node, "subject", &ba.subject, false);
                g.add_iri(&body_node, &format!("{RDF_NS}predicate"), &ba.predicate);
                add_reified_term(&mut g, &body_node, "object", &ba.obj, ba.obj_is_literal);
            }
        }

        // Inequality guards.
        for (i, (var_a, var_b)) in rule.distinct_pairs.iter().enumerate() {
            let distinct_node = format!("{LOGIC_NS}rule/{rule_id}/distinctBody/{i:04}");
            g.add_iri(&rule_node, &logic("distinctBody"), &distinct_node);
            g.add_iri(&distinct_node, RDF_TYPE, &format!("{RDF_NS}Statement"));
            g.add_lit(
                &distinct_node,
                &format!("{RDF_NS}subject"),
                RdfLiteral::simple(var_a),
            );
            g.add_lit(
                &distinct_node,
                &format!("{RDF_NS}object"),
                RdfLiteral::simple(var_b),
            );
        }

        // Rule scope.
        let scope = &rule.scope;
        if let Some(sp) = &scope.standpoint {
            g.add_iri(&rule_node, &logic("standpoint"), sp);
        }
        if let Some(t) = &scope.time {
            g.add_lit(&rule_node, &logic("time"), RdfLiteral::simple(t));
        }
        if let Some(c) = scope.confidence {
            g.add_lit(&rule_node, &logic("confidence"), decimal_literal(c));
        }
        if scope.modality != LogicModality::None {
            g.add_iri(
                &rule_node,
                &logic("modality"),
                &logic(scope.modality.as_str()),
            );
        }
        if let Some(p) = &scope.provenance {
            g.add_iri(&rule_node, &logic("provenance"), p);
        }
    }

    rdf_result("canonical-rdf12", g, "Canonical RDF 1.2", Vec::new())
}

/// Project a single [`ReasoningContract`] losslessly as DIRECT facet properties.
///
/// The subject node is the preset's IRI when the contract carries a preset (typed
/// `logic:ReasoningPreset`), else a deterministic, content-free contract node
/// `logic:contract/_NNNNNN` (typed `logic:ReasoningContract`) minted from the
/// contract's canonical position — exactly the `rule/_NNNNNN` scheme used for
/// anonymous rule nodes above.  Because the program's `contracts` vector is
/// canonically sorted, `idx` is a stable function of the program content.
///
/// Every facet selection is emitted as the SAME direct facet property the
/// front-end (`extract_contracts`) reads, so the projection round-trips:
/// single-valued → one `logic:<facetProp> logic:<Value>` triple; set-valued →
/// one triple per member; closure map → a `logic:ClosureEntry` node per entry
/// (`logic:closureKey` string + `logic:closureValue logic:<Value>`) plus the
/// `logic:defaultClosure logic:<Value>` default; complexity →
/// `logic:complexityClass`.
fn project_contract(
    g: &mut TripleSink,
    idx: usize,
    contract: &super::super::ir::ReasoningContract,
) {
    let node = match contract.preset {
        Some(preset) => {
            let pid = logic(preset.as_str());
            g.add_iri(&pid, RDF_TYPE, &logic("ReasoningPreset"));
            pid
        }
        None => {
            let node = format!("{LOGIC_NS}contract/_{:06}", idx + 1);
            g.add_iri(&node, RDF_TYPE, &logic("ReasoningContract"));
            node
        }
    };

    // Single-valued facets: (property local name, value).
    let singletons: [(&str, &Option<String>); 10] = [
        ("formulaFragment", &contract.formula_fragment),
        ("modelSemantics", &contract.model_semantics),
        ("truthAlgebra", &contract.truth_algebra),
        ("admissibleValuation", &contract.admissible_valuation),
        ("designatedValues", &contract.designated_values),
        ("evolution", &contract.evolution),
        ("argumentation", &contract.argumentation),
        ("revision", &contract.revision),
        ("equalityPolicy", &contract.equality_policy),
        ("defaultClosure", &contract.default_closure),
    ];
    for (prop, value) in singletons {
        if let Some(v) = value {
            g.add_iri(&node, &logic(prop), &facet_value_iri(v));
        }
    }

    // Set-valued facets: (property local name, sorted member set).
    let sets: [(&str, &std::collections::BTreeSet<String>); 5] = [
        ("negationOperator", &contract.negation_operators),
        ("contextAxis", &contract.context_axes),
        ("uncertaintyMeasure", &contract.uncertainty_measures),
        ("resourcePolicy", &contract.resource_policies),
        ("projectionTarget", &contract.projection_targets),
    ];
    for (prop, members) in sets {
        for member in members {
            g.add_iri(&node, &logic(prop), &facet_value_iri(member));
        }
    }

    // Closure map: one logic:ClosureEntry node per binding (BTreeMap ⇒ sorted),
    // each carrying its key string + closure value individual.
    for (i, (key, val)) in contract.closure_entries.iter().enumerate() {
        let entry = format!("{node}/closureEntry/{i:04}");
        g.add_iri(&node, &logic("closureEntry"), &entry);
        g.add_iri(&entry, RDF_TYPE, &logic("ClosureEntry"));
        g.add_lit(&entry, &logic("closureKey"), RdfLiteral::simple(key));
        g.add_iri(&entry, &logic("closureValue"), &facet_value_iri(val));
    }

    // Carried decidability data.
    if let Some(c) = &contract.complexity {
        g.add_lit(
            &node,
            &logic("complexityClass"),
            RdfLiteral::simple(c.label()),
        );
    }
}

/// Add a reified `rdf:subject`/`rdf:object` term: a `?`-variable is emitted as a
/// plain Literal (to round-trip), else IRI / literal per `is_literal`.
fn add_reified_term(g: &mut TripleSink, node: &str, role: &str, value: &str, is_literal: bool) {
    let pred = format!("{RDF_NS}{role}");
    // A `?`-variable round-trips as a plain Literal, exactly like an actual
    // literal object; only proper IRIs are emitted as IRIs.
    if value.starts_with('?') || is_literal {
        g.add_lit(node, &pred, RdfLiteral::simple(value));
    } else {
        g.add_iri(node, &pred, value);
    }
}

/// `Literal(value, datatype=xsd:decimal)` with a Python-`str(float)`-style lexical.
fn decimal_literal(value: f64) -> RdfLiteral {
    RdfLiteral::typed(format_decimal(value), format!("{XSD_NS}decimal"))
}

/// Format an f64 the way the lexical of an xsd:decimal literal reads (`0.9`),
/// matching the Python/rdflib decimal serialization for the corpus values.
fn format_decimal(value: f64) -> String {
    let s = format!("{value}");
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{s}.0")
    }
}

/// First 12 hex chars of SHA-256 of `s` — the content-stable reifier key hash
/// (`sha256(sort_key)[:12]`).
fn sha256_12(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(s.as_bytes());
    let mut out = String::with_capacity(12);
    for b in digest.iter().take(6) {
        out.push_str(&format!("{b:02x}"));
    }
    out
}
