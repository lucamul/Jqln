# Changelog

Notable changes to Jqln. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); Jqln aims to follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) once it reaches 1.0.

The release automation renames `## [Unreleased]` to the version being cut, so
keep new entries under that heading as you work.

## [Unreleased]

## [1.0.0] - 2026-08-29

### Added
- In-place text formatting: `Ctrl-B` bold, `Ctrl-I` italic (also `Tab` with a
  selection), `Ctrl-L` centre a line or the selected lines, `Ctrl-P` page break.
  Stored as plain Markdown; the editor styles the spans and fades the markers.
- `Ctrl-Z` as an undo alias alongside `Ctrl-U`.
- Drag inside the editor to select text; drag a card onto another to reorder the
  corkboard.
- **Book compile** (`F8`): a print-ready PDF via [Typst](https://typst.app) —
  generated front matter, chapter openers, running heads, page numbers, verse
  handling for `::: center` blocks.
- Front-matter folder support with title / copyright / dedication layouts.
- Book settings screen (`Ctrl-B` from the tree) editing the `[book]` table.
- Per-chapter heading override (`h`): numbered, the folder's own title, or a
  verbatim name like "Prologue" that takes no number.
- `[compile]` table in `jqln.toml` — `folder_headings`, `document_headings`,
  `separator`.
- `THIRD-PARTY-NOTICES`, generated with `cargo about`.

### Changed
- Licence is now the Jqln Source-Available License (was MIT).
- Prompts pre-filled with a value start fully selected, so the first keystroke
  replaces it.
- CRLF line endings are normalised to `\n` when a document loads.
- Source split from three large files into focused modules.
