// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Conservative target readers. Drafts retain source bytes, never invented semantics.

use purrdf::{RdfDataset, RdfLiteral, RdfTerm};
use purrdf_lex::xml::{Document, Node};

use super::{
    LANG, LOGIC, QUDT, RDF_TYPE, SKOS_EXACT, Source, Target, invalid, invalid_source, math,
};
use crate::{BridgeKind, Lifted, RunFrame, Rung, Sink};

const MML: &str = "http://www.w3.org/1998/Math/MathML";
const OM: &str = "http://www.openmath.org/OpenMath";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

/// Lift a complete external artifact against explicit symbol/unit reference data.
/// `subject` selects the RDF root for RDF targets; XML targets select the document root.
/// `mint_base` is the caller-owned namespace, ending in `/` or `#`.
///
/// # Errors
/// Rejects malformed or unsupported XML, unresolved symbols, unclassified units,
/// incompatible dimensions, missing expression anchors and ambiguous RDF inputs.
/// No partial graph is returned after refusal.
pub fn lift(
    bytes: &[u8],
    target: Target,
    subject: &str,
    reference: &RdfDataset,
    mint_base: &str,
) -> gmeow_errors::Result<Lifted> {
    if !mint_base.ends_with(['/', '#']) {
        return Err(invalid("math lift mint base must end in / or #"));
    }
    let text = std::str::from_utf8(bytes).map_err(|error| {
        invalid_source(
            format!("mathematical artifact is not UTF-8: {error}"),
            error,
        )
    })?;
    let mut frame = RunFrame::mint(BridgeKind::Projection(target), mint_base, bytes);
    let mut sink = Sink::new();
    let root = match target {
        Target::MathMl | Target::OpenMath => {
            let document = Document::parse(text).map_err(|error| {
                invalid_source(format!("invalid mathematical XML: {error}"), error)
            })?;
            let root = document.root_element();
            let (namespace, name) = if target == Target::MathMl {
                (MML, "math")
            } else {
                (OM, "OMOBJ")
            };
            if !root.has_tag_name((namespace, name)) {
                return Err(invalid(
                    "mathematical XML document has the wrong namespace or root",
                ));
            }
            for attribute in root.attributes() {
                let allowed = if target == Target::MathMl {
                    attribute.qname() == "display"
                } else {
                    attribute.qname() == "version" && attribute.value() == "2.0"
                };
                if !allowed {
                    return Err(invalid(format!(
                        "unsupported document attribute {}",
                        attribute.qname()
                    )));
                }
                if target == Target::MathMl {
                    frame.record_unmapped("MathML display placement is retained in the source witness, not asserted as mathematical content");
                }
            }
            let children = elements(root)?;
            let [expression] = children.as_slice() else {
                return Err(invalid(
                    "mathematical XML root must have exactly one expression",
                ));
            };
            if target == Target::MathMl && presentation(*expression) {
                for node in expression.descendants().filter(|node| node.is_element()) {
                    if node.tag_name().namespace() != Some(MML) {
                        return Err(invalid("presentation contains a foreign namespace"));
                    }
                }
                let rendering = frame.node("rendering", "presentation");
                sink.typed(&rendering, &math("ExpressionRendering"))?;
                // Content is the retained artifact; no expression AST identity is claimed.
                sink.iri(
                    &rendering,
                    &format!("{LANG}renderedContent"),
                    &frame.source_witness_iri,
                )?;
                sink.iri(
                    &rendering,
                    &format!("{LANG}renderingKind"),
                    &format!("{LANG}renderingNotation"),
                )?;
                sink.iri(&rendering, &format!("{LANG}renderingConvention"), MML)?;
                sink.iri(
                    &rendering,
                    &format!("{LANG}renderingPreservation"),
                    &format!("{LOGIC}ValidationOnly"),
                )?;
                frame.record_unmapped("MathML presentation carries notation only; mathematical content and AST identity are not inferred");
                rendering
            } else {
                let mut input = Source::new(reference);
                let expression =
                    read_expression(*expression, target, &mut input, &frame, &mut sink)?;
                frame.record_unmapped("The external expression supplies no GMEOW source theory, proof, standpoint or semantic type context");
                expression
            }
        }
        _ => {
            let dataset = purrdf::parse_dataset(bytes, "text/turtle", None)?;
            let mut input = Source::new(&dataset);
            let mut reference = Source::new(reference);
            let source = RdfTerm::iri(subject);
            let root = frame.node("draft", subject);
            match target {
                Target::Qudt => {
                    lift_quantity(&mut input, &mut reference, &source, &root, &mut sink)?
                }
                Target::DataCube => lift_cube(&mut input, &source, &root, &frame, &mut sink)?,
                Target::Stato => lift_model(&mut input, &mut reference, &source, &root, &mut sink)?,
                Target::MathMl | Target::OpenMath => unreachable!("XML targets handled above"),
            }
            for predicate in input.losses() {
                frame.record_unmapped(format!(
                    "External RDF construct not reconstructed: {predicate}"
                ));
            }
            frame.record_unmapped("External RDF supplies a mathematical draft; absent model assumptions, proof, source provenance and standpoint are not reconstructed");
            root
        }
    };
    frame.emit(&mut sink, Rung::lossy_crisp_with_witness())?;
    sink.typed(&frame.correspondence_iri, &math("ProjectionRecord"))?;
    sink.iri(
        &frame.correspondence_iri,
        &math("projectionSource"),
        &frame.source_witness_iri,
    )?;
    sink.string(
        &frame.correspondence_iri,
        &math("projectionTargetName"),
        "math:canonical-draft",
    )?;
    for construct in &frame.unmapped {
        sink.string(
            &frame.correspondence_iri,
            &format!("{LOGIC}unsupportedConstruct"),
            construct,
        )?;
    }
    frame.generated(&mut sink, &root)?;
    sink.iri(&root, &math("parseSource"), &frame.source_witness_iri)?;
    Lifted::seal(&frame, sink, 1)
}

