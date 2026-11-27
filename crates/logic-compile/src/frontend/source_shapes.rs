// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-owned validation and predicate-path shape extraction.

use super::*;

// --------------------------------------------------------------------------- //
// Path-shape extraction (absent logic:PathShape → empty list)
// --------------------------------------------------------------------------- //

/// Parse a positive-integer literal's lexical value (`xsd:positiveInteger`),
/// returning `None` on a non-numeric, overflowing, or non-positive value.
pub(super) fn parse_positive_int(lexical: &str) -> Option<u32> {
    match lexical.trim().parse::<u32>() {
        Ok(n) if n >= 1 => Some(n),
        _ => None,
    }
}

/// Derive closed-world [`ValidationShapeIr`]s from an ontology graph's OWL restrictions (the
/// closed-world validation reading of the open-world axioms). For every `Class rdfs:subClassOf [
/// owl:onProperty P ; owl:someValuesFrom C ]` the target `Class` gets a property shape on `P`
/// carrying `sh:class C` (or `sh:datatype C` when `C` is a concrete datatype — a literal is never
/// an instance of a class, so `sh:class` on a datatype would flag every node; or
/// `sh:nodeKind sh:BlankNodeOrIRI` when `C` is `owl:Thing` — a universal-top range says "any
/// individual", and spec-conformant `sh:class owl:Thing` would demand a never-materialized
/// `rdf:type owl:Thing` edge, so the faithful closed-world projection of the open range is a
/// node-kind constraint; or `sh:nodeKind sh:Literal` when `C` is `rdfs:Literal` — a
/// universal-literal-top range says "any literal", and `sh:datatype rdfs:Literal` never matches
/// a concrete literal, so the faithful projection is again a node-kind constraint). Unqualified
/// `owl:cardinality` / `owl:minCardinality` / `owl:maxCardinality` restrictions lower to
/// `sh:minCount`/`sh:maxCount` with [`ConstraintProvenance::OwlRestriction`]. Derived
/// constraints are grouped per class and sorted + deduped for determinism.
///
/// These are closed-world readings of open-world axioms, so their ledger polarity is
/// `logic:ValidationOnly` (an under-approximation) — never claimed as an entailment
/// (Principle 17). **Public** so the pipeline can run it over the merged authored ontology
/// (where the domain restrictions live), not just the logic: front-end source.
///
/// Hard-fails (returns `Err`) if a derived constraint is malformed (e.g. a cardinality
/// restriction with `minCardinality > maxCardinality`) rather than silently dropping it — a
/// required structural element that cannot be represented is a hard error, not a fallback.
/// The per-property / per-class **validation-reading opt-out** set (R3): a `logic:closureEntry`
/// whose `logic:closureValue` is `logic:OpenWorldClosure` and whose `logic:closureKey` names a
/// property or class IRI marks that axiom as *genuinely open-world only* — it takes no
/// closed-world validation reading, so no shape is derived for it. This reuses the existing
/// closure vocabulary verbatim (no new shape DSL); it is the single authored signal the issue
/// allows. The default is to derive a shape for every eligible axiom (MAXIMAL UTILITY), so an
/// absent annotation means "derive". Read directly off the merged authored store, consistent
/// with the dataset-derive architecture. Property-GLOBAL entries only: an entry carrying
/// `logic:onClass` is class-scoped and never suppresses corpus-wide (see
/// [`closure_keys_with_value`]).
pub(super) fn closure_validation_optouts(
    store: &RdfDataset,
) -> gmeow_errors::Result<std::collections::BTreeSet<String>> {
    closure_keys_with_value(store, "OpenWorldClosure")
}

/// The `logic:closureKey` set of every PROPERTY-GLOBAL `logic:closureEntry` whose
/// `logic:closureValue` is the closure-value individual named by `value_local`. An entry that
/// carries `logic:onClass` is CLASS-SCOPED — its (class, key) pair is read by
/// [`closure_validation_closed_requirements`] alone — and contributes NOTHING here: sweeping a
/// class-scoped key into a global set would promote a per-class closure into a corpus-wide law
/// the author never asserted. The predicate nodes are built once and reused across entries
/// (not re-minted per iteration).
pub(super) fn closure_keys_with_value(
    store: &RdfDataset,
    value_local: &str,
) -> gmeow_errors::Result<std::collections::BTreeSet<String>> {
    let target = Node::iri(logic_iri(value_local));
    let class_pred = nn(&logic_iri("onClass"));
    let mut set = std::collections::BTreeSet::new();
    for entry in subjects_with(store, &nn(&logic_iri("closureValue")), &target) {
        let key = closure_key(store, &entry, &subject_str(&entry)).map_err(|diagnostic| {
            Diag::of_kind(crate::error::Frontend::new(diagnostic.message))
                .with_focus(subject_str(&entry))
        })?;
        if value(store, &entry, &class_pred).is_some() {
            continue;
        }
        set.insert(key);
    }
    Ok(set)
}

/// The per-property **closed-world-reading opt-IN** set (R3, inverse polarity of
/// [`closure_validation_optouts`]): a `logic:closureEntry` whose `logic:closureValue` is
/// `logic:ClosedWorldClosure` and whose `logic:closureKey` names a property marks that property's
/// open-world `rdfs:domain`/`rdfs:range` axioms as ones that SHOULD take the closed-world
/// validation reading. Domain/range are inference axioms and open-world by default (a closed
/// reading over-claims on any graph relying on the entailment), so this is the single authored
/// signal that closes one — the exact peer of the opt-out, reusing the existing closure vocabulary
/// verbatim (no new shape DSL). An absent annotation means "open-world / derive no domain-range
/// shape". Read directly off the merged authored store, consistent with the dataset-derive
/// architecture. Property-GLOBAL entries only: an entry carrying `logic:onClass` is class-scoped
/// — it feeds [`closure_validation_closed_requirements`], never this global set, so it derives
/// no corpus-wide `sh:targetSubjectsOf`/`sh:targetObjectsOf` domain/range shape (see
/// [`closure_keys_with_value`]).
pub(super) fn closure_validation_closed_optins(
    store: &RdfDataset,
) -> gmeow_errors::Result<std::collections::BTreeSet<String>> {
    closure_keys_with_value(store, "ClosedWorldClosure")
}

/// Class/property pairs whose closure entry explicitly turns an `owl:allValuesFrom` restriction
/// into a required closed-world path. The pair is class-scoped: closing a predicate globally does
/// not imply that every class must carry it. This avoids an OWL existential in the reasoned core
/// while giving the validation projection an explicit, canonical `sh:minCount 1` authority.
pub(super) fn closure_validation_closed_requirements(
    store: &RdfDataset,
) -> gmeow_errors::Result<std::collections::BTreeSet<(String, String)>> {
    let closed = Node::iri(logic_iri("ClosedWorldClosure"));
    let class_pred = nn(&logic_iri("onClass"));
    let mut set = std::collections::BTreeSet::new();
    for entry in subjects_with(store, &nn(&logic_iri("closureValue")), &closed) {
        let key = closure_key(store, &entry, &subject_str(&entry)).map_err(|diagnostic| {
            Diag::of_kind(crate::error::Frontend::new(diagnostic.message))
                .with_focus(subject_str(&entry))
        })?;
        if let Some(Node::Iri(class)) = value(store, &entry, &class_pred) {
            set.insert((class, key));
        }
    }
    Ok(set)
}

/// Walk an `rdf:first`/`rdf:rest`/`rdf:nil` list from `head`, collecting its IRI members in
/// order. Blank / literal members are skipped (a non-resource list element has no IRI form),
/// and the walk terminates on `rdf:nil`, a cell missing `rdf:rest`, a non-resource cell, or a
/// cycle — a malformed list contributes only the members read so far rather than looping.
pub(super) fn read_iri_list(store: &RdfDataset, head: &Node) -> Vec<String> {
    const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    let first = nn(RDF_FIRST);
    let rest = nn(RDF_REST);
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut cursor = head.clone();
    while let Some(node) = term_as_subject(&cursor) {
        // Compute the node's subject string once and reuse for the nil-check and cycle-guard.
        let node_str = subject_str(&node);
        if node_str == RDF_NIL {
            break;
        }
        if !seen.insert(node_str) {
            break;
        }
        if let Some(Node::Iri(iri)) = value(store, &node, &first) {
            out.push(iri);
        }
        match value(store, &node, &rest) {
            Some(next) => cursor = next,
            None => break,
        }
    }
    out
}

/// Walk an `rdf:first`/`rdf:rest`/`rdf:nil` list from `head`, collecting each member as a
/// [`Subject`] (blank OR IRI) in order — the peer of [`read_iri_list`] for lists whose members
/// are themselves nodes to be read (e.g. the blank facet nodes of an `owl:withRestrictions`
/// list). Terminates on `rdf:nil`, a missing `rdf:rest`, a non-resource cell, or a cycle.
pub(super) fn read_list_member_subjects(store: &RdfDataset, head: &Node) -> Vec<Subject> {
    const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    let first = nn(RDF_FIRST);
    let rest = nn(RDF_REST);
    let mut out: Vec<Subject> = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut cursor = head.clone();
    while let Some(node) = term_as_subject(&cursor) {
        let node_str = subject_str(&node);
        if node_str == RDF_NIL || !seen.insert(node_str) {
            break;
        }
        if let Some(member) = value(store, &node, &first)
            && let Some(member_subj) = term_as_subject(&member)
        {
            out.push(member_subj);
        }
        match value(store, &node, &rest) {
            Some(next) => cursor = next,
            None => break,
        }
    }
    out
}

/// Derive the closed-world OWL fragment of an ontology graph into [`ValidationShapeIr`]s: the
/// per-class restriction walk (`some`/`allValuesFrom`, `hasValue`, unqualified + qualified
/// cardinality), the class-level axioms (`disjointWith`/`complementOf`/`oneOf`/
/// `AllDisjointClasses`), and the property-level axioms (`rdfs:domain`/`rdfs:range` and
/// `owl:Functional`/`InverseFunctionalProperty`). Every family accumulates by SHAPE TARGET —
/// one shape per target — so a class carrying both a restriction and a disjointness, or a
/// property carrying both a domain and a functionality axiom, folds into a single shape rather
/// than colliding. Constraints are sorted + deduped at construction so supply order never
/// affects the emitted surface (content-addressed determinism).
///
/// These are closed-world READINGS of open-world axioms — `logic:ValidationOnly` polarity,
/// never claimed as entailments (Principle 17). Derivation is scoped to the GMEOW authoring
/// namespace (only our own classes / properties own a shape; the target of an `sh:class` may
/// live in any namespace). The per-property / per-class opt-out (R3, see
/// [`closure_validation_optouts`]) suppresses any axiom whose subject a closure entry marks
/// `OpenWorldClosure`. `rdfs:domain`/`rdfs:range` are the one exception to derive-all: they are
/// open-world INFERENCE axioms, so they are OPEN-WORLD BY DEFAULT and derive a shape only for a
/// property explicitly opted IN with a `ClosedWorldClosure` closure entry (see
/// [`closure_validation_closed_optins`]).
///
/// Hard-fails (`Err`) rather than silently dropping a malformed REQUIRED constraint: a
/// cardinality with `min > max` (via [`PropertyConstraintIr::new`]), a qualified cardinality
/// with no `owl:onClass`, or an `owl:hasValue` whose fixed value is an anonymous node.
/// Conjunctively merge the property shapes that share the same `(path, inverse)` on one shape
/// target. FAMILY 1 emits one [`PropertyConstraintIr`] per restriction axiom, so a class that
/// authors a cardinality restriction AND an `owl:allValuesFrom` class restriction on ONE property
/// yields two same-path property shapes. SHACL reads several property shapes on one path
/// CONJUNCTIVELY. The IR can retain all those conjuncts in one record; the SHACL projector
/// splits independently qualified counts into sibling property shapes because that surface
/// permits only one `sh:qualifiedValueShape` per property shape. The merged record otherwise
/// matches a legacy shape's single merged block. Emitting them unmerged
/// keys distinctly from that block ([`PropertyConstraintIr::enforcement_key`] is per-path), which
/// would defeat the equivalence-before-deletion oracle for a shape whose covered fragment is
/// genuinely equivalent. Merging is sound: the lower bound is the tightest present (max of mins),
/// the upper bound the tightest present (min of maxes), the components the de-duplicated union
/// with node-kind facets intersected by the IR constructor, and
/// the reifier obligation the strengthened one (present shape / required OR). A same-path but
/// opposite-`inverse` pair constrains different statements (forward vs. inverse) and is never
/// merged. Deterministic: groups keep first-seen path order, and [`PropertyConstraintIr::new`]
/// sorts the merged components into canonical order. Explicit severities must agree;
/// an unannotated member does not erase the explicit value. A merged `min > max` is a genuine
/// contradiction and hard-fails through the constructor, never a silent drop.
pub(super) fn merge_same_path_properties(
    props: Vec<PropertyConstraintIr>,
) -> gmeow_errors::Result<Vec<PropertyConstraintIr>> {
    let mut order: Vec<(String, bool)> = Vec::new();
    let mut groups: BTreeMap<(String, bool), Vec<PropertyConstraintIr>> = BTreeMap::new();
    for p in props {
        let key = (p.path.clone(), p.inverse);
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(p);
    }
    let mut out = Vec::with_capacity(order.len());
    for key in order {
        let mut group = groups
            .remove(&key)
            .expect("group was inserted for every ordered key");
        if group.len() == 1 {
            out.push(group.pop().expect("a one-element group has its element"));
            continue;
        }
        let (path, inverse) = key;
        let mut min_count: Option<u32> = None;
        let mut max_count: Option<u32> = None;
        let mut components: Vec<ConstraintComponent> = Vec::new();
        let mut reifier_shape: Option<String> = None;
        let mut reification_required = false;
        let mut messages = Vec::new();
        let mut severity: Option<ShaclSeverity> = None;
        for p in group {
            if let Some(lo) = p.min_count {
                min_count = Some(min_count.map_or(lo, |cur| cur.max(lo)));
            }
            if let Some(hi) = p.max_count {
                max_count = Some(max_count.map_or(hi, |cur| cur.min(hi)));
            }
            for c in p.components {
                if !components.contains(&c) {
                    components.push(c);
                }
            }
            if p.reifier_shape.is_some() {
                reifier_shape = p.reifier_shape;
            }
            reification_required |= p.reification_required;
            messages.extend(p.messages);
            if let Some(explicit) = p.severity {
                if let Some(previous) = severity
                    && previous != explicit
                {
                    return Err(Diag::of_kind(crate::error::Frontend::new(format!(
                        "conflicting explicit severities on property {path}: {} and {}",
                        previous.as_str(),
                        explicit.as_str(),
                    ))));
                }
                severity = Some(explicit);
            }
        }
        let provenance = (min_count.is_some() || max_count.is_some())
            .then_some(ConstraintProvenance::OwlRestriction);
        let mut pc = PropertyConstraintIr::new(path, min_count, max_count, provenance, components)?
            .with_messages(messages)?;
        if let Some(explicit) = severity {
            pc = pc.with_severity(explicit);
        }
        if inverse {
            pc = pc.inverted();
        }
        if reifier_shape.is_some() || reification_required {
            pc = pc.with_reifier(reifier_shape, reification_required)?;
        }
        out.push(pc);
    }
    Ok(out)
}

