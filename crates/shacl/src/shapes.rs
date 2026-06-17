// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! SHACL shapes graph parser.
//!
//! Parses a SHACL Core shapes graph (loaded into an oxigraph [`Store`]) into a
//! fully typed [`Shapes`] structure.  No evaluation logic lives here — that is
//! Task 3.  Unsupported SHACL features (SPARQL constraints, qualified shapes,
//! complex path forms, …) cause a hard `Err` rather than a silent skip.

use std::collections::HashSet;

use oxigraph::model::{NamedNode, NamedNodeRef, NamedOrBlankNodeRef, Term};
use oxigraph::store::Store;

use crate::model::{gmeow, rdf, rdfs, sh};
use crate::report::Severity;

// ── Public types ───────────────────────────────────────────────────────────────

/// The `sh:nodeKind` value IRI mapped to a typed enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKindValue {
    /// `sh:IRI`
    Iri,
    /// `sh:BlankNode`
    BlankNode,
    /// `sh:Literal`
    Literal,
    /// `sh:BlankNodeOrIRI`
    BlankNodeOrIri,
    /// `sh:BlankNodeOrLiteral`
    BlankNodeOrLiteral,
    /// `sh:IRIOrLiteral`
    IriOrLiteral,
}

/// A SHACL property path.
#[derive(Debug, Clone)]
pub enum Path {
    /// A plain IRI predicate path (`ex:name`).
    Predicate(NamedNode),
    /// An inverse path (`[ sh:inversePath ex:parent ]`).
    Inverse(Box<Path>),
}

/// A SHACL target declaration on a node shape.
#[derive(Debug, Clone)]
pub enum Target {
    /// `sh:targetClass ex:SomeClass`
    Class(NamedNode),
    /// `sh:targetSubjectsOf ex:pred`
    SubjectsOf(NamedNode),
    /// `sh:targetObjectsOf ex:pred`
    ObjectsOf(NamedNode),
    /// `sh:targetNode ex:SomeNode` (or a literal)
    Node(Term),
    /// The shape node is itself an `rdfs:Class` → implicit class target.
    ImplicitClass(Term),
    /// `sh:target [ rdf:type sh:SPARQLTarget ; sh:select "SELECT ?this …" ]`
    Sparql(String),
}

/// A single SHACL constraint on a shape or property shape.
#[derive(Debug, Clone)]
pub enum Constraint {
    /// `sh:class ex:C`
    Class(NamedNode),
    /// `sh:datatype xsd:integer`
    Datatype(NamedNode),
    /// `sh:nodeKind sh:IRI` etc.
    NodeKind(NodeKindValue),
    /// `sh:minCount 1`
    MinCount(u64),
    /// `sh:maxCount 5`
    MaxCount(u64),
    /// `sh:in ( … )` — list of allowed values.
    In(Vec<Term>),
    /// `sh:hasValue ex:v`
    HasValue(Term),
    /// `sh:pattern "…"` with optional `sh:flags "…"`.
    Pattern {
        /// The regex string.
        regex: String,
        /// Optional regex flags (e.g. `"i"`).
        flags: Option<String>,
    },
    /// `sh:minLength 3`
    MinLength(u64),
    /// `sh:uniqueLang true`
    UniqueLang(bool),
    /// `sh:minInclusive "0"^^xsd:integer`
    MinInclusive(Term),
    /// `sh:maxInclusive "100"^^xsd:integer`
    MaxInclusive(Term),
    /// `sh:minExclusive "0"^^xsd:integer`
    MinExclusive(Term),
    /// `sh:maxExclusive "100"^^xsd:integer`
    MaxExclusive(Term),
    /// `sh:and ( … )` — list of shapes, all must conform.
    And(Vec<Shape>),
    /// `sh:or ( … )` — at least one must conform.
    Or(Vec<Shape>),
    /// `sh:xone ( … )` — exactly one must conform.
    Xone(Vec<Shape>),
    /// `sh:node <shape>` — focus node must conform to the referenced shape.
    Node(Box<Shape>),
    /// `sh:sparql [ sh:select "SELECT $this …" ]` — SPARQL-AF constraint.
    ///
    /// The constraint blank node carries its own optional `sh:message` and
    /// `sh:severity` overrides; missing values fall back to the shape defaults
    /// at evaluation time.
    Sparql {
        /// The SPARQL SELECT query text.
        select: String,
        /// Optional per-constraint message override (from `sh:message` on the
        /// constraint blank node).
        message: Option<String>,
        /// Optional per-constraint severity override (from `sh:severity` on the
        /// constraint blank node).
        severity: Option<Severity>,
    },
}

/// A property shape, reached via `sh:property` from a node shape.
#[derive(Debug, Clone)]
pub struct PropertyShape {
    /// The property path this shape applies to.
    pub path: Path,
    /// Constraints on values reached via the path.
    pub constraints: Vec<Constraint>,
    /// Node shapes that RDF 1.2 reifiers for this focus/path/value triple must conform to.
    pub reifier_shapes: Vec<Shape>,
    /// Whether at least one RDF 1.2 reifier is required for each focus/path/value triple.
    pub reification_required: bool,
    /// Severity override (default `Violation`).
    pub severity: Severity,
    /// Optional human-readable message.
    pub message: Option<String>,
    /// Optional GMEOW ABox/TBox/RBox/CBox role annotations on this property shape.
    pub box_roles: Vec<NamedNode>,
}

/// A node shape.
#[derive(Debug, Clone)]
pub struct Shape {
    /// The shape node identity (IRI or blank node).
    pub id: Term,
    /// Target declarations for this shape.
    pub targets: Vec<Target>,
    /// Node-level constraints (not path-scoped).
    pub constraints: Vec<Constraint>,
    /// Property shapes nested under this shape via `sh:property`.
    pub property_shapes: Vec<PropertyShape>,
    /// Severity override (default `Violation`).
    pub severity: Severity,
    /// Optional human-readable message.
    pub message: Option<String>,
    /// Whether `sh:deactivated true` is set.
    pub deactivated: bool,
    /// Optional GMEOW ABox/TBox/RBox/CBox role annotations on this shape.
    pub box_roles: Vec<NamedNode>,
}

/// The parsed shapes graph — a collection of top-level [`Shape`]s.
#[derive(Debug, Default, Clone)]
pub struct Shapes {
    /// Node shapes extracted from the shapes graph.
    pub node_shapes: Vec<Shape>,
}

// ── Public entry point ─────────────────────────────────────────────────────────

/// Parse shapes from an oxigraph store.
///
/// Identifies node shapes, parses all their targets, constraints, and property
/// shapes.  Unsupported SHACL features return `Err` immediately (hard-fail).
///
/// # Errors
///
/// Returns `Err(String)` when an unsupported SHACL construct is encountered or
/// when required structural data (e.g. `sh:path`) is missing.
pub fn from_store(store: &Store) -> Result<Shapes, String> {
    from_store_with_prefixes(store, &[])
}

