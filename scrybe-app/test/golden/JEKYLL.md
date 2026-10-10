# Real Jekyll evidence for synthetic goldens

Recorded by helper1 (Mac, GPT-6), 2026-10-09; Refs #253.
Golden baseline: `68c295652fa40052b966538010da0e90dcb72885`.

Run manually from the repository root using the already installed gems:

```sh
ruby scripts/golden-jekyll-evidence.rb scrybe-app/test/golden
```

Recorded exit status: **1** (rendered equivalence is not established for every
pair). Exit 0 requires all pairs to render and match both comparisons. Exit 2
rejects an invalid directory or missing/nonregular/symlinked synthetic input.
The script accepts only this checkout's canonical synthetic golden directory,
uses an explicit list of thirteen pairs, and ignores private-corpus environment
variables. It neither installs gems nor uses the network.

Environment: Ruby 4.0.7 (`arm64-darwin25`), Jekyll 3.10.0,
kramdown 2.4.0, kramdown-parser-gfm 1.1.0, Liquid 4.0.4, Rouge 3.30.0.
The script activates `gem "jekyll", "3.10.0"` before requiring Jekyll and also
pins kramdown 2.4.0. Other versions above are recorded from the loaded gems.

## Method and configuration

Each invocation owns one fresh block-scoped `Dir.mktmpdir`, automatically
removed on completion or exception. Beneath it, each of the 26 documents gets
an isolated source/destination pair and a real `Jekyll::Site#process` call.
Isolation lets the remaining documents finish when a Liquid link raises;
failed documents have no fabricated HTML, and equality is reported N/A.
Each source contains one golden page, synthetic data, and two static link
targets. The golden files themselves are never changed.

Existing front matter is copied byte-for-byte, including CRLF and the dots
closer. Pages without front matter receive an empty block to enable Jekyll
processing. A site default supplies `layout: null`; no layouts are installed.
`_data/nav.yml` contains `- title: Synthetic navigation title`. Static files
`_posts/example.md` and `_posts/x.md` contain `Synthetic link target.` and are
explicitly included so the original link tags resolve. These are synthetic
static targets, not dated blog posts. There are no backslash-prefixed targets.

These are the exact shared configuration overrides (Jekyll 3.10.0 supplies
remaining defaults). `source` and `destination` are unique temporary directories,
and `config: []` prevents loading any checkout or user site configuration.
The Markdown options match the requested GitHub Pages defaults; this is not a
full GitHub Pages plugin/theme deployment.

```json
{
  "markdown": "kramdown",
  "kramdown": {
    "input": "GFM",
    "hard_wrap": false
  },
  "highlighter": "rouge",
  "safe": true,
  "plugins": [],
  "theme": null,
  "quiet": true,
  "incremental": false,
  "include": [
    "_posts"
  ],
  "defaults": [
    {
      "scope": {
        "path": ""
      },
      "values": {
        "layout": null
      }
    }
  ]
}
```

Exact equality compares generated HTML strings. Whitespace equality applies
`html.gsub(/>\s+</, "><")` to each side: only whitespace *between tags* is
removed. It does not strip terminal newlines or normalize text inside elements.
This deliberately mechanical comparison can remove significant inter-tag spaces
(e.g. adjacent inline spans); it is not proof of semantic equivalence.

## Recorded results

Nine pairs match exactly, two differ, and two pinned documents fail to build.

| Pair | Exact equal | Whitespace equal | Note |
|---|---|---|---|
| autolink-backslash | yes | yes | Trailing backslash retained. |
| comment-definition | yes | yes | Dropped comment definition does not change this HTML. |
| fence-attributes | no | no | Original renders as a paragraph; pinned fence becomes highlighted Ruby code. |
| frontmatter-crlf | yes | yes | CRLF and dots-closed front matter produce the same body HTML. |
| hard-break | yes | yes | Two-space hard break preserved. |
| heading-id | yes | yes | Same HTML for the unchanged literal heading suffix. |
| indented-code | yes | yes | Indented and fenced forms produce identical code HTML. |
| liquid-expression | yes | yes | Both render the synthetic navigation title despite the inserted escape. |
| liquid-tag | N/A | N/A | Input builds; pinned escaped link target fails lookup. |
| probe | N/A | N/A | Input builds; pinned escaped link target fails before HTML can be compared. |
| reference-link | no | no | Input has one extra terminal LF; between-tag normalization does not trim it. |
| sample | yes | yes | Same HTML. |
| table-delimiter | yes | yes | Alignment and table HTML preserved. |

## Differing fragments and build errors

The script emits JSON with each smallest single differing span, found by removing
the longest shared prefix and nonoverlapping suffix, plus surrounding context.
For `fence-attributes`, the common prefix is `<` and suffix is `>\n`:

```text
input:  p>~~~ruby {linenos}\nputs “hello”\n~~~</p
pinned: div class="language-ruby highlighter-rouge"><div class="highlight"><pre class="highlight"><code><span class="nb">puts</span> <span class="s2">"hello"</span>\n</code></pre></div></div
```

The original tilde fence with `{linenos}` is a paragraph under this installed
GFM parser/configuration. The pinned Ruby fence produces highlighted code.
Both the exact and normalized HTML comparisons therefore differ; attributing
this only to lost line-number metadata would understate the observed change.

For `reference-link`, the smallest differing span at character offset 74 is
input `"\n"` versus pinned `""`, after the common
`title="Guide">reference</a>.</p>\n`. The link destination, title, and text agree.
The whitespace comparison also reports no because this LF is at EOF, not
between tags.

The following pinned outputs fail during real Liquid processing:

```text
liquid-tag: ArgumentError: Could not find document '\_posts/example.md' in tag 'link'.
probe: ArgumentError: Could not find document '\_posts/x.md' in tag 'link'.
```

Their original inputs both build successfully against the synthetic targets.
There is no pinned HTML for these cases, so no differing HTML fragment exists;
the script includes successful original HTML and the failing side's error.
The combined probe cannot establish the behavior of its later transformations
because its escaped link prevents the pinned page from completing.

For `liquid-expression`, both sides render exactly:

```html
<p>Synthetic navigation title</p>
```

This records Liquid 4.0.4 behavior with a populated synthetic `site.data.nav`;
it does not infer correctness for every Liquid expression or data context.
The removed comment definition and indented-to-fenced code conversion happen
to produce identical HTML in their isolated cases.

## Scope and validation

This supplies evidence level 3 for these thirteen synthetic pairs only. It does
not establish private-corpus safety, resolve the upstream edit-path defect, or
turn the Phase 2 gate into GO. The pre-existing README's unevaluated level-3
statement predates this separate evidence report and was left unchanged.

Validation: script syntax; all thirteen comparison rows; successful cleanup of
all temporary build sites; rejection of no argument, another directory, and a
symlink to another directory. No gems were installed and no goldens were edited.
