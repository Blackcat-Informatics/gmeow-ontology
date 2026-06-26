// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! SPARQL Results XML (SRX) serializer plus the additive, provenance-carrying
//! `gmeow` extension.
//!
//! Document shape follows <https://www.w3.org/TR/rdf-sparql-XMLres/>: a
//! `<sparql xmlns="http://www.w3.org/2005/sparql-results#">` root with a
//! `<head>` of `<variable>`s, then either `<results>` (SELECT) or `<boolean>`
//! (ASK). The CONSTRUCT (`Graph`) kind is undefined for SRX and hard-fails with
//! [`Error::Format`].
//!
//! Two additive, namespaced extensions are emitted only when present, so the
//! default output stays pure W3C: a `gmeow:dir` attribute on `<literal>` carries
//! an RDF-1.2 base direction, and a `<gmeow:provenance>` element (after
//! `</results>`/`<boolean>`) carries a non-empty [`ResultProvenance`]. Both
//! inline the `xmlns:gmeow="https://gmeow.dev/ns/results#"` declaration so the
//! document needs no fixed prologue namespace.

use crate::error::Error;
use crate::model::ResultProvenance;
use crate::SerializeOutcome;
use gmeow_rdf_core::{SparqlResult, TermValue};

/// The `xsd:string` IRI; a literal carrying it (with no language) serializes
/// bare (no `datatype` attribute), matching the JSON/Turtle abbreviation.
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// The `gmeow` results-extension namespace IRI.
const GMEOW_NS: &str = "https://gmeow.dev/ns/results#";

/// Serialize a [`SparqlResult`] to SPARQL Results XML, appending the additive
/// `gmeow` extensions when present.
///
/// XML carries everything that is requested, so the returned
/// [`SerializeOutcome::provenance_dropped`] is always `false`.
///
/// # Errors
///
/// Returns [`Error::Format`] for a `Graph` (CONSTRUCT) result, which has no
/// defined SRX representation.
pub fn to_xml(
    result: &SparqlResult,
    provenance: &ResultProvenance,
) -> Result<SerializeOutcome, Error> {
    let mut out = String::new();
    write_srx(result, provenance, &mut out)?;
    Ok(SerializeOutcome {
        bytes: out.into_bytes(),
        provenance_dropped: false,
    })
}

/// Write the full SRX document (root + head + body + optional provenance).
fn write_srx(
    result: &SparqlResult,
    provenance: &ResultProvenance,
    out: &mut String,
) -> Result<(), Error> {
    out.push_str("<?xml version=\"1.0\"?>\n");
    out.push_str("<sparql xmlns=\"http://www.w3.org/2005/sparql-results#\">\n");

    match result {
        SparqlResult::Solutions { variables, rows } => {
            write_head(variables, out);
            write_results(variables, rows, out)?;
        }
        SparqlResult::Boolean(value) => {
            // ASK has no variables → empty head.
            out.push_str("  <head></head>\n");
            out.push_str("  <boolean>");
            out.push_str(if *value { "true" } else { "false" });
            out.push_str("</boolean>\n");
        }
        SparqlResult::Graph(_) => {
            return Err(Error::Format(
                "SPARQL Results XML is undefined for CONSTRUCT graphs; serialize the graph as RDF"
                    .to_string(),
            ));
        }
    }

    if !provenance.is_empty() {
        write_provenance(result, provenance, out);
    }

    out.push_str("</sparql>\n");
    Ok(())
}

/// Write the `<head>` of `<variable>` declarations.
fn write_head(variables: &[String], out: &mut String) {
    if variables.is_empty() {
        out.push_str("  <head></head>\n");
        return;
    }
    out.push_str("  <head>\n");
    for var in variables {
        out.push_str("    <variable name=\"");
        xml_escape_attr(var, out);
        out.push_str("\"/>\n");
    }
    out.push_str("  </head>\n");
}

/// Write the `<results>` block (one `<result>` per row; unbound cells omitted).
fn write_results(
    variables: &[String],
    rows: &[Vec<Option<TermValue>>],
    out: &mut String,
) -> Result<(), Error> {
    out.push_str("  <results>\n");
    for row in rows {
        out.push_str("    <result>\n");
        for (column, cell) in row.iter().enumerate() {
            if let Some(value) = cell {
                let var = variables.get(column).ok_or_else(|| {
                    Error::MalformedTerm(format!(
                        "binding column {column} has no variable header (row has {} vars)",
                        variables.len()
                    ))
                })?;
                out.push_str("      <binding name=\"");
                xml_escape_attr(var, out);
                out.push_str("\">");
                write_term(value, out);
                out.push_str("</binding>\n");
            }
        }
        out.push_str("    </result>\n");
    }
    out.push_str("  </results>\n");
    Ok(())
}