/// THE single authoring-namespace authority: the four namespaces GMEOW dogfoods —
/// the core `gmeow:` vocabulary plus the grounding slices `math:`, `lang:`, `logic:`.
/// This is the dogfooding boundary [`derive_validation_shapes`] uses to decide which
/// classes/properties own a *derived* validation shape, and it is also the eligibility
/// test the `shape-migrate` injector (`gmeow-dev-cli`) consumes to decide which
/// hand-authored legacy shapes can be retired in favor of the derived projection. Any
/// other namespace (imported ontologies such as gUFO/FOAF, or anything external) is
/// linked, not validated/injected, by our surface. There is exactly one copy of this
/// set in the codebase; do not redeclare it elsewhere.
pub const AUTHORING_NAMESPACES: [&str; 4] = [
    "https://blackcatinformatics.ca/gmeow/",
    "https://blackcatinformatics.ca/math/",
    "https://blackcatinformatics.ca/lang/",
    "https://blackcatinformatics.ca/logic/",
];

/// Whether `iri` falls in an authoring namespace — see [`AUTHORING_NAMESPACES`].
pub fn is_authoring_namespace(iri: &str) -> bool {
    AUTHORING_NAMESPACES.iter().any(|ns| iri.starts_with(ns))
}

/// Admit a supplied numeric range endpoint; only a missing predicate is unbounded.
/// Validate all supplied objects before retaining the established first-value selection.
///
/// # Errors
/// Refuses nonliteral, unsupported, malformed or nonfinite bounds with their source context.
pub(super) fn numeric_range_bound(
    store: &RdfDataset,
    owner: &Subject,
    predicate: &Iri,
) -> gmeow_errors::Result<Option<f64>> {
    let mut selected = None;
    for bound in objects(store, owner, predicate) {
        let error = |reason: &str| {
            Diag::of_kind(crate::error::Frontend::new(format!(
                "range bound on {} at <{}>: supplied {bound:?}; requires a finite RDF numeric \
                     literal supported by the numeric range projection: {reason}",
                subject_str(owner),
                predicate.as_str(),
            )))
        };
        let Node::Lit(literal) = &bound else {
            return Err(error("the supplied term is not a literal"));
        };
        crate::ir::FiniteNumericLiteral::new(literal.clone())
            .map_err(|detail| error(detail.message()))?;
        let number = literal
            .lexical_form
            .trim()
            .parse::<f64>()
            .map_err(|detail| error(&detail.to_string()))?;
        if !number.is_finite() {
            return Err(error("the numeric range representation is not finite"));
        }
        // Preserve the existing first-value selection, but admit every supplied value.
        if selected.is_none() {
            selected = Some(number);
        }
    }
    Ok(selected)
}

