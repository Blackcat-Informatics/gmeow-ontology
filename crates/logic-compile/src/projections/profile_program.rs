// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit projection capability products. Query views alone do not carry the
//! mandatory selected-source checks, native identities or schema premises.

use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
use sha2::{Digest, Sha256};

use super::grounding_catalog::{
    GroundingCatalog, LiteralProjectionSupport, PropertyPremise, error,
};
use super::selected_templates::SelectedTemplate;

const BINDING_DOMAIN: purrdf_hash::Domain = purrdf_hash::Domain::new(b"projection-binding-v1");
const INPUT_CHECK_DOMAIN: purrdf_hash::Domain =
    purrdf_hash::Domain::new(b"projection-input-check-v1");
const PROFILE_DOMAIN: purrdf_hash::Domain = purrdf_hash::Domain::new(b"projection-profile-v1");

/// Realized get-leg body retained independently of a profile's execution product.
/// The source pattern occurs once; both forward construction and every input
/// check are derived from that same native algebra. The template query has an
/// empty body and therefore cannot smuggle in a second match restriction.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectionBindingProgram {
    pub id: String,
    pub correspondence: String,
    pub binding_key: String,
    pub source_mapping: String,
    pub profile: String,
    pub order: usize,
    pub source_pattern: String,
    pub template: String,
    pub selected: Vec<SelectedTemplate>,
    pub lossy_drops: Vec<String>,
}

impl ProjectionBindingProgram {
    pub(crate) fn new(
        cell: &super::get_leg::ProjectionCell,
        binding: &super::get_leg::ProjectionBinding,
        correspondence: String,
        order: usize,
        pattern: &purrdf::sparql::GraphPattern,
        template: &[purrdf::sparql::TriplePattern],
    ) -> gmeow_errors::Result<Self> {
        use purrdf::sparql::{GraphPattern, QuadPattern, Query, QueryDataset};
        let mut value = Self {
            id: String::new(),
            correspondence,
            binding_key: super::get_leg::binding_key(cell, binding),
            source_mapping: cell.iri.clone(),
            profile: binding.profile.clone(),
            order,
            source_pattern: purrdf::sparql::try_pattern_to_select_query_with_options(
                pattern,
                &purrdf::sparql::ParserOptions::default(),
            )
            .map_err(|why| {
                error(format!(
                    "native binding source cannot be represented: {why}"
                ))
            })?,
            template: super::sparql::native::emit(Query::Construct {
                template: template
                    .iter()
                    .cloned()
                    .map(|triple| QuadPattern {
                        triple,
                        graph: None,
                    })
                    .collect(),
                pattern: GraphPattern::Bgp {
                    patterns: Vec::new(),
                },
                dataset: QueryDataset::default(),
                base_iri: None,
                version: None,
            })?,
            selected: binding.selected_templates.clone(),
            lossy_drops: binding.lossy_drops.clone(),
        };
        value.id = value.expected_id();
        Ok(value)
    }

    fn expected_id(&self) -> String {
        let mut value = self.clone();
        value.id.clear();
        identity(BINDING_DOMAIN, &value)
    }

    /// Decode only the existing native grammar, with no execution or alternate
    /// query parser. The canonical source body supplies both consumers.
    pub(crate) fn body(
        &self,
    ) -> gmeow_errors::Result<(
        Vec<purrdf::sparql::TriplePattern>,
        purrdf::sparql::GraphPattern,
    )> {
        use purrdf::sparql::{
            GraphPattern, NamedNodePattern, Query, QueryDataset, SparqlParser, TermPattern,
        };
        if self.id != self.expected_id()
            || self.profile.is_empty()
            || self.source_mapping.is_empty()
        {
            return Err(error("invalid realized projection binding identity"));
        }
        let Query::Select {
            pattern,
            dataset,
            base_iri: None,
            version: None,
        } = SparqlParser::new().parse_query(&self.source_pattern)?
        else {
            return Err(error("realized binding requires a native source SELECT"));
        };
        if dataset != QueryDataset::default() {
            return Err(error("realized source cannot override the input dataset"));
        }
        let GraphPattern::ProjectStar { inner: pattern, .. } = pattern else {
            return Err(error(
                "realized binding source requires the complete wildcard source solution",
            ));
        };
        let Query::Construct {
            template,
            pattern: template_body,
            dataset,
            base_iri: None,
            version: None,
        } = SparqlParser::new().parse_query(&self.template)?
        else {
            return Err(error("realized binding requires a native template"));
        };
        if dataset != QueryDataset::default()
            || !matches!(template_body, GraphPattern::Bgp { patterns } if patterns.is_empty())
            || template.iter().any(|quad| quad.graph.is_some())
        {
            return Err(error(
                "realized template requires an empty default-graph body",
            ));
        }
        for selected in &self.selected {
            let subject =
                super::sparql::binding_variable(&self.binding_key, &selected.subject_variable);
            let object =
                super::sparql::binding_variable(&self.binding_key, &selected.object_variable);
            if !template.iter().any(|quad| matches!(&quad.triple.subject, TermPattern::Variable(value) if value.as_str() == subject)
                && matches!(&quad.triple.predicate, NamedNodePattern::NamedNode(value) if value.as_str() == selected.support.target_property)
                && matches!(&quad.triple.object, TermPattern::Variable(value) if value.as_str() == object)) {
                return Err(error("realized selected support has no matching literal-preserving output template"));
            }
        }
        Ok((
            template.into_iter().map(|quad| quad.triple).collect(),
            pattern.into_inner(),
        ))
    }
}

