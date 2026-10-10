# Headless Crepe golden evidence

Run from `scrybe-app` after `npm ci`:

```sh
node --test test/milkdown_golden.test.mjs
node --test test/*.test.mjs
npx tsc --noEmit
```

The suite runs Milkdown Crepe 7.22.2's real `CrepeBuilder`, with `root: null`
falling back to jsdom's `document.body`. It creates and destroys a fresh editor
on every pass and configures it with the unmodified `houseStyle` module. The
builder includes commonmark, GFM and the core editor plugins; optional browser
UI features (CodeMirror views, toolbars, etc.) are not mounted. This proves the
serialization pipeline, not browser interaction or Tauri integration. Latex is
not enabled, consistent with the demo. No browser download or dev server is used.

TypeScript modules are transpiled in memory with the existing TypeScript
package; bare imports are resolved before loading data URLs. DOM globals must
be installed before Milkdown is imported, including bound `addEventListener`,
`removeEventListener` and `dispatchEvent` for Milkdown's context timers.
Verified with Node 26.9.0 and jsdom 26.1.0; no Vitest fallback was needed.

Every input `name.md` has a committed first-pass `name.out.md`. Inputs are
synthetic. `sample.md` and `probe.md` are byte-for-byte copies of the demo at the
lane's base commit. The test never updates pins. It asserts the pinned output,
verbatim raw front matter, and stability across ten further fresh-editor cycles.
The summary reports input/output byte identity separately from idempotence.
Candidates are written under a new `os.tmpdir()` directory, printed by the test.
To change a pin, inspect that synthetic candidate's diff against both the input
and existing output, then deliberately copy just that file and review the commit.
A missing or changed pin fails the suite. Do not copy private outputs here.

## Reviewed synthetic first-pass differences

| Input | Byte-identical | Observed change |
|---|---|---|
| autolink-backslash.md | yes | Trailing backslash retained |
| comment-definition.md | no | Comment definition removed |
| fence-attributes.md | no | Tildes become backticks; `{linenos}` removed |
| frontmatter-crlf.md | yes | CRLF YAML, Unicode, dots closer and blank line retained |
| hard-break.md | yes | Two-space hard break retained |
| heading-id.md | yes | Literal `{#custom}` retained (does not establish HTML ID semantics) |
| indented-code.md | no | Becomes a fenced code block |
| liquid-expression.md | no | Opening bracket in the expression escaped |
| liquid-tag.md | no | Underscore in the tag argument escaped |
| probe.md | no | Combined Liquid, reference, comment and code normalizations |
| reference-link.md | no | Reference plus definition becomes an inline link |
| sample.md | yes | House-style sample unchanged |
| table-delimiter.md | no | Delimiter compacted to three dashes; alignment preserved |

All thirteen outputs are stable for ten further cycles. Pinning these changes
makes them reviewable; it does not approve information loss or establish that
Liquid/Jekyll semantics survive.

## Optional private corpus

Set `SCRYBE_GOLDEN_DIR` to a directory of UTF-8 `*.md` files. Immediate regular
files are checked in sorted order; symlinks and subdirectories are not followed.
Keep the corpus outside the repository. Every path that resolves inside the
checkout is rejected, including `node_modules` and symlinks into the checkout.
The environment variable is read only; no corpus is bundled, copied into the
checkout, or automatically pinned.

The suite checks raw front matter and ten-cycle stability and reports byte
identity with file names only. Every file gets a pass/fail summary row, including
files after a failure. Rows report byte identity (`null` if not measured) and the
number of completed stable cycles (zero if none). After checking all files, one
failure lists only the failing file names. Private failures withhold document
text and source paths. Outputs go exclusively to a new temporary directory
(mode 0700), with files mode 0600. Private outputs use a separate filename prefix.
They remain available there for local diff review; remove them when finished.
An unset variable produces an explicit skip. Refused paths report
`SCRYBE_GOLDEN_DIR must be outside the repository`; empty directories report
`private corpus has no Markdown files`. Other directory failures withhold details.
Private content must never be pasted into commits, test names, this README, or
handoff reports. No private corpus was used to produce these pins.

## Evidence levels and Phase 2 gate

1. **Source bytes:** raw front matter must be identical; full first-pass outputs
   are pinned, byte identity is reported, and subsequent cycles must be identical.
2. **Parsed YAML deep-equality:** exact raw/YAML source preservation implies
   equal parse results under the same YAML parser and schema. This harness checks
   the stronger source equality rather than introducing an independent YAML
   parser or reserializing YAML. It makes no claim that arbitrary YAML is valid.
3. **Real Jekyll HTML:** out of scope on this machine and **not evaluated**.
   Markdown HTML alone would not test Liquid/Jekyll processing.

The bold-punctuation defect now has passing edit-path tests using upstream
attention handlers. Image and supported save cases also run in both CrepeBuilder
and full Crepe with `crepeSettings` under JSDOM. Full Crepe's CodeMirror visibility
observer is a lifecycle stub, not a visual-layout test.

## Link and attention ordering: owner decision pending

The candidate retains link priority 0. It fixes `[*a* b](u)` and `[**a** b](u)`:
one anchor survives instead of splitting the linked text into separate anchors.
It changes the mirror class, where emphasis begins with a link:

| Input | First save | Later save |
|---|---|---|
| `*[a](u) text*` | `[*a*](u) *text*` | same |
| `*[a](u) [b](v)*` | `[*a*](u) *[b](v)*` | `[*a*](u) [*b*](v)` on save 2 |
| `*[a](u)*` | `[*a*](u)` | same |
| `**[a](u)**` | `[**a**](u)` | same |
| `***[a](u)***` | `[***a***](u)` | same |

These outputs flip HTML nesting; spaces between separately marked spans lose
emphasis. Default styling can hide this, but the HTML is different. The two-link
case is not stable from its first save. Ten visible TODO probes execute the
intended preservation assertions in both editor configurations. TODO status is
not approval or a passing preservation check.

Longest-run-outer can distinguish unequal ranges, but not identical ranges.
Passing collision tests prove `*[a](u)*` and `[*a*](u)` parse into exactly the same
ProseMirror JSON; strong and combined-mark variants do too. Their HTML differs.
Original nesting is lost before serialization. A serializer slice replacement
or look-ahead runner cannot reconstruct that missing information. Preserving
both forms requires storing source nesting in a changed document representation.
The current choice avoids duplicated anchors but needs the owner's sign-off on
the canonical form. No longest-run heuristic or source-preserving serializer is
shipped here.

`*text [a](u)*` and `**see [a](u) and [b](v)**` preserve exact output, HTML,
parsed structure, and ten cycles. Four older same-type nested emphasis/strong
TODOs remain unresolved in the current schema.

Already-saved container placeholders have a migration effect: `- <br />` now
becomes `-`; an empty paragraph between list-item paragraphs can disappear and
converge on the second save. Top-level placeholders retain prior behavior.
An originally blank code block that is filled and later emptied retains its
blank-line flag, so it saves with one blank line again.

**Phase 2 NO-GO on this evidence alone:** the canonical-form decision and nested
mark limitations remain open, private-corpus semantic review is not established,
and real Jekyll HTML has not been compared. A green process exit excludes TODO
failures; it does not establish preservation for those inputs.