/// Derive and merge closed-world validation shapes from the selected authored source.
/// Closure declarations, restrictions and validation descriptors determine the projected paths.
///
/// # Errors
/// Propagates malformed closure or restriction metadata and unsupported numeric range bounds.
pub fn derive_validation_shapes(
    store: &RdfDataset,
) -> gmeow_errors::Result<Vec<ValidationShapeIr>> {
    // The per-property/per-class closed-world-reading opt-out (R3). Default is derive-all; a
    // property/class named by an `OpenWorldClosure` closure entry is suppressed below.
    let optouts = closure_validation_optouts(store)?;
    // The per-property closed-world-reading opt-IN (R3, inverse polarity): rdfs:domain/range are
    // open-world by default (they are inference axioms), so a domain/range shape is derived only
    // for a property a `ClosedWorldClosure` closure entry explicitly closes. See FAMILY 3.
    let closed_optins = closure_validation_closed_optins(store)?;
    let closed_requirements = closure_validation_closed_requirements(store)?;
    let owl = gmeow_ns::OWL_NS;
    let rdfs = "http://www.w3.org/2000/01/rdf-schema#";
    let xsd = "http://www.w3.org/2001/XMLSchema#";
    let owl_thing = format!("{owl}Thing");
    // The canonical `logic:Thing` is the same individual-domain top as its `owl:Thing` view, so the
    // universal-top guards below (open node-kind, unqualified-cardinality collapse) must fire for
    // EITHER spelling — a slice authors `logic:onClass logic:Thing` after the surface flip.
    let logic_thing = logic_iri("Thing");
    let is_top_thing = |iri: &str| iri == owl_thing || iri == logic_thing;
    let rdfs_datatype = Node::iri(format!("{rdfs}Datatype"));
    let rdfs_literal = format!("{rdfs}Literal");
    let rdfs_resource = format!("{rdfs}Resource");
    let p_on = nn(&format!("{owl}onProperty"));
    let p_some = nn(&format!("{owl}someValuesFrom"));
    let p_all = nn(&format!("{owl}allValuesFrom"));
    let p_hasvalue = nn(&format!("{owl}hasValue"));
    let p_onclass = nn(&format!("{owl}onClass"));
    let p_ondatarange = nn(&format!("{owl}onDataRange"));
    let p_mincard = nn(&format!("{owl}minCardinality"));
    let p_maxcard = nn(&format!("{owl}maxCardinality"));
    let p_card = nn(&format!("{owl}cardinality"));
    let p_qmincard = nn(&format!("{owl}minQualifiedCardinality"));
    let p_qmaxcard = nn(&format!("{owl}maxQualifiedCardinality"));
    let p_qcard = nn(&format!("{owl}qualifiedCardinality"));
    let p_subclass = nn(&format!("{rdfs}subClassOf"));
    // Canonical `logic:` restriction spelling.  Declarative shape derivation reads the
    // merged AUTHORED dataset (before the OWL projection is materialized), so a
    // `logic:subClassOf [ a logic:Restriction ; ... ]` must be read through the same
    // semantic slots as its legacy RDFS/OWL spelling.  Keep the two spellings in one
    // view here; the ValidationShapeIr construction below remains the single lowering.
    let p_logic_on = nn(&logic_iri("onProperty"));
    let p_logic_some = nn(&logic_iri("someValuesFrom"));
    let p_logic_all = nn(&logic_iri("allValuesFrom"));
    let p_logic_hasvalue = nn(&logic_iri("hasValue"));
    let p_logic_onclass = nn(&logic_iri("onClass"));
    let p_logic_ondatarange = nn(&logic_iri("onDataRange"));
    let p_logic_mincard = nn(&logic_iri("minCardinality"));
    let p_logic_maxcard = nn(&logic_iri("maxCardinality"));
    let p_logic_card = nn(&logic_iri("cardinality"));
    let p_logic_qmincard = nn(&logic_iri("minQualifiedCardinality"));
    let p_logic_qmaxcard = nn(&logic_iri("maxQualifiedCardinality"));
    let p_logic_qcard = nn(&logic_iri("qualifiedCardinality"));
    let p_logic_subclass = nn(&logic_iri("subClassOf"));
    let p_disjoint = nn(&format!("{owl}disjointWith"));
    let p_complement = nn(&format!("{owl}complementOf"));
    let p_oneof = nn(&format!("{owl}oneOf"));
    let p_members = nn(&format!("{owl}members"));
    let p_domain = nn(&format!("{rdfs}domain"));
    let p_range = nn(&format!("{rdfs}range"));
    let owl_alldisjoint = Node::iri(format!("{owl}AllDisjointClasses"));
    // Canonical `logic:` twins of the class-/property-axiom predicates. Each is read alongside its
    // legacy `owl:`/`rdfs:` spelling (via `restriction_value`/`restriction_objects`) so an
    // `owl:`-authored and a `logic:`-authored corpus derive identical shapes across the surface
    // flip — the same dual-read the restriction-body slots above already use.
    // (`p_logic_oneof` / `p_logic_complement` are already bound below for the filler-classifier and
    // are reused by the class-axiom walk; only the remaining twins are introduced here.)
    let p_logic_disjoint = nn(&logic_iri("disjointWith"));
    let p_logic_members = nn(&logic_iri("members"));
    let p_logic_domain = nn(&logic_iri("domain"));
    let p_logic_range = nn(&logic_iri("range"));
    let logic_alldisjoint = Node::iri(logic_iri("AllDisjointClasses"));

    let restriction_value = |subject: &Subject, owl_predicate: &Iri, logic_predicate: &Iri| {
        value(store, subject, owl_predicate).or_else(|| value(store, subject, logic_predicate))
    };
    let restriction_objects = |subject: &Subject, owl_predicate: &Iri, logic_predicate: &Iri| {
        let mut out = objects(store, subject, owl_predicate);
        out.extend(objects(store, subject, logic_predicate));
        out
    };

    // GMEOW's authoring ground: derive validation shapes for our own domain classes /
    // properties across every dogfooded namespace (Principle 4 / maximal dogfooding) — the
    // core `gmeow:` vocabulary plus the grounding slices (`math:`, `lang:`, `logic:`), whose
    // hand-authored shapes migrate to these derived projections (declarative-migration wave 1).
    // Imported
    // ontologies (gUFO, FOAF, …) are linked, not validated by our surface — and their
    // namespaces are not registered in the downstream JSON-Schema discriminator. The TARGET of
    // an `sh:class` may live in any namespace; only the SHAPE-owning class / property must be
    // authoring-NS. `is_authoring_ns` here is a thin local alias of the module-level
    // authority ([`is_authoring_namespace`]) kept for call-site brevity in this function.
    let is_authoring_ns = is_authoring_namespace;

    // RDF language literals are datatypes too; they are not classes whose
    // instances happen to carry text. Read canonical datatype declarations
    // through the same structural typing view as other compiler declarations.
    let is_datatype = |iri: &str| -> bool {
        iri.starts_with(xsd)
            || iri == rdfs_literal
            || matches!(
                iri,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
                    | "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString"
            )
            || {
                let types = structural_classes(store, &Subject::Iri(iri.to_owned()));
                types.contains(&rdfs_datatype) || types.contains(&Node::iri(logic_iri("Datarange")))
            }
    };

    // The single classification component for an IRI target: `None` for `rdfs:Resource`, the
    // UNIVERSAL TOP (the class of everything, literals included) — a range/domain of
    // rdfs:Resource is VACUOUS, and `sh:class rdfs:Resource` would demand a never-materialized
    // `rdf:type rdfs:Resource` edge on every object, false-positiving universally; a vacuous top
    // must emit NO node/class constraint at all. The two BOUNDED universal-tops instead project to
    // an open node-kind constraint (`owl:Thing` → sh:BlankNodeOrIRI; `rdfs:Literal` →
    // sh:Literal) — the `owl:Thing` guard sits AHEAD of `is_datatype` so a quirk axiom
    // `owl:Thing a rdfs:Datatype` can never fall through to a vacuous `sh:datatype owl:Thing`.
    // A concrete datatype → sh:datatype; anything else → sh:class.
    let classify = |iri: &str| -> Option<ConstraintComponent> {
        if iri == rdfs_resource {
            None
        } else if is_top_thing(iri) {
            Some(ConstraintComponent::NodeKindShacl(
                ShaclNodeKind::BlankNodeOrIri,
            ))
        } else if iri == rdfs_literal {
            Some(ConstraintComponent::NodeKindShacl(ShaclNodeKind::Literal))
        } else if is_datatype(iri) {
            Some(ConstraintComponent::Datatype(iri.to_owned()))
        } else {
            Some(ConstraintComponent::Class(iri.to_owned()))
        }
    };

    // Whether `p` is a DECLARED datatype property (and not also an object property — a
    // corpus that declares both is contradictory OWL and gets the conservative object reading,
    // never a silently narrowed one). The declaration is read under EITHER spelling: the
    // canonical `logic:DatatypeProperty`/`logic:ObjectProperty` (Principle 17) or its legacy
    // `owl:` view, so the object/data divide is classified identically across the surface flip.
    let datatype_property_markers: Vec<Node> =
        crate::typing_vocab::both_spellings("DatatypeProperty")
            .into_iter()
            .map(Node::iri)
            .collect();
    let object_property_markers: Vec<Node> = crate::typing_vocab::both_spellings("ObjectProperty")
        .into_iter()
        .map(Node::iri)
        .collect();
    let is_datatype_property = |p: &str| -> bool {
        let types = structural_classes(store, &Subject::Iri(p.to_owned()));
        datatype_property_markers.iter().any(|m| types.contains(m))
            && !object_property_markers.iter().any(|m| types.contains(m))
    };

    // `classify`, resolved against the property the filler is a filler FOR.
    //
    // The two bounded universal tops are NOT interchangeable across the object/data divide:
    // `owl:Thing` is the top of the INDIVIDUAL domain, `rdfs:Literal` the top of the DATA
    // domain. A declared `owl:DatatypeProperty` takes literal values only, so an authored
    // `owl:someValuesFrom owl:Thing` / `owl:allValuesFrom owl:Thing` / `rdfs:range owl:Thing`
    // on such a property means "any value" in the only domain that property has — the data
    // domain — and its faithful projection is `sh:nodeKind sh:Literal`. Projecting the
    // individual-domain reading (`sh:nodeKind sh:BlankNodeOrIRI`) there is not a lossy
    // approximation but an INVERTED constraint: it rejects every correct literal value the
    // property is declared to carry (the `gmeow:spanStart` / `gmeow:spanEnd` integer offsets on
    // `gmeow:Chunk` are the corpus witness). The redirect is keyed on the property's own
    // declaration, so it can never narrow an object-valued or undeclared path.
    let classify_on = |on: &str, iri: &str| -> Option<ConstraintComponent> {
        if is_top_thing(iri) && is_datatype_property(on) {
            classify(&rdfs_literal)
        } else {
            classify(iri)
        }
    };

    // The closed-world reading of a faceted datatype filler
    // (`[ a rdfs:Datatype ; owl:onDatatype xsd:string ; owl:withRestrictions ( [ xsd:pattern "…" ]
    // [ xsd:minLength "…" ] … ) ]`): the base datatype plus each XSD length / pattern facet, as
    // SHACL components (`sh:datatype` + `sh:pattern` / `sh:minLength` / `sh:maxLength`). Returns an
    // empty vector for a blank node that carries no `owl:withRestrictions` (an anonymous class
    // expression, not a datatype facet) — the caller then skips it, unchanged.
    let p_ondatatype = nn(&format!("{owl}onDatatype"));
    let p_withrestrictions = nn(&format!("{owl}withRestrictions"));
    let p_logic_ondatatype = nn(&logic_iri("onDatatype"));
    let p_logic_withrestrictions = nn(&logic_iri("withRestrictions"));
    let xsd_pattern = nn(&format!("{xsd}pattern"));
    let xsd_minlength = nn(&format!("{xsd}minLength"));
    let xsd_maxlength = nn(&format!("{xsd}maxLength"));
    let xsd_mininclusive = nn(&format!("{xsd}minInclusive"));
    let xsd_maxinclusive = nn(&format!("{xsd}maxInclusive"));
    let xsd_minexclusive = nn(&format!("{xsd}minExclusive"));
    let xsd_maxexclusive = nn(&format!("{xsd}maxExclusive"));
    let datatype_facets = |filler: &Subject| -> gmeow_errors::Result<Vec<ConstraintComponent>> {
        let Some(list_head) =
            restriction_value(filler, &p_withrestrictions, &p_logic_withrestrictions)
        else {
            return Ok(Vec::new());
        };
        let mut comps = Vec::new();
        if let Some(Node::Iri(dt)) = restriction_value(filler, &p_ondatatype, &p_logic_ondatatype) {
            comps.push(ConstraintComponent::Datatype(dt));
        }
        // Numeric-bound facets accumulate into a SINGLE `NumericRange` (a min/max pair with per-
        // endpoint inclusivity) so an interval authored as two facet nodes projects to one
        // `sh:minInclusive`/`sh:maxInclusive` (or the exclusive peers) component, matching the
        // direct-emit oracle. An inclusive bound wins over an exclusive one on the same endpoint
        // under the declared endpoint-selection contract.
        let (mut lo, mut hi): (Option<f64>, Option<f64>) = (None, None);
        let (mut lo_incl, mut hi_incl) = (true, true);
        for facet in read_list_member_subjects(store, &list_head) {
            let min_inclusive = numeric_range_bound(store, &facet, &xsd_mininclusive)?;
            let min_exclusive = numeric_range_bound(store, &facet, &xsd_minexclusive)?;
            let max_inclusive = numeric_range_bound(store, &facet, &xsd_maxinclusive)?;
            let max_exclusive = numeric_range_bound(store, &facet, &xsd_maxexclusive)?;
            if let Some(Node::Lit(purrdf::RdfLiteral {
                lexical_form: regex,
                ..
            })) = value(store, &facet, &xsd_pattern)
            {
                comps.push(ConstraintComponent::Pattern { regex, flags: None });
            }
            if let Some(Node::Lit(purrdf::RdfLiteral {
                lexical_form: n, ..
            })) = value(store, &facet, &xsd_minlength)
                && let Ok(n) = n.trim().parse::<u32>()
            {
                comps.push(ConstraintComponent::MinLength(n));
            }
            if let Some(Node::Lit(purrdf::RdfLiteral {
                lexical_form: n, ..
            })) = value(store, &facet, &xsd_maxlength)
                && let Ok(n) = n.trim().parse::<u32>()
            {
                comps.push(ConstraintComponent::MaxLength(n));
            }
            if let Some(n) = min_inclusive {
                lo = Some(n);
                lo_incl = true;
            }
            if let Some(n) = min_exclusive
                && lo.is_none()
            {
                lo = Some(n);
                lo_incl = false;
            }
            if let Some(n) = max_inclusive {
                hi = Some(n);
                hi_incl = true;
            }
            if let Some(n) = max_exclusive
                && hi.is_none()
            {
                hi = Some(n);
                hi_incl = false;
            }
        }
        if lo.is_some() || hi.is_some() {
            comps.push(ConstraintComponent::NumericRange {
                min: lo,
                max: hi,
                min_inclusive: lo_incl,
                max_inclusive: hi_incl,
            });
        }
        Ok(comps)
    };

    // The closed-world reading of a blank ANONYMOUS-CLASS-EXPRESSION filler (an
    // `owl:someValuesFrom` / `owl:allValuesFrom` value whose filler is not a named class): an
    // `owl:unionOf ( C1 C2 … )` → `sh:or ( [ sh:class C1 ] … )`, an `owl:disjointUnionOf ( … )` →
    // `sh:xone ( … )`, an `owl:complementOf C` → `sh:not [ sh:class C ]`, and the nested value
    // negation `owl:complementOf [ owl:hasValue v ]` → `sh:not [ sh:hasValue v ]`. Returns `None`
    // for a filler that carries no such expression (the caller then leaves it in the canon).
    let p_unionof = nn(&format!("{owl}unionOf"));
    let p_disjointunion = nn(&format!("{owl}disjointUnionOf"));
    let p_logic_unionof = nn(&logic_iri("unionOf"));
    let p_logic_disjointunion = nn(&logic_iri("disjointUnionOf"));
    let p_logic_oneof = nn(&logic_iri("oneOf"));
    let p_logic_complement = nn(&logic_iri("complementOf"));
    let classify_filler = |fs: &Subject| -> Option<ConstraintComponent> {
        if let Some(head) = restriction_value(fs, &p_unionof, &p_logic_unionof) {
            let branches: Vec<ConstraintComponent> = read_iri_list(store, &head)
                .into_iter()
                .filter_map(|c| classify(&c))
                .collect();
            if !branches.is_empty() {
                return Some(ConstraintComponent::Or(branches));
            }
        }
        if let Some(head) = restriction_value(fs, &p_disjointunion, &p_logic_disjointunion) {
            let branches: Vec<ConstraintComponent> = read_iri_list(store, &head)
                .into_iter()
                .filter_map(|c| classify(&c))
                .collect();
            if !branches.is_empty() {
                return Some(ConstraintComponent::Xone(branches));
            }
        }
        // An enumerated filler (`owl:oneOf ( a b … )` on an anonymous class) → `sh:in ( a b … )`:
        // every value of the path must be one of the enumerated individuals. IRI members only —
        // a literal member would make the expression a data range, which the facet arm owns.
        if let Some(head) = restriction_value(fs, &p_oneof, &p_logic_oneof) {
            let members = read_iri_list(store, &head);
            if !members.is_empty() {
                return Some(ConstraintComponent::In(
                    members.into_iter().map(ShapeValue::Iri).collect(),
                ));
            }
        }
        match restriction_value(fs, &p_complement, &p_logic_complement) {
            Some(Node::Iri(d)) => classify(&d).map(|cc| ConstraintComponent::Not(Box::new(cc))),
            Some(inner @ Node::Blank { .. }) => {
                let bs = term_as_subject(&inner)?;
                let sv = match restriction_value(&bs, &p_hasvalue, &p_logic_hasvalue)? {
                    Node::Iri(i) => ShapeValue::Iri(i),
                    Node::Lit(literal) => ShapeValue::Literal(literal),
                    _ => return None,
                };
                Some(ConstraintComponent::Not(Box::new(
                    ConstraintComponent::HasValue(sv),
                )))
            }
            _ => None,
        }
    };

    // A non-negative-integer cardinality literal off a restriction blank node; `None` (never a
    // hard error here) for an absent or non-integer object — a broken count contributes nothing.
    let card_of = |restr: &Subject, p: &Iri| -> Option<u32> {
        match value(store, restr, p) {
            Some(Node::Lit(purrdf::RdfLiteral {
                lexical_form: lex, ..
            })) => lex.trim().parse::<u32>().ok(),
            _ => None,
        }
    };
    let restriction_card_of =
        |restr: &Subject, owl_predicate: &Iri, logic_predicate: &Iri| -> Option<u32> {
            match restriction_value(restr, owl_predicate, logic_predicate) {
                Some(Node::Lit(purrdf::RdfLiteral {
                    lexical_form: lex, ..
                })) => lex.trim().parse::<u32>().ok(),
                _ => None,
            }
        };

    // Accumulate by SHAPE TARGET: shape IRI → (target, node_components, properties). One shape
    // per target, so every family merges into a single shape rather than colliding.
    type Acc = BTreeMap<
        String,
        (
            ShapeTarget,
            Vec<ConstraintComponent>,
            Vec<PropertyConstraintIr>,
        ),
    >;
    let mut acc: Acc = BTreeMap::new();

    // The stable per-target shape IRI (distinct per target family so domain / range / class
    // shapes never collide).
    fn entry_for(
        acc: &mut Acc,
        target: ShapeTarget,
    ) -> &mut (
        ShapeTarget,
        Vec<ConstraintComponent>,
        Vec<PropertyConstraintIr>,
    ) {
        let iri = match &target {
            ShapeTarget::Class(c) => format!("{c}-shape"),
            ShapeTarget::SubjectsOf(p) => format!("{p}-domain-shape"),
            ShapeTarget::ObjectsOf(p) => format!("{p}-range-shape"),
            ShapeTarget::ValueKeyed { predicate, value } => {
                format!("{predicate}-{value}-value-shape")
            }
            // The declarative deriver never mints a direct-instance / raw-sparql target (those are
            // procedural-constraint selectors only), but the match stays exhaustive.
            ShapeTarget::DirectClass(c) => format!("{c}-direct-shape"),
            ShapeTarget::Sparql(_) => "sparql-target-shape".to_owned(),
        };
        acc.entry(iri)
            .or_insert_with(|| (target, Vec::new(), Vec::new()))
    }

    // ── Unique-language sugar (logic:UniqueLangConstraint) → declarative sh:uniqueLang ────────
    // A per-property unique-language record grounds a `sh:uniqueLang true` facet DECLARATIVELY on
    // its `logic:onClass` node shape (the localizable-prose convention): the value path localizes
    // at most once per language tag. Unlike the procedural sugars, uniqueLang is a faithful SHACL
    // Core facet, so it rides the class node shape as a covered property component.
    let unique_lang_ty = Node::iri(logic_iri("UniqueLangConstraint"));
    for rec in subjects_of_structural_class(store, &unique_lang_ty) {
        let (Some(Node::Iri(class_iri)), Some(Node::Iri(path))) = (
            value(store, &rec, &nn(&logic_iri("onClass"))),
            value(store, &rec, &nn(&logic_iri("valuePath"))),
        ) else {
            continue;
        };
        if !is_authoring_ns(&class_iri) || optouts.contains(&class_iri) || optouts.contains(&path) {
            continue;
        }
        let pc = PropertyConstraintIr::new(
            &path,
            None,
            None,
            None,
            vec![ConstraintComponent::UniqueLang],
        )?;
        entry_for(&mut acc, ShapeTarget::Class(class_iri))
            .2
            .push(pc);
    }

    // ── Node-kind sugar (logic:PathNodeKindConstraint) → declarative sh:nodeKind ──────────────
    // A CLASS-SCOPED node-kind record (one carrying `logic:onClass`) grounds a `sh:nodeKind`
    // facet DECLARATIVELY on that class's node shape, exactly as the unique-language sugar above
    // grounds `sh:uniqueLang`: `sh:nodeKind` is a faithful SHACL-Core facet, so it belongs on the
    // class node shape the declarative readers (JSON-Schema / Pydantic / ShEx) consume, not only
    // in the record's procedural SPARQL twin. It is the ONLY authored form for an IRI-vs-blank
    // obligation: `sh:class C` admits a blank node typed `C`, and OWL's universal fillers invert
    // to at most `sh:BlankNodeOrIRI` (`owl:Thing`) — neither can say "IRI, not blank".
    // A record with NO `logic:onClass` is PATH-scoped (its guard is the value path itself, and
    // its target is `sh:targetSubjectsOf`, not a class), so it has no class node shape to ride
    // and keeps its procedural projection alone.
    let node_kind_ty = Node::iri(logic_iri("PathNodeKindConstraint"));
    for rec in subjects_of_structural_class(store, &node_kind_ty) {
        let (Some(Node::Iri(class_iri)), Some(Node::Iri(path)), Some(kind)) = (
            value(store, &rec, &nn(&logic_iri("onClass"))),
            value(store, &rec, &nn(&logic_iri("valuePath"))),
            value(store, &rec, &nn(&logic_iri("nodeKind")))
                .and_then(|t| shacl_node_kind_of(&term_str(&t))),
        ) else {
            continue;
        };
        if !is_authoring_ns(&class_iri) || optouts.contains(&class_iri) || optouts.contains(&path) {
            continue;
        }
        let pc = PropertyConstraintIr::new(
            &path,
            None,
            None,
            None,
            vec![ConstraintComponent::NodeKindShacl(kind)],
        )?;
        entry_for(&mut acc, ShapeTarget::Class(class_iri))
            .2
            .push(pc);
    }

    // ── String-pattern sugar (logic:StringPatternConstraint) → declarative sh:pattern ─────────
    // A CLASS-SCOPED regex record (one carrying `logic:onClass`) grounds a `sh:pattern` facet
    // DECLARATIVELY on that class's node shape, on exactly the reasoning of the node-kind sugar
    // above: `sh:pattern` is a faithful SHACL-Core facet, so it belongs on the class node shape
    // the declarative readers consume, not only in the record's procedural SPARQL twin.
    //
    // This is the ONLY route by which a lexical-form obligation reaches a derived property
    // shape. The alternative — an `owl:withRestrictions` datarange carrying `xsd:pattern` — is
    // the only OTHER thing `datatype_facets` reads, and authoring one is not free: a LIVE
    // pattern facet is outside the fragment the native reasoner decides (XSD's regular-
    // expression dialect is not the host engine's, so `refute::datatype` withholds rather than
    // guess), and one undecided facet family makes `reason-verify` refuse a consistency verdict
    // for the WHOLE ontology. Deriving the component from the `logic:` constraint keeps the
    // obligation, keeps a single authored source, and leaves the reasoned closure in-fragment.
    //
    // Only `regexRequired` lowers. A `regexForbidden` record is a NEGATED pattern, which SHACL
    // Core cannot state as a property component (`sh:not` of a pattern needs a nested shape), so
    // it keeps its procedural projection alone rather than lowering to a wrong positive facet.
    let string_pattern_ty = Node::iri(logic_iri("StringPatternConstraint"));
    for rec in subjects_of_structural_class(store, &string_pattern_ty) {
        let (Some(Node::Iri(class_iri)), Some(Node::Iri(path)), Some(regex)) = (
            value(store, &rec, &nn(&logic_iri("onClass"))),
            value(store, &rec, &nn(&logic_iri("valuePath"))),
            match value(store, &rec, &nn(&logic_iri("stringPattern"))) {
                Some(Node::Lit(purrdf::RdfLiteral {
                    lexical_form: lexical,
                    ..
                })) => Some(lexical),
                _ => None,
            },
        ) else {
            continue;
        };
        let op = value(store, &rec, &nn(&logic_iri("stringOp"))).map(|t| term_str(&t));
        if op.as_deref() != Some("regexRequired") {
            continue;
        }
        if !is_authoring_ns(&class_iri) || optouts.contains(&class_iri) || optouts.contains(&path) {
            continue;
        }
        let pc = PropertyConstraintIr::new(
            &path,
            None,
            None,
            None,
            vec![ConstraintComponent::Pattern { regex, flags: None }],
        )?;
        entry_for(&mut acc, ShapeTarget::Class(class_iri))
            .2
            .push(pc);
    }

    // Restriction annotations belong to the property condition they describe.
    // Absence stays implicit; an explicit severity is preserved through same-path merging.
    let with_restriction_metadata = |property: PropertyConstraintIr,
                                     restriction: &Subject|
     -> gmeow_errors::Result<PropertyConstraintIr> {
        let mut property = property.with_messages(read_messages(store, restriction)?)?;
        if value(store, restriction, &nn(&logic_iri("severity"))).is_some() {
            property = property.with_severity(sugar_severity(store, restriction)?);
        }
        Ok(property)
    };

    // ── FAMILY 1 — per-class restriction walk (Class(C) target) ───────────────────────────
    // A shape-target class is found by its typing marker under EITHER spelling: the canonical
    // `logic:Class` (Principle 17) or its legacy `owl:Class` view. Reading both keeps the derived
    // shapes byte-identical across the authoring-surface flip (the restriction/enumeration slots
    // below are already dual-read the same way).
    let classes: Vec<Subject> = {
        let mut seen: HashSet<String> = HashSet::new();
        let mut out: Vec<Subject> = Vec::new();
        for marker in crate::typing_vocab::both_spellings("Class") {
            for s in subjects_of_structural_class(store, &Node::iri(marker)) {
                if seen.insert(subject_str(&s)) {
                    out.push(s);
                }
            }
        }
        out
    };
    for class in &classes {
        // An anonymous class expression (blank node) is not a shape target — skip it.
        if subject_is_blank(class) {
            continue;
        }
        let class_iri = subject_str(class);
        if !is_authoring_ns(&class_iri) || optouts.contains(&class_iri) {
            continue;
        }
        for restr in restriction_objects(class, &p_subclass, &p_logic_subclass) {
            let Some(restr_subj) = term_as_subject(&restr) else {
                continue;
            };
            // A blank superclass carrying `owl:unionOf ( [ owl:onProperty P1 ; owl:someValuesFrom
            // owl:Thing ] … )` — an either-of-these-properties existence obligation — reads
            // closed-world as the node-level `sh:or` over required property paths
            // ([`ConstraintComponent::OrProperties`]). Only the exact all-branches-are-bare-
            // existential form is read; any other union member (a named class, a qualified
            // filler) leaves the whole expression in the canon, never a partial disjunction.
            if restriction_value(&restr_subj, &p_on, &p_logic_on).is_none()
                && let Some(head) = restriction_value(&restr_subj, &p_unionof, &p_logic_unionof)
            {
                let members = read_list_member_subjects(store, &head);
                let mut paths: Vec<String> = Vec::with_capacity(members.len());
                let mut all_bare_existentials = !members.is_empty();
                for m in &members {
                    let on_p = restriction_value(m, &p_on, &p_logic_on);
                    let filler = restriction_value(m, &p_some, &p_logic_some);
                    match (on_p, filler) {
                        (Some(Node::Iri(p)), Some(Node::Iri(f)))
                            if is_top_thing(&f) && !optouts.contains(&p) =>
                        {
                            paths.push(p);
                        }
                        _ => {
                            all_bare_existentials = false;
                            break;
                        }
                    }
                }
                if all_bare_existentials && paths.len() >= 2 {
                    entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                        .1
                        .push(ConstraintComponent::OrProperties(paths));
                }
                continue;
            }
            // A restriction constrains exactly one property; skip a malformed one with no
            // IRI-valued `owl:onProperty`.
            let Some(Node::Iri(on)) = restriction_value(&restr_subj, &p_on, &p_logic_on) else {
                continue;
            };
            // Per-property validation-reading opt-out (R3).
            if optouts.contains(&on) {
                continue;
            }

            // owl:someValuesFrom is an OPEN-WORLD existential and therefore never becomes a
            // closed-world minimum by itself. Its validation projection is the conservative
            // value-typing under-approximation. Required paths are authored separately as a
            // class-scoped `ClosedWorldClosure` entry paired with `owl:allValuesFrom`; this keeps
            // the SHACL minimum explicit without causing the native reasoner to mint existential
            // witnesses into the shipped closure.
            match restriction_value(&restr_subj, &p_some, &p_logic_some) {
                Some(Node::Iri(cv)) => {
                    if let Some(cc) = classify_on(&on, &cv) {
                        let pc = PropertyConstraintIr::new(&on, None, None, None, vec![cc])?;
                        entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                            .2
                            .push(pc);
                    }
                }
                // A blank filler that is a faceted datatype (`owl:onDatatype` + `owl:withRestrictions`)
                // reads as the length/pattern facets its values must satisfy — the same closed-world
                // condition as `allValuesFrom` over that datatype. A blank filler with no facet is an
                // anonymous class expression, carried in the canon, never a bare blank shape → skip.
                Some(filler @ Node::Blank { .. }) => {
                    if let Some(fs) = term_as_subject(&filler) {
                        let facets = datatype_facets(&fs)?;
                        if !facets.is_empty() {
                            let pc = PropertyConstraintIr::new(&on, None, None, None, facets)?;
                            entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                                .2
                                .push(pc);
                        } else if let Some(cc) = classify_filler(&fs) {
                            let pc = PropertyConstraintIr::new(&on, None, None, None, vec![cc])?;
                            entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                                .2
                                .push(pc);
                        }
                    }
                }
                _ => {}
            }

            // owl:allValuesFrom is UNIVERSAL: every value satisfies the inner shape → a bare
            // `sh:class` / `sh:datatype` / `sh:nodeKind` on the path, or the length/pattern facets
            // of a faceted-datatype filler. A non-faceted blank filler → skip.
            match restriction_value(&restr_subj, &p_all, &p_logic_all) {
                Some(Node::Iri(cv)) => {
                    if let Some(cc) = classify_on(&on, &cv) {
                        let min = closed_requirements
                            .contains(&(class_iri.clone(), on.clone()))
                            .then_some(1);
                        let pc = PropertyConstraintIr::new(
                            &on,
                            min,
                            None,
                            min.map(|_| ConstraintProvenance::OwlRestriction),
                            vec![cc],
                        )?;
                        entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                            .2
                            .push(with_restriction_metadata(pc, &restr_subj)?);
                    }
                }
                Some(filler @ Node::Blank { .. }) => {
                    if let Some(fs) = term_as_subject(&filler) {
                        let facets = datatype_facets(&fs)?;
                        if !facets.is_empty() {
                            let min = closed_requirements
                                .contains(&(class_iri.clone(), on.clone()))
                                .then_some(1);
                            let pc = PropertyConstraintIr::new(
                                &on,
                                min,
                                None,
                                min.map(|_| ConstraintProvenance::OwlRestriction),
                                facets,
                            )?;
                            entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                                .2
                                .push(with_restriction_metadata(pc, &restr_subj)?);
                        } else if let Some(cc) = classify_filler(&fs) {
                            let min = closed_requirements
                                .contains(&(class_iri.clone(), on.clone()))
                                .then_some(1);
                            let pc = PropertyConstraintIr::new(
                                &on,
                                min,
                                None,
                                min.map(|_| ConstraintProvenance::OwlRestriction),
                                vec![cc],
                            )?;
                            entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                                .2
                                .push(with_restriction_metadata(pc, &restr_subj)?);
                        }
                    }
                }
                _ => {}
            }

            // owl:hasValue → `sh:hasValue` (a fixed required value). A blank / quoted-triple
            // fixed value is impossible (a fixed value cannot be an anonymous node) → hard-fail.
            match restriction_value(&restr_subj, &p_hasvalue, &p_logic_hasvalue) {
                None => {}
                Some(Node::Iri(v)) => {
                    let pc = PropertyConstraintIr::new(
                        &on,
                        None,
                        None,
                        None,
                        vec![ConstraintComponent::HasValue(ShapeValue::Iri(v))],
                    )?;
                    entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                        .2
                        .push(pc);
                }
                Some(Node::Lit(literal)) => {
                    // Preserve the fixed value's datatype / language tag: a typed
                    // `owl:hasValue "1"^^xsd:integer` derives a TYPED `sh:hasValue`, never a
                    // bare untyped `"1"` that would match the wrong literal.
                    let pc = PropertyConstraintIr::new(
                        &on,
                        None,
                        None,
                        None,
                        vec![ConstraintComponent::HasValue(ShapeValue::Literal(literal))],
                    )?;
                    entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                        .2
                        .push(pc);
                }
                Some(Node::Blank { .. }) | Some(Node::Triple(_)) => {
                    return Err(Diag::of_kind(crate::error::Frontend::new(format!(
                        "owl:hasValue on {on} is an anonymous node; a fixed required value \
                         cannot be a blank node or quoted triple"
                    ))));
                }
            }

            // Cardinality. Qualified (`owl:onClass` + a `qualified*Cardinality`) is distinct from
            // unqualified: it counts only the values satisfying the inner shape.
            let has_qcard = restriction_value(&restr_subj, &p_qcard, &p_logic_qcard).is_some()
                || restriction_value(&restr_subj, &p_qmincard, &p_logic_qmincard).is_some()
                || restriction_value(&restr_subj, &p_qmaxcard, &p_logic_qmaxcard).is_some();
            if has_qcard {
                let q_exact = restriction_card_of(&restr_subj, &p_qcard, &p_logic_qcard);
                let (mut qlo, mut qhi) = (
                    restriction_card_of(&restr_subj, &p_qmincard, &p_logic_qmincard),
                    restriction_card_of(&restr_subj, &p_qmaxcard, &p_logic_qmaxcard),
                );
                if let Some(n) = q_exact {
                    qlo = Some(n);
                    qhi = Some(n);
                }
                match restriction_value(&restr_subj, &p_onclass, &p_logic_onclass) {
                    // A qualified cardinality qualifies over EITHER an object filler
                    // (`owl:onClass <Class>`) or a datatype filler (`owl:onDataRange <Datatype>`).
                    // With no `owl:onClass`, fall through to the datatype-qualified peer.
                    None => {
                        match restriction_value(&restr_subj, &p_ondatarange, &p_logic_ondatarange) {
                            // `owl:onDataRange <Datatype>` is the datatype-qualified peer of
                            // `owl:onClass <Class>`. It reads as the datatype every counted value must
                            // carry, degraded to a PLAIN `sh:datatype` + `sh:minCount`/`sh:maxCount`
                            // (min→minCount, max→maxCount, exact→both — the same count the onClass arm
                            // carries). A bare `sh:datatype` is what the JSON-Schema deriver reads; a
                            // `sh:qualifiedValueShape [ sh:datatype … ]` it would ignore.
                            Some(Node::Iri(dt)) => {
                                // `classify` maps a concrete datatype → `sh:datatype`, `rdfs:Literal`
                                // → the `sh:Literal` node-kind, and `rdfs:Resource` → no component
                                // (the vacuous universal top) — the datatype analogue of the onClass
                                // arm's class handling.
                                let comps: Vec<_> = classify(&dt).into_iter().collect();
                                let pc = PropertyConstraintIr::new(
                                    &on,
                                    qlo,
                                    qhi,
                                    Some(ConstraintProvenance::OwlRestriction),
                                    comps,
                                )?;
                                entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                                    .2
                                    .push(pc);
                            }
                            Some(filler @ Node::Blank { .. }) => {
                                let subject = term_as_subject(&filler).expect("blank data range");
                                let facets = datatype_facets(&subject)?;
                                let comps = match classify_filler(&subject) {
                                    Some(component) => vec![component],
                                    None => facets,
                                };
                                if !comps.is_empty() {
                                    let pc = PropertyConstraintIr::new(
                                        &on,
                                        qlo,
                                        qhi,
                                        Some(ConstraintProvenance::OwlRestriction),
                                        comps,
                                    )?;
                                    entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                                        .2
                                        .push(pc);
                                }
                            }
                            // Non-resource fillers cannot name a datatype definition.
                            Some(Node::Lit(purrdf::RdfLiteral { .. })) | Some(Node::Triple(_)) => {}
                            None => {
                                return Err(Diag::of_kind(crate::error::Frontend::new(format!(
                                    "qualified cardinality on {on} requires owl:onClass or owl:onDataRange"
                                ))));
                            }
                        }
                    }
                    // An anonymous qualifying class expression is carried in the canon, never a
                    // bare blank shape — skip (do not emit).
                    Some(Node::Blank { .. })
                    | Some(Node::Lit(purrdf::RdfLiteral { .. }))
                    | Some(Node::Triple(_)) => {}
                    Some(Node::Iri(q)) if is_top_thing(&q) => {
                        // `logic:onClass logic:Thing` (canonical; `owl:onClass owl:Thing` is its
                        // generated projection) qualifies over "any individual" — the qualified
                        // count degrades to an unqualified `sh:minCount`/`sh:maxCount` rather than
                        // a vacuous inner shape.
                        let pc = PropertyConstraintIr::new(
                            &on,
                            qlo,
                            qhi,
                            Some(ConstraintProvenance::OwlRestriction),
                            vec![],
                        )?;
                        entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                            .2
                            .push(pc);
                    }
                    Some(Node::Iri(q)) => {
                        // The faithful projection of a qualified cardinality is the
                        // `sh:qualifiedValueShape` alone — it carries the count on exactly the
                        // values that satisfy the inner shape. A BARE `sh:class q` reads as the
                        // UNIVERSAL "every value of `on` is a q", which a qualified cardinality
                        // (min/max `onClass q`) does NOT entail, so emitting it unconditionally
                        // OVER-CLAIMS (caught by the lift/derive round-trip `certify` invariant).
                        // Emit the bare `sh:class` — which the JSON-Schema deriver and the
                        // purrdf-shapes object-class node-ref path read (they ignore the class
                        // nested inside a `sh:qualifiedValueShape`) — ONLY when a genuine universal
                        // backs it: an `owl:allValuesFrom q` is already emitted by its own arm
                        // above, and here we cover a closed-world-opted-in `rdfs:range on q` (the
                        // same `ClosedWorldClosure` opt-in that gates the property-level range
                        // shape). With neither universal, only the qualified shape is emitted.
                        let range_backed = closed_optins.contains(&on)
                            && restriction_objects(
                                &Subject::Iri(on.clone()),
                                &p_range,
                                &p_logic_range,
                            )
                            .into_iter()
                            .any(|c| matches!(c, Node::Iri(r) if r == q));
                        let mut comps = Vec::new();
                        if range_backed {
                            comps.push(ConstraintComponent::Class(q.clone()));
                        }
                        comps.push(ConstraintComponent::QualifiedValueShape {
                            shape: vec![ConstraintComponent::Class(q)],
                            min: qlo,
                            max: qhi,
                        });
                        let pc = PropertyConstraintIr::new(&on, None, None, None, comps)?;
                        entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                            .2
                            .push(pc);
                    }
                }
            } else {
                // Unqualified cardinality → sh:minCount / sh:maxCount with OwlRestriction
                // provenance (the open-world axiom read closed-world).
                let exact = restriction_card_of(&restr_subj, &p_card, &p_logic_card);
                let (mut lo, mut hi) = (
                    restriction_card_of(&restr_subj, &p_mincard, &p_logic_mincard),
                    restriction_card_of(&restr_subj, &p_maxcard, &p_logic_maxcard),
                );
                if let Some(n) = exact {
                    lo = Some(n);
                    hi = Some(n);
                }
                if lo.is_some() || hi.is_some() {
                    let pc = PropertyConstraintIr::new(
                        &on,
                        lo,
                        hi,
                        Some(ConstraintProvenance::OwlRestriction),
                        vec![],
                    )?;
                    entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                        .2
                        .push(pc);
                }
            }
        }
    }

    // ── FAMILY 2 — class-level axioms (Class(C) target, node_components) ───────────────────
    for class in &classes {
        if subject_is_blank(class) {
            continue;
        }
        let class_iri = subject_str(class);
        if !is_authoring_ns(&class_iri) || optouts.contains(&class_iri) {
            continue;
        }
        // {owl,logic}:disjointWith D → sh:not [ sh:class D ]. Blank operand → skip.
        for d in restriction_objects(class, &p_disjoint, &p_logic_disjoint) {
            if let Node::Iri(d) = d {
                entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                    .1
                    .push(ConstraintComponent::Not(Box::new(
                        ConstraintComponent::Class(d),
                    )));
            }
        }
        // {owl,logic}:complementOf D → sh:not [ sh:class D ]. Blank operand → skip.
        for d in restriction_objects(class, &p_complement, &p_logic_complement) {
            if let Node::Iri(d) = d {
                entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                    .1
                    .push(ConstraintComponent::Not(Box::new(
                        ConstraintComponent::Class(d),
                    )));
            }
        }
        // {owl,logic}:oneOf ( a b … ) → sh:in ( a b … ). Empty / malformed list contributes nothing.
        if let Some(head) = restriction_value(class, &p_oneof, &p_logic_oneof) {
            let members = read_iri_list(store, &head);
            if !members.is_empty() {
                let vals = members.into_iter().map(ShapeValue::Iri).collect();
                entry_for(&mut acc, ShapeTarget::Class(class_iri.clone()))
                    .1
                    .push(ConstraintComponent::In(vals));
            }
        }
    }

    // Named {owl,logic}:AllDisjointClasses ( c1 … cn ): each ordered pair (ci, cj), i≠j, contributes
    // sh:not [ sh:class cj ] to ci's shape (when ci is GMEOW-NS and not opted out).
    let all_disjoint_nodes: Vec<Subject> = {
        let mut seen: HashSet<String> = HashSet::new();
        let mut out: Vec<Subject> = Vec::new();
        for marker in [&owl_alldisjoint, &logic_alldisjoint] {
            for s in subjects_of_structural_class(store, marker) {
                if seen.insert(subject_str(&s)) {
                    out.push(s);
                }
            }
        }
        out
    };
    for adc in &all_disjoint_nodes {
        let Some(head) = restriction_value(adc, &p_members, &p_logic_members) else {
            continue;
        };
        let members = read_iri_list(store, &head);
        for ci in &members {
            if !is_authoring_ns(ci) || optouts.contains(ci) {
                continue;
            }
            for cj in &members {
                if ci == cj {
                    continue;
                }
                entry_for(&mut acc, ShapeTarget::Class(ci.clone())).1.push(
                    ConstraintComponent::Not(Box::new(ConstraintComponent::Class(cj.clone()))),
                );
            }
        }
    }

    // ── FAMILY 3 — property-level axioms (SubjectsOf(P) / ObjectsOf(P) targets) ────────────
    // Collect every GMEOW-NS property: the OWL object/datatype property-type declarations plus any
    // subject of rdfs:domain / rdfs:range. The functional / inverse-functional characteristics are
    // NOT seeded here — they are read from the canonical `logic:` carrier below (a functional-only
    // property, e.g. gmeow:unit, is discovered straight from its characteristic-assertion record,
    // not from a property-type marker).
    let mut props: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for ty in ["ObjectProperty", "DatatypeProperty"] {
        // Seed under EITHER spelling — the canonical `logic:ObjectProperty`/`logic:DatatypeProperty`
        // (Principle 17) or its legacy `owl:` view — so the property set is identical across the flip.
        for marker in crate::typing_vocab::both_spellings(ty) {
            for s in subjects_of_structural_class(store, &Node::iri(marker)) {
                if let Subject::Iri(iri) = &s
                    && is_authoring_ns(iri)
                {
                    props.insert(iri.clone());
                }
            }
        }
    }
    const DOMAIN_IRI: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
    const RANGE_IRI: &str = "http://www.w3.org/2000/01/rdf-schema#range";
    const LOGIC_DOMAIN_IRI: &str = "https://blackcatinformatics.ca/logic/domain";
    const LOGIC_RANGE_IRI: &str = "https://blackcatinformatics.ca/logic/range";
    for q in default_graph_quads(store) {
        let qp = q.predicate.as_str();
        if (qp == DOMAIN_IRI || qp == RANGE_IRI || qp == LOGIC_DOMAIN_IRI || qp == LOGIC_RANGE_IRI)
            && let Subject::Iri(iri) = &q.subject
            && is_authoring_ns(iri)
        {
            props.insert(iri.clone());
        }
    }

    for p in &props {
        if optouts.contains(p) {
            continue;
        }
        let p_subj = Subject::Iri(p.clone());
        // rdfs:domain / rdfs:range are open-world INFERENCE axioms — they ENTAIL the subject's /
        // object's type, they do not REQUIRE it to be asserted. Their closed-world `sh:class`
        // reading over-claims on any graph that (legitimately) relies on that entailment rather
        // than asserting the type standalone, so — unlike the genuinely closed-world constraints
        // (cardinality, value restrictions) which stay derive-all — domain/range are OPEN-WORLD BY
        // DEFAULT: a domain/range validation shape is derived ONLY for a property explicitly opted
        // IN via a `logic:ClosedWorldClosure` closure entry (the inverse polarity of the
        // `OpenWorldClosure` opt-out, reusing the same closure vocabulary — no new shape DSL).
        if closed_optins.contains(p) {
            // {rdfs,logic}:domain C → a SubjectsOf(P) node condition (every subject of P satisfies it).
            for c in restriction_objects(&p_subj, &p_domain, &p_logic_domain) {
                if let Node::Iri(c) = c
                    && let Some(cc) = classify(&c)
                {
                    entry_for(&mut acc, ShapeTarget::SubjectsOf(p.clone()))
                        .1
                        .push(cc);
                }
            }
            // {rdfs,logic}:domain [ owl:Restriction on Q ] → a SubjectsOf(P) PROPERTY condition: every
            // subject of P belongs to the anonymous restriction class, i.e. satisfies the
            // restriction on Q (the required-companion pattern: "a node lowered through P must
            // also declare Q"). Read closed-world only under the same explicit ClosedWorldClosure
            // opt-in that gates the named-domain shape, with unqualified cardinality and
            // named-class/datatype fillers — any other anonymous domain expression stays in the
            // canon.
            for c in restriction_objects(&p_subj, &p_domain, &p_logic_domain) {
                let Some(domain_subj) = term_as_subject(&c) else {
                    continue;
                };
                if !subject_is_blank(&domain_subj) {
                    continue;
                }
                let Some(Node::Iri(on_q)) = value(store, &domain_subj, &p_on) else {
                    continue;
                };
                if optouts.contains(&on_q) {
                    continue;
                }
                let mut comps: Vec<ConstraintComponent> = Vec::new();
                for vp in [&p_some, &p_all] {
                    if let Some(Node::Iri(cv)) = value(store, &domain_subj, vp)
                        && let Some(cc) = classify_on(&on_q, &cv)
                    {
                        comps.push(cc);
                    }
                }
                let exact = card_of(&domain_subj, &p_card);
                let (mut lo, mut hi) = (
                    card_of(&domain_subj, &p_mincard),
                    card_of(&domain_subj, &p_maxcard),
                );
                if let Some(n) = exact {
                    lo = Some(n);
                    hi = Some(n);
                }
                if comps.is_empty() && lo.is_none() && hi.is_none() {
                    continue;
                }
                let provenance =
                    (lo.is_some() || hi.is_some()).then_some(ConstraintProvenance::OwlRestriction);
                let pc = PropertyConstraintIr::new(&on_q, lo, hi, provenance, comps)?;
                entry_for(&mut acc, ShapeTarget::SubjectsOf(p.clone()))
                    .2
                    .push(pc);
            }
            // {rdfs,logic}:range C → an ObjectsOf(P) node condition (every object of P satisfies it).
            for c in restriction_objects(&p_subj, &p_range, &p_logic_range) {
                let components = match c {
                    Node::Iri(c) => classify_on(p, &c).into_iter().collect(),
                    filler @ Node::Blank { .. } => {
                        let fs = term_as_subject(&filler).expect("blank range has a subject");
                        let failure = |detail| {
                            Diag::of_kind(crate::error::Frontend::new(detail)).with_focus(p.clone())
                        };
                        let mut heads = Vec::new();
                        for head in restriction_objects(&fs, &p_unionof, &p_logic_unionof) {
                            if !heads.contains(&head) {
                                heads.push(head);
                            }
                        }
                        if let Some(head) = heads.first() {
                            if heads.len() != 1 {
                                return Err(failure(
                                    "a closed range has conflicting union lists".into(),
                                ));
                            }
                            let members = crate::lists::walk(store, head).map_err(|defect| {
                                failure(format!("invalid closed range union: {}", defect.reason()))
                            })?;
                            let mut branches = Vec::new();
                            let mut unrestricted = false;
                            for (_, member) in members {
                                let Node::Iri(iri) = member else {
                                    return Err(failure("a closed range union requires named class or datatype members".into()));
                                };
                                if let Some(component) = classify_on(p, &iri) {
                                    branches.push(component);
                                } else {
                                    unrestricted = true;
                                }
                            }
                            if unrestricted {
                                Vec::new()
                            } else if branches.is_empty() {
                                // The empty union admits no value, not every value.
                                vec![ConstraintComponent::In(Vec::new())]
                            } else {
                                vec![ConstraintComponent::Or(branches)]
                            }
                        } else {
                            return Err(failure(
                                "unsupported anonymous closed range expression".into(),
                            ));
                        }
                    }
                    _ => {
                        return Err(Diag::of_kind(crate::error::Frontend::new(
                            "a closed property range must be a resource".into(),
                        ))
                        .with_focus(p.clone()));
                    }
                };
                if !components.is_empty() {
                    entry_for(&mut acc, ShapeTarget::ObjectsOf(p.clone()))
                        .1
                        .extend(components);
                }
            }
        }
    }

    // ── Functional / inverse-functional characteristics — read from the canonical logic: carrier ──
    // The `owl:FunctionalProperty` / `owl:InverseFunctionalProperty` property-type markers are a
    // DEPRECATED projection source; the single authority is the `logic:PropertyCharacteristicAssertion`
    // record that joins `logic:characterizes` (the characterized property P) with
    // `logic:characteristicSort` (the marker) — the same carrier the native coherence gate reads.
    // A functional/inverse-functional characteristic is a genuine closed-world cardinality bound (it
    // CONSTRAINS, it does not merely infer), so it stays derive-all (+ the OpenWorldClosure opt-out),
    // independent of the domain/range opt-in above. The functional cap lands on BOTH the
    // property-scoped SubjectsOf(P) domain shape AND — so the declarative class-node reader
    // (Pydantic / ShEx) narrows the field to scalar — every rdfs:domain class node shape of P. A
    // property with NO rdfs:domain (e.g. gmeow:unit) keeps ONLY the property-scoped cap; no class
    // node shape is fabricated. A domain or range class outside the authoring namespaces likewise
    // gets no class node shape (a derived shape is named only in an authoring namespace); the
    // property-scoped cap still bounds every subject (object) of P, whatever its class. The merge
    // (`merge_same_path_properties`) folds the class-scoped cap cleanly into any restriction the
    // class already authored on P.
    let char_assertion_ty = Node::iri(logic_iri("PropertyCharacteristicAssertion"));
    let functional_sort = Node::iri(logic_iri("functionalProperty"));
    let inverse_functional_sort = Node::iri(logic_iri("inverseFunctionalProperty"));
    let p_characterizes = nn(&logic_iri("characterizes"));
    let p_characteristic_sort = nn(&logic_iri("characteristicSort"));
    for rec in subjects_of_structural_class(store, &char_assertion_ty) {
        let Some(Node::Iri(prop)) = value(store, &rec, &p_characterizes) else {
            continue;
        };
        if optouts.contains(&prop) {
            continue;
        }
        let sorts = objects(store, &rec, &p_characteristic_sort);
        let prop_subj = Subject::Iri(prop.clone());
        if sorts.contains(&functional_sort) {
            // Property-scoped cap: each subject of P has ≤1 value (sh:maxCount 1 on P).
            let pc = PropertyConstraintIr::new(
                &prop,
                None,
                Some(1),
                Some(ConstraintProvenance::OwlRestriction),
                vec![],
            )?;
            entry_for(&mut acc, ShapeTarget::SubjectsOf(prop.clone()))
                .2
                .push(pc);
            // Class-scoped cap: the same maxCount-1 on each {rdfs,logic}:domain class node shape, so
            // the class-node reader narrows the Python field to scalar.
            for c in restriction_objects(&prop_subj, &p_domain, &p_logic_domain) {
                if let Node::Iri(c) = c
                    && matches!(classify(&c), Some(ConstraintComponent::Class(_)))
                    && is_authoring_ns(&c)
                {
                    let pc = PropertyConstraintIr::new(
                        &prop,
                        None,
                        Some(1),
                        Some(ConstraintProvenance::OwlRestriction),
                        vec![],
                    )?;
                    entry_for(&mut acc, ShapeTarget::Class(c)).2.push(pc);
                }
            }
        }
        if sorts.contains(&inverse_functional_sort) {
            // Inverse-functional: each object of P has ≤1 subject via P (inverse sh:maxCount 1), on
            // both the property-scoped ObjectsOf(P) shape and each rdfs:range class node shape.
            let pc = PropertyConstraintIr::new(
                &prop,
                None,
                Some(1),
                Some(ConstraintProvenance::OwlRestriction),
                vec![],
            )?
            .inverted();
            entry_for(&mut acc, ShapeTarget::ObjectsOf(prop.clone()))
                .2
                .push(pc);
            for c in restriction_objects(&prop_subj, &p_range, &p_logic_range) {
                if let Node::Iri(c) = c
                    && matches!(classify(&c), Some(ConstraintComponent::Class(_)))
                    && is_authoring_ns(&c)
                {
                    let pc = PropertyConstraintIr::new(
                        &prop,
                        None,
                        Some(1),
                        Some(ConstraintProvenance::OwlRestriction),
                        vec![],
                    )?
                    .inverted();
                    entry_for(&mut acc, ShapeTarget::Class(c)).2.push(pc);
                }
            }
        }
    }

    // ── FAMILY 4 — logic:KeyAssertion (single-property keys → inverse-functional reading) ──────
    // A `logic:KeyAssertion` record (`logic:keyClass C`, `logic:keyProperty P`) says the value of P
    // uniquely identifies the C instance — the canonical logic: carrier of a datatype/single-property
    // key (the DEPRECATED `owl:hasKey ( P )` OWL-DL axiom is its lossy view, no longer a projection
    // source; an `owl:InverseFunctionalProperty` on a datatype property would be OWL 2 Full). Its
    // closed-world reading is the same inverse `sh:maxCount 1` (each P-value has ≤1 subject via P) the
    // InverseFunctionalProperty arm emits. A COMPOSITE key (a record naming several `logic:keyProperty`
    // values) asserts the TUPLE is unique, not each part — it has no single-path SHACL form, so it is
    // carried in the canon and no shape is derived (never a wrong per-part uniqueness claim).
    let key_assertion_ty = Node::iri(logic_iri("KeyAssertion"));
    let p_key_class = nn(&logic_iri("keyClass"));
    let p_key_property = nn(&logic_iri("keyProperty"));
    for rec in subjects_of_structural_class(store, &key_assertion_ty) {
        // The keyed class must be GMEOW-owned (the dogfooding guard every family applies).
        let Some(Node::Iri(class_iri)) = value(store, &rec, &p_key_class) else {
            continue;
        };
        if !is_authoring_ns(&class_iri) {
            continue;
        }
        let keys: Vec<String> = objects(store, &rec, &p_key_property)
            .into_iter()
            .filter_map(|o| match o {
                Node::Iri(i) => Some(i),
                _ => None,
            })
            .collect();
        // Single-property key only; a composite key has no single-path uniqueness shape.
        if let [key_prop] = keys.as_slice() {
            if optouts.contains(key_prop) {
                continue;
            }
            let pc = PropertyConstraintIr::new(
                key_prop,
                None,
                Some(1),
                Some(ConstraintProvenance::OwlRestriction),
                vec![],
            )?
            .inverted();
            entry_for(&mut acc, ShapeTarget::ObjectsOf(key_prop.clone()))
                .2
                .push(pc);
        }
    }

    // ── FAMILY 5 — statement-layer reifier obligations ────────────────────────────────────
    // The one closed-world condition with NO ordinary-OWL antecedent: "every K→P→value assertion
    // must be reified, and its reifier must conform to shape C." A slice declares it with the
    // classic RDF reification form (`rdf:Statement` + `rdf:subject`/`rdf:predicate`/`rdf:object`),
    // which this schema-obligation reader currently recognizes. This is separate from native
    // rule-atom authoring: RDF 1.2 reifiers travel through the statement layer to gmeow.gts,
    // and that transport does not require a base-quad reification scaffold.
    // No parallel shape vocabulary is minted; `C` must be a constrained GMEOW class so FAMILY 1
    // derives the `{C}-shape` node the `sh:reifierShape` reference resolves to. Lowered to a
    // property shape on path `P` carrying `sh:reifierShape {C}-shape` + `sh:reificationRequired true`.
    //
    // The declaration needs its type `C` to name that shape, which also makes it an instance of
    // `C` (and of every superclass of `C`) without being a reifier occurrence: it is schema
    // metadata. Each recognized declaration is therefore recorded under every class it is an
    // instance of, and the class shapes built below exclude exactly those declarations, by IRI,
    // from their focus sets. No other node is excluded, whatever form it is written in.
    let mut obligation_declarations: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let p_rdf_subject = nn(RDF_SUBJECT);
    let p_rdf_predicate = nn(RDF_PREDICATE);
    for r in subjects_with(store, &nn(RDF_TYPE), &Node::iri(RDF_STATEMENT)) {
        // The reified predicate `P` and subject class `K` must both be GMEOW-owned (the dogfooding
        // guard every family applies).
        let Some(Node::Iri(p)) = value(store, &r, &p_rdf_predicate) else {
            continue;
        };
        let Some(Node::Iri(k)) = value(store, &r, &p_rdf_subject) else {
            continue;
        };
        if !is_authoring_ns(&k) || !is_authoring_ns(&p) {
            continue;
        }
        // The reifier's GMEOW type `C` names the shape the reifier must conform to. An untyped
        // reifier carries no shape reference (a `sh:reifierShape` with no resolvable target would
        // dangle), so the typed-reifier form is required — an untyped one is not an obligation.
        let Some(c) = objects(store, &r, &nn(RDF_TYPE))
            .into_iter()
            .find_map(|t| match t {
                Node::Iri(i) if is_authoring_ns(&i) => Some(i),
                _ => None,
            })
        else {
            continue;
        };
        // A shape can exclude a node only by naming it, so a declaration must have an IRI.
        let Subject::Iri(declaration) = &r else {
            return Err(Diag::of_kind(crate::error::Frontend::new(format!(
                "reification obligation on subject class <{k}> and predicate <{p}> is declared \
                 by a blank node; the declaration must be an IRI so the shape derived for its \
                 reifier class <{c}> can exclude it from validation by identity"
            )))
            .with_focus(subject_str(&r)));
        };
        let mut pending: Vec<String> = objects(store, &r, &nn(RDF_TYPE))
            .into_iter()
            .filter_map(|t| match t {
                Node::Iri(i) if is_authoring_ns(&i) => Some(i),
                _ => None,
            })
            .collect();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        while let Some(class) = pending.pop() {
            if !seen.insert(class.clone()) {
                continue;
            }
            obligation_declarations
                .entry(class.clone())
                .or_default()
                .insert(declaration.clone());
            let class = Subject::Iri(class);
            pending.extend(
                restriction_objects(&class, &p_subclass, &p_logic_subclass)
                    .into_iter()
                    .filter_map(|superclass| match superclass {
                        Node::Iri(i) if is_authoring_ns(&i) => Some(i),
                        _ => None,
                    }),
            );
        }
        let property = PropertyConstraintIr::new(p.clone(), None, None, None, vec![])?
            .with_reifier(Some(format!("{c}-shape")), true)?;
        entry_for(&mut acc, ShapeTarget::Class(k.clone()))
            .2
            .push(property);
    }

    // ── FAMILY 6 — value-keyed general class inclusion (ValueKeyed(P,V) target) ────────────
    // A general class axiom whose SUBJECT is an anonymous value restriction
    // `[ owl:onProperty P ; owl:hasValue V ]` reads closed-world as "every focus node with P = V
    // must satisfy the superclass" — a value-keyed selection projected to an `sh:SPARQLTarget`
    // (`SELECT ?this WHERE { ?this P V }`). It is the modes-ride-one-class idiom: several kinds
    // share ONE class, discriminated by a VALUE (`inferenceModeOf gmeow:modeAbduction`), never a
    // subclass (Principle 9). The superclass restriction lowers to one property constraint exactly
    // as FAMILY 1 reads a subclass restriction. Only an IRI-valued key is value-keyable (the SPARQL
    // target binds an IRI object); a literal key or a superclass that is not a single-property
    // restriction is carried in the canon, never a wrong value-keyed shape.
    let subclassof_iri = format!("{rdfs}subClassOf");
    for q in default_graph_quads(store) {
        if q.predicate.as_str() != subclassof_iri || !subject_is_blank(&q.subject) {
            continue;
        }
        let key_subj = q.subject.clone();
        let Some(Node::Iri(key_pred)) = value(store, &key_subj, &p_on) else {
            continue;
        };
        let Some(Node::Iri(key_val)) = value(store, &key_subj, &p_hasvalue) else {
            continue;
        };
        if !is_authoring_ns(&key_pred) || optouts.contains(&key_pred) {
            continue;
        }
        let Some(super_subj) = term_as_subject(&q.object) else {
            continue;
        };
        let Some(Node::Iri(on)) = value(store, &super_subj, &p_on) else {
            continue;
        };
        if optouts.contains(&on) {
            continue;
        }
        let mut components: Vec<ConstraintComponent> = Vec::new();
        for vp in [&p_some, &p_all] {
            if let Some(Node::Iri(cv)) = value(store, &super_subj, vp)
                && let Some(cc) = classify_on(&on, &cv)
            {
                components.push(cc);
            }
        }
        if let Some(Node::Iri(v)) = value(store, &super_subj, &p_hasvalue) {
            components.push(ConstraintComponent::HasValue(ShapeValue::Iri(v)));
        }
        let exact = card_of(&super_subj, &p_card);
        let (mut lo, mut hi) = (
            card_of(&super_subj, &p_mincard),
            card_of(&super_subj, &p_maxcard),
        );
        if let Some(n) = exact {
            lo = Some(n);
            hi = Some(n);
        }
        if components.is_empty() && lo.is_none() && hi.is_none() {
            continue;
        }
        let provenance =
            (lo.is_some() || hi.is_some()).then_some(ConstraintProvenance::OwlRestriction);
        let pc = PropertyConstraintIr::new(&on, lo, hi, provenance, components)?;
        entry_for(
            &mut acc,
            ShapeTarget::ValueKeyed {
                predicate: key_pred,
                value: key_val,
            },
        )
        .2
        .push(pc);
    }

    // ── FAMILY 7 — pinned-value forbidden-pattern records (Class(C) target) ────────────────
    // An authored `logic:ForbiddenPatternConstraint` carrying `logic:onClass C`,
    // `logic:forbiddenPredicate P`, and a PINNED `logic:forbiddenValue v` is the canonical
    // validates-but-does-not-entail record for "an instance of C must never carry P = v".
    // It is the decidable authoring form of the value-complement pattern: the OWL rendering
    // (`owl:allValuesFrom [ owl:complementOf [ owl:hasValue v ] ]`) is outside the native
    // reasoner's decidable fragment, so it must never sit in the reasoning core as an axiom.
    // The record lowers declaratively to the exact SHACL-Core component the legacy shapes
    // carried — `sh:not [ sh:hasValue v ]` on path P of the `{C}-shape` — merging with the
    // class's other property conditions on that path. It is a validation descriptor, not an
    // OWL axiom being re-read, so the per-property `OpenWorldClosure` opt-out (which governs
    // the validation READING of reasoning axioms) does not apply, and its triples never enter
    // `prog.axioms` (`is_constraint_structural_predicate` / `is_constraint_sugar_class`).
    // LITERAL pins only: an IRI pin is the class-negation idiom (`logic:forbiddenPredicate
    // rdf:type`), whose declarative home is the node-level `sh:not [ sh:class … ]` the
    // disjointness family already derives — lowering it here would mint a redundant
    // `rdf:type`-path property shape that leaks into the schema projections. The IRI-pinned
    // and UNPINNED forms keep their procedural projection only.
    let forbidden_ty = Node::iri(logic_iri("ForbiddenPatternConstraint"));
    for record in subjects_of_structural_class(store, &forbidden_ty) {
        let Ok(ForbiddenPattern {
            class,
            predicate,
            pin: Some(pin @ ShapeValue::Literal(_)),
        }) = forbidden_pattern_fields(store, owner_source(store, &record), &mut BTreeSet::new())
        else {
            // The canonical constraint reader reports malformed fields.
            continue;
        };
        // Only an authoring-namespace class owns a derived shape (the same dogfooding
        // boundary every other family enforces).
        if !is_authoring_ns(&class) {
            continue;
        }
        let pc = PropertyConstraintIr::new(
            &predicate,
            None,
            None,
            None,
            vec![ConstraintComponent::Not(Box::new(
                ConstraintComponent::HasValue(pin),
            ))],
        )?;
        entry_for(&mut acc, ShapeTarget::Class(class)).2.push(pc);
    }

    // ── FAMILY 8 — value-range records (Class(C) target) ───────────────────────────────────
    // An authored `logic:ValueRangeConstraint` (`logic:onClass C`, `logic:valuePath P`, and an
    // inclusive `logic:minInclusiveBound` and/or `logic:maxInclusiveBound` literal) is the
    // canonical validates-but-does-not-entail record for a bounded numeric range on a path.
    // Like FAMILY 7 it is the decidable authoring form of an out-of-fragment OWL pattern: a
    // faceted-datatype `owl:allValuesFrom` filler is undecidable for the native reasoner as
    // soon as any literal is asserted on the constrained path (no datatype value-space
    // reasoning), so the range must never sit in the reasoning core. It lowers declaratively
    // to the exact components the legacy facet carried — `sh:minInclusive`/`sh:maxInclusive`
    // on path P of the `{C}-shape` — merging with the class's other conditions on that path.
    // A validation descriptor, never a re-read OWL axiom, so the `OpenWorldClosure` opt-out
    // does not apply and its triples never enter `prog.axioms`.
    let range_ty = Node::iri(logic_iri("ValueRangeConstraint"));
    let p_valuepath = nn(&logic_iri("valuePath"));
    let p_min_bound = nn(&logic_iri("minInclusiveBound"));
    let p_max_bound = nn(&logic_iri("maxInclusiveBound"));
    // Exclusive peers (`logic:minExclusiveBound` / `logic:maxExclusiveBound`) lower to
    // `sh:minExclusive` / `sh:maxExclusive` — the faithful record for a legacy shape whose bound
    // was authored open (e.g. `sh:minExclusive 0` for a strictly-positive denominator). An
    // inclusive bound wins over an exclusive one under the declared endpoint-selection contract.
    let p_min_excl = nn(&logic_iri("minExclusiveBound"));
    let p_max_excl = nn(&logic_iri("maxExclusiveBound"));
    for record in subjects_of_structural_class(store, &range_ty) {
        let Some(Node::Iri(class_iri)) = value(store, &record, &p_logic_onclass) else {
            continue;
        };
        if !is_authoring_ns(&class_iri) {
            continue;
        }
        let Some(Node::Iri(on)) = value(store, &record, &p_valuepath) else {
            continue;
        };
        let min_inclusive = numeric_range_bound(store, &record, &p_min_bound)?;
        let min_exclusive = numeric_range_bound(store, &record, &p_min_excl)?;
        let max_inclusive = numeric_range_bound(store, &record, &p_max_bound)?;
        let max_exclusive = numeric_range_bound(store, &record, &p_max_excl)?;
        // Inclusive bound wins over the exclusive peer on the same endpoint.
        let (lo, lo_incl) = match min_inclusive {
            Some(v) => (Some(v), true),
            None => (min_exclusive, false),
        };
        let (hi, hi_incl) = match max_inclusive {
            Some(v) => (Some(v), true),
            None => (max_exclusive, false),
        };
        if lo.is_none() && hi.is_none() {
            continue;
        }
        let pc = PropertyConstraintIr::new(
            &on,
            None,
            None,
            None,
            vec![ConstraintComponent::NumericRange {
                min: lo,
                max: hi,
                min_inclusive: lo_incl,
                max_inclusive: hi_incl,
            }],
        )?;
        entry_for(&mut acc, ShapeTarget::Class(class_iri))
            .2
            .push(pc);
    }

    // ── Build one shape per target ────────────────────────────────────────────────────────
    // Dedup by structural (`PartialEq`) identity so a duplicate axiom never double-counts; the IR
    // constructors then sort node_components + properties into canonical content-key order, so
    // supply order never affects the emitted bytes. Order-preserving and allocation-free (the
    // per-target component/property lists are small), so no `format!("{:?}")` per element.
    fn dedup_eq<T: PartialEq>(v: Vec<T>) -> Vec<T> {
        let mut out: Vec<T> = Vec::with_capacity(v.len());
        for x in v {
            if !out.contains(&x) {
                out.push(x);
            }
        }
        out
    }

    let mut shapes = Vec::new();
    for (iri, (target, node_components, properties)) in acc {
        let node_components = dedup_eq(node_components);
        let properties = merge_same_path_properties(dedup_eq(properties))?;
        if node_components.is_empty() && properties.is_empty() {
            continue;
        }
        let mut shape = ValidationShapeIr::new(iri, target.clone(), properties, None)?
            .with_node_components(node_components)?;
        if let ShapeTarget::Class(class) = &target
            && let Some(declarations) = obligation_declarations.get(class)
        {
            shape = shape
                .with_obligation_declarations_excluded(declarations.iter().cloned().collect())?;
        }
        // Failure metadata belongs to the canonical target term, never to a hand-authored SHACL
        // node. For a class-targeted shape that term is the CLASS; for the domain/range shapes
        // derived from `rdfs:domain P …` / `rdfs:range P …` it is the PROPERTY — the shape's whole
        // focus set is "the subjects (objects) of P", so P is the only authored term the shape
        // belongs to, and without this a property-scoped law's findings resolve to no failure class
        // at all (`<unmapped>`), leaving a conformance cell unable to say which class it just
        // proved. Collapse repeated identical values; hard-fail distinct values.
        let failure_source = match &target {
            ShapeTarget::Class(class) => Some(("class", class)),
            ShapeTarget::SubjectsOf(predicate) | ShapeTarget::ObjectsOf(predicate) => {
                Some(("property", predicate))
            }
            ShapeTarget::ValueKeyed { .. }
            | ShapeTarget::DirectClass(_)
            | ShapeTarget::Sparql(_) => None,
        };
        if let Some((kind, term)) = failure_source {
            let failure_classes = distinct_failure_classes(store, &Subject::Iri(term.clone()))?;
            if failure_classes.len() > 1 {
                return Err(Diag::of_kind(crate::error::Frontend::new(format!(
                    "{kind} {term} has distinct gmeow:enforcesFailureClass values"
                ))));
            }
            if let Some(failure_class) = failure_classes.first() {
                shape = shape.with_failure_class(failure_class)?;
            }
        }
        shapes.push(shape);
    }
    Ok(shapes)
}

