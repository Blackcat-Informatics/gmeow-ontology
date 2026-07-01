// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reads openEHR Operational Template (OPT) XML directly into the pure, crate-agnostic
//! carrier the `logic:` constraint axis lowers from.
//!
//! An OPT is the flattened, fully-expressed form of an ADL archetype: every
//! `ELEMENT` node carries a `node_id` (an at-code, e.g. `at0004`) and, when its
//! value is constrained to a `DV_QUANTITY`, a `magnitude` interval with four
//! boundary fields (`lower`, `upper`, `lower_included`, `upper_included`) plus
//! a sibling `units` string. [`read_magnitude_interval`] walks the OPT DOM to
//! find that interval for a given `node_id`, and [`read_opt_quantity_constraint`]
//! packages it as an [`gmeow_logic_compile::opt_lift::OptConstraintIr`] — the XML-free value
//! the `logic:` lift and the SHACL Core / ShEx projections consume. This crate does the XML
//! parsing ONLY; the SHACL/ShEx surfaces are projected in `gmeow-logic-compile` from the
//! canonical `logic:ValidationShape` (Principle 4 — the canon is the authoring ground; there
//! is no direct OPT→SHACL emit).
//!
//! OPT files reuse `lower_included`/`upper_included` field names inside many
//! unrelated interval blocks (`occurrences`, `existence`, `precision`, and
//! even a `DV_CODED_TEXT`-valued `ELEMENT` that happens to share an at-code
//! with a `DV_QUANTITY`-valued one elsewhere in the template). The reader
//! therefore does not grab the first interval it finds; it descends a fixed
//! structural path — `ELEMENT[node_id] → attributes[rm_attribute_name=value]
//! → children[xsi:type=C_DV_QUANTITY] → list → magnitude` — and hard-fails if
//! any step of that path is absent for the requested `node_id`.

use std::fmt;

use gmeow_logic_compile::opt_lift::{OptConstraintIr, OptConstraintKind, OptInterval};

/// A parsed `C_DV_QUANTITY` magnitude interval, read verbatim from an OPT.
#[derive(Debug, Clone, PartialEq)]
pub struct MagnitudeInterval {
    /// The interval's lower bound.
    pub lower: f64,
    /// The interval's upper bound.
    pub upper: f64,
    /// Whether `lower` itself is admitted by the interval.
    pub lower_included: bool,
    /// Whether `upper` itself is admitted by the interval.
    pub upper_included: bool,
    /// The magnitude's unit string (e.g. `mm[Hg]`).
    pub units: String,
}

/// Failure reading or navigating an OPT document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptError(String);

impl fmt::Display for OptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "openEHR OPT read error: {}", self.0)
    }
}

impl std::error::Error for OptError {}

fn opt_err(msg: impl Into<String>) -> OptError {
    OptError(msg.into())
}

/// Finds the first element child named `name` (namespace-agnostic on the
/// local name, matching how the OPT's default+xsi namespaces are declared).
fn child_element<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

fn xsi_type<'a>(node: roxmltree::Node<'a, '_>) -> Option<&'a str> {
    node.attributes()
        .find(|a| a.name() == "type")
        .map(|a| a.value())
}

fn element_text_f64(node: roxmltree::Node, name: &str) -> Result<f64, OptError> {
    let child = child_element(node, name).ok_or_else(|| {
        opt_err(format!(
            "missing <{name}> under <{}>",
            node.tag_name().name()
        ))
    })?;
    let text = child
        .text()
        .ok_or_else(|| opt_err(format!("<{name}> has no text content")))?;
    text.trim()
        .parse::<f64>()
        .map_err(|e| opt_err(format!("<{name}> value {text:?} is not a number: {e}")))
}

fn element_text_bool(node: roxmltree::Node, name: &str) -> Result<bool, OptError> {
    let child = child_element(node, name).ok_or_else(|| {
        opt_err(format!(
            "missing <{name}> under <{}>",
            node.tag_name().name()
        ))
    })?;
    let text = child
        .text()
        .ok_or_else(|| opt_err(format!("<{name}> has no text content")))?;
    match text.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(opt_err(format!(
            "<{name}> value {other:?} is not a boolean"
        ))),
    }
}

