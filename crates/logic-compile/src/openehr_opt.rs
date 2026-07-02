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
//! find that interval for a given `node_id`; [`read_all_opt_constraints`] is the
//! SINGLE production reader — it walks the whole OPT and packages every recognized
//! constraint node as an [`crate::opt_lift::OptConstraintIr`] — the XML-free value
//! the `logic:` lift and the SHACL Core / ShEx projections consume. This crate does the XML
//! parsing ONLY; the SHACL/ShEx surfaces are projected in `gmeow-logic-compile` from the
//! canonical `logic:ValidationShape` (Principle 4 — the canon is the authoring ground; there
//! is no direct OPT→SHACL emit).
//!
//! OPT files reuse `lower_included`/`upper_included` field names inside many
//! unrelated interval blocks (`occurrences`, `existence`, `precision`, and
//! even a `DV_CODED_TEXT`-valued `ELEMENT` that happens to share an at-code
//! with a `DV_QUANTITY`-valued one elsewhere in the template). [`read_magnitude_interval`]
//! therefore does not grab the first interval it finds; it descends a fixed
//! structural path — `ELEMENT[node_id] → attributes[rm_attribute_name=value]
//! → children[xsi:type=C_DV_QUANTITY] → list → magnitude` — and hard-fails if
//! any step of that path is absent for the requested `node_id`. [`read_all_opt_constraints`]
//! faces the same at-code reuse when naming shapes: it resolves each constraint's ENCLOSING
//! at-code from the nearest ancestor `<node_id>`, so two unrelated `ELEMENT`s that happen to
//! share an at-code (as above) still mint distinct, stable shape names.

use std::fmt;

use crate::opt_lift::{OptConstraintIr, OptConstraintKind, OptInterval};

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