/// The authoring-completeness invariant for the functional-characteristic carrier migration.
///
/// Every `gmeow:` property still declared `owl:FunctionalProperty` MUST also carry a
/// `logic:PropertyCharacteristicAssertion` record whose `logic:characteristicSort` is
/// `logic:functionalProperty` and whose `logic:characterizes` names it — the canonical carrier the
/// SHACL/Pydantic projection now reads (the `owl:FunctionalProperty` marker is a deprecated,
/// no-longer-projected source). A property in the returned set is an AUTHORING GAP: its
/// functionality would silently vanish from the derived projection because no carrier record
/// grounds it. Returned sorted (BTreeSet) for a deterministic diagnostic; a non-empty result is a
/// HARD FAIL on the sync path, never a soft warning — the caller must stop, not degrade.
///
/// The live sync-path caller is the `stage-compile-logic` shape-derivation stage
/// (`crates/pipeline/src/stages/compile_logic.rs`), which runs this over the merged authored
/// corpus and returns `Err` if the set is non-empty. Post-removal of the `owl:FunctionalProperty`
/// slice sources it is vacuously satisfied (nothing is `declared`), so it guards RE-introduction.
pub fn functional_properties_missing_logic_carrier(
    store: &RdfDataset,
) -> std::collections::BTreeSet<String> {
    use gmeow_ns::GMEOW_NS;
    let owl_functional = Node::iri("http://www.w3.org/2002/07/owl#FunctionalProperty");
    // Every gmeow:-owned property carrying the deprecated OWL functional marker.
    let mut declared: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for s in subjects_with(store, &nn(RDF_TYPE), &owl_functional) {
        if let Subject::Iri(iri) = &s
            && iri.starts_with(GMEOW_NS)
        {
            declared.insert(iri.clone());
        }
    }
    // Every property named by a functional-sort characteristic-assertion carrier record.
    let char_assertion_ty = Node::iri(logic_iri("PropertyCharacteristicAssertion"));
    let functional_sort = Node::iri(logic_iri("functionalProperty"));
    let p_characterizes = nn(&logic_iri("characterizes"));
    let p_characteristic_sort = nn(&logic_iri("characteristicSort"));
    let mut carried: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for rec in subjects_of_structural_class(store, &char_assertion_ty) {
        if objects(store, &rec, &p_characteristic_sort).contains(&functional_sort)
            && let Some(Node::Iri(prop)) = value(store, &rec, &p_characterizes)
        {
            carried.insert(prop);
        }
    }
    declared.difference(&carried).cloned().collect()
}

