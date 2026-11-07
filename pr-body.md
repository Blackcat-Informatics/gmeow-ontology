## Summary

`besselJ` and `besselY` carried indexed labels but were framed as unary scalar functions without a selected order. Represent them as integer-order families whose outputs are member functions. J members map real arguments to real values; Y members use the explicit open positive-real interval, excluding zero and negative arguments. Preserve both existing IRIs and labels.

Add two function spaces with realized membership conditions, two closed membership laws and two conditional member-frame laws. Keep order selection and member application as separate uses of the existing ternary `functionValueRel`. These laws state framing obligations; they do not construct selections, prove existence or uniqueness, characterize analytic values, or implement numerical evaluation.

The nine public terms receive complete English guidance and 63 new canonical translation pairs per locale in French and Chinese. Update the owning design explanation and one inaccurate mapping comment while preserving all mapping targets, strengths and provenance. Principles 4, 7, 17 and 19 govern canonical sources, evidence and grounding ownership.

Closes #2598. The original #1656 parent retains its full remaining contract.

## Validation

- [ ] `make lint`: not run manually; the user waived terms CI. Configured commit hooks passed normally.
- [ ] `make validate`: not run under the current execution boundary.
- [ ] `make reason`: not run; native admission and inference remain unqualified.
- [ ] `make mappings` / `make wikidata`: not run; only a mapping comment changed.
- [ ] `make rust-test`: not run; no native runtime change or qualification is claimed.
- [x] Owning design documentation updated with the family/member distinction and the laws' exact strength.
- [x] Independent source, mathematical, English, French, Chinese, caller, example, design and mapping review passed for the complete packet.
- [x] Exact five-file reconstruction and preservation, post-hook byte comparison, prose and whitespace checks passed.
- [x] Current catalogs passed the permitted cached catalog lint through the documented target: two languages, zero warnings, exit zero.

## Checklist

- [x] Scope is limited to the indexed-family source repair and its translations.
- [x] Existing fixtures, query bodies, native implementations, profiles, constraints and floors are preserved. Behavioral execution is not claimed.
- [x] Every selected public term has its label, definition, owning-module annotation and full guidance. Grounding reuses canonical mathematical classes and realized `logic:Formula` laws; no new external correspondence is authored.
- [x] No external reference-only axioms copied into the vocabulary.
- [x] No secrets or local-only paths introduced into shipped source.
- [x] Current license/CLA, hosted lint and helper hygiene checks passed. The remaining CI was cancelled under the user's terms-only waiver; no runtime acceptance is claimed.

## Notes

Terms CI is waived by the user. Broader native membership guards, analytic evaluation, generated and installed behavior, runtime consumers and measured coverage/floors remain unqualified under the original parent contract. Static review and catalog lint do not establish those results.

An earlier complete evidence archive was refused because its generated review capture contains prohibited attribution. The full review capture, original structured response and dispositions are retained unchanged as local campaign evidence and excluded from publication. The selected merge archive retains the complete required synchronization journal and documents that narrower scope. No hook, guard or publication invariant is changed. Publication still must succeed through the required merge tool.