fn element_text_u32(node: roxmltree::Node, name: &str) -> Result<u32, OptError> {
    let child = child_element(node, name).ok_or_else(|| {
        opt_err(format!(
            "missing <{name}> under <{}>",
            node.tag_name().name()
        ))
    })?;
    let text = child
        .text()
        .ok_or_else(|| opt_err(format!("<{name}> has no text content")))?;
    text.trim().parse::<u32>().map_err(|e| {
        opt_err(format!(
            "<{name}> value {text:?} is not a non-negative integer: {e}"
        ))
    })
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

/// Walks an OPT document and extracts EVERY constraint it recognizes — `C_DV_QUANTITY`
/// magnitude intervals, `C_STRING` regex patterns, `C_CODE_PHRASE` coded value sets,
/// `C_COMPLEX_OBJECT` `<occurrences>` cardinality, and `<term_bindings>` external terminology
/// references — as pure [`OptConstraintIr`] values in document order.
///
/// Shape/class IRIs are minted from the constraint's ENCLOSING at-code (the nearest ancestor
/// element — possibly the node itself — carrying a non-empty `<node_id>`), never from a
/// document-order index: the same OPT always yields the same shapes, and a shape's identity
/// survives unrelated edits elsewhere in the template. `naming` maps an at-code (e.g. `at0004`)
/// to a meaningful local name (e.g. `Systolic`); an at-code absent from `naming` falls back to
/// the raw at-code itself. The bare mapped name is reserved for the `Quantity` family (the
/// production data-value constraints `naming` exists to name); any other family whose enclosing
/// at-code collides with a `naming` entry is qualified with its family tag, and any further
/// collision (the same candidate name minted twice — e.g. two `C_COMPLEX_OBJECT` cardinality
/// nodes sharing an at-code) is disambiguated with a stable, content-derived counter suffix. A
/// constraint with no enclosing at-code at all (e.g. a root-level `<term_bindings>`) is named
/// from the OPT's own archetype/template id instead.
///
/// Hard-fails on a recognized-but-malformed constraint node (no silent skip) and on an OPT with
/// no recognized constraint at all.
///
/// This is the SINGLE production OPT reader: the constraints axis lifts every
/// [`OptConstraintIr`] this walker yields, not just the curated blood-pressure quantity pair.
pub fn read_all_opt_constraints(
    opt_xml: &str,
    base_iri: &str,
    naming: &std::collections::BTreeMap<String, String>,
) -> Result<Vec<OptConstraintIr>, OptError> {
    let doc = roxmltree::Document::parse(opt_xml)
        .map_err(|e| opt_err(format!("XML parse failure: {e}")))?;
    let archetype_tag = archetype_or_template_tag(&doc);
    let mut used_locals: std::collections::BTreeMap<String, u32> =
        std::collections::BTreeMap::new();
    let mut out = Vec::new();
    // `idx` is an internal loop counter for error-message context ONLY — it must never be
    // minted into a shape/target/path IRI (that was the positional-naming bug this walker
    // replaces).
    let mut idx = 0usize;
    for node in doc.descendants().filter(|n| n.is_element()) {
        let at_code = enclosing_at_code(node);
        let label = at_code.as_deref().unwrap_or(&archetype_tag);
        let kind = match xsi_type(node) {
            Some("C_DV_QUANTITY") => {
                let (interval, units) = magnitude_from_quantity(node, label)?;
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
            Some("C_CODE_PHRASE") => code_phrase_value_set(node, base_iri)?,
            // A complex-object node carries its multiplicity as a direct `<occurrences>`
            // interval — the OPT's cardinality/occurrences constraint node kind.
            Some("C_COMPLEX_OBJECT") => cardinality_from_occurrences(node, base_iri, label)?,
            // `<term_bindings>` is not an `xsi:type`-tagged node; match it by tag name.
            _ if node.tag_name().name() == "term_bindings" => {
                terminology_from_bindings(node, base_iri)
            }
            _ => None,
        };
        if let Some(kind) = kind {
            let family = family_tag(&kind);
            let is_priority_family = matches!(kind, OptConstraintKind::Quantity { .. });
            let candidate = match &at_code {
                Some(code) => match naming.get(code) {
                    Some(name) if is_priority_family => name.clone(),
                    Some(name) => format!("{name}-{family}"),
                    None => code.clone(),
                },
                None => format!("{archetype_tag}-{family}"),
            };
            let local = dedupe_local(candidate, &mut used_locals);
            out.push(OptConstraintIr {
                shape_iri: format!("{base_iri}{local}Shape"),
                target_class: format!("{base_iri}{local}"),
                kind,
            });
            idx += 1;
        }
    }
    let _ = idx;
    if out.is_empty() {
        return Err(opt_err(
            "no C_DV_QUANTITY / C_STRING / C_CODE_PHRASE / occurrences / term_binding constraint \
             found in OPT",
        ));
    }
    Ok(out)
}

/// The at-code (`<node_id>` text) of the nearest ancestor of `node` — possibly `node` itself —
/// that carries a non-empty `<node_id>` child. Returns `None` when no ancestor up to the
/// document root carries one (a genuinely at-code-free constraint, e.g. a root `term_bindings`).
fn enclosing_at_code(node: roxmltree::Node) -> Option<String> {
    node.ancestors().find_map(|n| {
        child_element(n, "node_id")
            .and_then(|nid| nid.text())
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
    })
}

/// A stable tag for an OPT document: the root `<template_id>/<value>`, falling back to the
/// `<definition>`'s `<archetype_id>/<value>`, falling back to a fixed `"root"` tag. Used to
/// name at-code-free constraints (e.g. a root-level `<term_bindings>`).
fn archetype_or_template_tag(doc: &roxmltree::Document) -> String {
    let root = doc.root_element();
    if let Some(v) = child_element(root, "template_id")
        .and_then(|t| child_element(t, "value"))
        .and_then(|v| v.text())
    {
        return v.trim().to_owned();
    }
    if let Some(def) = child_element(root, "definition") {
        if let Some(v) = child_element(def, "archetype_id")
            .and_then(|a| child_element(a, "value"))
            .and_then(|v| v.text())
        {
            return v.trim().to_owned();
        }
    }
    "root".to_owned()
}

/// A short, stable family tag for a constraint kind — used to qualify a local name that would
/// otherwise collide with another family sharing the same at-code, and to name at-code-free
/// constraints from the archetype tag.
fn family_tag(kind: &OptConstraintKind) -> &'static str {
    match kind {
        OptConstraintKind::Quantity { .. } => "quantity",
        OptConstraintKind::Cardinality { .. } => "cardinality",
        OptConstraintKind::ValueSet { .. } => "valueSet",
        OptConstraintKind::DateTime { .. } => "dateTime",
        OptConstraintKind::StringPattern { .. } => "stringPattern",
        OptConstraintKind::TerminologyBinding { .. } => "termBinding",
    }
}

/// Disambiguates `candidate` against every local name minted so far in this walk, appending a
/// deterministic `-2`, `-3`, … counter suffix on a repeat (content-derived, NOT a document-order
/// index — the same candidate always grows the same suffix sequence).
fn dedupe_local(candidate: String, used: &mut std::collections::BTreeMap<String, u32>) -> String {
    match used.get_mut(&candidate) {
        None => {
            used.insert(candidate.clone(), 2);
            candidate
        }
        Some(next) => {
            let disambiguated = format!("{candidate}-{next}");
            *next += 1;
            disambiguated
        }
    }
}

/// Extract the magnitude interval + units from a `C_DV_QUANTITY` node's `<list><magnitude>`. An
/// unbounded end (`lower_unbounded`/`upper_unbounded` = `true`, or the bound element simply
/// absent) is an open interval end (`None`); a bound that is PRESENT but not a valid number is a
/// hard failure, as is a `<list>`/`<magnitude>`/`<units>` structure that is missing outright.
fn magnitude_from_quantity(
    node: roxmltree::Node,
    label: &str,
) -> Result<(OptInterval, String), OptError> {
    let list = child_element(node, "list")
        .ok_or_else(|| opt_err(format!("C_DV_QUANTITY {label:?}: no <list>")))?;
    let magnitude = child_element(list, "magnitude").ok_or_else(|| {
        opt_err(format!(
            "C_DV_QUANTITY {label:?}: <list> has no <magnitude>"
        ))
    })?;
    let units = child_element(list, "units")
        .and_then(|u| u.text())
        .ok_or_else(|| opt_err(format!("C_DV_QUANTITY {label:?}: no <units>")))?
        .to_owned();
    Ok((
        OptInterval {
            lower: optional_bound(magnitude, "lower", "lower_unbounded")?,
            upper: optional_bound(magnitude, "upper", "upper_unbounded")?,
            lower_included: element_text_bool(magnitude, "lower_included")?,
            upper_included: element_text_bool(magnitude, "upper_included")?,
        },
        units,
    ))
}

/// Reads one open-or-closed magnitude bound: an explicit `<{unbounded_name}>true</>` (mirroring
/// [`cardinality_from_occurrences`]'s `*_unbounded` handling) or a genuinely missing bound
/// element both mean "open" (`None`); a bound element that IS present must parse as a number.
fn optional_bound(
    node: roxmltree::Node,
    name: &str,
    unbounded_name: &str,
) -> Result<Option<f64>, OptError> {
    if let Some(u) = child_element(node, unbounded_name) {
        let text = u
            .text()
            .ok_or_else(|| opt_err(format!("<{unbounded_name}> has no text content")))?;
        match text.trim() {
            "true" => return Ok(None),
            "false" => {}
            other => {
                return Err(opt_err(format!(
                    "<{unbounded_name}> value {other:?} is not a boolean"
                )))
            }
        }
    }
    match child_element(node, name) {
        None => Ok(None),
        Some(child) => {
            let text = child
                .text()
                .ok_or_else(|| opt_err(format!("<{name}> has no text content")))?;
            text.trim()
                .parse::<f64>()
                .map(Some)
                .map_err(|e| opt_err(format!("<{name}> value {text:?} is not a number: {e}")))
        }
    }
}

/// Extract a coded value set (`<code_list>` codes qualified by `<terminology_id><value>`)
/// from a `C_CODE_PHRASE` node. `Ok(None)` when the node carries no `<code_list>`; hard-fails
/// when a `<code_list>` is present but its `<terminology_id>` qualifier is absent (a malformed
/// OPT — never mint a bogus `unknown` terminology).
fn code_phrase_value_set(
    node: roxmltree::Node,
    base_iri: &str,
) -> Result<Option<OptConstraintKind>, OptError> {
    let raw_codes: Vec<&str> = node
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "code_list")
        .filter_map(|c| c.text())
        .collect();
    if raw_codes.is_empty() {
        return Ok(None);
    }
    let terminology = child_element(node, "terminology_id")
        .and_then(|t| child_element(t, "value"))
        .and_then(|v| v.text())
        .map(|s| s.trim().to_owned())
        .ok_or_else(|| {
            opt_err("C_CODE_PHRASE with a <code_list> is missing its <terminology_id>/<value>")
        })?;
    // A value set is a SET — carry the members in canonical (sorted) order so the lifted shape
    // (whose value-set members are sorted at construction) recovers to the identical IR.
    let mut codes: Vec<String> = raw_codes
        .iter()
        .map(|s| format!("{base_iri}terminology/{terminology}/{}", s.trim()))
        .collect();
    codes.sort();
    Ok(Some(OptConstraintKind::ValueSet {
        path: format!("{base_iri}definingCode"),
        codes,
    }))
}

