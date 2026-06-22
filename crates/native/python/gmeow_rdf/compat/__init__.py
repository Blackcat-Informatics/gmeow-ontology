# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Compatibility shims layered over the native ``gmeow_rdf`` surface.

``gmeow_rdf.compat.rdflib`` is the purrdf P0 subset of the eventual P9 public
rdflib drop-in: a pure-Python facade (terms, namespaces, ``Graph``, SPARQL
results, ``Collection``, comparison, format detection) so the internal toolchain
runs with no ``rdflib`` dependency on the default path.
"""
