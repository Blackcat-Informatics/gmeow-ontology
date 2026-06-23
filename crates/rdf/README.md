<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: AGPL-3.0-only -->

# gmeow-rdf

`gmeow-rdf` is the oxigraph adapter layer of the GMEOW RDF 1.2 kernel. It depends
on and re-exports the oxigraph-free [`gmeow-rdf-core`](../rdf-core) crate (the
shared RDF model, diagnostics, interned IR, store traits, `DatasetView`, and GTS
readers) and adds the oxigraph-backed surface: parsing/materialization, turtle
normalization, statement codecs, GTS↔oxigraph store builders, and the PyO3
bindings folded into the unified `gmeow_native` extension (#630).

The core/adapter split (#885, P2b) makes the oxigraph boundary a **crate
boundary**: `gmeow-rdf-core` never names oxigraph, so leaks are compile errors.
Consumers depend on `gmeow-rdf` and reach the kernel transparently through its
re-exports.

The crate deliberately does not emit SARIF. It exposes structured RDF diagnostics
and source locations so callers can translate them into `gmeow-diagnostics`
findings or SARIF without coupling the RDF core to a reporting format.