fn token_text(node: Node<'_, '_>) -> gmeow_errors::Result<String> {
    if node.children().any(|child| child.is_element()) {
        return Err(invalid("numeric token cannot contain nested markup"));
    }
    Ok(node
        .children()
        .filter(|child| child.is_text())
        .filter_map(|child| child.text())
        .collect())
}

fn openmath_integer(node: Node<'_, '_>) -> gmeow_errors::Result<String> {
    let text: String = token_text(node)?
        .chars()
        .filter(|character| !matches!(character, ' ' | '\t' | '\r' | '\n'))
        .collect();
    let digits = text.strip_prefix('-').unwrap_or(&text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid(
            "OpenMath decimal integer requires digits and an optional minus sign",
        ));
    }
    Ok(text)
}

fn elements<'d, 'a>(node: Node<'d, 'a>) -> gmeow_errors::Result<Vec<Node<'d, 'a>>> {
    for child in node.children() {
        if child.is_text() && child.text().is_some_and(|text| !text.trim().is_empty()) {
            return Err(invalid("unexpected mixed text in mathematical container"));
        }
    }
    Ok(node.children().filter(|node| node.is_element()).collect())
}

fn presentation(node: Node<'_, '_>) -> bool {
    matches!(
        node.tag_name().name(),
        "mi" | "mn"
            | "mo"
            | "mrow"
            | "mfrac"
            | "msqrt"
            | "msup"
            | "msub"
            | "msubsup"
            | "munder"
            | "mover"
            | "munderover"
            | "mtable"
            | "mtext"
            | "mfenced"
            | "menclose"
            | "mroot"
    )
}

