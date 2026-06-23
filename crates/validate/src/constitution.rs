// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3-free native enforcement-coverage check for the constitution gate (#809).
//!
//! Ports `constitution._check_enforcement_coverage` — the one purely
//! graph-resident constitution check — to native Rust over a `gmeow_rdf`/oxigraph
//! [`Store`] (the B3 template). The OTHER constitution checks (principle↔markdown
//! title sync, cited-artifact / symbol / make-target / CLI-command existence,
//! generator-registry, supersession markers) are inherently Python-introspection
//! — they probe the filesystem, parse Python ASTs, introspect the Typer app, and
//! read the live generator registry — so they cannot move to RDF and stay in
//! Python, emitting *granular* findings through this same canonical model.
//!
//! Coverage rules (verbatim from the Python): every principle must declare ≥1
//! existing enforcement; a principle whose only enforcements are `meta:Practice`
//! is the honor system (a **warning**); an enforcement mapped to no principle is
//! an orphan (an **error**); a principle citing an undeclared enforcement is an
//! error. Message phrases are preserved so the substring-asserting tests pass.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_diagnostics::{Finding, Severity};
use oxigraph::model::{NamedNode, NamedNodeRef, NamedOrBlankNode, Term};
use oxigraph::store::Store;

use crate::model::rdf;

/// The governance meta namespace (`constitution.META`).
const META: &str = "https://blackcatinformatics.ca/gmeow/meta#";
/// The enforcement classes; `Practice` is the honor-system kind.
const ENFORCEMENT_KINDS: &[&str] = &["Lint", "TestSuite", "Shape", "Gate", "Practice"];
/// `rdfs:Class` — a node ALSO typed as this is a class declaration, not an
/// enforcement instance (mirrors the Python `(node, RDF.type, RDFS.Class)` skip).
const RDFS_CLASS: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("http://www.w3.org/2000/01/rdf-schema#Class");

/// One principle reconstructed from the manifest graph.
struct Principle {
    number: i64,
    title: String,
    enforced_by: Vec<String>,
}

fn meta(local: &str) -> NamedNode {
    NamedNode::new(format!("{META}{local}")).expect("valid meta IRI")
}

/// Whether `node` is also declared an `rdfs:Class` (a class definition to skip).
fn is_rdfs_class(store: &Store, node: &NamedNode) -> bool {
    store
        .quads_for_pattern(
            Some(node.as_ref().into()),
            Some(rdf::TYPE),
            Some(RDFS_CLASS.into()),
            None,
        )
        .next()
        .transpose()
        .expect("rdfs:Class lookup: in-memory store query failed")
        .is_some()
}

/// Collect the declared enforcement instances → their kind.
fn collect_enforcements(store: &Store) -> BTreeMap<String, String> {
    let mut enforcements = BTreeMap::new();
    for kind in ENFORCEMENT_KINDS {
        let type_node = meta(kind);
        for quad in store
            .quads_for_pattern(None, Some(rdf::TYPE), Some(type_node.as_ref().into()), None)
            .flatten()
        {
            if let NamedOrBlankNode::NamedNode(node) = quad.subject {
                if is_rdfs_class(store, &node) {
                    continue;
                }
                enforcements.insert(node.as_str().to_string(), (*kind).to_string());
            }
        }
    }
    enforcements
}

/// Collect the principles (number, title, enforced_by edges).
fn collect_principles(store: &Store) -> Vec<Principle> {
    let principle_type = meta("Principle");
    let number_p = meta("number");
    let title_p = meta("title");
    let enforced_p = meta("enforcedBy");

    let mut principles = Vec::new();
    for quad in store
        .quads_for_pattern(
            None,
            Some(rdf::TYPE),
            Some(principle_type.as_ref().into()),
            None,
        )
        .flatten()
    {
        let NamedOrBlankNode::NamedNode(node) = quad.subject else {
            continue;
        };
        let number = store
            .quads_for_pattern(
                Some(node.as_ref().into()),
                Some(number_p.as_ref()),
                None,
                None,
            )
            .flatten()
            .find_map(|q| literal_i64(&q.object))
            .unwrap_or(-1);
        let title = store
            .quads_for_pattern(
                Some(node.as_ref().into()),
                Some(title_p.as_ref()),
                None,
                None,
            )
            .flatten()
            .find_map(|q| literal_string(&q.object))
            .unwrap_or_default();
        let mut enforced_by: Vec<String> = store
            .quads_for_pattern(
                Some(node.as_ref().into()),
                Some(enforced_p.as_ref()),
                None,
                None,
            )
            .flatten()
            .filter_map(|q| match q.object {
                Term::NamedNode(n) => Some(n.as_str().to_string()),
                _ => None,
            })
            .collect();
        enforced_by.sort();
        principles.push(Principle {
            number,
            title,
            enforced_by,
        });
    }
    principles.sort_by_key(|p| p.number);
    principles
}

fn literal_i64(term: &Term) -> Option<i64> {
    match term {
        Term::Literal(lit) => lit.value().parse().ok(),
        _ => None,
    }
}

fn literal_string(term: &Term) -> Option<String> {
    match term {
        Term::Literal(lit) => Some(lit.value().to_string()),
        _ => None,
    }
}

