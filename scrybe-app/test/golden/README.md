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

The executable `test.todo` builds the otherwise inexpressible document state
`text("uncertainty") + strong(".")`. Milkdown emits `uncertainty**.**`, then
`uncertainty\*\*.\*\*` after reparsing. The TODO fails the intended idempotence
assertion and records those observed outputs; Node excludes TODO failures from
its exit status. A separate passing upstream check pins
`uncertaint&#x79;**.**` and verifies that it reparses as strong and is stable.
The fix target is Milkdown's `text`/`strong`/`emphasis` handlers, not an override
of the upstream serializer version. No production fix is included here.

**Phase 2 NO-GO on this evidence alone:** the edit-path defect remains open,
private-corpus semantic diff review is not established, and real Jekyll HTML
has not been compared. A green harness confirms the recorded observations;
it does not turn the known TODO or missing evidence into a GO.