fn read_expression(
    root: Node<'_, '_>,
    target: Target,
    reference: &mut Source,
    frame: &RunFrame,
    sink: &mut Sink,
) -> gmeow_errors::Result<String> {
    let mut pending = vec![(root, false)];
    let mut completed = std::collections::BTreeMap::new();
    while let Some((node, ready)) = pending.pop() {
        if !ready && matches!(node.tag_name().name(), "apply" | "OMA") {
            let children = elements(node)?;
            let Some((operator, operands)) = children.split_first() else {
                return Err(invalid("empty mathematical application"));
            };
            let external = external_symbol(*operator, target)?;
            if !(target == Target::OpenMath
                && external == "http://www.openmath.org/cd/nums1#rational")
            {
                pending.push((node, true));
                pending.extend(operands.iter().rev().map(|operand| (*operand, false)));
                continue;
            }
        }
        let expression = emit_expression(node, target, reference, frame, sink, &completed)?;
        completed.insert(node.id(), expression);
    }
    completed
        .remove(&root.id())
        .ok_or_else(|| invalid("selected XML expression was not structured"))
}

fn emit_expression(
    node: Node<'_, '_>,
    target: Target,
    reference: &mut Source,
    frame: &RunFrame,
    sink: &mut Sink,
    completed: &std::collections::BTreeMap<purrdf_lex::xml::NodeId, String>,
) -> gmeow_errors::Result<String> {
    let namespace = if target == Target::MathMl { MML } else { OM };
    if node.tag_name().namespace() != Some(namespace) {
        return Err(invalid(
            "foreign namespace in mathematical content expression",
        ));
    }
    let iri = frame.node("expression", &node.offset().to_string());
    let name = node.tag_name().name();
    for attribute in node.attributes() {
        let allowed = match name {
            "cn" => ["type"].contains(&attribute.qname()),
            "OMS" => ["cd", "name", "cdbase"].contains(&attribute.qname()),
            "OMV" => attribute.qname() == "name",
            "csymbol" => ["cd", "definitionURL"].contains(&attribute.qname()),
            _ => false,
        };
        if !allowed {
            return Err(invalid(format!(
                "unsupported content attribute {} on {name}",
                attribute.qname()
            )));
        }
    }
    if target == Target::MathMl && name == "cn" && node.attribute("type") == Some("rational") {
        let children: Vec<_> = node
            .children()
            .filter(|child| child.is_element() || child.is_text())
            .collect();
        let [numerator, separator, denominator] = children.as_slice() else {
            return Err(invalid(
                "rational cn must contain numerator, sep and denominator",
            ));
        };
        if !numerator.is_text()
            || !separator.has_tag_name((MML, "sep"))
            || !denominator.is_text()
            || !separator.attributes().is_empty()
            || separator.children().next().is_some()
        {
            return Err(invalid("malformed rational MathML content"));
        }
        return rational(
            numerator.text().unwrap_or_default(),
            denominator.text().unwrap_or_default(),
            &iri,
            frame,
            sink,
        );
    }
    match (target, name) {
        (Target::MathMl, "cn") | (Target::OpenMath, "OMI") => {
            if node.children().any(|child| child.is_element()) {
                return Err(invalid(
                    "structured numeric notation requires a supported numeric reader",
                ));
            }
            let kind = if name == "OMI" {
                "integer"
            } else {
                node.attribute("type").unwrap_or("real")
            };
            let text = if name == "OMI" {
                openmath_integer(node)?
            } else {
                token_text(node)?
            };
            let (numerator, denominator, class, system) = match kind {
                "integer" => (
                    super::expression::canonical_integer(&text)?,
                    "1".to_owned(),
                    "Integer",
                    "integers",
                ),
                "real" => {
                    let (numerator, denominator) = super::expression::decimal_ratio(&text)?;
                    (numerator, denominator, "RealNumber", "realNumbers")
                }
                _ => return Err(invalid("unsupported content numeric kind")),
            };
            number(&numerator, &denominator, (class, system), &iri, frame, sink)?;
        }
        (Target::MathMl, "ci") | (Target::OpenMath, "OMV") => {
            if target == Target::OpenMath
                && node.children().any(|child| {
                    child.is_text() && child.text().is_some_and(|text| !text.trim().is_empty())
                })
            {
                return Err(invalid("OpenMath variable must be an empty token"));
            }
            let name = if target == Target::MathMl {
                node.text()
            } else {
                node.attribute("name")
            }
            .map(|name| name.trim_matches([' ', '\t', '\r', '\n']))
            .filter(|name| !name.is_empty())
            .ok_or_else(|| invalid("empty content variable identity"))?;
            if target == Target::OpenMath && !purrdf_lex::terminals::is_ncname(name) {
                return Err(invalid("OpenMath variable name must be an XML NCName"));
            }
            if node.children().any(|child| child.is_element()) {
                return Err(invalid("variable token cannot contain markup"));
            }
            let declaration = frame.node("free-variable", name);
            let occurrence = frame.node("variable-occurrence", &node.offset().to_string());
            sink.typed(&iri, &math("VariableExpression"))?;
            sink.typed(&declaration, &math("FreeVariableDeclaration"))?;
            sink.typed(&occurrence, &math("VariableOccurrence"))?;
            sink.iri(&iri, &math("variableOccurrence"), &occurrence)?;
            sink.iri(&occurrence, &math("declaredVariable"), &declaration)?;
        }
        (Target::MathMl, "csymbol") | (Target::OpenMath, "OMS") => {
            let external = external_symbol(node, target)?;
            let local = resolve_symbol(reference, &external)?;
            if super::expression::is_number_type(&reference.values(&RdfTerm::iri(&local), RDF_TYPE))
            {
                super::expression::number_system(reference, &RdfTerm::iri(&local))?;
                sink.typed(&iri, &math("NumberLiteral"))?;
                sink.iri(&iri, &math("literalValue"), &local)?;
            } else {
                sink.typed(&iri, &math("SymbolReference"))?;
                sink.iri(&iri, &math("hasMathematicalSymbol"), &local)?;
            }
        }
        (Target::MathMl, "apply") | (Target::OpenMath, "OMA") => {
            let children = elements(node)?;
            let Some((operator, operands)) = children.split_first() else {
                return Err(invalid("empty mathematical application"));
            };
            if operands.is_empty() {
                return Err(invalid("mathematical application has no operands"));
            }
            let mut external = external_symbol(*operator, target)?;
            if target == Target::MathMl && operator.tag_name().name() == "minus" {
                match operands.len() {
                    1 => external = "http://www.openmath.org/cd/arith1#unary_minus".to_owned(),
                    2 => {}
                    _ => return Err(invalid("MathML minus requires one or two operands")),
                }
            }
            if target == Target::MathMl
                && ["divide", "power"].contains(&operator.tag_name().name())
                && operands.len() != 2
            {
                return Err(invalid("MathML division and power require two operands"));
            }
            if target == Target::OpenMath && external == "http://www.openmath.org/cd/nums1#rational"
            {
                let [numerator, denominator] = operands else {
                    return Err(invalid("OpenMath rational requires two integer operands"));
                };
                if !numerator.has_tag_name((OM, "OMI")) || !denominator.has_tag_name((OM, "OMI")) {
                    return Err(invalid("OpenMath rational operands must be integers"));
                }
                if !numerator.attributes().is_empty() || !denominator.attributes().is_empty() {
                    return Err(invalid(
                        "qualified OpenMath rational operands are unsupported",
                    ));
                }
                return rational(
                    &openmath_integer(*numerator)?,
                    &openmath_integer(*denominator)?,
                    &iri,
                    frame,
                    sink,
                );
            }
            let local = resolve_symbol(reference, &external)?;
            if super::expression::is_number_type(&reference.values(&RdfTerm::iri(&local), RDF_TYPE))
            {
                return Err(invalid(
                    "a number reference cannot serve as an application operator",
                ));
            }
            sink.typed(&iri, &math("ApplicationExpression"))?;
            sink.iri(&iri, &math("operator"), &local)?;
            for (index, operand) in operands.iter().enumerate() {
                let child = completed
                    .get(&operand.id())
                    .ok_or_else(|| invalid("application operand was not fully structured"))?;
                let slot = frame.node("slot", &format!("{}:{index}", node.offset()));
                sink.typed(&slot, &math("ArgumentSlot"))?;
                sink.integer(
                    &slot,
                    &math("slotIndex"),
                    i64::try_from(index).map_err(|_| invalid("argument index exceeds i64"))?,
                )?;
                sink.iri(&slot, &math("slotExpression"), child)?;
                sink.iri(&iri, &math("argumentSlot"), &slot)?;
            }
        }
        _ => {
            return Err(invalid(format!(
                "unsupported mathematical element {name}; no opaque expression fallback is permitted"
            )));
        }
    }
    Ok(iri)
}

