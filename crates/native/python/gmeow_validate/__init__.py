# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Compatibility shim: gmeow_validate → gmeow_native.validate.

Single-cdylib unification (#630): all five native extensions now live in one
`gmeow_native` cdylib, so a Report built by the validate orchestration is the
SAME type the diagnostics module operates on. This shim swaps itself for the real
submodule so the legacy `import gmeow_validate` returns the exact submodule
object — same pyclasses.
"""

import sys

from gmeow_native import validate as _module

sys.modules[__name__] = _module