// --------------------------------------------------------------------------- //
// Functional-carrier integrity (migration-surviving, non-vacuous)
// --------------------------------------------------------------------------- //

/// The frozen completeness ledger of every property that MUST bear a functional
/// `logic:PropertyCharacteristicAssertion` carrier — one canonical IRI per line, sorted, with `#`
/// comment lines and blank lines ignored.
///
/// This is a DELIBERATE, human-blessed committed artifact, exactly like the `dl_oracle_gold`
/// frozen verdicts and the `reasoning_session_semver` drift-pins: it is NEVER auto-updated. Any
/// property that gains or loses a functional carrier moves this file in the SAME commit that moves
/// the carrier, a conscious re-bless the author performs by hand. The
/// [`functional_carrier_integrity`] gate asserts current-set == ledger and hard-fails on any drift
/// (a silently-dropped or unexpectedly-added carrier), so a drift can never land unnoticed.
///
/// # Re-blessing (a deliberate human act)
///
/// When a functional carrier is intentionally added or removed, regenerate this file from the
/// current corpus and commit it alongside the carrier change. The set is exactly the
/// `logic:characterizes` targets of the `logic:functionalProperty`-sort
/// `logic:PropertyCharacteristicAssertion` records over the merged authored corpus (see
/// [`functional_carrier_property_iris`]). The pipeline's `compile_logic` stage checks that merged
/// corpus with [`functional_carrier_integrity`] and reports each missing or unexpected property
/// on a mismatch.
const FUNCTIONAL_CARRIER_LEDGER: &str = include_str!("functional_carrier_ledger.txt");

