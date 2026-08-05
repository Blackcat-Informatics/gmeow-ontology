// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A typed OntoUML metamodel model plus a native Turtle reader for the
//! FAIR OntoUML/UFO catalog serialization.
//!
//! The reader dogfoods the native `purrdf::parse_dataset` Turtle codec (never a
//! second parser) and lifts the metamodel's `ontouml:` vocabulary into three
//! typed collections — stereotyped classes, generalizations, and mediation
//! relations — that the lowerer projects onto the world-scoped `logic:`
//! stereotype ABox the foundation-discipline chase consumes.
//!
//! Fragment boundary (no-optionality / hard-fail):
//!
//! * A **malformed** serialization (a Turtle parse failure, or a generalization
//!   missing an end) is an [`OntoumlError::Syntax`] — a hard parse failure.
//! * A **well-formed but out-of-fragment** construct (a stereotype outside the
//!   five supported disciplines, or a mediation whose ends the walk cannot
//!   resolve to a relator and a mediated relatum) is an
//!   [`OntoumlError::Unsupported`] — an honest capability gap the caller records
//!   as a DlGap ledger row, never a silently-swallowed pass. An unrecognized
//!   stereotype is recorded verbatim at parse time and surfaces its gap only at
//!   *lower* time, where the stereotype→`logic:` map is applied.

use std::collections::BTreeMap;

use purrdf::{TermRef, parse_dataset};

/// The OntoUML metamodel vocabulary namespace (local names appended verbatim).
pub const ONTOUML_NS: &str = "https://w3id.org/ontouml#";
/// The `logic:` vocabulary namespace the foundation chase reads.
pub use gmeow_ns::LOGIC_NS;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// A parse/lower outcome error, split so the caller can route it correctly.
///
/// Mirrors [`TptpError`](crate::external::tptp::TptpError) semantics exactly:
/// [`Syntax`](OntoumlError::Syntax) is a malformed source (a hard failure);
/// [`Unsupported`](OntoumlError::Unsupported) is a well-formed construct outside
/// the supported five-discipline fragment (an honest capability gap, never a
/// silent success).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OntoumlError {
    /// The source is malformed OntoUML — a hard parse failure (no recovery).
    Syntax(String),
    /// The source is well-formed but uses a construct outside the fragment the
    /// native foundation disciplines can carry. The caller records this as a
    /// capability gap (DlGap), never a silent `incomplete`.
    Unsupported(String),
}

impl std::fmt::Display for OntoumlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OntoumlError::Syntax(m) => write!(f, "OntoUML syntax error: {m}"),
            OntoumlError::Unsupported(m) => {
                write!(f, "OntoUML construct outside the native fragment: {m}")
            }
        }
    }
}

impl std::error::Error for OntoumlError {}

/// Map an OntoUML stereotype local name to its `logic:` stereotype local name.
///
/// The nine sortal/non-sortal/relator stereotypes the five native disciplines
/// range over map through; any other stereotype (`quantity`, `collective`,
/// `mode`, `quality`, `type`, `event`, `situation`, `historicalRole`, …) is an
/// honest [`OntoumlError::Unsupported`] gap — it is outside the discipline
/// fragment, never silently dropped.
pub fn logic_local_for_stereotype(ontouml_local: &str) -> Result<&'static str, OntoumlError> {
    let mapped = match ontouml_local {
        "kind" => "Kind",
        "subkind" => "SubKind",
        "role" => "Role",
        "phase" => "Phase",
        "category" => "Category",
        "mixin" => "Mixin",
        "roleMixin" => "RoleMixin",
        "phaseMixin" => "PhaseMixin",
        "relator" => "Relator",
        other => {
            return Err(OntoumlError::Unsupported(format!(
                "stereotype `{other}` is outside the five-discipline fragment \
                 (only kind/subkind/role/phase/category/mixin/roleMixin/phaseMixin/relator \
                 are carried)"
            )));
        }
    };
    Ok(mapped)
}

/// A stereotyped OntoUML class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntoClass {
    /// The class IRI.
    pub iri: String,
    /// The OntoUML stereotype local names asserted on this class. Usually one;
    /// zero or two-or-more are permitted and are exactly what the
    /// StereotypeCardinality discipline detects — passed through, never rejected
    /// at parse time.
    pub stereotypes: Vec<String>,
}