/// Write a single bound term element (`<uri>`/`<bnode>`/`<literal>`/`<triple>`).
fn write_term(value: &TermValue, out: &mut String) {
    match value {
        TermValue::Iri(iri) => {
            out.push_str("<uri>");
            xml_escape_text(iri, out);
            out.push_str("</uri>");
        }
        TermValue::Blank { label, .. } => {
            out.push_str("<bnode>");
            xml_escape_text(label, out);
            out.push_str("</bnode>");
        }
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            direction,
        } => {
            out.push_str("<literal");
            if let Some(language) = language {
                out.push_str(" xml:lang=\"");
                xml_escape_attr(language, out);
                out.push('"');
            } else if datatype != XSD_STRING {
                out.push_str(" datatype=\"");
                xml_escape_attr(datatype, out);
                out.push('"');
            }
            if let Some(direction) = direction {
                out.push_str(" gmeow:dir=\"");
                out.push_str(direction.as_str());
                out.push_str("\" xmlns:gmeow=\"");
                out.push_str(GMEOW_NS);
                out.push('"');
            }
            out.push('>');
            xml_escape_text(lexical_form, out);
            out.push_str("</literal>");
        }
        TermValue::Triple { s, p, o } => {
            out.push_str("<triple><subject>");
            write_term(s, out);
            out.push_str("</subject><predicate>");
            write_term(p, out);
            out.push_str("</predicate><object>");
            write_term(o, out);
            out.push_str("</object></triple>");
        }
    }
}

/// Write the additive `<gmeow:provenance>` element (only present fields).
fn write_provenance(result: &SparqlResult, provenance: &ResultProvenance, out: &mut String) {
    out.push_str("  <gmeow:provenance xmlns:gmeow=\"");
    out.push_str(GMEOW_NS);
    out.push_str("\">\n");

    out.push_str("    <gmeow:queryForm>");
    out.push_str(query_form(result));
    out.push_str("</gmeow:queryForm>\n");

    if let Some(query_hash) = &provenance.query_hash {
        out.push_str("    <gmeow:queryHash>");
        xml_escape_text(query_hash, out);
        out.push_str("</gmeow:queryHash>\n");
    }
    if let Some(engine) = &provenance.engine {
        out.push_str("    <gmeow:engine>");
        xml_escape_text(engine, out);
        out.push_str("</gmeow:engine>\n");
    }
    for solution in &provenance.solutions {
        out.push_str("    <gmeow:solution>\n");
        for source in &solution.sources {
            out.push_str("      <gmeow:source>");
            xml_escape_text(source, out);
            out.push_str("</gmeow:source>\n");
        }
        out.push_str("    </gmeow:solution>\n");
    }

    out.push_str("  </gmeow:provenance>\n");
}

/// The `queryForm` discriminator emitted in provenance. The `Graph` arm is
/// unreachable here (CONSTRUCT hard-fails earlier) but is named exhaustively.
fn query_form(result: &SparqlResult) -> &'static str {
    match result {
        SparqlResult::Solutions { .. } => "select",
        SparqlResult::Boolean(_) => "ask",
        SparqlResult::Graph(_) => "construct",
    }
}

