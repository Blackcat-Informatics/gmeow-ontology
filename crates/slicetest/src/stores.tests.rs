// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn rdfs_close_infers_type_and_subclass() {
    // A synthetic graph exercising the entailments competency questions rely
    // on: a domain typing (rdfs2) and type propagation up a subclass chain
    // (rdfs9 + rdfs11) — neither present in the asserted data.
    let nt = concat!(
        "<https://example.org/hasPet> <http://www.w3.org/2000/01/rdf-schema#domain> <https://example.org/Owner> .\n",
        "<https://example.org/Owner> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://example.org/Person> .\n",
        "<https://example.org/Person> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://example.org/Agent> .\n",
        "<https://example.org/ada> <https://example.org/hasPet> <https://example.org/cat> .\n",
    );
    let base = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("synthetic NT must load");
    let closed = rdfs_close(base).expect("rdfs closure must converge");

    // rdfs2: ada hasPet => ada a Owner. rdfs9/11: => Person, => Agent.
    for cls in ["Owner", "Person", "Agent"] {
        assert!(
            ask_type(
                &closed,
                "https://example.org/ada",
                &format!("https://example.org/{cls}")
            ),
            "expected ex:ada to be inferred a ex:{cls}"
        );
    }
}

fn ask_type(store: &Arc<RdfDataset>, s: &str, c: &str) -> bool {
    let q = format!("ASK {{ <{s}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{c}> }}");
    matches!(
        native_query::query(store, &q).expect("ask"),
        SparqlResult::Boolean(true)
    )
}
