// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit mathematical consumer projections and their source-bound loss records.
//! No target vocabulary is imported into the canonical mathematical theory.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use purrdf::{RdfDataset, RdfLiteral, RdfTerm};

use crate::Sink;
use crate::ns::{LOGIC, RDF_TYPE, math};

mod expression;
mod inbound;

pub use inbound::lift;

const LANG: &str = "https://blackcatinformatics.ca/lang/";
const SKOS_EXACT: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";
const QUDT: &str = "http://qudt.org/schema/qudt/";

/// A supported, explicitly selected external mathematical surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// MathML content markup, through the authored ExpressionRendering seam.
    MathMl,
    /// OpenMath content objects with resolved content-dictionary symbols.
    OpenMath,
    /// RDF Data Cube observations with declared statistical datasets.
    DataCube,
    /// STATO method references with declared expression anchors.
    Stato,
    /// QUDT quantity values with classified, dimension-compatible units.
    Qudt,
}

impl Target {
    /// Parse the documented CLI/registry target name.
    ///
    /// # Errors
    /// Rejects unknown target selections; no substitute format is chosen.
    pub fn parse(value: &str) -> gmeow_errors::Result<Self> {
        match value {
            "mathml" => Ok(Self::MathMl),
            "openmath" => Ok(Self::OpenMath),
            "data-cube" => Ok(Self::DataCube),
            "stato" => Ok(Self::Stato),
            "qudt" => Ok(Self::Qudt),
            _ => Err(invalid(format!("unknown mathematical target {value:?}"))),
        }
    }

    /// Stable target name used by requests and loss records.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::MathMl => "mathml",
            Self::OpenMath => "openmath",
            Self::DataCube => "data-cube",
            Self::Stato => "stato",
            Self::Qudt => "qudt",
        }
    }

    /// Consumer artifact extension.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::MathMl => "mml",
            Self::OpenMath => "omobj",
            Self::DataCube | Self::Stato | Self::Qudt => "ttl",
        }
    }
}

/// One atomic target emission and its mandatory canonical loss-ledger contribution.
pub struct Emission {
    /// Target serialization; never a partial result after a failed projection.
    pub bytes: Vec<u8>,
    /// Canonical Turtle records using math:ProjectionRecord and logic: preservation.
    pub ledger: String,
    /// Content-bound identity of this source/target emission.
    pub record: String,
}

fn invalid(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::ProjectionInput {
        detail: detail.into(),
    })
}

fn invalid_source(
    detail: impl Into<String>,
    source: impl std::error::Error + Send + Sync + 'static,
) -> gmeow_errors::Diag {
    use gmeow_errors::DiagKind;
    let kind = crate::error::ProjectionInput {
        detail: detail.into(),
    };
    let mut diag =
        gmeow_errors::Diag::new(kind.code(), kind.grade(), kind.to_string()).with_source(source);
    if let Some(class) = kind.failure_class() {
        diag = diag.with_failure_class(class);
    }
    diag
}

/// Exact native RDF terms indexed independently of serialization order.
/// Graph names remain source context and are explicitly represented in loss evidence.
struct Source {
    rows: HashMap<RdfTerm, BTreeMap<String, Vec<RdfTerm>>>,
    contexts: HashSet<RdfTerm>,
    visited: HashSet<RdfTerm>,
    used: HashSet<(RdfTerm, String)>,
    boundaries: BTreeSet<String>,
}

impl Source {
    fn new(dataset: &RdfDataset) -> Self {
        let mut value = Self {
            rows: HashMap::new(),
            contexts: HashSet::new(),
            visited: HashSet::new(),
            used: HashSet::new(),
            boundaries: BTreeSet::new(),
        };
        value.extend(dataset);
        value
    }

    fn extend(&mut self, dataset: &RdfDataset) {
        for quad in purrdf::native_quads::flat_rdf_quads(dataset) {
            if let Some(graph) = quad.graph_name {
                self.contexts.insert(graph);
            }
            let values = self
                .rows
                .entry(quad.subject)
                .or_default()
                .entry(quad.predicate)
                .or_default();
            if !values.contains(&quad.object) {
                values.push(quad.object);
            }
        }
    }

    fn values(&mut self, subject: &RdfTerm, predicate: &str) -> Vec<RdfTerm> {
        self.visited.insert(subject.clone());
        self.used.insert((subject.clone(), predicate.to_owned()));
        self.rows
            .get(subject)
            .and_then(|row| row.get(predicate))
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    }