/// Reads the `C_DV_QUANTITY` magnitude interval for the `ELEMENT` whose
/// `<node_id>` text equals `node_id`.
///
/// Navigates: the `ELEMENT` (`C_COMPLEX_OBJECT`) with a direct `<node_id>`
/// child matching `node_id` → its `<attributes xsi:type="C_SINGLE_ATTRIBUTE">`
/// with `<rm_attribute_name>value</rm_attribute_name>` → that attribute's
/// `<children xsi:type="C_DV_QUANTITY">` → `<list>` → `<magnitude>`, reading
/// `lower_included`, `upper_included`, `lower`, `upper`, and the sibling
/// `<units>` under `<list>`.
///
/// An OPT may contain multiple `ELEMENT`s sharing the same `node_id` at
/// different archetype-slot expansions, and only some of those are
/// `DV_QUANTITY`-valued (others may be `DV_CODED_TEXT` or other RM types).
/// This function scans every `ELEMENT` with the requested `node_id` and
/// returns the first one whose value is a `C_DV_QUANTITY`; if none match,
/// it hard-fails rather than defaulting.
pub fn read_magnitude_interval(
    opt_xml: &str,
    node_id: &str,
) -> Result<MagnitudeInterval, OptError> {
    let doc = roxmltree::Document::parse(opt_xml)
        .map_err(|e| opt_err(format!("XML parse failure: {e}")))?;

    let candidate_elements = doc.descendants().filter(|n| {
        n.is_element()
            && n.tag_name().name() == "children"
            && xsi_type(*n) == Some("C_COMPLEX_OBJECT")
            && child_element(*n, "node_id").and_then(|nid| nid.text()) == Some(node_id)
    });

    for element in candidate_elements {
        let attributes = match child_element(element, "attributes") {
            Some(a) if xsi_type(a) == Some("C_SINGLE_ATTRIBUTE") => a,
            _ => continue,
        };
        let rm_attribute_name =
            child_element(attributes, "rm_attribute_name").and_then(|n| n.text());
        if rm_attribute_name != Some("value") {
            continue;
        }
        let value_children = match child_element(attributes, "children") {
            Some(c) if xsi_type(c) == Some("C_DV_QUANTITY") => c,
            _ => continue,
        };
        let list = child_element(value_children, "list").ok_or_else(|| {
            opt_err(format!(
                "node_id {node_id:?}: C_DV_QUANTITY has no <list> child"
            ))
        })?;
        let magnitude = child_element(list, "magnitude").ok_or_else(|| {
            opt_err(format!(
                "node_id {node_id:?}: <list> has no <magnitude> child"
            ))
        })?;
        let units_node = child_element(list, "units")
            .ok_or_else(|| opt_err(format!("node_id {node_id:?}: <list> has no <units> child")))?;
        let units = units_node
            .text()
            .ok_or_else(|| opt_err(format!("node_id {node_id:?}: <units> has no text content")))?
            .to_string();

        return Ok(MagnitudeInterval {
            lower: element_text_f64(magnitude, "lower")?,
            upper: element_text_f64(magnitude, "upper")?,
            lower_included: element_text_bool(magnitude, "lower_included")?,
            upper_included: element_text_bool(magnitude, "upper_included")?,
            units,
        });
    }

    Err(opt_err(format!(
        "no C_DV_QUANTITY-valued ELEMENT with node_id {node_id:?} found in OPT"
    )))
}

