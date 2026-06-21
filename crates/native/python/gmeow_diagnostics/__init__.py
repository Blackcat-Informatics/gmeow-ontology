# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Compatibility shim: gmeow_diagnostics → gmeow_native.diagnostics.

Single-cdylib unification (#630): all five native extensions now live in one
`gmeow_native` cdylib, so the diagnostics Report/Finding pyclasses are a single
shared type. This shim swaps itself for the real submodule so the legacy
`import gmeow_diagnostics` returns the exact submodule object — same pyclasses.
"""

import sys

from gmeow_native import diagnostics as _module

sys.modules[__name__] = _module
