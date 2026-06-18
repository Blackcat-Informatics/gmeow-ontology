<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: AGPL-3.0-only -->

# gmeow-rdf

`gmeow-rdf` is the PyO3-free RDF 1.2 kernel for the GMEOW Rust workspace. It owns
the shared RDF model, RDF diagnostics, store traits, and adapter boundary between
GTS and oxigraph-backed consumers.

The crate deliberately does not emit SARIF. It exposes structured RDF diagnostics
and source locations so callers can translate them into `gmeow-diagnostics`
findings or SARIF without coupling the RDF core to a reporting format.
