# Recorded native world results

Explicit read-only consumers of the same three authenticated historical products.
The implementation is 646bf451f, with the af8936e6 native pin. The later fuzz-only
dependency alignment does not change these executable paths. All worlds use the
complete production object EDB before native graph selection. No corpus producer,
current fingerprint, cache repair, profile change or host probe ran. Principles
4 and 7. Raw results are native-final-*.log; the corrected default operation is
native-projection-default.log. Exact input identities remain in
recorded-input-locator.json and each operation's hydration observations.

World suffixes below use https://blackcatinformatics.ca/gmeow/graph/.
Times are prepare / decide / source-bound proof replay in milliseconds. These
concurrently observed runs are not a controlled speedup comparison.

| World | Assertions | Native result | Boundaries | Times (ms) | Replayed runs |
| --- | ---: | --- | --- | --- | ---: |
| default | 179177 | Preparation refused | Invalid object property, private term 9336 | no decision | 0 |
| imports | 8099 | True / DecidedWithinBoundaries | DataRange | 27 / 36 / 29 | 1 |
| logic | 52366 | True / DecidedWithinBoundaries | DataRange, UnrecognizedTerm | 152 / 132 / 97 | 1 |
| examples | 13436 | True / DecidedWithinBoundaries | DataRange, UnrecognizedTerm | 28 / 6 / 55 | 1 |
| relational-core | 377397 | True / DecidedWithinBoundaries | DataRange | 921 / 654 / 515 | 1 |
| statements | 114 | True / Decided | none | 0 / 0 / 0 | 1 |
| demo/jointly-acyclic | 22 | True / Decided | none | 0 / 0 / 0 | 1 |
| demo/model-summarizing-acyclic | 18 | True / Decided | none | 0 / 0 / 0 | 1 |
| demo/super-weakly-acyclic | 22 | True / Decided | none | 0 / 0 / 0 | 1 |

All eight named selections exited successfully and retained their native
certificates and proof contexts. A bounded result is not unrestricted consistency.
The default refusal is malformed-input admission, not False or an Unknown
decision. None of these OWL Direct operations evaluates GMEOW canonical admission
or authenticates current complete sources. Native delegation must preserve GMEOW
class/source/world/domain semantics before deleting the still-active class
implementation. Dropping unsupported premises is not a repair.
