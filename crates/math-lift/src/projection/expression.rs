// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Target serialization of the authored expression grammar, without an identity fork.

use std::collections::{BTreeMap, HashSet};

use purrdf::RdfTerm;

use super::{RDF_TYPE, SKOS_EXACT, Source, Target, invalid, invalid_source, math, xml};

const OPENMATH: &str = "http://www.openmath.org/cd/";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

pub(super) fn render(
    input: &mut Source,
    root: &RdfTerm,
    target: Target,
) -> gmeow_errors::Result<String> {
    enum Step {
        Enter(RdfTerm),
        Exit(RdfTerm),
    }
    let mut pending = vec![Step::Enter(root.clone())];
    let mut active = HashSet::new();
    let mut output = String::new();
    while let Some(step) = pending.pop() {
        let node = match step {
            Step::Exit(node) => {
                output.push_str(if target == Target::MathMl {
                    "</apply>"
                } else {
                    "</OMA>"
                });
                active.remove(&node);
                continue;
            }
            Step::Enter(node) => node,
        };
        if !active.insert(node.clone()) {
            return Err(invalid(
                "cyclic mathematical expression cannot be serialized as a content tree",
            ));
        }
        let types = input.values(&node, RDF_TYPE);
        let forms = [
            "NumberLiteral",
            "SymbolReference",
            "VariableExpression",
            "ApplicationExpression",
            "BindingExpression",
        ];
        let selected: Vec<_> = forms
            .into_iter()
            .filter(|class| types.contains(&RdfTerm::iri(math(class))))
            .collect();
        let leaf = match selected.as_slice() {
            ["NumberLiteral"] => number(input, &node, target)?,
            ["SymbolReference"] => {
                let symbol = input.one(&node, &math("hasMathematicalSymbol"))?;
                symbol_xml(input, &symbol, target)?
            }
            ["VariableExpression"] => variable(input, &node, target)?,
            ["ApplicationExpression"] => {
                let operator = input.one(&node, &math("operator"))?;
                let operator = symbol_xml(input, &operator, target)?;
                let slots = input.values(&node, &math("argumentSlot"));
                if slots.is_empty() {
                    return Err(invalid("application must supply indexed argument slots"));
                }
                let mut ordered = BTreeMap::new();
                for slot in slots {
                    input.typed(&slot, &math("ArgumentSlot"))?;
                    let position = input.literal(&slot, &math("slotIndex"))?;
                    if position.language.is_some()
                        || position.direction.is_some()
                        || ![format!("{XSD}integer"), format!("{XSD}nonNegativeInteger")]
                            .contains(&position.datatype_iri().to_owned())
                    {
                        return Err(invalid(
                            "argument slot position requires an integer datatype",
                        ));
                    }
                    let index: usize =
                        canonical_integer(&position.lexical_form)?
                            .parse()
                            .map_err(|_| {
                                invalid(
                                    "argument slot index must be a nonnegative in-range integer",
                                )
                            })?;
                    let operand = input.one(&slot, &math("slotExpression"))?;
                    if ordered.insert(index, operand).is_some() {
                        return Err(invalid("duplicate argument slot index"));
                    }
                }
                for (expected, index) in ordered.keys().enumerate() {
                    if expected != *index {
                        return Err(invalid("argument slots must be contiguous from zero"));
                    }
                }
                output.push_str(if target == Target::MathMl {
                    "<apply>"
                } else {
                    "<OMA>"
                });
                output.push_str(&operator);
                pending.push(Step::Exit(node));
                pending.extend(ordered.into_values().rev().map(Step::Enter));
                continue;
            }
            ["BindingExpression"] => {
                return Err(invalid(
                    "selected content codec does not admit this mathematical binder",
                ));
            }
            _ => {
                return Err(invalid(format!(
                    "expression {node:?} requires one supported, unambiguous grammar form"
                )));
            }
        };
        output.push_str(&leaf);
        active.remove(&node);
    }
    Ok(output)
}

fn symbol_xml(
    input: &mut Source,
    symbol: &RdfTerm,
    target: Target,
) -> gmeow_errors::Result<String> {
    if target == Target::MathMl {
        let RdfTerm::Iri(iri) = symbol else {
            return Err(invalid("MathML symbol requires a named canonical identity"));
        };
        if !input.rows.contains_key(symbol) {
            return Err(invalid(
                "MathML symbol has no declared canonical definition",
            ));
        }
        input.visited.insert(symbol.clone());
        return Ok(format!(
            "<csymbol definitionURL=\"{}\">{}</csymbol>",
            xml(iri),
            xml(iri)
        ));
    }
    let mut mappings = input.values(symbol, SKOS_EXACT);
    if mappings.is_empty() {
        mappings = input.values(symbol, "http://www.w3.org/2004/02/skos/core#closeMatch");
        if !mappings.is_empty() {
            if let RdfTerm::Iri(iri) = symbol {
                input.boundaries.insert(iri.clone());
            }
            input
                .boundaries
                .insert("http://www.w3.org/2004/02/skos/core#closeMatch".to_owned());
        }
    }

    let symbols: Vec<_> = mappings
        .iter()
        .filter_map(|term| match term {
            RdfTerm::Iri(iri) if iri.starts_with(OPENMATH) => Some(iri),
            _ => None,
        })
        .collect();
    let [iri] = symbols.as_slice() else {
        return Err(invalid(format!(
            "{symbol:?} requires one resolved OpenMath content-dictionary symbol"
        )));
    };
    let (cd, name) = iri[OPENMATH.len()..]
        .split_once('#')
        .ok_or_else(|| invalid("OpenMath symbol must name a dictionary fragment"))?;
    if !purrdf_lex::terminals::is_ncname(cd) || !purrdf_lex::terminals::is_ncname(name) {
        return Err(invalid("malformed OpenMath content-dictionary identity"));
    }
    Ok(format!(
        "<OMS cd=\"{}\" name=\"{}\" cdbase=\"http://www.openmath.org/cd\"/>",
        xml(cd),
        xml(name)
    ))
}

