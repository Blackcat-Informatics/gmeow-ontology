# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Compatibility shim: gmeow_slice → gmeow_native.slice.

Single-cdylib unification (#630/#820 S8): the native slice catalog + ownership
analyzer lives in the unified `gmeow_native` cdylib as the `slice` submodule.
This shim swaps itself for the real submodule so the public `import gmeow_slice`
returns the exact submodule object — same pyclasses.
"""

import sys

from gmeow_native import slice as _module

# PyO3 submodules carry no `__file__`. The top-level name is expected to be
# locatable (tooling/tracebacks expect it), so point the submodule at this shim
# before swapping it in.
_module.__file__ = __file__

sys.modules[__name__] = _module
