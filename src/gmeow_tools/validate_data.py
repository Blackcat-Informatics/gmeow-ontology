"""Repo-free RDF data conformance for the consumer ``gmeow validate`` command.

Thin UI-surface glue: the native ``gmeow_validate.validate_data`` engine loads the
SHACL shape union and the OntoUML disciplines from a bundled ``gmeow.gts`` and
validates an external RDF data graph (Tier-1: no reasoner). Shape selection and
all validation live in Rust; this module only maps file extensions to native
format ids and forwards raw bytes — it holds no ontology-selection logic.
"""

from __future__ import annotations

from typing import Any

#: The native diagnostics ``Report`` (a PyO3 pyclass without a typed stub); the
#: rest of the codebase types it as ``Any`` (see :mod:`gmeow_tools.diagnostics`).
Report = Any

#: RDF file extensions the data-conformance path accepts, mapped to the native
#: format id understood by ``gmeow_validate.validate_data``. ``.jsonld`` is RDF
#: here (JSON-LD is a graph syntax); the JSON-Schema instance path is selected
#: only for ``.json``/``.yaml`` or when ``--schema`` is given.
RDF_SUFFIXES: dict[str, str] = {
    ".nq": "nquads",
    ".nquads": "nquads",
    ".trig": "trig",
    ".ttl": "turtle",
    ".turtle": "turtle",
    ".nt": "ntriples",
    ".ntriples": "ntriples",
    ".rdf": "rdf+xml",
    ".owl": "rdf+xml",
    ".jsonld": "json-ld",
}


def format_for_suffix(suffix: str) -> str | None:
    """Return the native RDF format id for *suffix* (e.g. ``.nq``), or ``None``."""
    return RDF_SUFFIXES.get(suffix.lower())


def validate_rdf(
    data_bytes: bytes,
    fmt: str,
    gts_bytes: bytes,
    namespace: str,
    origin: str,
) -> Report:
    """Validate *data_bytes* (RDF in *fmt*) against the shapes in *gts_bytes*.

    Returns the native diagnostics :class:`Report`. *origin* is the data file's
    display path, recorded as each finding's physical location. Raises
    :class:`ValueError` from the native engine on a parse failure or a bundle
    missing its shape surface.
    """
    import gmeow_validate

    return gmeow_validate.validate_data(data_bytes, fmt, gts_bytes, namespace, origin)
