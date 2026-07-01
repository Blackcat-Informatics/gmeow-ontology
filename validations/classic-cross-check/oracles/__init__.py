"""Classic-cross-check oracle lanes (relocated out of the mainline, #1087).

Modules here call the foreign classical engines — ELK, HermiT, ROBOT, Apache
Jena, and the upstream ``owlrl`` OWL 2 RL reasoner — to cross-check GMEOW's
native Rust authority. They are NOT production-path implementation modules and
live in this standalone ``validations/`` lane, outside make check / maint-* / CI.
They still import shared helpers (``gmeow_tools.config``, ``.reason``,
``.runner``, ``.diagnostics``) and the native extensions from the built
repository — a forward dependency intrinsic to cross-checking native reasoning
against classical oracles.
"""