fn external_symbol(node: Node<'_, '_>, target: Target) -> gmeow_errors::Result<String> {
    for attribute in node.attributes() {
        let allowed = if target == Target::MathMl {
            ["cd", "definitionURL"].contains(&attribute.qname())
        } else {
            ["cd", "name", "cdbase"].contains(&attribute.qname())
        };
        if !allowed {
            return Err(invalid(format!(
                "unsupported symbol attribute {}",
                attribute.qname()
            )));
        }
    }
    if node.children().any(|child| child.is_element()) {
        return Err(invalid("symbol token cannot contain nested content"));
    }
    if target == Target::MathMl {
        if node.tag_name().namespace() != Some(MML) {
            return Err(invalid("foreign namespace for MathML operator"));
        }
        let name = node.tag_name().name();
        if name != "csymbol" {
            if !token_text(node)?.trim().is_empty() {
                return Err(invalid("built-in MathML operator must be empty"));
            }
            let symbol = match name {
                "plus" => "arith1#plus",
                "minus" => "arith1#minus",
                "times" => "arith1#times",
                "divide" => "arith1#divide",
                "power" => "arith1#power",
                "root" => {
                    return Err(invalid(
                        "MathML root qualifiers are outside the admitted content profile",
                    ));
                }
                _ => return Err(invalid("unsupported content operator")),
            };
            if node.children().any(|child| child.is_element()) || !node.attributes().is_empty() {
                return Err(invalid("unsupported qualified content operator"));
            }
            return Ok(format!("http://www.openmath.org/cd/{symbol}"));
        }
        let cd = node.attribute("cd");
        let name = node.text().unwrap_or_default().trim();
        let declared = node.attribute("definitionURL");
        let derived = cd.map(|cd| format!("http://www.openmath.org/cd/{cd}#{name}"));
        match (declared, derived) {
            (Some(declared), Some(derived)) if declared != derived => Err(invalid(
                "MathML symbol dictionary and definitionURL disagree",
            )),
            (Some(declared), _) => Ok(declared.to_owned()),
            (None, Some(derived)) if !name.is_empty() => Ok(derived),
            _ => Err(invalid(
                "MathML csymbol requires an explicit resolvable symbol identity",
            )),
        }
    } else {
        if !node.has_tag_name((OM, "OMS")) {
            return Err(invalid("unsupported OpenMath operator"));
        }
        if !token_text(node)?.trim().is_empty() {
            return Err(invalid("OpenMath symbol must be empty"));
        }
        let cd = node
            .attribute("cd")
            .ok_or_else(|| invalid("OpenMath symbol has no content dictionary"))?;
        let name = node
            .attribute("name")
            .ok_or_else(|| invalid("OpenMath symbol has no name"))?;
        let base = node
            .attribute("cdbase")
            .unwrap_or("http://www.openmath.org/cd");
        if !purrdf_lex::terminals::is_ncname(cd) || !purrdf_lex::terminals::is_ncname(name) {
            return Err(invalid("malformed OpenMath symbol identity"));
        }
        Ok(format!("{}/{cd}#{name}", base.trim_end_matches('/')))
    }
}