    fn one(&mut self, subject: &RdfTerm, predicate: &str) -> gmeow_errors::Result<RdfTerm> {
        let values = self.values(subject, predicate);
        if let [value] = values.as_slice() {
            return Ok(value.clone());
        }
        Err(invalid(format!(
            "{subject:?} requires exactly one {predicate}, found {}",
            values.len()
        )))
    }

    fn iri(&mut self, subject: &RdfTerm, predicate: &str) -> gmeow_errors::Result<String> {
        match self.one(subject, predicate)? {
            RdfTerm::Iri(iri) => Ok(iri),
            other => Err(invalid(format!(
                "{predicate} requires an IRI, got {other:?}"
            ))),
        }
    }

    fn literal(&mut self, subject: &RdfTerm, predicate: &str) -> gmeow_errors::Result<RdfLiteral> {
        match self.one(subject, predicate)? {
            RdfTerm::Literal(literal) => Ok(literal),
            other => Err(invalid(format!(
                "{predicate} requires a literal, got {other:?}"
            ))),
        }
    }

    fn typed(&mut self, subject: &RdfTerm, class: &str) -> gmeow_errors::Result<()> {
        if self
            .values(subject, RDF_TYPE)
            .contains(&RdfTerm::iri(class))
        {
            return Ok(());
        }
        Err(invalid(format!("{subject:?} must be typed {class}")))
    }