/// Run the enforcement-coverage check over the parsed manifest store, returning
/// granular `constitution.<code>` findings (verbatim phrases from the Python).
pub fn check_enforcement_coverage(store: &Store) -> Vec<Finding> {
    let enforcements = collect_enforcements(store);
    let principles = collect_principles(store);

    let mut findings = Vec::new();
    let mut cited: BTreeSet<String> = BTreeSet::new();

    for principle in &principles {
        // Single pass over the cited enforcements: an entry mapped to a known
        // enforcement is recorded in `cited` (and tells whether ≥1 is a real,
        // non-Practice enforcement); an unknown entry is an `undeclared` finding.
        let mut any_known = false;
        let mut has_non_practice = false;
        for e in &principle.enforced_by {
            match enforcements.get(e) {
                Some(kind) => {
                    any_known = true;
                    has_non_practice |= kind != "Practice";
                    cited.insert(e.clone());
                }
                None => findings.push(error(
                    "undeclared-enforcement",
                    format!(
                        "principle {} cites undeclared enforcement {e}",
                        principle.number
                    ),
                )),
            }
        }
        if !any_known {
            findings.push(error(
                "principle-unenforced",
                format!(
                    "principle {} ({:?}) has zero registered enforcement",
                    principle.number, principle.title
                ),
            ));
        } else if !has_non_practice {
            findings.push(
                Finding::new(
                    Severity::Warning,
                    "constitution.honor-system",
                    format!(
                        "principle {} ({:?}) is enforced only by review practice (honor system)",
                        principle.number, principle.title
                    ),
                )
                .with_tool("constitution"),
            );
        }
    }

    for orphan in enforcements.keys() {
        if !cited.contains(orphan) {
            findings.push(error(
                "orphaned-enforcement",
                format!("orphaned enforcement {orphan} maps to no principle — why does it exist?"),
            ));
        }
    }

    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    findings
}

/// Build one `constitution.<code>` error finding.
fn error(code: &str, message: String) -> Finding {
    Finding::new(Severity::Error, format!("constitution.{code}"), message).with_tool("constitution")
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::model::Literal;

    // cargo-mutants (T9, #790) surfaced surviving mutants in `literal_i64` /
    // `literal_string` — the helpers had no direct coverage, so replacing their
    // body with `None`/`Some(0)`/deleting the match arm went undetected. These
    // tests pin both the literal path and the non-literal fallthrough, killing
    // that mutant cluster.
    #[test]
    fn literal_i64_parses_only_integer_literals() {
        assert_eq!(
            literal_i64(&Term::Literal(Literal::new_simple_literal("42"))),
            Some(42)
        );
        assert_eq!(
            literal_i64(&Term::Literal(Literal::new_simple_literal("-7"))),
            Some(-7)
        );
        assert_eq!(
            literal_i64(&Term::Literal(Literal::new_simple_literal("notanint"))),
            None
        );
        assert_eq!(
            literal_i64(&Term::NamedNode(NamedNode::new("https://e/x").unwrap())),
            None
        );
    }

    #[test]
    fn literal_string_extracts_only_literal_lexical_values() {
        assert_eq!(
            literal_string(&Term::Literal(Literal::new_simple_literal("hello"))),
            Some("hello".to_string())
        );
        assert_eq!(
            literal_string(&Term::NamedNode(NamedNode::new("https://e/x").unwrap())),
            None
        );
    }

    fn store_from(ttl: &str) -> Store {
        use oxigraph::io::{RdfFormat, RdfParser};
        let store = Store::new().unwrap();
        for triple in RdfParser::from_format(RdfFormat::Turtle)
            .lenient()
            .for_reader(ttl.as_bytes())
        {
            store.insert(&triple.unwrap()).unwrap();
        }
        store
    }

    const PREFIX: &str = "@prefix meta: <https://blackcatinformatics.ca/gmeow/meta#> .\n\
         @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n";

    #[test]
    fn unenforced_principle_is_an_error() {
        let store = store_from(&format!(
            "{PREFIX}meta:P1 a meta:Principle ; meta:number 1 ; meta:title \"Solo\" .\n"
        ));
        let msgs: Vec<String> = check_enforcement_coverage(&store)
            .into_iter()
            .map(|f| f.message)
            .collect();
        assert!(msgs
            .iter()
            .any(|m| m.contains("zero registered enforcement")));
    }

    #[test]
    fn practice_only_principle_warns_and_orphan_errors() {
        let store = store_from(&format!(
            "{PREFIX}\
             meta:P1 a meta:Principle ; meta:number 1 ; meta:title \"Honor\" ; meta:enforcedBy meta:rev .\n\
             meta:rev a meta:Practice .\n\
             meta:gate-orphan a meta:Gate .\n"
        ));
        let findings = check_enforcement_coverage(&store);
        assert!(findings
            .iter()
            .any(|f| f.severity == Severity::Warning && f.message.contains("review practice")));
        assert!(findings
            .iter()
            .any(|f| f.code == "constitution.orphaned-enforcement"
                && f.message.contains("gate-orphan")));
    }
}