/// Parse [`FUNCTIONAL_CARRIER_LEDGER`] into its sorted IRI set (ignoring `#` comments / blanks).
pub(super) fn functional_carrier_ledger() -> std::collections::BTreeSet<String> {
    FUNCTIONAL_CARRIER_LEDGER
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// The RDF/OWL/`logic:` types whose subject "exists as a declared property" for the orphan check:
/// an `rdf:Property` or any of its OWL/`logic:` sub-kinds. This is the property-declaration reading
/// the validation-shape derivation uses (`{owl,logic}:ObjectProperty` / `…DatatypeProperty`),
/// widened to the full property-declaration vocabulary so a carrier target declared with ANY
/// property type is recognised, and a merely misspelled / never-declared target is not. Both the
/// canonical `logic:` declaration/characteristic spellings (Principle 17) and their legacy `owl:`
/// views are listed, so the check bites identically across the authoring-surface flip. (The
/// canonical characteristic locals are lower-camel — `logic:transitiveProperty` — mirroring
/// the OWL projection's `owl_for_char` map.)
const PROPERTY_DECLARATION_TYPES: [&str; 21] = [
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property",
    "http://www.w3.org/2002/07/owl#ObjectProperty",
    "http://www.w3.org/2002/07/owl#DatatypeProperty",
    "http://www.w3.org/2002/07/owl#AnnotationProperty",
    "http://www.w3.org/2002/07/owl#FunctionalProperty",
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
    "http://www.w3.org/2002/07/owl#TransitiveProperty",
    "http://www.w3.org/2002/07/owl#SymmetricProperty",
    "http://www.w3.org/2002/07/owl#AsymmetricProperty",
    "http://www.w3.org/2002/07/owl#ReflexiveProperty",
    "http://www.w3.org/2002/07/owl#IrreflexiveProperty",
    "https://blackcatinformatics.ca/logic/ObjectProperty",
    "https://blackcatinformatics.ca/logic/DatatypeProperty",
    "https://blackcatinformatics.ca/logic/AnnotationProperty",
    "https://blackcatinformatics.ca/logic/functionalProperty",
    "https://blackcatinformatics.ca/logic/inverseFunctionalProperty",
    "https://blackcatinformatics.ca/logic/transitiveProperty",
    "https://blackcatinformatics.ca/logic/symmetricProperty",
    "https://blackcatinformatics.ca/logic/asymmetricProperty",
    "https://blackcatinformatics.ca/logic/reflexiveProperty",
    "https://blackcatinformatics.ca/logic/irreflexiveProperty",
];

/// Every functional carrier record's `logic:characterizes` target, as a property → carrier-count
/// multiset over the store. A property named functional by two distinct
/// `logic:PropertyCharacteristicAssertion` records appears with count 2 (the duplicate signal).
pub(super) fn functional_carrier_multiset(
    store: &RdfDataset,
) -> std::collections::BTreeMap<String, usize> {
    let char_assertion_ty = Node::iri(logic_iri("PropertyCharacteristicAssertion"));
    let functional_sort = Node::iri(logic_iri("functionalProperty"));
    let p_characterizes = nn(&logic_iri("characterizes"));
    let p_characteristic_sort = nn(&logic_iri("characteristicSort"));
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for rec in subjects_of_structural_class(store, &char_assertion_ty) {
        if objects(store, &rec, &p_characteristic_sort).contains(&functional_sort)
            && let Some(Node::Iri(prop)) = value(store, &rec, &p_characterizes)
        {
            *counts.entry(prop).or_insert(0) += 1;
        }
    }
    counts
}

/// The set of properties bearing at least one functional carrier record — the exact set the
/// completeness ledger pins. Sorted and deterministic; this is the generator a re-bless copies
/// into `frontend/functional_carrier_ledger.txt`.
pub fn functional_carrier_property_iris(store: &RdfDataset) -> std::collections::BTreeSet<String> {
    functional_carrier_multiset(store).into_keys().collect()
}

/// Every subject typed as an RDF/OWL property (see [`PROPERTY_DECLARATION_TYPES`]).
pub(super) fn declared_property_iris(store: &RdfDataset) -> std::collections::BTreeSet<String> {
    let mut props: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for ty in PROPERTY_DECLARATION_TYPES {
        for s in subjects_of_structural_class(store, &Node::iri(ty)) {
            if let Subject::Iri(iri) = s {
                props.insert(iri);
            }
        }
    }
    props
}

/// A single functional-carrier integrity violation, each a HARD FAIL on the sync path.
///
/// The variants make the migration-surviving completeness invariant NON-VACUOUS: unlike the bare
/// [`functional_properties_missing_logic_carrier`] RE-introduction guard (vacuous once the
/// `owl:FunctionalProperty` markers were removed), these bite against the LIVE carrier corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionalCarrierViolation {
    /// A property still typed `owl:FunctionalProperty` with no functional carrier record — the
    /// original RE-introduction regression guard, retained.
    ReintroducedOwlMarker { property: String },
    /// A functional carrier's `logic:characterizes` names an IRI that is not a declared property
    /// anywhere in the store (a misspelled / never-declared target).
    OrphanCarrier { property: String },
    /// A property is named functional by more than one carrier record.
    DuplicateCarrier { property: String, count: usize },
    /// A property is in the frozen ledger but bears NO functional carrier now — a silently-dropped
    /// carrier.
    LedgerMissing { property: String },
    /// A property bears a functional carrier but is ABSENT from the frozen ledger — an unexpected
    /// new carrier that was never blessed.
    LedgerUnexpected { property: String },
}

