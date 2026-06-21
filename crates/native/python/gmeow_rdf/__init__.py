# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Compatibility shim: gmeow_rdf → gmeow_native.rdf.

Single-cdylib unification (#630): all five native extensions now live in one
`gmeow_native` cdylib. This shim swaps itself for the real submodule so the
legacy `import gmeow_rdf` returns the exact submodule object — same pyclasses.

The hand-written `__init__.pyi` stub + PEP 561 `py.typed` marker beside this file
keep mypy type-checking every `gmeow_rdf` call site (the native oxigraph
Store/SPARQL/parse/canonicalize surface, #667).
"""

import sys

from gmeow_native import rdf as _module

# PyO3 submodules carry no `__file__`. The legacy top-level name is expected to be
# locatable (CI imports it and reads `__file__`, and tooling/tracebacks expect it),
# so point the submodule at this shim before swapping it in.
_module.__file__ = __file__

sys.modules[__name__] = _module