/// Escape XML *text content*: `&`→`&amp;` first, then `<`/`>`.
fn xml_escape_text(value: &str, out: &mut String) {
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

/// Escape an XML *attribute value*: as text plus `"`→`&quot;`. `&` first.
fn xml_escape_attr(value: &str, out: &mut String) {
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SolutionProvenance;
    use gmeow_rdf_core::{BlankScope, RdfDatasetBuilder, RdfQuad, RdfTerm, RdfTextDirection};
    use pretty_assertions::assert_eq;

    const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
    const RDF_LANGSTRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

    fn xml_text(result: &SparqlResult, prov: &ResultProvenance) -> String {
        let outcome = to_xml(result, prov).expect("serialization succeeds");
        assert!(!outcome.provenance_dropped, "xml never drops provenance");
        String::from_utf8(outcome.bytes).expect("UTF-8 output")
    }

    fn lit(lex: &str, datatype: &str) -> TermValue {
        TermValue::Literal {
            lexical_form: lex.to_string(),
            datatype: datatype.to_string(),
            language: None,
            direction: None,
        }
    }

    #[test]
    fn select_full_shape() {
        let result = SparqlResult::Solutions {
            variables: vec![
                "s".to_string(),
                "b".to_string(),
                "name".to_string(),
                "age".to_string(),
                "label".to_string(),
            ],
            rows: vec![
                vec![
                    Some(TermValue::Iri("http://example.org/s".to_string())),
                    Some(TermValue::Blank {
                        label: "b0".to_string(),
                        scope: BlankScope(0),
                    }),
                    Some(lit("Ada", XSD_STRING)),
                    Some(lit("42", XSD_INTEGER)),
                    Some(TermValue::Literal {
                        lexical_form: "bonjour".to_string(),
                        datatype: RDF_LANGSTRING.to_string(),
                        language: Some("fr".to_string()),
                        direction: None,
                    }),
                ],
                vec![
                    Some(TermValue::Iri("http://example.org/s2".to_string())),
                    None,
                    Some(lit("Bob", XSD_STRING)),
                    None,
                    Some(lit("Grace", XSD_STRING)),
                ],
            ],
        };
        let expected = concat!(
            "<?xml version=\"1.0\"?>\n",
            "<sparql xmlns=\"http://www.w3.org/2005/sparql-results#\">\n",
            "  <head>\n",
            "    <variable name=\"s\"/>\n",
            "    <variable name=\"b\"/>\n",
            "    <variable name=\"name\"/>\n",
            "    <variable name=\"age\"/>\n",
            "    <variable name=\"label\"/>\n",
            "  </head>\n",
            "  <results>\n",
            "    <result>\n",
            "      <binding name=\"s\"><uri>http://example.org/s</uri></binding>\n",
            "      <binding name=\"b\"><bnode>b0</bnode></binding>\n",
            "      <binding name=\"name\"><literal>Ada</literal></binding>\n",
            "      <binding name=\"age\"><literal datatype=\"http://www.w3.org/2001/XMLSchema#integer\">42</literal></binding>\n",
            "      <binding name=\"label\"><literal xml:lang=\"fr\">bonjour</literal></binding>\n",
            "    </result>\n",
            "    <result>\n",
            "      <binding name=\"s\"><uri>http://example.org/s2</uri></binding>\n",
            "      <binding name=\"name\"><literal>Bob</literal></binding>\n",
            "      <binding name=\"label\"><literal>Grace</literal></binding>\n",
            "    </result>\n",
            "  </results>\n",
            "</sparql>\n",
        );
        assert_eq!(xml_text(&result, &ResultProvenance::default()), expected);
    }

    #[test]
    fn ask_true_exact() {
        let result = SparqlResult::Boolean(true);
        let expected = concat!(
            "<?xml version=\"1.0\"?>\n",
            "<sparql xmlns=\"http://www.w3.org/2005/sparql-results#\">\n",
            "  <head></head>\n",
            "  <boolean>true</boolean>\n",
            "</sparql>\n",
        );
        assert_eq!(xml_text(&result, &ResultProvenance::default()), expected);
    }

    #[test]
    fn ask_false_exact() {
        let result = SparqlResult::Boolean(false);
        let expected = concat!(
            "<?xml version=\"1.0\"?>\n",
            "<sparql xmlns=\"http://www.w3.org/2005/sparql-results#\">\n",
            "  <head></head>\n",
            "  <boolean>false</boolean>\n",
            "</sparql>\n",
        );
        assert_eq!(xml_text(&result, &ResultProvenance::default()), expected);
    }

    #[test]
    fn triple_term_shape() {
        let triple = TermValue::Triple {
            s: Box::new(TermValue::Iri("http://example.org/s".to_string())),
            p: Box::new(TermValue::Iri("http://example.org/p".to_string())),
            o: Box::new(TermValue::Iri("http://example.org/o".to_string())),
        };
        let result = SparqlResult::Solutions {
            variables: vec!["t".to_string()],
            rows: vec![vec![Some(triple)]],
        };
        let text = xml_text(&result, &ResultProvenance::default());
        assert!(
            text.contains(concat!(
                "<binding name=\"t\"><triple>",
                "<subject><uri>http://example.org/s</uri></subject>",
                "<predicate><uri>http://example.org/p</uri></predicate>",
                "<object><uri>http://example.org/o</uri></object>",
                "</triple></binding>",
            )),
            "unexpected triple shape: {text}"
        );
    }

    #[test]
    fn directional_literal_carries_dir_and_ns() {
        let result = SparqlResult::Solutions {
            variables: vec!["d".to_string()],
            rows: vec![vec![Some(TermValue::Literal {
                lexical_form: "hello".to_string(),
                datatype: RDF_LANGSTRING.to_string(),
                language: Some("en".to_string()),
                direction: Some(RdfTextDirection::Ltr),
            })]],
        };
        let text = xml_text(&result, &ResultProvenance::default());
        assert!(text.contains("gmeow:dir=\"ltr\""), "missing dir: {text}");
        assert!(
            text.contains("xmlns:gmeow=\"https://gmeow.dev/ns/results#\""),
            "missing inline ns: {text}"
        );
        // xml:lang precedes the gmeow ns+dir on the literal element.
        assert!(
            text.contains(
                "<literal xml:lang=\"en\" gmeow:dir=\"ltr\" xmlns:gmeow=\"https://gmeow.dev/ns/results#\">hello</literal>"
            ),
            "unexpected directional literal: {text}"
        );
    }

    #[test]
    fn non_directional_literal_is_clean() {
        let result = SparqlResult::Solutions {
            variables: vec!["v".to_string()],
            rows: vec![vec![Some(lit("x", XSD_STRING))]],
        };
        let text = xml_text(&result, &ResultProvenance::default());
        assert!(!text.contains("gmeow:dir"), "must stay clean: {text}");
    }

    #[test]
    fn escaping_in_text_and_attr() {
        let result = SparqlResult::Solutions {
            variables: vec!["v<&>\"".to_string()],
            rows: vec![vec![Some(lit("a & b < c > d \"e\"", XSD_STRING))]],
        };
        let text = xml_text(&result, &ResultProvenance::default());
        assert!(
            text.contains("<variable name=\"v&lt;&amp;&gt;&quot;\"/>"),
            "attr escaping: {text}"
        );
        assert!(
            text.contains("<literal>a &amp; b &lt; c &gt; d \"e\"</literal>"),
            "text escaping (no quot in text): {text}"
        );
    }

    #[test]
    fn populated_provenance_present() {
        let result = SparqlResult::Solutions {
            variables: vec!["s".to_string()],
            rows: vec![vec![Some(TermValue::Iri(
                "http://example.org/s".to_string(),
            ))]],
        };
        let provenance = ResultProvenance {
            query_hash: Some("deadbeef".to_string()),
            engine: Some("gmeow-sparql-eval".to_string()),
            solutions: vec![SolutionProvenance {
                sources: vec!["http://example.org/g1".to_string()],
            }],
        };
        let text = xml_text(&result, &provenance);
        assert!(
            text.contains("<gmeow:provenance xmlns:gmeow=\"https://gmeow.dev/ns/results#\">"),
            "missing provenance: {text}"
        );
        assert!(
            text.contains("<gmeow:queryForm>select</gmeow:queryForm>"),
            "missing queryForm: {text}"
        );
        assert!(
            text.contains("<gmeow:queryHash>deadbeef</gmeow:queryHash>"),
            "missing queryHash: {text}"
        );
        assert!(
            text.contains("<gmeow:engine>gmeow-sparql-eval</gmeow:engine>"),
            "missing engine: {text}"
        );
        assert!(
            text.contains("<gmeow:source>http://example.org/g1</gmeow:source>"),
            "missing source: {text}"
        );
        // Provenance sits after </results>, before </sparql>.
        let after_results = text
            .split_once("</results>")
            .map(|(_, rest)| rest)
            .unwrap_or("");
        assert!(
            after_results.contains("<gmeow:provenance"),
            "provenance must follow </results>: {text}"
        );
    }

    #[test]
    fn graph_is_format_error() {
        let mut builder = RdfDatasetBuilder::new();
        builder.push_owned_quad(&RdfQuad {
            subject: RdfTerm::iri("http://example.org/s"),
            predicate: "http://example.org/p".to_string(),
            object: RdfTerm::iri("http://example.org/o"),
            graph_name: None,
            location: None,
        });
        let dataset = builder.freeze().expect("dataset freezes");
        let result = SparqlResult::Graph(dataset);
        let err = to_xml(&result, &ResultProvenance::default()).expect_err("graph rejected");
        assert!(matches!(err, Error::Format(_)), "expected Format: {err:?}");
    }
}