impl fmt::Display for FunctionalCarrierViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FunctionalCarrierViolation::ReintroducedOwlMarker { property } => write!(
                f,
                "re-introduced owl:FunctionalProperty on {property} without a \
                 logic:PropertyCharacteristicAssertion functionalProperty carrier record"
            ),
            FunctionalCarrierViolation::OrphanCarrier { property } => write!(
                f,
                "orphan functional carrier: logic:characterizes names {property}, which is not a \
                 declared rdf:Property / owl:ObjectProperty / owl:DatatypeProperty in the corpus"
            ),
            FunctionalCarrierViolation::DuplicateCarrier { property, count } => write!(
                f,
                "duplicate functional carrier: {property} is named functional by {count} \
                 PropertyCharacteristicAssertion records (exactly one is required)"
            ),
            FunctionalCarrierViolation::LedgerMissing { property } => write!(
                f,
                "completeness drift: {property} is in the frozen functional-carrier ledger but \
                 bears no functional carrier now (a silently-dropped carrier); re-bless the \
                 ledger only if this drop is intended"
            ),
            FunctionalCarrierViolation::LedgerUnexpected { property } => write!(
                f,
                "completeness drift: {property} bears a functional carrier but is absent from the \
                 frozen functional-carrier ledger (an unexpected new carrier); re-bless the \
                 ledger only if this addition is intended"
            ),
        }
    }
}

