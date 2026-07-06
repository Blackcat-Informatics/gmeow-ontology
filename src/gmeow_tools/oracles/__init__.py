"""Maintainer-only differential oracle lanes.

Modules in this package may call a foreign engine such as owlrl (via the
purrdf.compat.rdflib facade) to cross-check native Rust authority. They are
not production-path implementation modules.
"""