/// Parse shapes from a store, with the shapes document's `@prefix` declarations
/// available as a fallback prefix map for SHACL-AF `sh:select` queries.
///
/// SHACL-AF queries may use prefixed names. The spec resolves them via
/// `sh:prefixes`/`sh:declare`, but real-world shapes (and pySHACL) also rely on
/// the shapes *document's* own `@prefix` declarations. Since an oxigraph `Store`
/// does not retain document prefix maps, the caller (the engine) captures them
/// from the Turtle parser and threads them here. `sh:prefixes` declarations take
/// precedence over these document-level fallbacks.
///
/// # Errors
///
/// Returns `Err(String)` on any unsupported SHACL construct or missing
/// structural data — see [`from_store`].
pub fn from_store_with_prefixes(
    store: &Store,
    doc_prefixes: &[(String, String)],
) -> Result<Shapes, String> {
    let mut parser = Parser::new(store, doc_prefixes);
    parser.parse()
}

// ── Hard-fail predicate set ────────────────────────────────────────────────────

/// The set of SHACL predicate IRIs that are part of SHACL spec but are NOT
/// modelled in this implementation.  Encountering any of them on a shape node
/// is a hard error (no silent skip).
///
/// The full list is compiled once.  Benign shape-metadata predicates
/// (`sh:name`, `sh:description`, `sh:order`, `sh:group`, `sh:message`,
/// `sh:severity`, `sh:deactivated`, `sh:path`, `sh:property`, `sh:flags`)
/// are NOT in this set — they are handled or deliberately ignored.
fn unsupported_predicates() -> HashSet<&'static str> {
    [
        sh::QUALIFIED_VALUE_SHAPE.as_str(),
        sh::QUALIFIED_MIN_COUNT.as_str(),
        sh::QUALIFIED_MAX_COUNT.as_str(),
        sh::LESS_THAN.as_str(),
        sh::LESS_THAN_OR_EQUALS.as_str(),
        sh::EQUALS.as_str(),
        sh::DISJOINT.as_str(),
        sh::NOT.as_str(),
        sh::CLOSED.as_str(),
        sh::IGNORED_PROPERTIES.as_str(),
        sh::LANGUAGE_IN.as_str(),
        sh::MAX_LENGTH.as_str(),
        // unsupported path forms (checked on bnode path objects)
        sh::ALTERNATIVE_PATH.as_str(),
        sh::ZERO_OR_MORE_PATH.as_str(),
        sh::ONE_OR_MORE_PATH.as_str(),
        sh::ZERO_OR_ONE_PATH.as_str(),
    ]
    .into_iter()
    .collect()
}

// ── Internal parser ────────────────────────────────────────────────────────────

struct Parser<'s> {
    store: &'s Store,
    unsupported: HashSet<&'static str>,
    /// Tracks shape nodes currently being parsed to prevent infinite recursion
    /// through `sh:node` or `sh:and/or/xone` cycles.
    in_flight: HashSet<String>,
    /// The shapes document's `@prefix` map (prefix → namespace), used as the
    /// fallback PREFIX header for SHACL-AF `sh:select` queries.
    doc_prefixes: Vec<(String, String)>,
}

impl<'s> Parser<'s> {
    fn new(store: &'s Store, doc_prefixes: &[(String, String)]) -> Self {
        Self {
            store,
            unsupported: unsupported_predicates(),
            in_flight: HashSet::new(),
            doc_prefixes: doc_prefixes.to_vec(),
        }
    }

    fn parse(&mut self) -> Result<Shapes, String> {
        // --- collect all top-level shape node terms ---
        let mut shape_ids: HashSet<Term> = HashSet::new();
        // Track which nodes are property-shape-only (reachable only via sh:property)
        // so we don't list them as top-level node shapes.
        let mut property_shape_nodes: HashSet<Term> = HashSet::new();

        // 1. Nodes typed sh:NodeShape
        for quad in self
            .store
            .quads_for_pattern(
                None,
                Some(rdf::TYPE),
                Some(Term::NamedNode(sh::NODE_SHAPE.into()).as_ref()),
                None,
            )
            .flatten()
        {
            shape_ids.insert(Term::from(quad.subject));
        }

        // 2. Nodes typed sh:PropertyShape (collect to exclude from top-level)
        for quad in self
            .store
            .quads_for_pattern(
                None,
                Some(rdf::TYPE),
                Some(Term::NamedNode(sh::PROPERTY_SHAPE.into()).as_ref()),
                None,
            )
            .flatten()
        {
            property_shape_nodes.insert(Term::from(quad.subject));
        }

        // 3. Subjects of sh:targetClass / sh:targetSubjectsOf / sh:targetObjectsOf / sh:targetNode
        for pred in [
            sh::TARGET_CLASS,
            sh::TARGET_SUBJECTS_OF,
            sh::TARGET_OBJECTS_OF,
            sh::TARGET_NODE,
        ] {
            for quad in self
                .store
                .quads_for_pattern(None, Some(pred), None, None)
                .flatten()
            {
                shape_ids.insert(Term::from(quad.subject));
            }
        }

        // 4. Nodes that are sh:property owners with shape constraints (implicit shapes)
        //    and nodes that are rdfs:Class AND carry sh:targetClass or sh:NodeShape type
        //    (already caught above).  We also add any node that has sh:property
        //    and is thus acting as a shape container.
        for quad in self
            .store
            .quads_for_pattern(None, Some(sh::PROPERTY), None, None)
            .flatten()
        {
            let subj_term = Term::from(quad.subject);
            // Only add if not exclusively a property shape itself
            if !property_shape_nodes.contains(&subj_term) {
                shape_ids.insert(subj_term);
            }
        }

        // 5. Nodes that carry sh:property as objects → record as property-shape-only
        for quad in self
            .store
            .quads_for_pattern(None, Some(sh::PROPERTY), None, None)
            .flatten()
        {
            property_shape_nodes.insert(quad.object.clone());
        }

        // Remove property-shape-only nodes from top-level set
        for ps in &property_shape_nodes {
            shape_ids.remove(ps);
        }

        // Parse each top-level node shape in stable (sorted) order
        let mut node_shapes: Vec<Shape> = Vec::new();
        let mut ids: Vec<Term> = shape_ids.into_iter().collect();
        ids.sort_by_key(|t| t.to_string());
        for term in ids {
            let shape = self.parse_node_shape(term.clone())?;
            node_shapes.push(shape);
        }

        Ok(Shapes { node_shapes })
    }