/// Reads a `C_DV_QUANTITY` constraint for `node_id` and packages it as the pure,
/// crate-agnostic [`OptConstraintIr`] the `logic:` lift consumes. Reuses the hard-fail
/// fixed-path descent of [`read_magnitude_interval`]; `magnitude_path`/`units_path` are the
/// domain predicates the lifted shape constrains.
pub fn read_opt_quantity_constraint(
    opt_xml: &str,
    node_id: &str,
    shape_iri: &str,
    target_class: &str,
    magnitude_path: &str,
    units_path: &str,
) -> Result<OptConstraintIr, OptError> {
    let m = read_magnitude_interval(opt_xml, node_id)?;
    Ok(OptConstraintIr {
        shape_iri: shape_iri.to_owned(),
        target_class: target_class.to_owned(),
        kind: OptConstraintKind::Quantity {
            magnitude_path: magnitude_path.to_owned(),
            interval: OptInterval {
                lower: Some(m.lower),
                upper: Some(m.upper),
                lower_included: m.lower_included,
                upper_included: m.upper_included,
            },
            units_path: units_path.to_owned(),
            units: m.units,
        },
    })
}

/// Walks an OPT document and extracts EVERY constraint it recognizes — `C_DV_QUANTITY`
/// magnitude intervals, `C_STRING` regex patterns, and `C_CODE_PHRASE` coded value sets — as
/// pure [`OptConstraintIr`] values in document order. Shape/class IRIs are minted
/// deterministically from `base_iri` + a document-order index, so the same OPT always yields
/// the same shapes. Hard-fails on a recognized-but-malformed constraint node (no silent skip)
/// and on an OPT with no recognized constraint at all.
///
/// This is the general native-parser surface: `read_opt_quantity_constraint` targets one
/// at-coded quantity ELEMENT (what the production pipeline lifts, the meaningful data-value
/// constraints); this walker proves the reader handles the other ADL2 constraint node kinds
/// present in a real OPT.
pub fn read_all_opt_constraints(
    opt_xml: &str,
    base_iri: &str,
) -> Result<Vec<OptConstraintIr>, OptError> {
    let doc = roxmltree::Document::parse(opt_xml)
        .map_err(|e| opt_err(format!("XML parse failure: {e}")))?;
    let mut out = Vec::new();
    let mut idx = 0usize;
    for node in doc.descendants().filter(|n| n.is_element()) {
        let kind = match xsi_type(node) {
            Some("C_DV_QUANTITY") => {
                let (interval, units) = magnitude_from_quantity(node, idx)?;
                Some(OptConstraintKind::Quantity {
                    magnitude_path: format!("{base_iri}magnitude"),
                    interval,
                    units_path: format!("{base_iri}units"),
                    units,
                })
            }
            Some("C_STRING") => {
                child_element(node, "pattern")
                    .and_then(|p| p.text())
                    .map(|regex| OptConstraintKind::StringPattern {
                        path: format!("{base_iri}text"),
                        regex: regex.trim().to_owned(),
                        flags: None,
                    })
            }
            Some("C_CODE_PHRASE") => code_phrase_value_set(node, base_iri),
            _ => None,
        };
        if let Some(kind) = kind {
            out.push(OptConstraintIr {
                shape_iri: format!("{base_iri}shape-{idx}"),
                target_class: format!("{base_iri}Constraint-{idx}"),
                kind,
            });
            idx += 1;
        }
    }
    if out.is_empty() {
        return Err(opt_err(
            "no C_DV_QUANTITY / C_STRING / C_CODE_PHRASE constraint found in OPT",
        ));
    }
    Ok(out)
}

/// Extract the magnitude interval + units from a `C_DV_QUANTITY` node's `<list><magnitude>`.
fn magnitude_from_quantity(
    node: roxmltree::Node,
    idx: usize,
) -> Result<(OptInterval, String), OptError> {
    let list = child_element(node, "list")
        .ok_or_else(|| opt_err(format!("C_DV_QUANTITY #{idx}: no <list>")))?;
    let magnitude = child_element(list, "magnitude")
        .ok_or_else(|| opt_err(format!("C_DV_QUANTITY #{idx}: <list> has no <magnitude>")))?;
    let units = child_element(list, "units")
        .and_then(|u| u.text())
        .ok_or_else(|| opt_err(format!("C_DV_QUANTITY #{idx}: no <units>")))?
        .to_owned();
    Ok((
        OptInterval {
            lower: Some(element_text_f64(magnitude, "lower")?),
            upper: Some(element_text_f64(magnitude, "upper")?),
            lower_included: element_text_bool(magnitude, "lower_included")?,
            upper_included: element_text_bool(magnitude, "upper_included")?,
        },
        units,
    ))
}