/// Reconstruct a profile from its independently retained, ordered get-leg IR.
/// Transported forward text and check lists never establish their own roster.
pub fn profile_from_bindings(
    profile: &str,
    bindings: &[ProjectionBindingProgram],
    catalog_id: &str,
) -> gmeow_errors::Result<ProjectionProfileProgram> {
    let mut bindings: Vec<_> = bindings
        .iter()
        .filter(|binding| binding.profile == profile)
        .collect();
    bindings.sort_by_key(|binding| binding.order);
    let mut get = super::sparql::native::LegBuilder::default();
    let mut checks = Vec::new();
    let mut drops = Vec::new();
    for (index, binding) in bindings.iter().enumerate() {
        if binding.order != index {
            return Err(error(
                "realized projection binding order is missing or duplicated",
            ));
        }
        let (template, pattern) = binding.body()?;
        get.push(&template, &pattern);
        let mut occurrences = std::collections::BTreeSet::new();
        for selected in &binding.selected {
            if selected.profile != profile || !occurrences.insert(selected.template_index) {
                return Err(error("conflicting realized selected-template roster"));
            }
            checks.push(ProjectionInputCheck::new(
                binding.correspondence.clone(),
                binding.binding_key.clone(),
                binding.id.clone(),
                binding.source_mapping.clone(),
                binding.source_pattern.clone(),
                selected.clone(),
            ));
        }
        for drop in &binding.lossy_drops {
            if !drops.contains(drop) {
                drops.push(drop.clone());
            }
        }
    }
    let get = get.finish().ok_or_else(|| {
        error(format!(
            "no realized bindings for required profile {profile}"
        ))
    })?;
    let drops = if drops.is_empty() {
        ".".to_owned()
    } else {
        format!("; drops: {}.", drops.join("; "))
    };
    let forward = format!(
        "# Projection: GMEOW → pure {profile}. {}\n# Lossy and directional by design{drops}\n{}",
        super::sparql::GENERATED_BANNER,
        super::sparql::native::emit(get)?
    );
    Ok(ProjectionProfileProgram::new(
        profile.to_owned(),
        forward,
        checks,
        catalog_id.to_owned(),
    ))
}

/// A native SELECT of exactly the forward binding's lowered source pattern.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectionInputCheck {
    pub id: String,
    pub binding_iri: String,
    pub binding_key: String,
    pub binding_program: String,
    pub source_mapping: String,
    pub query: String,
    pub selected: SelectedTemplate,
}

/// The complete capability payload bound into the program identity.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectionProfilePayload {
    pub catalog_id: String,
    pub profile: String,
    pub forward: String,
    pub checks: Vec<ProjectionInputCheck>,
}

/// A mandatory forward program and all of its mandatory input checks.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectionProfileProgram {
    pub id: String,
    pub payload: ProjectionProfilePayload,
}

fn identity<T: ToJson>(domain: purrdf_hash::Domain, value: &T) -> String {
    let mut digest = Sha256::new();
    digest.update(domain.as_bytes());
    digest.update([0]);
    digest.update(purrdf_lex::json::record::to_vec(value));
    format!(
        "urn:gmeow:{}:{}",
        domain.as_str(),
        purrdf_hash::hex::encode(&digest.finalize())
    )
}