/// Extract a cardinality constraint from a complex-object node's direct `<occurrences>`
/// multiplicity interval — the OPT's `occurrences`/`existence` constraint node kind. Returns
/// `None` when the node has no `<occurrences>` child. An unbounded end (`*_unbounded = true`)
/// maps to an open count (`None`); a bounded end reads its non-negative-integer value.
fn cardinality_from_occurrences(
    node: roxmltree::Node,
    base_iri: &str,
    label: &str,
) -> Result<Option<OptConstraintKind>, OptError> {
    let Some(occ) = child_element(node, "occurrences") else {
        return Ok(None);
    };
    let min = if element_text_bool(occ, "lower_unbounded")? {
        None
    } else {
        Some(element_text_u32(occ, "lower")?)
    };
    let max = if element_text_bool(occ, "upper_unbounded")? {
        None
    } else {
        Some(element_text_u32(occ, "upper")?)
    };
    Ok(Some(OptConstraintKind::Cardinality {
        path: format!("{base_iri}occurrences/{label}"),
        min,
        max,
    }))
}

/// Extract an external terminology binding from a `<term_bindings terminology="…">` block: the
/// `terminology` attribute is the id, each `<items>/<value>/<code_string>` is a bound code. The
/// codes are sorted (a binding set is order-free) so the lifted shape recovers to the identical
/// IR. Returns `None` when the block binds no code.
fn terminology_from_bindings(node: roxmltree::Node, base_iri: &str) -> Option<OptConstraintKind> {
    let terminology_id = node
        .attributes()
        .find(|a| a.name() == "terminology")
        .map(|a| a.value())
        .unwrap_or("unknown")
        .trim()
        .to_owned();
    let mut codes: Vec<String> = node
        .descendants()
        .filter(|d| d.is_element() && d.tag_name().name() == "code_string")
        .filter_map(|d| d.text())
        .map(|s| s.trim().to_owned())
        .collect();
    if codes.is_empty() {
        return None;
    }
    codes.sort();
    Some(OptConstraintKind::TerminologyBinding {
        path: format!("{base_iri}termBinding"),
        terminology_id,
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

    #[test]
    fn code_phrase_without_terminology_hard_fails() {
        // A <code_list> with no <terminology_id> is malformed — hard-fail rather than mint a
        // bogus `unknown` terminology.
        let xml = "<c xsi:type=\"C_CODE_PHRASE\" \
                   xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\
                   <code_list>at0004</code_list></c>";
        let naming = std::collections::BTreeMap::new();
        let err = read_all_opt_constraints(xml, "https://ex/", &naming).unwrap_err();
        assert!(err.to_string().contains("terminology_id"), "got: {err}");
    }
}

#[cfg(test)]
mod walker_tests {
    use super::*;
    use crate::opt_lift::{lift_opt_to_validation_shape, recover_opt_from_shape};
    use std::path::PathBuf;

    fn blutdruck() -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("../../validations/openehr-bloodpressure/Blutdruck.opt");
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    const BASE: &str = "https://gmeow.example/openehr/bp/";

    /// The naming map the production pipeline uses for the vendored Blutdruck OPT: the
    /// systolic/diastolic at-codes get meaningful local names, every other recognized at-code
    /// falls back to its raw form.
    fn bp_naming() -> std::collections::BTreeMap<String, String> {
        std::collections::BTreeMap::from([
            ("at0004".to_string(), "Systolic".to_string()),
            ("at0005".to_string(), "Diastolic".to_string()),
        ])
    }

    #[test]
    fn walker_extracts_every_constraint_family_present_in_the_real_opt() {
        let naming = std::collections::BTreeMap::new();
        let all =
            read_all_opt_constraints(&blutdruck(), BASE, &naming).expect("walk Blutdruck.opt");
        // The vendored OPT carries C_DV_QUANTITY magnitudes, C_STRING patterns, a C_CODE_PHRASE
        // code list, C_COMPLEX_OBJECT <occurrences> cardinality, and a <term_bindings> block —
        // the walker must surface every one of those families.
        let has = |pred: fn(&OptConstraintKind) -> bool| all.iter().any(|c| pred(&c.kind));
        assert!(
            has(|k| matches!(k, OptConstraintKind::Quantity { .. })),
            "no quantity extracted from {} constraints",
            all.len()
        );
        assert!(
            has(|k| matches!(k, OptConstraintKind::StringPattern { .. })),
            "no string pattern extracted"
        );
        assert!(
            has(|k| matches!(k, OptConstraintKind::ValueSet { .. })),
            "no coded value set extracted"
        );
        assert!(
            has(|k| matches!(k, OptConstraintKind::Cardinality { .. })),
            "no occurrences cardinality extracted"
        );
        assert!(
            has(|k| matches!(k, OptConstraintKind::TerminologyBinding { .. })),
            "no terminology binding extracted"
        );
    }

    #[test]
    fn every_walked_constraint_round_trips_u_after_d_is_identity() {
        // The section/retraction law holds on EVERY constraint the walker reads from real XML.
        let naming = bp_naming();
        let all = read_all_opt_constraints(&blutdruck(), BASE, &naming).expect("walk");
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
        let naming = bp_naming();
        let a = read_all_opt_constraints(&blutdruck(), BASE, &naming).unwrap();
        let b = read_all_opt_constraints(&blutdruck(), BASE, &naming).unwrap();
        assert_eq!(a, b, "the walker must be deterministic in document order");
        let empty_naming = std::collections::BTreeMap::new();
        let err =
            read_all_opt_constraints("<template></template>", BASE, &empty_naming).unwrap_err();
        assert!(err.to_string().contains("no C_DV_QUANTITY"), "got: {err}");
    }

    #[test]
    fn walker_mints_meaningful_stable_names_from_the_naming_map_not_positional_ones() {
        // Proves the walker mints the SAME shape/target identity the curated production reader
        // used to hand-wire, purely from the enclosing at-code + naming map — never a
        // `Constraint-{idx}` positional name.
        let naming = bp_naming();
        let all = read_all_opt_constraints(&blutdruck(), BASE, &naming).expect("walk");
        let systolic = all
            .iter()
            .find(|c| {
                matches!(&c.kind, OptConstraintKind::Quantity { units, .. } if units == "mm[Hg]")
                    && c.target_class == format!("{BASE}Systolic")
            })
            .expect("a Systolic quantity shape");
        assert_eq!(systolic.target_class, format!("{BASE}Systolic"));
        assert_eq!(systolic.shape_iri, format!("{BASE}SystolicShape"));
        assert!(
            !all.iter()
                .any(|c| c.shape_iri.contains("shape-") || c.target_class.contains("Constraint-")),
            "no minted IRI may be positional: {all:?}"
        );
    }
}