/// Extract a coded value set (`<code_list>` codes qualified by `<terminology_id><value>`)
/// from a `C_CODE_PHRASE` node. Returns `None` when the node carries no `<code_list>`.
fn code_phrase_value_set(node: roxmltree::Node, base_iri: &str) -> Option<OptConstraintKind> {
    let terminology = child_element(node, "terminology_id")
        .and_then(|t| child_element(t, "value"))
        .and_then(|v| v.text())
        .unwrap_or("unknown")
        .trim()
        .to_owned();
    let codes: Vec<String> = node
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "code_list")
        .filter_map(|c| c.text())
        .map(|s| format!("{base_iri}terminology/{terminology}/{}", s.trim()))
        .collect();
    if codes.is_empty() {
        return None;
    }
    Some(OptConstraintKind::ValueSet {
        path: format!("{base_iri}definingCode"),
        codes,
    })
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_node_id_hard_fails() {
        let err = read_magnitude_interval("<template></template>", "at9999").unwrap_err();
        assert!(err.to_string().contains("at9999"));
    }
}

#[cfg(test)]
mod walker_tests {
    use super::*;
    use gmeow_logic_compile::opt_lift::{lift_opt_to_validation_shape, recover_opt_from_shape};
    use std::path::PathBuf;

    fn blutdruck() -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("../../validations/openehr-bloodpressure/Blutdruck.opt");
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    const BASE: &str = "https://gmeow.example/openehr/bp/";

    #[test]
    fn walker_extracts_quantity_string_and_coded_families_from_the_real_opt() {
        let all = read_all_opt_constraints(&blutdruck(), BASE).expect("walk Blutdruck.opt");
        // The vendored OPT has 2 C_DV_QUANTITY magnitudes, several C_STRING patterns, and
        // a C_CODE_PHRASE code list — the walker must find all three families.
        let has_quantity = all
            .iter()
            .any(|c| matches!(c.kind, OptConstraintKind::Quantity { .. }));
        let has_pattern = all
            .iter()
            .any(|c| matches!(c.kind, OptConstraintKind::StringPattern { .. }));
        let has_value_set = all
            .iter()
            .any(|c| matches!(c.kind, OptConstraintKind::ValueSet { .. }));
        assert!(
            has_quantity,
            "no quantity extracted from {} constraints",
            all.len()
        );
        assert!(
            has_pattern,
            "no string pattern extracted from {} constraints",
            all.len()
        );
        assert!(
            has_value_set,
            "no coded value set extracted from {} constraints",
            all.len()
        );
    }

    #[test]
    fn every_walked_constraint_round_trips_u_after_d_is_identity() {
        // The section/retraction law holds on EVERY constraint the walker reads from real XML.
        let all = read_all_opt_constraints(&blutdruck(), BASE).expect("walk");
        assert!(
            all.len() >= 3,
            "expected several constraints, got {}",
            all.len()
        );
        for c in &all {
            let shape = lift_opt_to_validation_shape(c).expect("lift");
            let recovered = recover_opt_from_shape(&shape).expect("recover");
            assert_eq!(&recovered, c, "u∘d must be the identity on {c:?}");
        }
    }

    #[test]
    fn walker_is_deterministic_and_hard_fails_on_a_constraint_free_document() {
        let a = read_all_opt_constraints(&blutdruck(), BASE).unwrap();
        let b = read_all_opt_constraints(&blutdruck(), BASE).unwrap();
        assert_eq!(a, b, "the walker must be deterministic in document order");
        let err = read_all_opt_constraints("<template></template>", BASE).unwrap_err();
        assert!(err.to_string().contains("no C_DV_QUANTITY"), "got: {err}");
    }
}