    /// Convert a `Term` to a `NamedOrBlankNodeRef` for use in store queries.
    fn term_as_subject_ref<'t>(term: &'t Term) -> Option<NamedOrBlankNodeRef<'t>> {
        match term {
            Term::NamedNode(n) => Some(NamedOrBlankNodeRef::NamedNode(n.as_ref())),
            Term::BlankNode(b) => Some(NamedOrBlankNodeRef::BlankNode(b.as_ref())),
            _ => None,
        }
    }

    /// Collect all predicate→object pairs for a given subject term.
    fn predicates_of(&self, subject: &Term) -> Vec<(NamedNode, Term)> {
        let Some(subj_ref) = Self::term_as_subject_ref(subject) else {
            return vec![];
        };
        self.store
            .quads_for_pattern(Some(subj_ref), None, None, None)
            .flatten()
            .map(|q| (q.predicate, q.object))
            .collect()
    }

    /// Return all objects for `(subject, predicate, ?)`.
    fn objects_of(&self, subject: &Term, predicate: NamedNodeRef<'_>) -> Vec<Term> {
        let Some(subj_ref) = Self::term_as_subject_ref(subject) else {
            return vec![];
        };
        self.store
            .quads_for_pattern(Some(subj_ref), Some(predicate), None, None)
            .flatten()
            .map(|q| q.object)
            .collect()
    }

    /// Return the first object for `(subject, predicate, ?)`, if any.
    fn first_object_of(&self, subject: &Term, predicate: NamedNodeRef<'_>) -> Option<Term> {
        self.objects_of(subject, predicate).into_iter().next()
    }

    /// Collect deterministic GMEOW graph-box role annotations from a shape node.
    fn box_roles_of(&self, subject: &Term) -> Vec<NamedNode> {
        let mut roles: Vec<NamedNode> = self
            .objects_of(subject, gmeow::GRAPH_BOX_ROLE)
            .into_iter()
            .filter_map(|t| match t {
                Term::NamedNode(n) => Some(n),
                _ => None,
            })
            .collect();
        roles.sort_unstable();
        roles.dedup();
        roles
    }

    /// Build the SPARQL `PREFIX` header prepended to a SHACL-AF `sh:select` query.
    ///
    /// oxigraph's SPARQL parser has no SHACL awareness, so prefixed names in a
    /// query must be declared in the query text. Two sources contribute, lowest
    /// precedence first:
    ///
    /// 1. The shapes **document's** `@prefix` declarations (the pySHACL-compatible
    ///    fallback — real shapes rely on these without `sh:prefixes`).
    /// 2. SHACL-AF `sh:prefixes` → `sh:declare` on the shape and/or the
    ///    `sh:sparql` / `sh:SPARQLTarget` node (spec §5.2.1), which **override**
    ///    the document fallback for any colliding prefix.
    ///
    /// Output is one `PREFIX p: <ns>` line per prefix, sorted (a `BTreeMap` keeps
    /// it deterministic and one-entry-per-prefix). Empty when nothing is declared.
    fn prefix_header(&self, owners: &[&Term]) -> String {
        let mut map: std::collections::BTreeMap<String, String> = self
            .doc_prefixes
            .iter()
            .map(|(p, ns)| (p.clone(), ns.clone()))
            .collect();
        // `sh:prefix` is a string literal; `sh:namespace` is typically an
        // `xsd:anyURI` literal but the SHACL spec also permits a bare IRI
        // (NamedNode). Accept both lexical forms so an IRI-valued namespace is
        // not silently dropped (which would omit a PREFIX line and break the
        // dependent SHACL-AF query — a silent under-validation, P11/§11).
        let term_value = |t: Term| match t {
            Term::Literal(lit) => Some(lit.value().to_owned()),
            Term::NamedNode(node) => Some(node.as_str().to_owned()),
            _ => None, // blank node / quoted triple: not a prefix or namespace value
        };
        for owner in owners {
            for prefixes_node in self.objects_of(owner, sh::PREFIXES) {
                for declare in self.objects_of(&prefixes_node, sh::DECLARE) {
                    let prefix = self
                        .first_object_of(&declare, sh::PREFIX)
                        .and_then(term_value);
                    let namespace = self
                        .first_object_of(&declare, sh::NAMESPACE)
                        .and_then(term_value);
                    if let (Some(p), Some(ns)) = (prefix, namespace) {
                        map.insert(p, ns); // sh:prefixes overrides the document fallback
                    }
                }
            }
        }
        let mut header = String::new();
        for (prefix, namespace) in map {
            header.push_str(&format!("PREFIX {prefix}: <{namespace}>\n"));
        }
        header
    }

    /// Parse a top-level node shape.
    fn parse_node_shape(&mut self, id: Term) -> Result<Shape, String> {
        let id_str = id.to_string();

        // Guard against recursive `sh:node` cycles
        if self.in_flight.contains(&id_str) {
            // Return a minimal stand-in to break the cycle; cyclic shapes are
            // unusual but not forbidden.
            return Ok(Shape {
                id,
                targets: vec![],
                constraints: vec![],
                property_shapes: vec![],
                severity: Severity::Violation,
                message: None,
                deactivated: false,
                box_roles: vec![],
            });
        }
        self.in_flight.insert(id_str.clone());
        let result = self.parse_shape_inner(&id);
        self.in_flight.remove(&id_str);
        result
    }

    /// Inner parse logic shared between top-level and anonymous/inline shapes.
    fn parse_shape_inner(&mut self, id: &Term) -> Result<Shape, String> {
        let id_str = id.to_string();
        let preds = self.predicates_of(id);

        // -- Check for unsupported predicates FIRST (hard-fail) --
        for (pred, _obj) in &preds {
            let iri = pred.as_str();
            if self.unsupported.contains(iri) {
                return Err(format!("unsupported SHACL term <{iri}> on shape {id_str}"));
            }
        }

        // -- Severity --
        let severity = self
            .first_object_of(id, sh::SEVERITY)
            .and_then(|t| match &t {
                Term::NamedNode(n) => Severity::from_iri(n.as_str()),
                _ => None,
            })
            .unwrap_or(Severity::Violation);

        // -- Message (take first by stable sort of string representation) --
        let mut messages: Vec<String> = self
            .objects_of(id, sh::MESSAGE)
            .into_iter()
            .filter_map(|t| match t {
                Term::Literal(lit) => Some(lit.value().to_owned()),
                _ => None,
            })
            .collect();
        messages.sort();
        let message = messages.into_iter().next();

        // -- Deactivated --
        let deactivated = self
            .first_object_of(id, sh::DEACTIVATED)
            .map(|t| match &t {
                Term::Literal(lit) => lit.value() == "true",
                _ => false,
            })
            .unwrap_or(false);

        // -- Targets (only for top-level node shapes; anonymous shapes have none) --
        let targets = self.parse_targets(id)?;

        // -- Property shapes (via sh:property) --
        let mut property_shape_nodes: Vec<Term> = self.objects_of(id, sh::PROPERTY);
        property_shape_nodes.sort_by_key(|t| t.to_string());
        let mut property_shapes = Vec::new();
        for ps_node in property_shape_nodes {
            property_shapes.push(self.parse_property_shape(&ps_node)?);
        }

        // -- Node-level constraints --
        let constraints = self.parse_constraints(id)?;
        let box_roles = self.box_roles_of(id);

        Ok(Shape {
            id: id.clone(),
            targets,
            constraints,
            property_shapes,
            severity,
            message,
            deactivated,
            box_roles,
        })
    }

    /// Parse all target declarations for a shape node.
    fn parse_targets(&self, id: &Term) -> Result<Vec<Target>, String> {
        let mut targets: Vec<Target> = Vec::new();

        // sh:targetClass
        let mut tc: Vec<NamedNode> = self
            .objects_of(id, sh::TARGET_CLASS)
            .into_iter()
            .filter_map(|t| match t {
                Term::NamedNode(n) => Some(n),
                _ => None,
            })
            .collect();
        tc.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        for n in tc {
            targets.push(Target::Class(n));
        }

        // sh:targetSubjectsOf
        let mut tso: Vec<NamedNode> = self
            .objects_of(id, sh::TARGET_SUBJECTS_OF)
            .into_iter()
            .filter_map(|t| match t {
                Term::NamedNode(n) => Some(n),
                _ => None,
            })
            .collect();
        tso.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        for n in tso {
            targets.push(Target::SubjectsOf(n));
        }

        // sh:targetObjectsOf
        let mut too: Vec<NamedNode> = self
            .objects_of(id, sh::TARGET_OBJECTS_OF)
            .into_iter()
            .filter_map(|t| match t {
                Term::NamedNode(n) => Some(n),
                _ => None,
            })
            .collect();
        too.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        for n in too {
            targets.push(Target::ObjectsOf(n));
        }

        // sh:targetNode
        let mut tn: Vec<Term> = self.objects_of(id, sh::TARGET_NODE);
        tn.sort_by_key(|t| t.to_string());
        for t in tn {
            targets.push(Target::Node(t));
        }

        // Implicit class target: shape node is itself typed rdfs:Class
        if let Term::NamedNode(_) = id {
            let is_rdfs_class = self
                .store
                .quads_for_pattern(
                    Self::term_as_subject_ref(id),
                    Some(rdf::TYPE),
                    Some(Term::NamedNode(rdfs::CLASS.into()).as_ref()),
                    None,
                )
                .flatten()
                .next()
                .is_some();
            if is_rdfs_class {
                targets.push(Target::ImplicitClass(id.clone()));
            }
        }

        // sh:target — SHACL-AF extension targets.  Only sh:SPARQLTarget is
        // supported; any other rdf:type on the target blank node is a hard error.
        let mut sparql_targets: Vec<Term> = self.objects_of(id, sh::TARGET);
        sparql_targets.sort_by_key(|t| t.to_string());
        for t_node in sparql_targets {
            // Require rdf:type sh:SPARQLTarget on the target blank node.
            let is_sparql_target = self
                .store
                .quads_for_pattern(
                    Self::term_as_subject_ref(&t_node),
                    Some(rdf::TYPE),
                    Some(Term::NamedNode(sh::SPARQL_TARGET.into()).as_ref()),
                    None,
                )
                .flatten()
                .next()
                .is_some();
            if !is_sparql_target {
                return Err(format!(
                    "unsupported sh:target type on shape {id}: target node {t_node} \
                     is not typed sh:SPARQLTarget"
                ));
            }

            // sh:select is required on the SPARQLTarget blank node.
            let raw_select = self
                .first_object_of(&t_node, sh::SELECT)
                .and_then(|t| match t {
                    Term::Literal(lit) => Some(lit.value().to_owned()),
                    _ => None,
                })
                .ok_or_else(|| {
                    format!("sh:SPARQLTarget on shape {id} is missing a sh:select string literal")
                })?;
            // SHACL-AF sh:prefixes may be declared on the shape or the target node.
            let select = format!("{}{raw_select}", self.prefix_header(&[id, &t_node]));

            // Parse-time query validation (hard-fail on unparsable queries).
            let _ = oxigraph::sparql::SparqlEvaluator::new()
                .parse_query(&select)
                .map_err(|e| {
                    format!("sh:SPARQLTarget on shape {id} has an unparsable sh:select query: {e}")
                })?;

            targets.push(Target::Sparql(select));
        }

        Ok(targets)
    }

    /// Parse node-level (non-path-scoped) constraints from a shape node.
    ///
    /// Does NOT include `sh:property` sub-shapes (handled separately).
    fn parse_constraints(&mut self, id: &Term) -> Result<Vec<Constraint>, String> {
        let mut constraints: Vec<Constraint> = Vec::new();

        // sh:class — sorted for determinism
        let mut classes: Vec<NamedNode> = self
            .objects_of(id, sh::CLASS)
            .into_iter()
            .filter_map(|t| match t {
                Term::NamedNode(n) => Some(n),
                _ => None,
            })
            .collect();
        classes.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        for n in classes {
            constraints.push(Constraint::Class(n));
        }

        // sh:datatype
        let mut datatypes: Vec<NamedNode> = self
            .objects_of(id, sh::DATATYPE)
            .into_iter()
            .filter_map(|t| match t {
                Term::NamedNode(n) => Some(n),
                _ => None,
            })
            .collect();
        datatypes.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        for n in datatypes {
            constraints.push(Constraint::Datatype(n));
        }

        // sh:nodeKind
        for t in self.objects_of(id, sh::NODE_KIND) {
            if let Term::NamedNode(n) = &t {
                let nk = parse_node_kind(n.as_str())
                    .ok_or_else(|| format!("unknown sh:nodeKind value <{}> on {id}", n.as_str()))?;
                constraints.push(Constraint::NodeKind(nk));
            }
        }

        // sh:minCount
        for t in self.objects_of(id, sh::MIN_COUNT) {
            let v = parse_u64(&t).ok_or_else(|| {
                format!("sh:minCount value is not a non-negative integer on {id}")
            })?;
            constraints.push(Constraint::MinCount(v));
        }

        // sh:maxCount
        for t in self.objects_of(id, sh::MAX_COUNT) {
            let v = parse_u64(&t).ok_or_else(|| {
                format!("sh:maxCount value is not a non-negative integer on {id}")
            })?;
            constraints.push(Constraint::MaxCount(v));
        }

        // sh:minLength
        for t in self.objects_of(id, sh::MIN_LENGTH) {
            let v = parse_u64(&t).ok_or_else(|| {
                format!("sh:minLength value is not a non-negative integer on {id}")
            })?;
            constraints.push(Constraint::MinLength(v));
        }

        // sh:uniqueLang
        for t in self.objects_of(id, sh::UNIQUE_LANG) {
            if let Term::Literal(lit) = &t {
                let flag = lit.value() == "true";
                constraints.push(Constraint::UniqueLang(flag));
            }
        }

        // sh:minInclusive / sh:maxInclusive
        let mut min_inc: Vec<Term> = self.objects_of(id, sh::MIN_INCLUSIVE);
        min_inc.sort_by_key(|t| t.to_string());
        for t in min_inc {
            constraints.push(Constraint::MinInclusive(t));
        }

        let mut max_inc: Vec<Term> = self.objects_of(id, sh::MAX_INCLUSIVE);
        max_inc.sort_by_key(|t| t.to_string());
        for t in max_inc {
            constraints.push(Constraint::MaxInclusive(t));
        }

        // sh:minExclusive / sh:maxExclusive
        let mut min_exc: Vec<Term> = self.objects_of(id, sh::MIN_EXCLUSIVE);
        min_exc.sort_by_key(|t| t.to_string());
        for t in min_exc {
            constraints.push(Constraint::MinExclusive(t));
        }

        let mut max_exc: Vec<Term> = self.objects_of(id, sh::MAX_EXCLUSIVE);
        max_exc.sort_by_key(|t| t.to_string());
        for t in max_exc {
            constraints.push(Constraint::MaxExclusive(t));
        }

        // sh:hasValue
        let mut hv: Vec<Term> = self.objects_of(id, sh::HAS_VALUE);
        hv.sort_by_key(|t| t.to_string());
        for t in hv {
            constraints.push(Constraint::HasValue(t));
        }

        // sh:in
        let mut in_lists: Vec<Term> = self.objects_of(id, sh::IN);
        in_lists.sort_by_key(|t| t.to_string());
        for list_head in in_lists {
            let items = self.walk_rdf_list(&list_head, id)?;
            constraints.push(Constraint::In(items));
        }

        // sh:pattern + optional sh:flags
        let mut patterns: Vec<String> = self
            .objects_of(id, sh::PATTERN)
            .into_iter()
            .filter_map(|t| match t {
                Term::Literal(lit) => Some(lit.value().to_owned()),
                _ => None,
            })
            .collect();
        patterns.sort();
        let flags_val: Option<String> = self
            .objects_of(id, sh::FLAGS)
            .into_iter()
            .filter_map(|t| match t {
                Term::Literal(lit) => Some(lit.value().to_owned()),
                _ => None,
            })
            .min(); // take the lexicographically smallest if multiple
        for regex in patterns {
            constraints.push(Constraint::Pattern {
                regex,
                flags: flags_val.clone(),
            });
        }

        // sh:and / sh:or / sh:xone — each is an RDF list of shape nodes
        let mut and_lists: Vec<Term> = self.objects_of(id, sh::AND);
        and_lists.sort_by_key(|t| t.to_string());
        for list_head in and_lists {
            let members = self.parse_shape_list(&list_head, id)?;
            constraints.push(Constraint::And(members));
        }

        let mut or_lists: Vec<Term> = self.objects_of(id, sh::OR);
        or_lists.sort_by_key(|t| t.to_string());
        for list_head in or_lists {
            let members = self.parse_shape_list(&list_head, id)?;
            constraints.push(Constraint::Or(members));
        }

        let mut xone_lists: Vec<Term> = self.objects_of(id, sh::XONE);
        xone_lists.sort_by_key(|t| t.to_string());
        for list_head in xone_lists {
            let members = self.parse_shape_list(&list_head, id)?;
            constraints.push(Constraint::Xone(members));
        }

        // sh:node
        let mut node_refs: Vec<Term> = self.objects_of(id, sh::NODE);
        node_refs.sort_by_key(|t| t.to_string());
        for node_ref in node_refs {
            let inner = self.parse_node_shape(node_ref)?;
            constraints.push(Constraint::Node(Box::new(inner)));
        }

        // sh:sparql — SHACL-AF SPARQL constraint components.
        // The blank node may or may not carry rdf:type sh:SPARQLConstraint;
        // we require only sh:select (which must be a SELECT query).
        let mut sparql_cnodes: Vec<Term> = self.objects_of(id, sh::SPARQL);
        sparql_cnodes.sort_by_key(|t| t.to_string());
        for c_node in sparql_cnodes {
            // sh:select is required.
            let raw_select = self
                .first_object_of(&c_node, sh::SELECT)
                .and_then(|t| match t {
                    Term::Literal(lit) => Some(lit.value().to_owned()),
                    _ => None,
                })
                .ok_or_else(|| {
                    format!(
                        "sh:sparql constraint on shape {id} is missing a sh:select string literal"
                    )
                })?;
            // SHACL-AF sh:prefixes may be declared on the shape or the sh:sparql node.
            let select = format!("{}{raw_select}", self.prefix_header(&[id, &c_node]));

            // Parse-time query validation (hard-fail on unparsable queries).
            let _ = oxigraph::sparql::SparqlEvaluator::new()
                .parse_query(&select)
                .map_err(|e| {
                    format!(
                        "sh:sparql constraint on shape {id} has an unparsable sh:select query: {e}"
                    )
                })?;

            // Optional per-constraint sh:message override.
            let mut messages: Vec<String> = self
                .objects_of(&c_node, sh::MESSAGE)
                .into_iter()
                .filter_map(|t| match t {
                    Term::Literal(lit) => Some(lit.value().to_owned()),
                    _ => None,
                })
                .collect();
            messages.sort();
            let message = messages.into_iter().next();

            // Optional per-constraint sh:severity override.
            let severity = self
                .first_object_of(&c_node, sh::SEVERITY)
                .and_then(|t| match &t {
                    Term::NamedNode(n) => Severity::from_iri(n.as_str()),
                    _ => None,
                });

            constraints.push(Constraint::Sparql {
                select,
                message,
                severity,
            });
        }

        Ok(constraints)
    }

    /// Parse a property shape node.
    fn parse_property_shape(&mut self, ps_node: &Term) -> Result<PropertyShape, String> {
        let ps_str = ps_node.to_string();

        // Check for unsupported predicates first
        for (pred, _) in self.predicates_of(ps_node) {
            let iri = pred.as_str();
            if self.unsupported.contains(iri) {
                return Err(format!(
                    "unsupported SHACL term <{iri}> on property shape {ps_str}"
                ));
            }
        }

        // sh:path is required
        let path_node = self
            .first_object_of(ps_node, sh::PATH)
            .ok_or_else(|| format!("property shape {ps_str} missing sh:path"))?;

        let path = self.parse_path(&path_node, ps_node)?;

        // severity
        let severity = self
            .first_object_of(ps_node, sh::SEVERITY)
            .and_then(|t| match &t {
                Term::NamedNode(n) => Severity::from_iri(n.as_str()),
                _ => None,
            })
            .unwrap_or(Severity::Violation);

        // message
        let mut messages: Vec<String> = self
            .objects_of(ps_node, sh::MESSAGE)
            .into_iter()
            .filter_map(|t| match t {
                Term::Literal(lit) => Some(lit.value().to_owned()),
                _ => None,
            })
            .collect();
        messages.sort();
        let message = messages.into_iter().next();

        // constraints on the property shape
        let constraints = self.parse_constraints(ps_node)?;
        let box_roles = self.box_roles_of(ps_node);
        let reification_required = self
            .objects_of(ps_node, sh::REIFICATION_REQUIRED)
            .into_iter()
            .any(|t| matches!(t, Term::Literal(lit) if lit.value() == "true"));

        let mut reifier_shape_nodes: Vec<Term> = self.objects_of(ps_node, sh::REIFIER_SHAPE);
        reifier_shape_nodes.sort_by_key(|t| t.to_string());
        if (!reifier_shape_nodes.is_empty() || reification_required)
            && !matches!(path, Path::Predicate(_))
        {
            return Err(format!(
                "sh:reifierShape or sh:reificationRequired on property shape {ps_str} requires an IRI sh:path"
            ));
        }
        let mut reifier_shapes = Vec::new();
        for node in reifier_shape_nodes {
            reifier_shapes.push(self.parse_node_shape(node)?);
        }

        Ok(PropertyShape {
            path,
            constraints,
            reifier_shapes,
            reification_required,
            severity,
            message,
            box_roles,
        })
    }

    /// Parse an `sh:path` value into a [`Path`].
    fn parse_path(&self, path_node: &Term, shape_id: &Term) -> Result<Path, String> {
        match path_node {
            Term::NamedNode(nn) => Ok(Path::Predicate(nn.clone())),
            Term::BlankNode(_) => {
                // Check for unsupported path forms first
                for unsupported_path_pred in [
                    sh::ALTERNATIVE_PATH,
                    sh::ZERO_OR_MORE_PATH,
                    sh::ONE_OR_MORE_PATH,
                    sh::ZERO_OR_ONE_PATH,
                ] {
                    if self
                        .first_object_of(path_node, unsupported_path_pred)
                        .is_some()
                    {
                        return Err(format!(
                            "unsupported SHACL term <{}> on shape {shape_id}",
                            unsupported_path_pred.as_str()
                        ));
                    }
                }

                // sh:inversePath
                if let Some(inner) = self.first_object_of(path_node, sh::INVERSE_PATH) {
                    let inner_path = self.parse_path(&inner, shape_id)?;
                    return Ok(Path::Inverse(Box::new(inner_path)));
                }

                // RDF-list sequence paths: if the bnode has rdf:first it's a sequence
                if self.first_object_of(path_node, rdf::FIRST).is_some() {
                    return Err(format!(
                        "unsupported SHACL sequence path on shape {shape_id}"
                    ));
                }

                Err(format!(
                    "unrecognised sh:path blank node structure on shape {shape_id}"
                ))
            }
            _ => Err(format!(
                "sh:path on shape {shape_id} must be an IRI or blank node, got {path_node}"
            )),
        }
    }

    /// Walk an RDF list (`rdf:first`/`rdf:rest`/`rdf:nil`) and collect items.
    fn walk_rdf_list(&self, head: &Term, shape_id: &Term) -> Result<Vec<Term>, String> {
        let nil = Term::NamedNode(NamedNode::from(rdf::NIL));
        let mut items = Vec::new();
        let mut current = head.clone();
        let mut seen: HashSet<String> = HashSet::new();

        loop {
            if current == nil {
                break;
            }
            let key = current.to_string();
            if seen.contains(&key) {
                return Err(format!("cyclic RDF list on shape {shape_id}"));
            }
            seen.insert(key);

            if let Some(first) = self.first_object_of(&current, rdf::FIRST) {
                items.push(first);
            }
            match self.first_object_of(&current, rdf::REST) {
                Some(rest) => current = rest,
                None => break,
            }
        }
        Ok(items)
    }

    /// Walk an RDF list of shape nodes, parsing each as an anonymous shape.
    ///
    /// Members that carry `sh:path` are treated as single-property inline shapes
    /// (the path+constraints go into a `property_shapes` entry); otherwise the
    /// constraints are node-level.
    fn parse_shape_list(&mut self, head: &Term, shape_id: &Term) -> Result<Vec<Shape>, String> {
        let items = self.walk_rdf_list(head, shape_id)?;
        let mut shapes = Vec::new();
        for item in items {
            let shape = self.parse_inline_shape(item)?;
            shapes.push(shape);
        }
        Ok(shapes)
    }

    /// Parse an inline / anonymous shape (member of `sh:and`, `sh:or`, etc.).
    ///
    /// If the node has `sh:path`, it is treated as an inline property shape:
    /// the path+constraints are wrapped into a single `PropertyShape` entry and
    /// the resulting `Shape` has empty node-level constraints.
    fn parse_inline_shape(&mut self, id: Term) -> Result<Shape, String> {
        let id_str = id.to_string();

        // Guard against cycles
        if self.in_flight.contains(&id_str) {
            return Ok(Shape {
                id,
                targets: vec![],
                constraints: vec![],
                property_shapes: vec![],
                severity: Severity::Violation,
                message: None,
                deactivated: false,
                box_roles: vec![],
            });
        }

        // Check for unsupported predicates
        for (pred, _) in self.predicates_of(&id) {
            let iri = pred.as_str();
            if self.unsupported.contains(iri) {
                return Err(format!(
                    "unsupported SHACL term <{iri}> on inline shape {id_str}"
                ));
            }
        }

        if self.first_object_of(&id, sh::PATH).is_some() {
            // Treat as an inline property shape
            self.in_flight.insert(id_str.clone());
            let ps = self.parse_property_shape(&id);
            self.in_flight.remove(&id_str);
            let ps = ps?;
            Ok(Shape {
                id,
                targets: vec![],
                constraints: vec![],
                property_shapes: vec![ps],
                severity: Severity::Violation,
                message: None,
                deactivated: false,
                box_roles: vec![],
            })
        } else {
            self.parse_node_shape(id)
        }
    }
}