/// An OntoUML generalization (subclass) edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generalization {
    /// The more general (super) class IRI.
    pub general: String,
    /// The more specific (sub) class IRI.
    pub specific: String,
}

/// A mediation relation: the relator-end class, the mediated-end class IRIs, and
/// whether the mediated end is functional (upper bound 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mediation {
    /// The mediation relation IRI.
    pub relation_iri: String,
    /// The relator-end class IRI.
    pub relator: String,
    /// The mediated-end class IRIs.
    pub mediated: Vec<String>,
    /// `true` iff the mediated end is functional (upper-bound-1); a single
    /// functional mediated relatum is exactly the RelComp anti-pattern shape.
    pub functional: bool,
}

/// A parsed OntoUML model: the three typed collections the lowerer projects.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OntoumlModel {
    /// The stereotyped classes.
    pub classes: Vec<OntoClass>,
    /// The generalization edges.
    pub generalizations: Vec<Generalization>,
    /// The mediation relations.
    pub mediations: Vec<Mediation>,
}

/// Return the local name of an IRI: the tail after the `ONTOUML_NS` prefix when
/// present, else the tail after the last `#` or `/`.
fn local_name(iri: &str) -> &str {
    if let Some(local) = iri.strip_prefix(ONTOUML_NS) {
        return local;
    }
    if let Some(pos) = iri.rfind(['#', '/']) {
        &iri[pos + 1..]
    } else {
        iri
    }
}

/// The per-subject accumulator used while walking the dataset.
#[derive(Default)]
struct Row {
    /// The `rdf:type` object IRIs asserted on this subject.
    types: Vec<String>,
    /// The `ontouml:stereotype` object IRIs asserted on this subject.
    stereotypes: Vec<String>,
    /// `ontouml:general` (a generalization's super end).
    general: Option<String>,
    /// `ontouml:specific` (a generalization's sub end).
    specific: Option<String>,
    /// `ontouml:relationEnd` / `ontouml:property` — property-node IRIs (shape a).
    property_nodes: Vec<String>,
    /// A property node's `ontouml:propertyType` class IRI (shape a, one hop).
    property_type: Option<String>,
    /// A property node's `ontouml:cardinality` `ontouml:Cardinality` node IRI
    /// (shape a, one hop from the property to its cardinality node).
    cardinality: Option<String>,
    /// A cardinality node's `ontouml:upperBound` literal lexical form
    /// (`"1"`, `"*"`, …); `"1"` marks a functional end.
    upper_bound: Option<String>,
    /// `ontouml:relatorEnd` class IRI (shape b convenience predicate).
    relator_end: Option<String>,
    /// `ontouml:mediatedEnd` class IRIs (shape b convenience predicate).
    mediated_ends: Vec<String>,
    /// `ontouml:functionalMediation` boolean flag (shape b convenience predicate).
    functional: bool,
    /// `true` iff a `ontouml:relationEnd`/`ontouml:property` object was a blank
    /// node the walk cannot resolve to a property-type IRI (shape a).
    has_blank_end: bool,
}