    fn losses(&self) -> BTreeSet<String> {
        let mut losses = self.boundaries.clone();
        let mut pending: Vec<_> = self.visited.iter().cloned().collect();
        let mut seen = HashSet::new();
        while let Some(subject) = pending.pop() {
            if !seen.insert(subject.clone()) {
                continue;
            }
            let Some(predicates) = self.rows.get(&subject) else {
                continue;
            };
            for (predicate, objects) in predicates {
                let omitted = !self.used.contains(&(subject.clone(), predicate.clone()));
                if omitted {
                    losses.insert(predicate.clone());
                }
                if ![crate::ns::MATH, LANG, LOGIC]
                    .iter()
                    .any(|namespace| predicate.starts_with(*namespace))
                {
                    continue;
                }
                for object in objects {
                    if omitted && let RdfTerm::Iri(iri) = object {
                        losses.insert(iri.clone());
                    }
                    let Some(fields) = self.rows.get(object) else {
                        continue;
                    };
                    let vocabulary = fields.get(RDF_TYPE).is_some_and(|types| {
                        [
                            "Class",
                            "ObjectProperty",
                            "DatatypeProperty",
                            "AnnotationProperty",
                            "NamedIndividual",
                        ]
                        .iter()
                        .any(|kind| types.contains(&RdfTerm::iri(format!("{LOGIC}{kind}"))))
                    });
                    if !vocabulary {
                        pending.push(object.clone());
                    }
                }
            }
        }
        if !self.contexts.is_empty() {
            losses.insert("http://www.w3.org/ns/prov#hadPrimarySource".to_owned());
        }
        losses
    }
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Project one named canonical source with explicit target selection.
///
/// MathML selects an ExpressionRendering record, not an arbitrary display string.
/// Other targets select their native expression, observation, method or quantity.
/// Source graph/context and canonical identity omitted by a target remain named loss.
///
/// # Errors
/// Rejects missing, malformed, ambiguous or unsupported selected input. Produces no
/// artifact or partial ledger on failure.
pub fn project(
    dataset: &RdfDataset,
    source: &str,
    target: Target,
    reference: &RdfDataset,
) -> gmeow_errors::Result<Emission> {
    let mut input = Source::new(dataset);
    input.extend(reference);
    project_source(&mut input, source, target)
}

fn project_source(
    input: &mut Source,
    source: &str,
    target: Target,
) -> gmeow_errors::Result<Emission> {
    input.visited.clear();
    input.used.clear();
    input.boundaries.clear();
    let root = RdfTerm::iri(source);
    if !input.rows.contains_key(&root) {
        return Err(invalid(format!("projection source {source} is absent")));
    }
    let mut losses = BTreeSet::new();
    let bytes = match target {
        Target::MathMl => {
            input.typed(&root, &math("ExpressionRendering"))?;
            let kind = input.iri(&root, &format!("{LANG}renderingKind"))?;
            if kind != format!("{LANG}renderingNotation") {
                return Err(invalid("MathML rendering must explicitly select notation"));
            }
            let expression = input.one(&root, &format!("{LANG}renderedContent"))?;
            let convention = input.iri(&root, &format!("{LANG}renderingConvention"))?;
            let preservation = input.iri(&root, &format!("{LANG}renderingPreservation"))?;
            if convention != "http://www.w3.org/1998/Math/MathML" {
                return Err(invalid(
                    "MathML rendering must select the MathML notation convention",
                ));
            }
            if preservation != format!("{LOGIC}ExactPreservation") {
                return Err(invalid(
                    "MathML AST rendering requires an exact structural rendering contract",
                ));
            }
            let body = expression::render(input, &expression, target)?;
            losses.insert(source.to_owned());
            format!("<math xmlns=\"http://www.w3.org/1998/Math/MathML\">{body}</math>\n")
                .into_bytes()
        }
        Target::OpenMath => {
            let body = expression::render(input, &root, target)?;
            losses.insert(source.to_owned());
            format!(
                "<OMOBJ xmlns=\"http://www.openmath.org/OpenMath\" version=\"2.0\">{body}</OMOBJ>\n"
            )
            .into_bytes()
        }
        Target::Qudt => quantity(input, &root, source)?.into_bytes(),
        Target::Stato => method(input, &root, source)?.into_bytes(),
        Target::DataCube => observation(input, &root, source)?.into_bytes(),
    };
    losses.extend(input.losses());
    // Target vocabulary classes and source-scoped typing are distinct. The target
    // does not retain every canonical classification merely because it retains value.
    losses.insert(RDF_TYPE.to_owned());
    let mut identity = Vec::new();
    for loss in &losses {
        identity.extend_from_slice(
            &u64::try_from(loss.len())
                .map_err(|_| invalid("loss identity exceeds u64"))?
                .to_le_bytes(),
        );
        identity.extend_from_slice(loss.as_bytes());
    }
    for field in [
        source.as_bytes(),
        target.name().as_bytes(),
        bytes.as_slice(),
    ] {
        identity.extend_from_slice(
            &u64::try_from(field.len())
                .map_err(|_| invalid("projection identity field exceeds u64"))?
                .to_le_bytes(),
        );
        identity.extend_from_slice(field);
    }
    let digest = purrdf_hash::hex::encode(purrdf_hash::blake3::hash(&identity).as_bytes());
    let record = format!("urn:gmeow:math-projection:{digest}");
    let mut sink = Sink::new();
    sink.typed(&record, &math("ProjectionRecord"))?;
    sink.typed(&record, &format!("{LOGIC}Correspondence"))?;
    sink.iri(&record, &format!("{LOGIC}sourceEndpoint"), source)?;
    let artifact = purrdf_hash::hex::encode(purrdf_hash::blake3::hash(&bytes).as_bytes());
    sink.iri(
        &record,
        &format!("{LOGIC}targetEndpoint"),
        &format!("urn:gmeow:math-artifact:{artifact}"),
    )?;
    sink.iri(
        &record,
        &format!("{LOGIC}morphismClass"),
        &format!("{LOGIC}LossyLens"),
    )?;
    sink.iri(
        &record,
        &format!("{LOGIC}correspondenceRelation"),
        &format!("{LOGIC}RelatedMatch"),
    )?;
    sink.iri(
        &record,
        &format!("{LOGIC}hasDeterminacy"),
        &format!("{LOGIC}Crisp"),
    )?;
    sink.boolean(&record, &format!("{LOGIC}mnemomorphic"), false)?;
    sink.iri(&record, &math("projectionSource"), source)?;
    sink.string(&record, &math("projectionTargetName"), target.name())?;
    sink.iri(
        &record,
        &format!("{LOGIC}preservationKind"),
        &format!("{LOGIC}ValidationOnly"),
    )?;
    for loss in losses {
        sink.iri(&record, &format!("{LOGIC}unsupportedConstruct"), &loss)?;
    }
    Ok(Emission {
        bytes,
        ledger: sink.serialize()?,
        record,
    })
}

fn quantity(input: &mut Source, root: &RdfTerm, source: &str) -> gmeow_errors::Result<String> {
    input.typed(root, &math("Quantity"))?;
    let value = input.literal(root, &math("quantityValue"))?;
    numeric(&value)?;
    let unit = input.iri(root, "https://blackcatinformatics.ca/gmeow/unit")?;
    let unit_term = RdfTerm::iri(&unit);
    input.typed(&unit_term, &format!("{QUDT}Unit"))?;
    let dimension = input.one(root, &math("hasDimension"))?;
    let unit_dimension = input.one(&unit_term, &math("hasDimension"))?;
    let target_dimension = input.iri(&unit_term, &format!("{QUDT}hasDimensionVector"))?;
    if dimension != unit_dimension {
        return Err(invalid(
            "quantity and selected unit have incompatible dimension identities",
        ));
    }
    let mut sink = Sink::new();
    sink.typed(source, &format!("{QUDT}QuantityValue"))?;
    sink.lit(source, &format!("{QUDT}numericValue"), value)?;
    sink.iri(source, &format!("{QUDT}unit"), &unit)?;
    sink.typed(&unit, &format!("{QUDT}Unit"))?;
    sink.iri(
        &unit,
        &format!("{QUDT}hasDimensionVector"),
        &target_dimension,
    )?;
    sink.serialize()
}

fn statistical_profile(
    input: &mut Source,
    root: &RdfTerm,
) -> gmeow_errors::Result<(&'static str, &'static str)> {
    let types = input.values(root, RDF_TYPE);
    let model = types.contains(&RdfTerm::iri(math("StatisticalModel")))
        || types.contains(&RdfTerm::iri(math("FittedModel")));
    let statistic = types.contains(&RdfTerm::iri(math("Statistic")));
    let test = types.contains(&RdfTerm::iri(math("HypothesisTest")));
    match (model, statistic, test) {
        (true, false, false) => Ok(("StatisticalModel", "modelFormula")),
        (false, true, false) => Ok(("Statistic", "definingExpression")),
        (false, false, true) => Ok(("HypothesisTest", "testStatistic")),
        _ => Err(invalid(
            "STATO projection requires one unambiguous model, statistic or hypothesis-test profile",
        )),
    }
}

fn expression_anchor(
    input: &mut Source,
    expression: &str,
    class: &str,
) -> gmeow_errors::Result<()> {
    let expression = RdfTerm::iri(expression);
    if class == "StatisticalModel" {
        return input.typed(&expression, &math("ModelFormula"));
    }
    if class == "HypothesisTest" {
        input.typed(&expression, &math("Statistic"))?;
        let body = input.iri(&expression, &math("definingExpression"))?;
        return expression_anchor(input, &body, "Statistic");
    }
    let types = input.values(&expression, RDF_TYPE);
    if ![
        "MathematicalExpression",
        "NumberLiteral",
        "SymbolReference",
        "VariableExpression",
        "ApplicationExpression",
        "BindingExpression",
        "ModelFormula",
    ]
    .iter()
    .any(|kind| types.contains(&RdfTerm::iri(math(kind))))
    {
        return Err(invalid(
            "statistical expression anchor has no declared mathematical expression form",
        ));
    }
    Ok(())
}

fn method(input: &mut Source, root: &RdfTerm, source: &str) -> gmeow_errors::Result<String> {
    let (class, anchor) = statistical_profile(input, root)?;
    let expression = input.iri(root, &math(anchor))?;
    expression_anchor(input, &expression, class)?;
    let mut mappings = input.values(root, SKOS_EXACT);
    for class in input
        .values(root, RDF_TYPE)
        .into_iter()
        .chain([RdfTerm::iri(math(class))])
    {
        mappings.extend(input.values(&class, SKOS_EXACT));
        let close = input.values(&class, "http://www.w3.org/2004/02/skos/core#closeMatch");
        if close.iter().any(|term| matches!(term, RdfTerm::Iri(iri) if iri.starts_with("http://purl.obolibrary.org/obo/STATO_"))) {
            if let RdfTerm::Iri(iri) = &class {
                input.boundaries.insert(iri.clone());
            }
            input.boundaries.insert("http://www.w3.org/2004/02/skos/core#closeMatch".to_owned());
        }
        mappings.extend(close);
    }
    let targets: BTreeSet<_> = mappings
        .iter()
        .filter_map(|term| match term {
            RdfTerm::Iri(iri) if iri.starts_with("http://purl.obolibrary.org/obo/STATO_") => {
                Some(iri)
            }
            _ => None,
        })
        .collect();
    if targets.len() != 1 {
        return Err(invalid(
            "statistical projection requires exactly one STATO class correspondence",
        ));
    }
    let target = targets.into_iter().next().expect("one target");
    let mut sink = Sink::new();
    sink.typed(source, target)?;
    sink.iri(
        source,
        "http://www.w3.org/2000/01/rdf-schema#seeAlso",
        &expression,
    )?;
    sink.serialize()
}

fn observation(input: &mut Source, root: &RdfTerm, source: &str) -> gmeow_errors::Result<String> {
    let types = input.values(root, RDF_TYPE);
    if !["Matrix", "DatasetMatrix"]
        .iter()
        .any(|class| types.contains(&RdfTerm::iri(math(class))))
    {
        return Err(invalid(
            "Data Cube projection requires a declared matrix or dataset matrix",
        ));
    }
    let entries = input.values(root, &math("hasEntry"));
    if entries.is_empty() {
        return Err(invalid(
            "Data Cube projection requires explicit matrix entries; no observations are fabricated from a dataset reference",
        ));
    }
    let mut cells = BTreeMap::new();
    for entry in entries {
        input.typed(&entry, &math("MatrixEntry"))?;
        let row = input.literal(&entry, &math("atRow"))?;
        let column = input.literal(&entry, &math("atColumn"))?;
        let row_index = nonnegative_index(&row)?;
        let column_index = nonnegative_index(&column)?;
        let rational = input.one(&entry, &math("entryValue"))?;
        input.typed(&rational, &math("RationalValue"))?;
        let numerator = input.literal(&rational, &math("numerator"))?;
        let denominator = input.literal(&rational, &math("denominator"))?;
        expression::integer_literal(&numerator)?;
        expression::integer_literal(&denominator)?;
        if expression::canonical_integer(&denominator.lexical_form)? == "0" {
            return Err(invalid("matrix rational denominator cannot be zero"));
        }
        if cells
            .insert(
                (row_index, column_index),
                (row, column, numerator, denominator),
            )
            .is_some()
        {
            return Err(invalid(
                "multiple matrix entries occupy the same row and column",
            ));
        }
    }
    let qb = "http://purl.org/linked-data/cube#";
    let structure = format!("{source}:cube-structure");
    let mut sink = Sink::new();
    sink.typed(source, &format!("{qb}DataSet"))?;
    sink.iri(source, &format!("{qb}structure"), &structure)?;
    sink.typed(&structure, &format!("{qb}DataStructureDefinition"))?;
    for (index, (local, kind)) in [
        ("atRow", "dimension"),
        ("atColumn", "dimension"),
        ("numerator", "measure"),
        ("denominator", "measure"),
    ]
    .iter()
    .enumerate()
    {
        let property = math(local);
        let component = format!("{structure}:component:{index}");
        sink.iri(&structure, &format!("{qb}component"), &component)?;
        sink.typed(&component, &format!("{qb}ComponentSpecification"))?;
        sink.iri(&component, &format!("{qb}{kind}"), &property)?;
        sink.typed(
            &property,
            &format!(
                "{qb}{}Property",
                if *kind == "dimension" {
                    "Dimension"
                } else {
                    "Measure"
                }
            ),
        )?;
    }
    for ((row_index, column_index), (row, column, numerator, denominator)) in cells {
        let observation = format!("{source}:cube-observation:{row_index}:{column_index}");
        sink.typed(&observation, &format!("{qb}Observation"))?;
        sink.iri(&observation, &format!("{qb}dataSet"), source)?;
        for (predicate, value) in [
            ("atRow", row),
            ("atColumn", column),
            ("numerator", numerator),
            ("denominator", denominator),
        ] {
            sink.lit(&observation, &math(predicate), value)?;
        }
    }
    sink.serialize()
}

fn nonnegative_index(value: &RdfLiteral) -> gmeow_errors::Result<u64> {
    if value.language.is_some()
        || value.direction.is_some()
        || ![
            "http://www.w3.org/2001/XMLSchema#integer",
            "http://www.w3.org/2001/XMLSchema#nonNegativeInteger",
        ]
        .contains(&value.datatype_iri())
    {
        return Err(invalid("matrix position must have an integer datatype"));
    }
    expression::canonical_integer(&value.lexical_form)?
        .parse()
        .map_err(|_| invalid("matrix position must be a nonnegative in-range integer"))
}

/// Execute the explicitly authored math:ProjectionRequest selection.
/// Keys are target-relative content-addressed artifact names, independent of paths
/// or serializer iteration order. Every artifact has a separate mandatory ledger.
///
/// # Errors
/// Rejects malformed requests, unsupported selected targets and any failed emission.
/// No partial artifact map is returned.
pub fn project_requests(
    dataset: &RdfDataset,
    reference: &RdfDataset,
    alignments: &RdfDataset,
) -> gmeow_errors::Result<BTreeMap<String, Vec<u8>>> {
    let mut input = Source::new(dataset);
    input.extend(reference);
    input.extend(alignments);
    let class = RdfTerm::iri(math("ProjectionRequest"));
    if input.rows.iter().any(|(subject, predicates)| {
        !matches!(subject, RdfTerm::Iri(_))
            && predicates
                .get(RDF_TYPE)
                .is_some_and(|values| values.contains(&class))
    }) {
        return Err(invalid("projection requests require named identities"));
    }
    let requests: BTreeSet<_> = input
        .rows
        .iter()
        .filter_map(|(subject, predicates)| {
            if predicates
                .get(RDF_TYPE)
                .is_some_and(|values| values.contains(&class))
            {
                match subject {
                    RdfTerm::Iri(iri) => Some(iri.clone()),
                    _ => None,
                }
            } else {
                None
            }
        })
        .collect();
    let mut artifacts = BTreeMap::new();
    let mut rdf = BTreeMap::<String, Vec<u8>>::new();
    for request in requests {
        let subject = RdfTerm::iri(&request);
        let source = input.iri(&subject, &math("projectionSource"))?;
        let name = input.literal(&subject, &math("projectionTargetName"))?;
        if name.language.is_some()
            || name.direction.is_some()
            || name.datatype_iri() != "http://www.w3.org/2001/XMLSchema#string"
        {
            return Err(invalid(
                "projection request target name must be an untagged string",
            ));
        }
        let target = Target::parse(&name.lexical_form)?;
        let emission = project_source(&mut input, &source, target)?;
        let key =
            purrdf_hash::hex::encode(purrdf_hash::blake3::hash(request.as_bytes()).as_bytes());
        let stem = format!("generated/projections/math/{}", target.name());
        if target.extension() == "ttl" {
            rdf.entry(format!("{stem}.ttl"))
                .or_default()
                .extend(emission.bytes);
        } else {
            artifacts.insert(
                format!("{stem}-{key}.{}", target.extension()),
                emission.bytes,
            );
        }
        rdf.entry("generated/projections/math/loss-ledger.ttl".to_owned())
            .or_default()
            .extend(emission.ledger.into_bytes());
    }
    for (path, bytes) in rdf {
        let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)?;
        let bytes = purrdf::serialize_dataset(
            &dataset,
            "text/turtle",
            purrdf::SerializeGraph::DefaultGraph,
        )?;
        artifacts.insert(path, bytes);
    }
    Ok(artifacts)
}