fn variable(input: &mut Source, node: &RdfTerm, target: Target) -> gmeow_errors::Result<String> {
    let occurrence = input.one(node, &math("variableOccurrence"))?;
    input.typed(&occurrence, &math("VariableOccurrence"))?;
    let declaration = input.one(&occurrence, &math("declaredVariable"))?;
    // Binders need scope-aware lowering; this leaf profile admits only explicit free
    // declarations. A label never creates a declaration or changes its identity.
    input.typed(&declaration, &math("FreeVariableDeclaration"))?;
    let RdfTerm::Iri(iri) = declaration else {
        return Err(invalid(
            "free variable declaration requires a named identity",
        ));
    };
    Ok(if target == Target::MathMl {
        format!("<ci>{}</ci>", xml(&iri))
    } else {
        // OpenMath names are NCNames, whereas canonical declarations are IRIs.
        // A reversible byte encoding preserves distinct free-variable identities
        // without placing a colon or slash in the target name.
        input.boundaries.insert(iri.clone());
        format!(
            "<OMV name=\"v{}\"/>",
            purrdf_hash::hex::encode(iri.as_bytes())
        )
    })
}

fn number(input: &mut Source, node: &RdfTerm, target: Target) -> gmeow_errors::Result<String> {
    let value = input.one(node, &math("literalValue"))?;
    let value = match value {
        RdfTerm::Literal(value) => value,
        number @ RdfTerm::Iri(_) => {
            input.typed(&number, &math("RationalValue"))?;
            let numerator = input.literal(&number, &math("numerator"))?;
            let denominator = input.literal(&number, &math("denominator"))?;
            integer_literal(&numerator)?;
            integer_literal(&denominator)?;
            let numerator = canonical_integer(&numerator.lexical_form)?;
            let denominator = canonical_integer(&denominator.lexical_form)?;
            if denominator == "0" {
                return Err(invalid("exact rational denominator cannot be zero"));
            }
            return Ok(if target == Target::MathMl {
                format!("<cn type=\"rational\">{numerator}<sep/>{denominator}</cn>")
            } else {
                format!(
                    "<OMA><OMS cd=\"nums1\" name=\"rational\"/><OMI>{numerator}</OMI><OMI>{denominator}</OMI></OMA>"
                )
            });
        }
        _ => {
            return Err(invalid(
                "numeric expression requires a typed literal or an explicit rational value",
            ));
        }
    };
    if value.language.is_some() || value.direction.is_some() {
        return Err(invalid("numeric literal cannot have language or direction"));
    }
    super::numeric(&value)?;
    let lexical = value.lexical_form.trim_matches([' ', '\t', '\r', '\n']);
    let datatype = value.datatype_iri();
    if datatype == format!("{XSD}integer") {
        let lexical = canonical_integer(lexical)?;
        return Ok(if target == Target::MathMl {
            format!("<cn type=\"integer\">{}</cn>", xml(&lexical))
        } else {
            format!("<OMI>{}</OMI>", xml(&lexical))
        });
    }
    if datatype == format!("{XSD}decimal") {
        let unsigned = lexical.strip_prefix(['+', '-']).unwrap_or(lexical);
        let mut parts = unsigned.split('.');
        let integral = parts.next().unwrap_or_default();
        let fraction = parts.next().unwrap_or_default();
        if parts.next().is_some()
            || (integral.is_empty() && fraction.is_empty())
            || !integral
                .bytes()
                .chain(fraction.bytes())
                .all(|byte| byte.is_ascii_digit())
        {
            return Err(invalid("invalid xsd:decimal lexical value"));
        }
        if target == Target::MathMl {
            return Ok(format!("<cn type=\"real\">{}</cn>", xml(lexical)));
        }
        let numerator = format!(
            "{}{}{}",
            if lexical.starts_with('-') { "-" } else { "" },
            integral,
            fraction
        );
        let denominator = format!("1{}", "0".repeat(fraction.len()));
        return Ok(format!(
            "<OMA><OMS cd=\"nums1\" name=\"rational\"/><OMI>{numerator}</OMI><OMI>{denominator}</OMI></OMA>"
        ));
    }
    Err(invalid(format!(
        "numeric datatype {datatype} has no admitted exact content encoding"
    )))
}

fn integer(value: &str) -> gmeow_errors::Result<()> {
    purrdf::xsd::parse_by_iri(
        value.trim_matches([' ', '\t', '\r', '\n']),
        &format!("{XSD}integer"),
    )
    .map_err(|error| invalid_source(format!("invalid integer literal: {error}"), error))?
    .ok_or_else(|| invalid("integer datatype is not admitted"))?;
    Ok(())
}

pub(super) fn integer_literal(value: &purrdf::RdfLiteral) -> gmeow_errors::Result<()> {
    if value.datatype_iri() != format!("{XSD}integer")
        || value.language.is_some()
        || value.direction.is_some()
    {
        return Err(invalid("exact rational cells require xsd:integer literals"));
    }
    integer(&value.lexical_form)
}

pub(super) fn canonical_integer(value: &str) -> gmeow_errors::Result<String> {
    let value = purrdf::xsd::parse_by_iri(
        value.trim_matches([' ', '\t', '\r', '\n']),
        &format!("{XSD}integer"),
    )
    .map_err(|error| invalid_source(format!("invalid integer literal: {error}"), error))?
    .ok_or_else(|| invalid("integer datatype is not admitted"))?;
    Ok(value.canonical_lexical())
}