/// Parse a FAIR OntoUML/UFO catalog Turtle serialization into a typed
/// [`OntoumlModel`].
///
/// Dogfoods `purrdf::parse_dataset`; a Turtle parse failure is an
/// [`OntoumlError::Syntax`]. A generalization missing either end is a
/// [`OntoumlError::Syntax`]. A mediation relation whose ends the walk cannot
/// resolve to a relator and a mediated relatum — including a blank-node property
/// structure the one-hop walk cannot follow — is an [`OntoumlError::Unsupported`]
/// capability gap.
///
/// All three collections are sorted deterministically by IRI before returning.
pub fn parse_ontouml_model(source: &str, base: Option<&str>) -> Result<OntoumlModel, OntoumlError> {
    let ds = parse_dataset(source.as_bytes(), "text/turtle", base)
        .map_err(|e| OntoumlError::Syntax(format!("OntoUML Turtle parse failed: {e}")))?;

    let type_class = format!("{ONTOUML_NS}Class");
    let type_generalization = format!("{ONTOUML_NS}Generalization");
    let type_relation = format!("{ONTOUML_NS}Relation");
    let mediation_iri = format!("{ONTOUML_NS}mediation");

    let p_stereotype = format!("{ONTOUML_NS}stereotype");
    let p_general = format!("{ONTOUML_NS}general");
    let p_specific = format!("{ONTOUML_NS}specific");
    let p_relation_end = format!("{ONTOUML_NS}relationEnd");
    let p_property = format!("{ONTOUML_NS}property");
    let p_property_type = format!("{ONTOUML_NS}propertyType");
    let p_cardinality = format!("{ONTOUML_NS}cardinality");
    let p_upper_bound = format!("{ONTOUML_NS}upperBound");
    let p_relator_end = format!("{ONTOUML_NS}relatorEnd");
    let p_mediated_end = format!("{ONTOUML_NS}mediatedEnd");
    let p_functional = format!("{ONTOUML_NS}functionalMediation");

    let mut rows: BTreeMap<String, Row> = BTreeMap::new();

    for q in ds.quad_refs() {
        let TermRef::Iri(subj) = q.s else { continue };
        let TermRef::Iri(pred) = q.p else { continue };
        let row = rows.entry(subj.to_owned()).or_default();

        if pred == RDF_TYPE {
            if let TermRef::Iri(t) = q.o {
                row.types.push(t.to_owned());
            }
        } else if pred == p_stereotype {
            if let TermRef::Iri(o) = q.o {
                row.stereotypes.push(o.to_owned());
            }
        } else if pred == p_general {
            if let TermRef::Iri(o) = q.o {
                row.general = Some(o.to_owned());
            }
        } else if pred == p_specific {
            if let TermRef::Iri(o) = q.o {
                row.specific = Some(o.to_owned());
            }
        } else if pred == p_relation_end || pred == p_property {
            match q.o {
                TermRef::Iri(o) => row.property_nodes.push(o.to_owned()),
                // A blank-node property structure the one-hop walk cannot resolve.
                _ => row.has_blank_end = true,
            }
        } else if pred == p_property_type {
            if let TermRef::Iri(o) = q.o {
                row.property_type = Some(o.to_owned());
            }
        } else if pred == p_cardinality {
            if let TermRef::Iri(o) = q.o {
                row.cardinality = Some(o.to_owned());
            }
        } else if pred == p_upper_bound {
            if let TermRef::Literal { lexical, .. } = q.o {
                row.upper_bound = Some(lexical.to_owned());
            }
        } else if pred == p_relator_end {
            if let TermRef::Iri(o) = q.o {
                row.relator_end = Some(o.to_owned());
            }
        } else if pred == p_mediated_end {
            if let TermRef::Iri(o) = q.o {
                row.mediated_ends.push(o.to_owned());
            }
        } else if pred == p_functional
            && let TermRef::Literal { lexical, .. } = q.o
            && lexical == "true"
        {
            row.functional = true;
        }
    }

    // Classes: every ontouml:Class subject, with its stereotype local names.
    let mut classes = Vec::new();
    for (iri, row) in &rows {
        if !row.types.iter().any(|t| t == &type_class) {
            continue;
        }
        let mut stereotypes: Vec<String> = row
            .stereotypes
            .iter()
            .map(|s| local_name(s).to_owned())
            .collect();
        stereotypes.sort();
        stereotypes.dedup();
        classes.push(OntoClass {
            iri: iri.clone(),
            stereotypes,
        });
    }

    // A class → its stereotype local names, for relator-end resolution below.
    let class_stereos: BTreeMap<&str, &[String]> = classes
        .iter()
        .map(|c| (c.iri.as_str(), c.stereotypes.as_slice()))
        .collect();

    // Generalizations: every ontouml:Generalization node; both ends required.
    let mut generalizations = Vec::new();
    for (iri, row) in &rows {
        if !row.types.iter().any(|t| t == &type_generalization) {
            continue;
        }
        let (Some(general), Some(specific)) = (&row.general, &row.specific) else {
            return Err(OntoumlError::Syntax(format!(
                "generalization {iri} is missing an ontouml:general or ontouml:specific end"
            )));
        };
        generalizations.push(Generalization {
            general: general.clone(),
            specific: specific.clone(),
        });
    }

    // Mediations: every ontouml:Relation with an ontouml:stereotype of
    // ontouml:mediation, its ends resolved via shape (a) or shape (b).
    let mut mediations = Vec::new();
    for (iri, row) in &rows {
        if !row.types.iter().any(|t| t == &type_relation) {
            continue;
        }
        if !row.stereotypes.iter().any(|s| s == &mediation_iri) {
            continue;
        }
        let mediation = resolve_mediation(iri, row, &rows, &class_stereos)?;
        mediations.push(mediation);
    }

    classes.sort_by(|a, b| a.iri.cmp(&b.iri));
    generalizations.sort_by(|a, b| (&a.specific, &a.general).cmp(&(&b.specific, &b.general)));
    mediations.sort_by(|a, b| a.relation_iri.cmp(&b.relation_iri));

    Ok(OntoumlModel {
        classes,
        generalizations,
        mediations,
    })
}

