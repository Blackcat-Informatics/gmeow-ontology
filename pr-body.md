Production fails before math validation because the functional-carrier ledger still requires the deliberately retired `monetaryValue` property. Remove that single stale entry and correct the ledger instructions to name the actual pipeline integrity check.

MonetaryAmount retains its required decimal quantity value and class-local maximum-one constraint. The generic quantity value property gains no global functional restriction, and the completeness gate remains unchanged.

Validation: focused carrier controls and normal hooks are recorded in the delivery evidence. Full corpus equality awaits hosted production; no local corpus or full-gate run under the campaign exception. Refs #1894. Principles 4, 7 and 17.