impl ProjectionInputCheck {
    /// Construct from the compiler's admitted binding and the native serializer
    /// over its unchanged lowered source pattern.
    pub fn new(
        binding_iri: String,
        binding_key: String,
        binding_program: String,
        source_mapping: String,
        query: String,
        selected: SelectedTemplate,
    ) -> Self {
        let mut value = Self {
            id: String::new(),
            binding_iri,
            binding_key,
            binding_program,
            source_mapping,
            query,
            selected,
        };
        value.id = value.expected_id();
        value
    }

    fn expected_id(&self) -> String {
        let mut payload = self.clone();
        payload.id.clear();
        identity(INPUT_CHECK_DOMAIN, &payload)
    }

    /// Object variable after the binding's existing hygienic native lowering.
    pub fn checked_variable(&self) -> String {
        super::sparql::binding_variable(&self.binding_key, &self.selected.object_variable)
    }

    /// The subject column uses the same native hygiene owner as the body.
    pub fn checked_subject(&self) -> String {
        super::sparql::binding_variable(&self.binding_key, &self.selected.subject_variable)
    }

    /// Reconstruct support against the required catalog. The transported JSON
    /// is not itself evidence of an admitted native correspondence or property.
    pub fn validate(&self, catalog: &GroundingCatalog) -> gmeow_errors::Result<()> {
        if self.id != self.expected_id() {
            return Err(error("projection input-check identity mismatch"));
        }
        let expected = catalog.literal_support(
            &self.selected.support.alias,
            &self.selected.support.source_property,
        )?;
        if expected != self.selected.support {
            return Err(error(format!(
                "{}: selected native identity or canonical property premises differ from required catalog",
                self.id
            )));
        }
        let (_, digest) = self
            .binding_key
            .rsplit_once("binding=")
            .ok_or_else(|| error("missing source binding digest"))?;
        if digest.len() != 64 || !digest.bytes().all(|value| value.is_ascii_hexdigit()) {
            return Err(error("malformed source binding digest"));
        }
        let expected_binding = format!(
            "https://blackcatinformatics.ca/logic/correspondence/projection-mapping/{digest}"
        );
        if self.binding_iri != expected_binding
            || !catalog
                .program()
                .correspondences
                .iter()
                .any(|cell| cell.iri == self.binding_iri)
        {
            return Err(error(format!(
                "{}: missing or conflicting canonical binding identity",
                self.id
            )));
        }
        let bindings: Vec<_> = catalog
            .program()
            .projection_bindings
            .iter()
            .filter(|binding| binding.correspondence == self.binding_iri)
            .collect();
        let [binding] = bindings.as_slice() else {
            return Err(error(
                "selected input check requires exactly one realized binding body",
            ));
        };
        if binding.binding_key != self.binding_key
            || binding.id != self.binding_program
            || binding.source_mapping != self.source_mapping
            || binding.source_pattern != self.query
            || !binding.selected.contains(&self.selected)
            || !catalog.program().correspondences.iter().any(|cell| {
                cell.iri == self.binding_iri && cell.get_leg.as_deref() == Some(binding.id.as_str())
            })
        {
            return Err(error(
                "selected input check differs from its independently retained get-leg body",
            ));
        }
        if self.query.trim().is_empty()
            || self.selected.subject_variable.is_empty()
            || self.selected.object_variable.is_empty()
        {
            return Err(error(format!(
                "{}: missing required query or variable",
                self.id
            )));
        }
        let query = purrdf::sparql::SparqlParser::new()
            .parse_query(&self.query)
            .map_err(|why| {
                error(format!(
                    "{}: malformed native source SELECT: {why}",
                    self.id
                ))
            })?;
        let purrdf::sparql::Query::Select {
            pattern,
            dataset,
            base_iri: None,
            version: None,
        } = query
        else {
            return Err(error(format!(
                "{}: input check requires the native default-dataset SELECT capability",
                self.id
            )));
        };
        if dataset != purrdf::sparql::QueryDataset::default() {
            return Err(error(
                "selected source check cannot override its input dataset",
            ));
        }
        let subject =
            super::sparql::binding_variable(&self.binding_key, &self.selected.subject_variable);
        if !mandatory_source(
            &pattern,
            &subject,
            &self.selected.support.source_property,
            &self.checked_variable(),
            0,
        )? {
            return Err(error(format!(
                "{}: native SELECT does not prove its required source-property/variable flow",
                self.id
            )));
        }
        Ok(())
    }
}