/// Resolve one mediation relation's ends into a typed [`Mediation`].
///
/// Supports shape (a) — the relation carries `ontouml:relationEnd`/`ontouml:property`
/// property nodes each linking to an `ontouml:propertyType` class — and shape (b),
/// the self-authored `ontouml:relatorEnd`/`ontouml:mediatedEnd` convenience
/// predicates. A relation with no identifiable relator end, no mediated end, or a
/// blank-node property structure the one-hop walk cannot follow is an
/// [`OntoumlError::Unsupported`] gap.
fn resolve_mediation(
    iri: &str,
    row: &Row,
    rows: &BTreeMap<String, Row>,
    class_stereos: &BTreeMap<&str, &[String]>,
) -> Result<Mediation, OntoumlError> {
    // Shape (b): direct relatorEnd / mediatedEnd convenience predicates.
    if row.relator_end.is_some() || !row.mediated_ends.is_empty() {
        let Some(relator) = &row.relator_end else {
            return Err(OntoumlError::Unsupported(format!(
                "mediation {iri} has no identifiable relator end (no ontouml:relatorEnd)"
            )));
        };
        if row.mediated_ends.is_empty() {
            return Err(OntoumlError::Unsupported(format!(
                "mediation {iri} has no mediated end (no ontouml:mediatedEnd)"
            )));
        }
        let mut mediated = row.mediated_ends.clone();
        mediated.sort();
        mediated.dedup();
        return Ok(Mediation {
            relation_iri: iri.to_owned(),
            relator: relator.clone(),
            mediated,
            functional: row.functional,
        });
    }

    // Shape (a): relationEnd / property nodes → propertyType class (one hop).
    if row.has_blank_end {
        return Err(OntoumlError::Unsupported(format!(
            "mediation {iri} uses a blank-node property structure the one-hop walk \
             cannot resolve to a propertyType IRI"
        )));
    }
    if row.property_nodes.is_empty() {
        return Err(OntoumlError::Unsupported(format!(
            "mediation {iri} declares no relation ends (no ontouml:relationEnd/property)"
        )));
    }

    // Each end is its propertyType class plus the `ontouml:upperBound` of its
    // `ontouml:cardinality` node (two hops: property → cardinality → upperBound).
    // The bound is `None` when the end declares no cardinality — an honest absence,
    // never guessed at.
    let mut end_classes: Vec<(String, Option<String>)> = Vec::new();
    for node in &row.property_nodes {
        let Some(node_row) = rows.get(node) else {
            return Err(OntoumlError::Unsupported(format!(
                "mediation {iri} property node {node} has no ontouml:propertyType"
            )));
        };
        let Some(class_iri) = &node_row.property_type else {
            return Err(OntoumlError::Unsupported(format!(
                "mediation {iri} property node {node} has no ontouml:propertyType IRI"
            )));
        };
        let upper = node_row
            .cardinality
            .as_ref()
            .and_then(|card| rows.get(card))
            .and_then(|card_row| card_row.upper_bound.clone());
        end_classes.push((class_iri.clone(), upper));
    }

    // The relator end is the propertyType class carrying a `relator` stereotype; the
    // remaining ends are the mediated relata (each carrying its own upper bound).
    let mut relator: Option<String> = None;
    let mut mediated_ends: Vec<(String, Option<String>)> = Vec::new();
    for (class_iri, upper) in &end_classes {
        let is_relator = class_stereos
            .get(class_iri.as_str())
            .is_some_and(|stereos| stereos.iter().any(|s| s == "relator"));
        if is_relator && relator.is_none() {
            relator = Some(class_iri.clone());
        } else {
            mediated_ends.push((class_iri.clone(), upper.clone()));
        }
    }

    let Some(relator) = relator else {
        return Err(OntoumlError::Unsupported(format!(
            "mediation {iri} has no identifiable relator end (no mediated propertyType \
             class carries a `relator` stereotype)"
        )));
    };
    if mediated_ends.is_empty() {
        return Err(OntoumlError::Unsupported(format!(
            "mediation {iri} has no mediated end (every end is the relator)"
        )));
    }
    let mut mediated: Vec<String> = mediated_ends.iter().map(|(c, _)| c.clone()).collect();
    mediated.sort();
    mediated.dedup();
    // Functional (the RelComp shape) iff the relator reaches a single distinct
    // mediated relatum whose end has an upper bound of exactly 1 — the real FAIR
    // catalog serialization's `ontouml:upperBound "1"`, the shape-(a) analogue of
    // the shape-(b) `ontouml:functionalMediation` convenience flag.
    let functional = mediated.len() == 1
        && mediated_ends
            .iter()
            .any(|(_, upper)| upper.as_deref() == Some("1"));
    Ok(Mediation {
        relation_iri: iri.to_owned(),
        relator,
        mediated,
        functional,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FREE_ROLE: &str = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Person a ontouml:Class ; ontouml:stereotype ontouml:kind .\n\
ex:Customer a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:Wanderer a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:g1 a ontouml:Generalization ; ontouml:general ex:Person ; ontouml:specific ex:Customer .\n";

    #[test]
    fn parses_free_role_model() {
        let m = parse_ontouml_model(FREE_ROLE, None).unwrap();
        assert_eq!(m.classes.len(), 3);
        // Deterministic sort by IRI: Customer, Person, Wanderer.
        assert_eq!(m.classes[0].iri, "https://example.org/onto/Customer");
        assert_eq!(m.classes[0].stereotypes, vec!["role".to_string()]);
        assert_eq!(m.classes[1].stereotypes, vec!["kind".to_string()]);
        assert_eq!(m.generalizations.len(), 1);
        assert_eq!(
            m.generalizations[0].general,
            "https://example.org/onto/Person"
        );
        assert_eq!(
            m.generalizations[0].specific,
            "https://example.org/onto/Customer"
        );
        assert!(m.mediations.is_empty());
    }

    #[test]
    fn parses_mediation_shape_b() {
        let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Marriage a ontouml:Class ; ontouml:stereotype ontouml:relator .\n\
ex:Spouse a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:relatorEnd ex:Marriage ; ontouml:mediatedEnd ex:Spouse ;\n\
    ontouml:functionalMediation true .\n";
        let m = parse_ontouml_model(src, None).unwrap();
        assert_eq!(m.mediations.len(), 1);
        let med = &m.mediations[0];
        assert_eq!(med.relator, "https://example.org/onto/Marriage");
        assert_eq!(
            med.mediated,
            vec!["https://example.org/onto/Spouse".to_string()]
        );
        assert!(med.functional);
    }

    #[test]
    fn parses_mediation_shape_a() {
        let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Employment a ontouml:Class ; ontouml:stereotype ontouml:relator .\n\
ex:Employee a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:Employer a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:relationEnd ex:p1 , ex:p2 , ex:p3 .\n\
ex:p1 ontouml:propertyType ex:Employment .\n\
ex:p2 ontouml:propertyType ex:Employee .\n\
ex:p3 ontouml:propertyType ex:Employer .\n";
        let m = parse_ontouml_model(src, None).unwrap();
        assert_eq!(m.mediations.len(), 1);
        let med = &m.mediations[0];
        assert_eq!(med.relator, "https://example.org/onto/Employment");
        assert_eq!(
            med.mediated,
            vec![
                "https://example.org/onto/Employee".to_string(),
                "https://example.org/onto/Employer".to_string()
            ]
        );
        assert!(!med.functional);
    }

    #[test]
    fn parses_mediation_shape_a_functional_via_cardinality() {
        // The real FAIR-catalog serialization: a mediation Relation with two
        // `ontouml:relationEnd` Property nodes, each carrying an `ontouml:cardinality`
        // → `ontouml:Cardinality` → `ontouml:upperBound`. The mediated (Spouse) end has
        // upper bound "1", so the mediation is functional (the RelComp shape) — WITHOUT
        // the self-authored `ontouml:functionalMediation` convenience flag.
        let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Marriage a ontouml:Class ; ontouml:stereotype ontouml:relator .\n\
ex:Spouse a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:relationEnd ex:pR , ex:pM .\n\
ex:pR a ontouml:Property ; ontouml:propertyType ex:Marriage ; ontouml:cardinality ex:cR .\n\
ex:cR a ontouml:Cardinality ; ontouml:lowerBound \"1\" ; ontouml:upperBound \"*\" .\n\
ex:pM a ontouml:Property ; ontouml:propertyType ex:Spouse ; ontouml:cardinality ex:cM .\n\
ex:cM a ontouml:Cardinality ; ontouml:lowerBound \"1\" ; ontouml:upperBound \"1\" .\n";
        let m = parse_ontouml_model(src, None).unwrap();
        assert_eq!(m.mediations.len(), 1);
        let med = &m.mediations[0];
        assert_eq!(med.relator, "https://example.org/onto/Marriage");
        assert_eq!(
            med.mediated,
            vec!["https://example.org/onto/Spouse".to_string()]
        );
        assert!(
            med.functional,
            "a single mediated end with ontouml:upperBound \"1\" is functional"
        );
    }

    #[test]
    fn shape_a_unbounded_mediated_end_is_not_functional() {
        // A mediated end with upper bound "*" (unbounded) is NOT functional — the
        // relator can reach many relata, so RelComp must not fire.
        let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Marriage a ontouml:Class ; ontouml:stereotype ontouml:relator .\n\
ex:Spouse a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:relationEnd ex:pR , ex:pM .\n\
ex:pR a ontouml:Property ; ontouml:propertyType ex:Marriage .\n\
ex:pM a ontouml:Property ; ontouml:propertyType ex:Spouse ; ontouml:cardinality ex:cM .\n\
ex:cM a ontouml:Cardinality ; ontouml:lowerBound \"1\" ; ontouml:upperBound \"*\" .\n";
        let m = parse_ontouml_model(src, None).unwrap();
        let med = &m.mediations[0];
        assert!(
            !med.functional,
            "an unbounded mediated end is not functional"
        );
    }

    #[test]
    fn unsupported_stereotype_surfaces_at_lower_time_not_parse_time() {
        let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Water a ontouml:Class ; ontouml:stereotype ontouml:quantity .\n";
        // Parse records the raw stereotype without complaint.
        let m = parse_ontouml_model(src, None).unwrap();
        assert_eq!(m.classes[0].stereotypes, vec!["quantity".to_string()]);
        // The gap surfaces only when the stereotype is mapped.
        let err = logic_local_for_stereotype("quantity").unwrap_err();
        assert!(matches!(err, OntoumlError::Unsupported(_)), "{err}");
    }

    #[test]
    fn mediation_without_relator_is_unsupported() {
        let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Left a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:mediatedEnd ex:Left .\n";
        let err = parse_ontouml_model(src, None).unwrap_err();
        assert!(matches!(err, OntoumlError::Unsupported(_)), "{err}");
    }

    #[test]
    fn generalization_missing_end_is_syntax() {
        let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:g1 a ontouml:Generalization ; ontouml:general ex:Person .\n";
        let err = parse_ontouml_model(src, None).unwrap_err();
        assert!(matches!(err, OntoumlError::Syntax(_)), "{err}");
    }

    #[test]
    fn malformed_turtle_is_syntax() {
        let err = parse_ontouml_model("@prefix bad <", None).unwrap_err();
        assert!(matches!(err, OntoumlError::Syntax(_)), "{err}");
    }
}
