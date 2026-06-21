# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Package wrapper for the unified gmeow_native PyO3 extension (#630).

Mixed Rust/Python maturin layout: the compiled cdylib is installed as
``gmeow_native.gmeow_native`` (a submodule .so next to this file); this wrapper
re-exports its public surface — the five engine submodules ``rdf``,
``diagnostics``, ``shacl``, ``validate``, ``logic`` — so ``import gmeow_native``
works and ``gmeow_native.diagnostics.Report`` is the single shared pyclass the
whole extension agrees on. The compiled module also registers each submodule in
``sys.modules`` under ``gmeow_native.<name>`` so ``import gmeow_native.validate``
resolves directly.
"""

from . import gmeow_native as _ext
from .gmeow_native import *  # noqa: F403

__doc__ = _ext.__doc__
if hasattr(_ext, "__all__"):
    __all__ = _ext.__all__
