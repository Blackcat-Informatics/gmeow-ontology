"""The gmeow-ontology wheel version — a single-source projection.

This is the ontology's owl:versionInfo (ontology/gmeow.ttl), verbatim.
pyproject.toml's [tool.hatch.version] reads __version__ from here. To
release a new wheel version, bump owl:versionInfo in ontology/gmeow.ttl
and run `make check` — never hand-edit this file or set `version`
in pyproject.toml directly.
"""

from __future__ import annotations

__version__ = "0.1.0"