impl ProjectionProfileProgram {
    pub fn new(
        profile: String,
        forward: String,
        checks: Vec<ProjectionInputCheck>,
        catalog_id: String,
    ) -> Self {
        let mut checks = checks;
        checks.sort_by(|a, b| a.id.cmp(&b.id));
        let payload = ProjectionProfilePayload {
            catalog_id,
            profile,
            forward,
            checks,
        };
        Self {
            id: identity(PROFILE_DOMAIN, &payload),
            payload,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        purrdf_lex::json::record::to_vec(self)
    }

    /// Decode duplicate-sensitive native records and admit all capabilities.
    pub fn from_bytes(bytes: &[u8], catalog: &GroundingCatalog) -> gmeow_errors::Result<Self> {
        let value: Self = purrdf_lex::json::record::from_slice(bytes)
            .map_err(|e| error(format!("invalid projection program: {e}")))?;
        value.validate(catalog)?;
        Ok(value)
    }

    pub fn validate(&self, catalog: &GroundingCatalog) -> gmeow_errors::Result<()> {
        if self.id != identity(PROFILE_DOMAIN, &self.payload)
            || self.payload.profile.is_empty()
            || self.payload.forward.trim().is_empty()
            || self.payload.catalog_id != catalog.identity()
        {
            return Err(error(
                "projection profile identity or required forward capability is invalid",
            ));
        }
        let expected = profile_from_bindings(
            &self.payload.profile,
            &catalog.program().projection_bindings,
            catalog.identity(),
        )?;
        if expected != *self {
            return Err(error(format!(
                "{}: forward query or required checks differ from admitted catalog (extra, missing, duplicate or swapped capability)",
                self.payload.profile
            )));
        }
        for check in &self.payload.checks {
            check.validate(catalog)?;
        }
        let query = purrdf::sparql::SparqlParser::new()
            .parse_query(&self.payload.forward)
            .map_err(|why| {
                error(format!(
                    "{}: malformed native forward query: {why}",
                    self.id
                ))
            })?;
        if !matches!(query, purrdf::sparql::Query::Construct { .. }) {
            return Err(error("projection forward capability must be CONSTRUCT"));
        }
        Ok(())
    }
}

impl FromJson for ProjectionBindingProgram {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "realized projection binding")?;
        let result = Self {
            id: fields.required("id")?,
            correspondence: fields.required("correspondence")?,
            binding_key: fields.required("binding_key")?,
            source_mapping: fields.required("source_mapping")?,
            profile: fields.required("profile")?,
            order: fields.required("order")?,
            source_pattern: fields.required("source_pattern")?,
            template: fields.required("template")?,
            selected: fields.required("selected")?,
            lossy_drops: fields.required("lossy_drops")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
impl ToJson for ProjectionBindingProgram {
    fn to_json(&self) -> Value {
        purrdf_lex::json::Object::new()
            .with("id", self.id.to_json())
            .with("correspondence", self.correspondence.to_json())
            .with("binding_key", self.binding_key.to_json())
            .with("source_mapping", self.source_mapping.to_json())
            .with("profile", self.profile.to_json())
            .with("order", self.order.to_json())
            .with("source_pattern", self.source_pattern.to_json())
            .with("template", self.template.to_json())
            .with("selected", self.selected.to_json())
            .with("lossy_drops", self.lossy_drops.to_json())
            .into()
    }
}

// This proves one declared mandatory source occurrence in the existing native
// algebra. OPTIONAL/right-side MINUS/EXISTS never supply a required witness; no
// query is evaluated or approximated by this admission walk.
fn mandatory_source(
    pattern: &purrdf::sparql::GraphPattern,
    subject: &str,
    predicate: &str,
    object: &str,
    depth: usize,
) -> gmeow_errors::Result<bool> {
    use purrdf::sparql::{GraphPattern as P, NamedNodePattern, TermPattern};
    if depth >= 256 {
        return Err(error(
            "selected source-pattern proof exceeds native admission depth",
        ));
    }
    let descend = |pattern| mandatory_source(pattern, subject, predicate, object, depth + 1);
    Ok(match pattern {
        P::Bgp { patterns } => patterns.iter().any(|triple|
            matches!(&triple.subject, TermPattern::Variable(value) if value.as_str() == subject)
                && matches!(&triple.predicate, NamedNodePattern::NamedNode(value) if value.as_str() == predicate)
                && matches!(&triple.object, TermPattern::Variable(value) if value.as_str() == object)),
        P::Join { left, right } => descend(left)? || descend(right)?,
        P::LeftJoin { left, .. } | P::Minus { left, .. } => descend(left)?,
        P::Filter { inner, .. } | P::Extend { inner, .. } | P::Project { inner, .. } | P::ProjectStar { inner, .. }
            | P::Distinct { inner } | P::Reduced { inner } | P::OrderBy { inner, .. } | P::Slice { inner, .. } => descend(inner)?,
        P::Union { arms } => {
            let mut present = true;
            for arm in arms { present &= descend(arm)?; }
            present
        },
        _ => false,
    })
}

impl FromJson for PropertyPremise {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "canonical property premise")?;
        let result = Self {
            subject: fields.required("subject")?,
            predicate: fields.required("predicate")?,
            object: fields.required("object")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
impl ToJson for PropertyPremise {
    fn to_json(&self) -> Value {
        purrdf_lex::json::Object::new()
            .with("subject", self.subject.to_json())
            .with("predicate", self.predicate.to_json())
            .with("object", self.object.to_json())
            .into()
    }
}

impl FromJson for LiteralProjectionSupport {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "selected literal support")?;
        let result = Self {
            alias: fields.required("alias")?,
            correspondence: fields.required("correspondence")?,
            correspondence_key: fields.required("correspondence_key")?,
            source_property: fields.required("source_property")?,
            target_property: fields.required("target_property")?,
            owner: fields.required("owner")?,
            premises: fields.required("premises")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
impl ToJson for LiteralProjectionSupport {
    fn to_json(&self) -> Value {
        purrdf_lex::json::Object::new()
            .with("alias", self.alias.to_json())
            .with("correspondence", self.correspondence.to_json())
            .with("correspondence_key", self.correspondence_key.to_json())
            .with("source_property", self.source_property.to_json())
            .with("target_property", self.target_property.to_json())
            .with("owner", self.owner.to_json())
            .with("premises", self.premises.to_json())
            .into()
    }
}

impl FromJson for SelectedTemplate {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "selected template")?;
        let result = Self {
            profile: fields.required("profile")?,
            template_index: fields.required("template_index")?,
            subject_variable: fields.required("subject_variable")?,
            object_variable: fields.required("object_variable")?,
            support: fields.required("support")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
impl ToJson for SelectedTemplate {
    fn to_json(&self) -> Value {
        purrdf_lex::json::Object::new()
            .with("profile", self.profile.to_json())
            .with("template_index", self.template_index.to_json())
            .with("subject_variable", self.subject_variable.to_json())
            .with("object_variable", self.object_variable.to_json())
            .with("support", self.support.to_json())
            .into()
    }
}

impl FromJson for ProjectionInputCheck {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "projection input check")?;
        let result = Self {
            id: fields.required("id")?,
            binding_iri: fields.required("binding_iri")?,
            binding_key: fields.required("binding_key")?,
            binding_program: fields.required("binding_program")?,
            source_mapping: fields.required("source_mapping")?,
            query: fields.required("query")?,
            selected: fields.required("selected")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
impl ToJson for ProjectionInputCheck {
    fn to_json(&self) -> Value {
        purrdf_lex::json::Object::new()
            .with("id", self.id.to_json())
            .with("binding_iri", self.binding_iri.to_json())
            .with("binding_key", self.binding_key.to_json())
            .with("binding_program", self.binding_program.to_json())
            .with("source_mapping", self.source_mapping.to_json())
            .with("query", self.query.to_json())
            .with("selected", self.selected.to_json())
            .into()
    }
}

impl FromJson for ProjectionProfilePayload {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "projection profile payload")?;
        let result = Self {
            catalog_id: fields.required("catalog_id")?,
            profile: fields.required("profile")?,
            forward: fields.required("forward")?,
            checks: fields.required("checks")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
impl ToJson for ProjectionProfilePayload {
    fn to_json(&self) -> Value {
        purrdf_lex::json::Object::new()
            .with("catalog_id", self.catalog_id.to_json())
            .with("profile", self.profile.to_json())
            .with("forward", self.forward.to_json())
            .with("checks", self.checks.to_json())
            .into()
    }
}

impl FromJson for ProjectionProfileProgram {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "projection profile program")?;
        let result = Self {
            id: fields.required("id")?,
            payload: fields.required("payload")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
impl ToJson for ProjectionProfileProgram {
    fn to_json(&self) -> Value {
        purrdf_lex::json::Object::new()
            .with("id", self.id.to_json())
            .with("payload", self.payload.to_json())
            .into()
    }
}
