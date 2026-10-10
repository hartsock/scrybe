# Shared front matter fixtures

Each case directory contains `input.md` (original source) and `body.md`
(expected Markdown body). Optional `frontmatter.yaml` contains the exact expected
front matter, with one trailing LF or CRLF removed. An absent file means no
front matter; an empty file means a recognized empty block. Compare bytes without
normalizing line endings or adding final newlines. The `crlf` case uses CRLF.

Splitting follows Jekyll: only an opener at byte zero qualifies, and a missing
closer leaves the entire input unchanged. Delimiter whitespace uses Ruby's ASCII
whitespace set, including newlines. The body is always a borrowed byte suffix.
The `hr-pair-*` cases retain both mid-document thematic breaks for render tests.
The `heading-in-yaml*` cases ensure YAML comments do not become document titles.
