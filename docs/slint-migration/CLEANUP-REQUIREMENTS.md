# Accepted scope update — 2026-10-16 18:11 UTC

The user explicitly requested the following eight features from the supplied
Typeless screenshots. These are now implementation requirements for this work,
not merely deferred competitor gaps. Screenshots themselves must not be uploaded
to the public repository. This document records behavior only.

| ID | Required behavior | Settings/UI and acceptance |
|---|---|---|
| CL01 | Remove non-semantic fillers | Toggle; conservative phrase-context handling; retain meaningful quoted/lexical uses of 嗯/呃 |
| CL02 | Remove accidental repetition | Toggle; retain emphasis, repeated names/numbers and intentional quoted repetition |
| CL03 | Resolve spoken self-corrections | Preserve final intended value, e.g. 早上7點，不對，改下午3點 → 下午3點; keep raw text and explain changes |
| CL04 | Format lists, steps and paragraphs | User-selectable output style; preserve item order, conditions and numbering |
| CL05 | Conservative wording improvements | Readability style; do not invent facts, change negation, names or commitments; editable preview and original restore |
| CL06 | Personal style/tone preferences | Explicit preference editor; suggested learning requires confirmation; no inspection of other applications |
| CL07 | Personal dictionary and suggested terms | Manual create/edit/delete; generated name/term suggestions from the user's current opted-in content; explicit accept/reject before persistence |
| CL08 | Automatic language detection / mixed-language input | Auto + explicit language controls; preserve mixed scripts; separate provider-supported languages from actually tested languages; initial fixed corpus covers Traditional Chinese, English, Japanese and mixtures |

All eight require reachable Slint settings/actions and a before/after preview
with restore-original. No hidden cloud call when key or provider consent is
missing. Offline deterministic rules may be tested with a fixed corpus, but
model semantics/ASR quality remain separately NOT_RUN until authorized provider
evaluation. Do not advertise "100+ languages" from screenshots.

Next implementation slice after checkpoint 02: shared cleanup configuration,
structured original/cleaned result contract, fixed corpus including numbers,
negation, names, corrections and meaningful filler homographs, preference and
dictionary suggestion confirmation. Keep original content in memory unless
explicit text-history consent permits persistence. Existing dictionary and
voice-command features remain release-blocking regressions to preserve.