fn numeric(value: &RdfLiteral) -> gmeow_errors::Result<()> {
    if value.language.is_some() || value.direction.is_some() {
        return Err(invalid(
            "numeric value cannot carry a language or direction",
        ));
    }
    let parsed = purrdf::xsd::parse_by_iri(
        value.lexical_form.trim_matches([' ', '\t', '\r', '\n']),
        value.datatype_iri(),
    )
    .map_err(|error| invalid_source(format!("invalid numeric literal: {error}"), error))?;
    match parsed {
        Some(purrdf::xsd::XsdValue::Integer { .. } | purrdf::xsd::XsdValue::Decimal(_)) => Ok(()),
        Some(purrdf::xsd::XsdValue::Double(value)) if value.is_finite() => Ok(()),
        Some(purrdf::xsd::XsdValue::Float(value)) if value.is_finite() => Ok(()),
        _ => Err(invalid(
            "quantity value requires an admitted finite numeric datatype",
        )),
    }
}

#[cfg(test)]
#[path = "projection/tests.rs"]
mod tests;

/// Registered consumer competency queries, authored in the owning mathematics slice.
/// Each query runs against the selected emitted loss-ledger graph, including an
/// extracted standalone ledger. The pipeline ships these exact source bytes.
pub const COMPETENCY_QUERIES: &[(&str, &str)] = &[
    (
        "projection-losses.rq",
        include_str!("../../../slices/grounding/math/queries/competency/projection-losses.rq"),
    ),
    (
        "projection-expression-losses.rq",
        include_str!(
            "../../../slices/grounding/math/queries/competency/projection-expression-losses.rq"
        ),
    ),
    (
        "projection-preservation.rq",
        include_str!(
            "../../../slices/grounding/math/queries/competency/projection-preservation.rq"
        ),
    ),
    (
        "projection-sensitive-losses.rq",
        include_str!(
            "../../../slices/grounding/math/queries/competency/projection-sensitive-losses.rq"
        ),
    ),
];