// ── Helper functions ───────────────────────────────────────────────────────────

/// Parse `sh:nodeKind` object IRI into a [`NodeKindValue`].
fn parse_node_kind(iri: &str) -> Option<NodeKindValue> {
    match iri {
        "http://www.w3.org/ns/shacl#IRI" => Some(NodeKindValue::Iri),
        "http://www.w3.org/ns/shacl#BlankNode" => Some(NodeKindValue::BlankNode),
        "http://www.w3.org/ns/shacl#Literal" => Some(NodeKindValue::Literal),
        "http://www.w3.org/ns/shacl#BlankNodeOrIRI" => Some(NodeKindValue::BlankNodeOrIri),
        "http://www.w3.org/ns/shacl#BlankNodeOrLiteral" => Some(NodeKindValue::BlankNodeOrLiteral),
        "http://www.w3.org/ns/shacl#IRIOrLiteral" => Some(NodeKindValue::IriOrLiteral),
        _ => None,
    }
}

/// Parse a typed integer literal or plain literal integer into a `u64`.
fn parse_u64(term: &Term) -> Option<u64> {
    if let Term::Literal(lit) = term {
        lit.value().parse::<u64>().ok()
    } else {
        None
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::io::RdfFormat;

    fn load_store(ttl: &str) -> Store {
        let store = Store::new().expect("store creation is infallible");
        store
            .load_from_reader(RdfFormat::Turtle, ttl.as_bytes())
            .expect("Turtle parse error");
        store
    }

    const PREFIXES: &str = r#"
        @prefix sh:   <http://www.w3.org/ns/shacl#> .
        @prefix ex:   <http://example.org/ns#> .
        @prefix rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        @prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .
    "#;

    // ── Test 1: targetClass + sh:property with minCount/maxCount ──────────────

    #[test]
    fn test_target_class_and_property_min_max_count() {
        let ttl = format!(
            r#"{PREFIXES}
            ex:PersonShape a sh:NodeShape ;
                sh:targetClass ex:Person ;
                sh:property [
                    sh:path ex:name ;
                    sh:minCount 1 ;
                    sh:maxCount 1 ;
                ] .
        "#
        );
        let store = load_store(&ttl);
        let shapes = from_store(&store).expect("parse must succeed");

        assert_eq!(shapes.node_shapes.len(), 1, "expected exactly 1 node shape");
        let shape = &shapes.node_shapes[0];

        // Must have one Class target pointing at ex:Person
        assert_eq!(shape.targets.len(), 1);
        match &shape.targets[0] {
            Target::Class(nn) => {
                assert_eq!(nn.as_str(), "http://example.org/ns#Person");
            }
            other => panic!("expected Target::Class, got {other:?}"),
        }

        // Must have one property shape
        assert_eq!(shape.property_shapes.len(), 1);
        let ps = &shape.property_shapes[0];

        // Path must be Predicate(ex:name)
        match &ps.path {
            Path::Predicate(nn) => {
                assert_eq!(nn.as_str(), "http://example.org/ns#name");
            }
            other => panic!("expected Path::Predicate, got {other:?}"),
        }

        // Constraints must include MinCount(1) and MaxCount(1)
        let has_min = ps
            .constraints
            .iter()
            .any(|c| matches!(c, Constraint::MinCount(1)));
        let has_max = ps
            .constraints
            .iter()
            .any(|c| matches!(c, Constraint::MaxCount(1)));
        assert!(has_min, "expected MinCount(1)");
        assert!(has_max, "expected MaxCount(1)");
    }

    // ── Test 2: sh:or with two property-path members ──────────────────────────

    #[test]
    fn test_or_with_two_predicate_path_members() {
        let ttl = format!(
            r#"{PREFIXES}
            ex:AltShape a sh:NodeShape ;
                sh:or (
                    [ sh:path ex:a ; sh:minCount 1 ]
                    [ sh:path ex:b ; sh:minCount 1 ]
                ) .
        "#
        );
        let store = load_store(&ttl);
        let shapes = from_store(&store).expect("parse must succeed");

        assert_eq!(shapes.node_shapes.len(), 1);
        let shape = &shapes.node_shapes[0];

        // Must have exactly one Or constraint
        let or_constraints: Vec<&Vec<Shape>> = shape
            .constraints
            .iter()
            .filter_map(|c| match c {
                Constraint::Or(members) => Some(members),
                _ => None,
            })
            .collect();
        assert_eq!(or_constraints.len(), 1, "expected exactly one sh:or");

        let members = or_constraints[0];
        assert_eq!(members.len(), 2, "expected two members in sh:or");

        // Each member should be an inline property shape with one property_shape entry
        for (i, member) in members.iter().enumerate() {
            assert_eq!(
                member.property_shapes.len(),
                1,
                "member {i} should have one property shape"
            );
            let ps = &member.property_shapes[0];
            let has_min = ps
                .constraints
                .iter()
                .any(|c| matches!(c, Constraint::MinCount(1)));
            assert!(has_min, "member {i} property shape should have MinCount(1)");
        }
    }

    // ── Test 3: sh:inversePath parses to Path::Inverse ────────────────────────

    #[test]
    fn test_inverse_path() {
        let ttl = format!(
            r#"{PREFIXES}
            ex:InverseShape a sh:NodeShape ;
                sh:targetClass ex:Child ;
                sh:property [
                    sh:path [ sh:inversePath ex:parent ] ;
                    sh:minCount 1 ;
                ] .
        "#
        );
        let store = load_store(&ttl);
        let shapes = from_store(&store).expect("parse must succeed");

        assert_eq!(shapes.node_shapes.len(), 1);
        let shape = &shapes.node_shapes[0];
        assert_eq!(shape.property_shapes.len(), 1);
        let ps = &shape.property_shapes[0];

        match &ps.path {
            Path::Inverse(inner) => match inner.as_ref() {
                Path::Predicate(nn) => {
                    assert_eq!(nn.as_str(), "http://example.org/ns#parent");
                }
                other => panic!("expected inner Predicate, got {other:?}"),
            },
            other => panic!("expected Path::Inverse, got {other:?}"),
        }
    }

    // ── Test 4a: sh:sparql with valid query → parses successfully ────────────────

    #[test]
    fn test_sparql_constraint_parses() {
        // The sh:select value is a self-contained SPARQL query using full IRIs
        // (no prefix declarations needed) so the SPARQL parser can validate it.
        let ttl = format!(
            r#"{PREFIXES}
            ex:SparqlShape a sh:NodeShape ;
                sh:targetClass ex:Foo ;
                sh:sparql [
                    sh:select "SELECT $this WHERE {{ $this <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/ns#Foo> . }}" ;
                ] .
        "#
        );
        let store = load_store(&ttl);
        let shapes = from_store(&store).expect("sh:sparql with a valid query must parse");

        assert_eq!(shapes.node_shapes.len(), 1, "expected exactly 1 node shape");
        let shape = &shapes.node_shapes[0];

        let sparql_c = shape.constraints.iter().find_map(|c| match c {
            Constraint::Sparql { select, .. } => Some(select.as_str()),
            _ => None,
        });
        assert!(
            sparql_c.is_some(),
            "expected a Constraint::Sparql, got: {:?}",
            shape.constraints
        );
        assert!(
            sparql_c.unwrap().contains("$this"),
            "select text should contain '$this'"
        );
    }

    // ── sh:namespace as a bare IRI (NamedNode), not an xsd:anyURI literal ────────
    #[test]
    fn test_sparql_prefixes_namespace_as_iri() {
        // SHACL §5.2.1 permits sh:namespace to be an IRI (NamedNode); the PREFIX
        // line must still be injected so the prefixed query name resolves.
        let ttl = format!(
            r#"{PREFIXES}
            ex:NsDecls sh:declare [ sh:prefix "ex" ; sh:namespace <http://example.org/ns#> ] .
            ex:PrefShape a sh:NodeShape ;
                sh:targetClass ex:Foo ;
                sh:prefixes ex:NsDecls ;
                sh:sparql [
                    sh:select "SELECT $this WHERE {{ $this a ex:Foo . }}" ;
                ] .
        "#
        );
        let store = load_store(&ttl);
        let shapes = from_store(&store).expect("IRI-valued sh:namespace must parse");
        let shape = &shapes.node_shapes[0];
        let select = shape
            .constraints
            .iter()
            .find_map(|c| match c {
                Constraint::Sparql { select, .. } => Some(select.clone()),
                _ => None,
            })
            .expect("expected a Constraint::Sparql");
        assert!(
            select.contains("PREFIX ex: <http://example.org/ns#>"),
            "IRI-valued sh:namespace must inject a PREFIX line; got: {select}"
        );
    }

    // ── Test 4b: sh:sparql with malformed query → Err at parse time ──────────────

    #[test]
    fn test_sparql_constraint_malformed_query_errs() {
        let ttl = format!(
            r#"{PREFIXES}
            ex:BadShape a sh:NodeShape ;
                sh:targetClass ex:Foo ;
                sh:sparql [
                    sh:select "SELECT $this WHERE {{" ;
                ] .
        "#
        );
        let store = load_store(&ttl);
        let result = from_store(&store);
        assert!(
            result.is_err(),
            "a malformed sh:select must cause a hard parse-time error, got {result:?}"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("unparsable") || err.contains("parse") || err.contains("syntax"),
            "error message should indicate a query parse failure, got: {err}"
        );
    }

    // ── Test 5: sh:qualifiedValueShape → Err ──────────────────────────────────

    #[test]
    fn test_qualified_value_shape_returns_err() {
        let ttl = format!(
            r#"{PREFIXES}
            ex:QShape a sh:NodeShape ;
                sh:targetClass ex:Bar ;
                sh:property [
                    sh:path ex:item ;
                    sh:qualifiedValueShape [ sh:class ex:Item ] ;
                    sh:qualifiedMinCount 1 ;
                ] .
        "#
        );
        let store = load_store(&ttl);
        let result = from_store(&store);
        assert!(
            result.is_err(),
            "sh:qualifiedValueShape must cause a hard error"
        );
    }

    // ── Test 6: severity, message, deactivated metadata ───────────────────────

    #[test]
    fn test_shape_metadata() {
        let ttl = format!(
            r#"{PREFIXES}
            ex:MetaShape a sh:NodeShape ;
                sh:targetClass ex:Thing ;
                sh:severity sh:Warning ;
                sh:message "This is a warning" ;
                sh:deactivated true .
        "#
        );
        let store = load_store(&ttl);
        let shapes = from_store(&store).expect("parse must succeed");
        assert_eq!(shapes.node_shapes.len(), 1);
        let shape = &shapes.node_shapes[0];
        assert_eq!(shape.severity, Severity::Warning);
        assert_eq!(shape.message.as_deref(), Some("This is a warning"));
        assert!(shape.deactivated);
    }

    // ── Test 7: sh:in list ─────────────────────────────────────────────────────

    #[test]
    fn test_in_list_constraint() {
        let ttl = format!(
            r#"{PREFIXES}
            ex:InShape a sh:NodeShape ;
                sh:targetClass ex:Color ;
                sh:property [
                    sh:path ex:value ;
                    sh:in ( "red" "green" "blue" ) ;
                ] .
        "#
        );
        let store = load_store(&ttl);
        let shapes = from_store(&store).expect("parse must succeed");
        assert_eq!(shapes.node_shapes.len(), 1);
        let ps = &shapes.node_shapes[0].property_shapes[0];
        let in_constraint = ps.constraints.iter().find_map(|c| match c {
            Constraint::In(items) => Some(items),
            _ => None,
        });
        assert!(in_constraint.is_some(), "expected In constraint");
        assert_eq!(in_constraint.unwrap().len(), 3);
    }

    // ── Test 8: sh:nodeKind ────────────────────────────────────────────────────

    #[test]
    fn test_node_kind_iri() {
        let ttl = format!(
            r#"{PREFIXES}
            ex:IriShape a sh:NodeShape ;
                sh:targetClass ex:Resource ;
                sh:nodeKind sh:IRI .
        "#
        );
        let store = load_store(&ttl);
        let shapes = from_store(&store).expect("parse must succeed");
        let shape = &shapes.node_shapes[0];
        let has_nk = shape
            .constraints
            .iter()
            .any(|c| matches!(c, Constraint::NodeKind(NodeKindValue::Iri)));
        assert!(has_nk, "expected NodeKind(Iri)");
    }

    // ── Test 9: sh:pattern + sh:flags ─────────────────────────────────────────

    #[test]
    fn test_pattern_with_flags() {
        let ttl = format!(
            r#"{PREFIXES}
            ex:PatShape a sh:NodeShape ;
                sh:targetClass ex:Code ;
                sh:property [
                    sh:path ex:code ;
                    sh:pattern "^[A-Z]+" ;
                    sh:flags "i" ;
                ] .
        "#
        );
        let store = load_store(&ttl);
        let shapes = from_store(&store).expect("parse must succeed");
        let ps = &shapes.node_shapes[0].property_shapes[0];
        let pat = ps.constraints.iter().find_map(|c| match c {
            Constraint::Pattern { regex, flags } => Some((regex, flags)),
            _ => None,
        });
        assert!(pat.is_some(), "expected Pattern constraint");
        let (regex, flags) = pat.unwrap();
        assert_eq!(regex.as_str(), "^[A-Z]+");
        assert_eq!(flags.as_deref(), Some("i"));
    }

    // ── Test 10: sh:and ────────────────────────────────────────────────────────

    #[test]
    fn test_and_constraint() {
        let ttl = format!(
            r#"{PREFIXES}
            ex:AndShape a sh:NodeShape ;
                sh:targetClass ex:Entity ;
                sh:and (
                    [ sh:class ex:Named ]
                    [ sh:nodeKind sh:IRI ]
                ) .
        "#
        );
        let store = load_store(&ttl);
        let shapes = from_store(&store).expect("parse must succeed");
        let shape = &shapes.node_shapes[0];
        let and_c = shape.constraints.iter().find_map(|c| match c {
            Constraint::And(members) => Some(members),
            _ => None,
        });
        assert!(and_c.is_some(), "expected And constraint");
        assert_eq!(and_c.unwrap().len(), 2);
    }

    // ── Test 11: unsupported path form → Err ──────────────────────────────────

    #[test]
    fn test_zero_or_more_path_returns_err() {
        let ttl = format!(
            r#"{PREFIXES}
            ex:StarShape a sh:NodeShape ;
                sh:targetClass ex:Node ;
                sh:property [
                    sh:path [ sh:zeroOrMorePath ex:link ] ;
                    sh:minCount 0 ;
                ] .
        "#
        );
        let store = load_store(&ttl);
        let result = from_store(&store);
        assert!(
            result.is_err(),
            "sh:zeroOrMorePath must cause a hard error, got {result:?}"
        );
    }

    #[test]
    fn test_reifier_shape_requires_predicate_path() {
        let ttl = format!(
            r#"{PREFIXES}
            ex:ContextualShape a sh:NodeShape ;
                sh:targetClass ex:Node ;
                sh:property [
                    sh:path [ sh:inversePath ex:knows ] ;
                    sh:reifierShape ex:StatementContextShape ;
                ] .

            ex:StatementContextShape a sh:NodeShape .
        "#
        );
        let store = load_store(&ttl);
        let result = from_store(&store);
        assert!(
            result.is_err(),
            "sh:reifierShape on a non-IRI path must cause a hard error"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("requires an IRI sh:path"),
            "error should document the supported path boundary, got: {err}"
        );
    }

    #[test]
    fn test_reification_required_requires_predicate_path() {
        let ttl = format!(
            r#"{PREFIXES}
            ex:ContextualShape a sh:NodeShape ;
                sh:targetClass ex:Node ;
                sh:property [
                    sh:path [ sh:inversePath ex:knows ] ;
                    sh:reificationRequired true ;
                ] .
        "#
        );
        let store = load_store(&ttl);
        let result = from_store(&store);
        assert!(
            result.is_err(),
            "sh:reificationRequired on a non-IRI path must cause a hard error"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("sh:reifierShape or sh:reificationRequired")
                && err.contains("requires an IRI sh:path"),
            "error should document the supported path boundary, got: {err}"
        );
    }
}
