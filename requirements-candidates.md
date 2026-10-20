# Requirement candidates — issue #1807

Lines from the issue body and comments that matched a lexical rule, copied
verbatim with the rule that fired. This is a CANDIDATE list: rules both
over-match (a "should" in passing) and under-match (a requirement stated
without a keyword). The issue-analyzer keeps or drops each row, adds what
the rules missed, and accounts for every silent comment. Code fences are
skipped.

| ID | Source | Line | Rules | Text |
|---|---|---|---|---|
| B1 | body | 12 | modal | `xsd:dateTime`, so a day- or year-precision time must be padded into |
| B2 | body | 67 | modal | - Should the SHACL constraint be one shared shape (a "natural-language |
| B3 | body | 68 | question | literal" node shape) that properties reference, or stated per property? |
| B4 | body | 69 | question | - How does this interact with the `@x-gmeow-*` canonical tags of #164? |
| B5 | body | 70 | modal | The constraint should admit both private-use and public tags. Is a |
| B6 | body | 71 | question | tag check (well-formed BCP 47) wanted as well? |
| B7 | body | 97 | modal | that carries the scheme. A generic digest datatype must never be |
| B8 | body | 109 | question | beside `contentDigest`? |
| B9 | body | 114 | question | deprecating the old one preferred? |
| B10 | body | 131 | modal | **Proposal.** Either of these, and GMEOW should choose: |
| B11 | body | 142 | question | the temporal slices as an interval entity the time points to? |
| C1.1 | comment 1 (paudley, 2026-10-10T20:21:12Z) | 5 | question | - be complemented by a structured form, so a no-JSON consumer has a sanctioned home? |

## Comments that matched no rule

None.
