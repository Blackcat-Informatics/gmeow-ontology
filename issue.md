# Issue #2685: vocabulary: a transitive partial order between standpoints as dated claims

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, logic, terms

## Body

Request: a transitive, partial ordering between standpoints, declared only
as dated claims attributed to the party who ranks them.

Need: some readers rank frames by how entrenched they are, and the ranking
is often only partly known. A total order is wrong: a missing claim should
mean the two are incomparable, never equal and never lower. Today there is
no typed way to record such a ranking with its date and its attribution.

Proposed term: moreEntrenchedThan, between standpoints, transitive.

Rules: it is a partial order, declared only as dated claims attributed to
the party who ranks. A missing claim means incomparable. It is never derived
from sharpening: a logic engine that reads the sharper frame as the more
entrenched would invert such a chain. It is a presentation aid, not a
confidence or a truth value.


## Comments (0)

