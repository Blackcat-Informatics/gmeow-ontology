# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Compatibility shim: gmeow_logic → gmeow_native.logic.

Single-cdylib unification (#630): all five native extensions now live in one
`gmeow_native` cdylib. This shim swaps itself for the real submodule so the
legacy `import gmeow_logic` returns the exact submodule object — same pyclasses.

The hand-written `__init__.pyi` stub + PEP 561 `py.typed` marker beside this file
keep mypy type-checking every `gmeow_logic` call site (query/materialize/certify).
"""

import sys

from gmeow_native import logic as _module

sys.modules[__name__] = _module