fn resolve_symbol(reference: &mut Source, external: &str) -> gmeow_errors::Result<String> {
    let native = RdfTerm::iri(external);
    if reference.rows.get(&native).is_some_and(|fields| {
        fields
            .get(RDF_TYPE)
            .is_some_and(|types| native_symbol(types))
    }) {
        return Ok(external.to_owned());
    }
    let matched: Vec<_> = reference
        .rows
        .iter()
        .filter_map(|(subject, predicates)| {
            if predicates
                .get(SKOS_EXACT)
                .is_some_and(|values| values.contains(&RdfTerm::iri(external)))
                && predicates
                    .get(RDF_TYPE)
                    .is_some_and(|types| native_symbol(types))
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
    let [local] = matched.as_slice() else {
        return Err(invalid(format!(
            "external symbol {external} has no unique canonical alignment"
        )));
    };
    Ok(local.clone())
}

fn native_symbol(types: &[RdfTerm]) -> bool {
    super::expression::is_number_type(types)
        || ["Operation", "ArithmeticOperation", "MathematicalSymbol"]
            .iter()
            .any(|kind| types.contains(&RdfTerm::iri(math(kind))))
}

fn lift_quantity(
    input: &mut Source,
    reference: &mut Source,
    source: &RdfTerm,
    root: &str,
    sink: &mut Sink,
) -> gmeow_errors::Result<()> {
    input.typed(source, &format!("{QUDT}QuantityValue"))?;
    let unit = input.iri(source, &format!("{QUDT}unit"))?;
    let unit_term = RdfTerm::iri(&unit);
    reference.typed(&unit_term, &format!("{QUDT}Unit"))?;
    let vector = reference.iri(&unit_term, &format!("{QUDT}hasDimensionVector"))?;
    let dimension = reference.iri(&unit_term, &math("hasDimension"))?;
    let supplied = input.values(&unit_term, &format!("{QUDT}hasDimensionVector"));
    if supplied.iter().any(|value| value != &RdfTerm::iri(&vector)) {
        return Err(invalid(
            "artifact unit dimension contradicts the selected classified reference",
        ));
    }
    let frame = reference.iri(
        source,
        "https://blackcatinformatics.ca/gmeow/hasReferenceFrame",
    )?;
    let value = input.literal(source, &format!("{QUDT}numericValue"))?;
    super::numeric(&value)?;
    sink.typed(root, &math("Quantity"))?;
    sink.lit(root, &math("quantityValue"), value)?;
    sink.iri(root, "https://blackcatinformatics.ca/gmeow/unit", &unit)?;
    sink.iri(root, &math("hasDimension"), &dimension)?;
    sink.iri(
        root,
        "https://blackcatinformatics.ca/gmeow/hasReferenceFrame",
        &frame,
    )?;
    Ok(())
}

fn lift_model(
    input: &mut Source,
    reference: &mut Source,
    source: &RdfTerm,
    root: &str,
    sink: &mut Sink,
) -> gmeow_errors::Result<()> {
    let classes: Vec<_> = input.values(source, RDF_TYPE).into_iter().filter(|class| matches!(class, RdfTerm::Iri(iri) if iri.starts_with("http://purl.obolibrary.org/obo/STATO_"))).collect();
    let [external] = classes.as_slice() else {
        return Err(invalid("STATO draft requires one explicit class reference"));
    };
    let mut admitted = Vec::new();
    for (class, anchor) in [
        ("StatisticalModel", "modelFormula"),
        ("Statistic", "definingExpression"),
        ("HypothesisTest", "testStatistic"),
    ] {
        let subject = RdfTerm::iri(math(class));
        let mut correspondences = reference.values(&subject, SKOS_EXACT);
        correspondences
            .extend(reference.values(&subject, "http://www.w3.org/2004/02/skos/core#closeMatch"));
        if correspondences.contains(external) {
            admitted.push((class, anchor));
        }
    }
    let [(class, anchor)] = admitted.as_slice() else {
        return Err(invalid(
            "STATO class has no unique admitted native statistical profile",
        ));
    };
    let expression = input.iri(source, "http://www.w3.org/2000/01/rdf-schema#seeAlso")?;
    super::expression_anchor(reference, &expression, class)?;
    sink.typed(root, &math(class))?;
    sink.iri(root, &math(anchor), &expression)?;
    let RdfTerm::Iri(external) = external else {
        unreachable!("class reference checked above")
    };
    sink.iri(
        root,
        "http://www.w3.org/2000/01/rdf-schema#seeAlso",
        external,
    )?;
    Ok(())
}

fn lift_cube(
    input: &mut Source,
    source: &RdfTerm,
    root: &str,
    frame: &RunFrame,
    sink: &mut Sink,
) -> gmeow_errors::Result<()> {
    let qb = "http://purl.org/linked-data/cube#";
    input.typed(source, &format!("{qb}DataSet"))?;
    let observations: Vec<_> = input
        .rows
        .iter()
        .filter_map(|(subject, predicates)| {
            predicates
                .get(&format!("{qb}dataSet"))
                .filter(|values| values.contains(source))
                .map(|_| subject.clone())
        })
        .collect();
    if observations.is_empty() {
        return Err(invalid("selected cube dataset has no observations"));
    }
    sink.typed(root, &math("DatasetMatrix"))?;
    let mut positions = std::collections::BTreeSet::new();
    for observation in observations {
        input.typed(&observation, &format!("{qb}Observation"))?;
        let row = input.literal(&observation, &math("atRow"))?;
        let column = input.literal(&observation, &math("atColumn"))?;
        let position = (
            super::nonnegative_index(&row)?,
            super::nonnegative_index(&column)?,
        );
        if !positions.insert(position) {
            return Err(invalid(
                "cube has multiple observations at the same matrix position",
            ));
        }
        let numerator = input.literal(&observation, &math("numerator"))?;
        let denominator = input.literal(&observation, &math("denominator"))?;
        super::expression::validate_integer_payload(&numerator)?;
        super::expression::validate_integer_payload(&denominator)?;
        if super::expression::canonical_integer(&denominator.lexical_form)? == "0" {
            return Err(invalid("cube rational denominator is zero"));
        }
        let entry = frame.node("entry", &format!("{}:{}", position.0, position.1));
        let rational = frame.node("rational", &format!("{}:{}", position.0, position.1));
        sink.typed(&entry, &math("MatrixEntry"))?;
        sink.typed(&rational, &math("RationalValue"))?;
        sink.iri(root, &math("hasEntry"), &entry)?;
        sink.iri(&entry, &math("entryValue"), &rational)?;
        for (subject, predicate, value) in [
            (&entry, "atRow", row),
            (&entry, "atColumn", column),
            (&rational, "numerator", numerator),
            (&rational, "denominator", denominator),
        ] {
            sink.lit(subject, &math(predicate), value)?;
        }
    }
    Ok(())
}

fn rational(
    numerator: &str,
    denominator: &str,
    expression: &str,
    frame: &RunFrame,
    sink: &mut Sink,
) -> gmeow_errors::Result<String> {
    number(
        numerator,
        denominator,
        ("RationalNumber", "rationalNumbers"),
        expression,
        frame,
        sink,
    )
}

fn number(
    numerator: &str,
    denominator: &str,
    profile: (&str, &str),
    expression: &str,
    frame: &RunFrame,
    sink: &mut Sink,
) -> gmeow_errors::Result<String> {
    let (class, system) = profile;
    let numerator = super::expression::canonical_integer(numerator)?;
    let denominator = super::expression::canonical_integer(denominator)?;
    if denominator == "0" {
        return Err(invalid("rational denominator cannot be zero"));
    }
    let value = frame.node("number", &format!("{system}:{numerator}/{denominator}"));
    sink.typed(expression, &math("NumberLiteral"))?;
    sink.typed(&value, &math("RationalValue"))?;
    sink.typed(&value, &math("Number"))?;
    sink.typed(&value, &math(class))?;
    sink.iri(&value, &math("inNumberSystem"), &math(system))?;
    sink.lit(
        &value,
        &math("isExact"),
        RdfLiteral::typed("true", format!("{XSD}boolean")),
    )?;
    sink.iri(expression, &math("literalValue"), &value)?;
    sink.lit(
        &value,
        &math("numerator"),
        RdfLiteral::typed(numerator, format!("{XSD}integer")),
    )?;
    sink.lit(
        &value,
        &math("denominator"),
        RdfLiteral::typed(denominator, format!("{XSD}integer")),
    )?;
    Ok(expression.to_owned())
}
