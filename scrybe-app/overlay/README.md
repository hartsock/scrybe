# Source-preserving body editing

`source.ts` maps the original editable body to the editor's own remark positions.
Unchanged, moved and restored blocks retain their source slices. Changed windows
use a bounded line/token merge and must parse back to the normalized editor document.
The server still decides which bodies and fields may be edited.

A save cannot fall back to canonicalizing the entire document. Wider verified
windows, lost invisible metadata, and excess source-line changes require an inline
full-diff review. `ALLOW_REFORMAT_CONSENT` is the single switch to refuse those
changes instead. Adjacent same-kind lists and structural edits around HTML refuse
publication because the two Markdown dialects cannot be proven equivalent there.

Recovery chooses the first verified source candidate, using canonical Markdown
only as a last recovery fallback. An unverified recovery is labeled and requires
explicit review before restoration. Failed restoration leaves the live document
intact. Save replies rebase the captured sent document, preserving later typing.
Source history lasts for the editor session so undo can recover retired spelling
and original separators.

## Local validation

Run `npm test`, `npm run build:overlay`, and
`node scripts/overlay-check.mjs dist-overlay` from `scrybe-app`.

Set `SITE_CORPUS` to an extracted site checkout to run:

```sh
node --test test/overlay-gate-corpus.test.mjs test/overlay-source-corpus.test.mjs
```

The gate checks every Markdown file and the pinned baseline manifest. The property
suite selects a deterministic 40-file sample; set `SITE_CORPUS_FULL=1` for the entire
archive. It checks each paragraph/heading word edit, structural operations, at least
2,000 seeded multi-edit trials, verified rebase and undo, locality and timing.
The publication oracle uses local Ruby, kramdown 2.4.0 and kramdown-parser-gfm,
and reports an explicit skip if they are unavailable. It checks block skeletons
where the original renderers agree; it does not claim identical inline styling,
list looseness, or agreement for sources already rendered differently.