/// The migration-surviving functional-carrier integrity gate — the NON-VACUOUS successor to
/// [`functional_properties_missing_logic_carrier`].
///
/// It runs four checks over the merged authored corpus and returns every violation, sorted
/// deterministically (each kind is drawn from a `BTree*`, so the diagnostic bytes are stable):
///
/// 1. **RE-introduction guard** (retained) — any property still typed `owl:FunctionalProperty`
///    with no functional carrier ([`FunctionalCarrierViolation::ReintroducedOwlMarker`]).
/// 2. **Orphan carrier** — a carrier `logic:characterizes` target that is not a declared property
///    ([`FunctionalCarrierViolation::OrphanCarrier`]).
/// 3. **Duplicate carrier** — a property named functional by more than one record
///    ([`FunctionalCarrierViolation::DuplicateCarrier`]).
/// 4. **Positive completeness ledger** — the set of carrier-bearing properties must equal the
///    committed frozen ledger; any drift yields
///    [`FunctionalCarrierViolation::LedgerMissing`] / [`FunctionalCarrierViolation::LedgerUnexpected`].
///
/// A non-empty result is a HARD FAIL on the sync path (see
/// `crates/pipeline/src/stages/compile_logic.rs`), never a soft warning — the caller must stop.
pub fn functional_carrier_integrity(store: &RdfDataset) -> Vec<FunctionalCarrierViolation> {
    let mut violations: Vec<FunctionalCarrierViolation> = Vec::new();

    // (1) RE-introduction guard — kept verbatim from the pre-migration completeness gate.
    for property in functional_properties_missing_logic_carrier(store) {
        violations.push(FunctionalCarrierViolation::ReintroducedOwlMarker { property });
    }

    let multiset = functional_carrier_multiset(store);
    let declared = declared_property_iris(store);

    // (2) Orphan carrier + (3) duplicate carrier — both keyed off the sorted carrier multiset.
    for (property, count) in &multiset {
        if !declared.contains(property) {
            violations.push(FunctionalCarrierViolation::OrphanCarrier {
                property: property.clone(),
            });
        }
        if *count > 1 {
            violations.push(FunctionalCarrierViolation::DuplicateCarrier {
                property: property.clone(),
                count: *count,
            });
        }
    }

    // (4) Positive completeness ledger — the exact carrier-bearing set must equal the frozen ledger.
    let carried: std::collections::BTreeSet<String> = multiset.into_keys().collect();
    let ledger = functional_carrier_ledger();
    for property in ledger.difference(&carried) {
        violations.push(FunctionalCarrierViolation::LedgerMissing {
            property: property.clone(),
        });
    }
    for property in carried.difference(&ledger) {
        violations.push(FunctionalCarrierViolation::LedgerUnexpected {
            property: property.clone(),
        });
    }

    violations
}

/// Read `logic:PathShape` individuals into [`PathShapeIr`]s.
///
/// Fail-soft, like the rest of the front-end: a malformed shape (a step that is
/// both named and wildcard, neither named nor wildcard, a non-positive-integer
/// depth, or `min > max`) emits a `MALFORMED_PATH_SHAPE` warning and is skipped —
/// never silently dropped.
pub(super) fn extract_path_shapes(
    store: &RdfDataset,
    diagnostics: &mut Vec<Diagnostic>,
    lowerings: &mut Vec<OwnerLowering>,
) -> Vec<OwnerEmission<PathShapeIr>> {
    let mut shapes: Vec<OwnerEmission<PathShapeIr>> = Vec::new();

    let path_shape_ty = Node::iri(logic_iri("PathShape"));
    let p_step = nn(&logic_iri("pathStepPredicate"));
    let p_wildcard = nn(&logic_iri("pathWildcard"));
    let p_min = nn(&logic_iri("pathMinDepth"));
    let p_max = nn(&logic_iri("pathMaxDepth"));
    let p_ns = nn(&logic_iri("pathNamespaceScope"));
    let p_param = nn(&logic_iri("pathDepthParam"));

    for node in subjects_of_structural_class(store, &path_shape_ty) {
        if let Some(emitted) = capture_owner(
            owner_source(store, &node),
            OwnerFamily::PathShape,
            diagnostics,
            lowerings,
            |diagnostics| {
                let subj = subject_str(&node);

                // Base step: named-predicate XOR wildcard.
                let step_pred = value(store, &node, &p_step);
                // xsd:boolean lexical space: "true"/"1" = true, "false"/"0" = false.
                // Any other literal is a hard-fail (no silent coercion to false).
                let wildcard_result: Result<bool, ()> = match value(store, &node, &p_wildcard) {
                    None => Ok(false),
                    Some(t) => match term_str(&t).as_str() {
                        "true" | "1" => Ok(true),
                        "false" | "0" => Ok(false),
                        other => {
                            diagnostics.push(Diagnostic::warning(
                                "MALFORMED_PATH_SHAPE",
                                format!(
                                    "logic:pathWildcard has unrecognized boolean literal {:?}; \
                             expected \"true\", \"false\", \"1\", or \"0\"; shape skipped",
                                    other
                                ),
                                Some(subj.clone()),
                            ));
                            Err(())
                        }
                    },
                };
                let wildcard = match wildcard_result {
                    Ok(b) => b,
                    Err(()) => return Err(Vec::new()),
                };

                let base = match (step_pred.as_ref(), wildcard) {
                    (Some(_), true) => {
                        diagnostics.push(Diagnostic::warning(
                            "MALFORMED_PATH_SHAPE",
                            "logic:PathShape declares BOTH logic:pathStepPredicate and \
                     logic:pathWildcard true (a step is a named predicate XOR a \
                     wildcard); shape skipped",
                            Some(subj.clone()),
                        ));
                        return Err(Vec::new());
                    }
                    (Some(p), false) => match p {
                        // A step predicate MUST be an IRI: a literal or blank-node object
                        // would produce a malformed predicate IRI downstream (no silent
                        // coercion). Skip the shape with a diagnostic, like every other
                        // malformed-shape branch.
                        Node::Iri(_) => PathBase::NamedPredicate(term_str(p)),
                        _ => {
                            diagnostics.push(Diagnostic::warning(
                                "MALFORMED_PATH_SHAPE",
                                "logic:pathStepPredicate must be an IRI named node; shape skipped",
                                Some(subj.clone()),
                            ));
                            return Err(Vec::new());
                        }
                    },
                    (None, true) => PathBase::Wildcard,
                    (None, false) => {
                        diagnostics.push(Diagnostic::warning(
                            "MALFORMED_PATH_SHAPE",
                            "logic:PathShape declares neither logic:pathStepPredicate nor \
                     logic:pathWildcard true (a step needs a named predicate or a \
                     wildcard); shape skipped",
                            Some(subj.clone()),
                        ));
                        return Err(Vec::new());
                    }
                };

                // Depth bounds (min defaults to 1; absent max ⇒ unbounded).
                let min_depth = match value(store, &node, &p_min) {
                    Some(t) => match parse_positive_int(&term_str(&t)) {
                        Some(n) => n,
                        None => {
                            diagnostics.push(Diagnostic::warning(
                        "MALFORMED_PATH_SHAPE",
                        format!(
                            "logic:pathMinDepth is not a positive integer ({:?}); shape skipped",
                            term_str(&t)
                        ),
                        Some(subj.clone()),
                    ));
                            return Err(Vec::new());
                        }
                    },
                    None => 1,
                };
                let max_depth = match value(store, &node, &p_max) {
                    Some(t) => match parse_positive_int(&term_str(&t)) {
                        Some(n) => Some(n),
                        None => {
                            diagnostics.push(Diagnostic::warning(
                        "MALFORMED_PATH_SHAPE",
                        format!(
                            "logic:pathMaxDepth is not a positive integer ({:?}); shape skipped",
                            term_str(&t)
                        ),
                        Some(subj.clone()),
                    ));
                            return Err(Vec::new());
                        }
                    },
                    None => None,
                };

                let namespace_scope = value(store, &node, &p_ns).map(|t| term_str(&t));
                let depth_param = if value(store, &node, &p_param).is_some() {
                    match sugar_selector_literal(store, &node, "pathDepthParam") {
                        Ok(name) => Some(name),
                        Err(error) => {
                            diagnostics.push(Diagnostic::warning(
                                "MALFORMED_PATH_SHAPE",
                                error.message().to_owned(),
                                Some(subj.clone()),
                            ));
                            return Err(Vec::new());
                        }
                    }
                } else {
                    None
                };

                match PathShapeIr::new(
                    subj.clone(),
                    base,
                    min_depth,
                    max_depth,
                    namespace_scope,
                    depth_param,
                ) {
                    Ok(shape) => Ok(shape),
                    Err(msg) => {
                        diagnostics.push(Diagnostic::warning(
                            "MALFORMED_PATH_SHAPE",
                            msg.message().to_owned(),
                            Some(subj),
                        ));
                        Err(Vec::new())
                    }
                }
            },
        ) {
            shapes.push(emitted);
        }
    }

    shapes
}
