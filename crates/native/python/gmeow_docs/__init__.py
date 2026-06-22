# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Compatibility shim: gmeow_docs → gmeow_native.docs.

Single-cdylib unification (#630): all native engine surfaces live in one
`gmeow_native` cdylib. The typed documentation model (#853) is the
`gmeow_native.docs` submodule; this shim swaps itself for the real submodule so
the legacy `import gmeow_docs` returns the exact submodule object.
"""

import sys

from gmeow_native import docs as _module

# PyO3 submodules carry no `__file__`. The legacy top-level name is expected to be
# locatable (CI imports it and reads `__file__`, and tooling/tracebacks expect it),
# so point the submodule at this shim before swapping it in.
_module.__file__ = __file__

sys.modules[__name__] = _module
