# Changelog

Notable changes to Jqln. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); Jqln aims to follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) once it reaches 1.0.

The release automation renames `## [Unreleased]` to the version being cut, so
keep new entries under that heading as you work.

## [Unreleased]

## [1.3.0] - 2026-09-05

### Added
- **Trash** — `d` now moves a document or folder (and its subtree) into a Trash
  folder at the bottom of the tree instead of erasing it. `Enter` on a trashed
  item restores it to exactly where it was; `d` again (or on the Trash folder)
  deletes for good after a confirmation; `X` empties the whole Trash. The Trash
  is excluded from compiles, word counts and search, and persists in
  `jqln.toml`. The Trash row and a status-bar marker turn yellow past ~20 items.
- The status bar shows a `⭯` marker once a project holds ≥ 50 snapshots.
- **Assistant panel** (`F9`) — a right-hand AI chat, Anthropic or OpenAI. Built
  into the normal binary but inert unless launched with `jqln
  --with-ai-assistant` (`--no-default-features` leaves it out of the build
  entirely). Your key comes from an env var or a paste-in popup that saves it
  to `~/.config/jqln/config.toml`. Streams replies, shows the context sent and a
  running token/cost estimate, and (with `allow_comments`) proposes inline
  `{>>…<<}` comments that `/apply` inserts on the current document. It never
  rewrites prose; nothing is sent until you send a message and confirm.
  Configured in the `[assistant]` table of `jqln.toml`.

### Fixed
- Deleting a document now also removes its `snapshots/<id>/` folder, which was
  previously left orphaned forever.

## [1.2.0] - 2026-08-30

### Added
- **Copy out** (`Ctrl-C` in the editor): the selection goes to the system
  clipboard via OSC 52 and the platform clipboard tool (`pbcopy` / `wl-copy` /
  `xclip` / `clip`). The selection is left intact.
- **Document notes** (`N` in the tree): a free-form note on any document or
  folder, shown above the prose while you write and marked `✎` in the tree,
  cards, and editor title. Stored one file per node in `notes/<id>.md`.
- **Inline comments** (`Ctrl-N` in the editor): CriticMarkup annotations
  anchored in the text — `{==phrase==}{>>note<<}` around a selection, or a bare
  `{>>note<<}` at the cursor. `Ctrl-N` on an existing comment re-edits it;
  clearing the text drops the comment but keeps the phrase it flagged.
  Underlined in the editor, stripped by every compile, and not counted in word
  counts.

## [1.1.0] - 2026-08-30

### Added
- English spell check. A bundled `en_US` dictionary underlines misspellings as
  you write — no install, no network. `Ctrl-G` on a flagged word offers
  corrections (`Enter` / a number to apply, `a` to learn it); `Ctrl-G` from the
  tree toggles the feature. The personal word list and the on/off state live in
  the `[spelling]` table of `jqln.toml`.

## [1.0.1] - 2026-08-30

### Added
- Card view shows the level the selection sits on — a chapter shows all the
  chapters. `Enter` descends into a folder card, `Backspace` steps back out.
- Reorder in the card view with `K` / `J`, `Alt`+arrows, or drag.
- `--` typed together becomes an em dash (`—`).

### Changed
- A new item made inside an open folder lands first (right below the folder
  row), not last.
- `?` is a literal question mark in the editor; `F1` still opens help there.

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
