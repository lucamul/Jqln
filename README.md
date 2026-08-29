# Jqln

A terminal writing studio for long-form prose — novels, theses, screenplays,
anything long enough that a single file stops being the right shape.

You write in small documents and arrange them in a tree. Jqln keeps the
structure; you keep the words. Nothing is locked in: your prose lives on disk
as ordinary Markdown files you can grep, diff, back up, and read with any
other tool.

```
┌ The Salt Road ─────────────────┐┌ Opening Scene ───────────────────────────────────────┐
│▾ Manuscript                    ││The one where it begins.                              │
│  ▾ Chapter One                 ││                                                      │
│    · Opening Scene             ││The road out of the salt flats was white and it went  │
│▫ Research  ○                   ││on for a very long way indeed.                        │
└────────────────────────────────┘└──────────────────────────────────────────────────────┘
 ● 19 w  ·  19 total  ·  0% of 50000  ·  +19 session
```

## Installing

Jqln needs **Rust 1.88 or newer**. Check with `rustc --version`, and update
with `rustup update stable` if you are behind.

```sh
git clone git@github.com:lucamul/Jqln.git
cd Jqln
cargo install --path .
```

That puts a `jqln` binary in `~/.cargo/bin`. If your shell cannot find it,
add that directory to your `PATH`:

```sh
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
```

To run without installing, use `cargo run --release -- <project-dir>`.

## Getting started

```sh
jqln my-novel     # creates my-novel/ if it does not exist, then opens it
jqln              # opens the project in the current directory
```

A new project starts with a Manuscript folder, one chapter, one scene, and a
Research folder that is excluded from compiling.

Press <kbd>F1</kbd> at any time for the key list. Press <kbd>Ctrl</kbd>+<kbd>S</kbd>
to save and <kbd>Ctrl</kbd>+<kbd>Q</kbd> to save and quit.

## How a project is stored

A project is a plain directory:

```
my-novel/
  jqln.toml                     structure and metadata
  docs/
    r4006vz-opening-scene.md    one document, prose only
    ...
  snapshots/
    r4006vz/20260829-143148.md  saved versions of that document
  my-novel.md                   output, written when you compile
```

`jqln.toml` owns the tree and every piece of metadata. The Markdown files
contain nothing but your text — no front matter, no Jqln-specific markup.
Emphasis is written as ordinary Markdown (`**bold**`, `*italic*`), so the
files stay readable on their own and in any other editor:

```toml
[[node]]
id = "r4006vz"
title = "Opening Scene"
kind = "text"
parent = "ed0y5vz"
file = "r4006vz-opening-scene.md"
synopsis = "The one where it begins."
include = true
```

Both files are line-oriented and diff cleanly, so a project is worth putting
under version control. Reordering a chapter shows up as a moved block in
`jqln.toml`, not as a rewrite of your prose.

## The four views

Switch with the function keys. All four show the same project.

| Key | View | What it is for |
| --- | --- | --- |
| <kbd>F2</kbd> | **Editor** | The tree beside one document. Where you write. |
| <kbd>F3</kbd> | **Cards** | One index card per document, showing synopses instead of prose. For judging structure without reading. |
| <kbd>F4</kbd> | **Outline** | Every document as a row with word count, status, and compile flag. For seeing where the weight sits. |
| <kbd>F6</kbd> | **Continuous** | Every document in the current folder as one scrolling flow, so a chapter reads as a chapter. Toggles on top of the editor. |

Continuous mode does not merge your files. Each document keeps its own editor
and its own place on disk; they are only stacked for reading and writing.

## Keys

Jqln is modeless. The tree pane owns single-letter commands; the editor takes
raw text. Focus decides what a key means, so `n` starts a new document in the
tree and types the letter `n` in the editor.

### Anywhere

| Key | Action |
| --- | --- |
| <kbd>F1</kbd> or `?` | Help |
| <kbd>F2</kbd> / <kbd>F3</kbd> / <kbd>F4</kbd> | Editor / cards / outline |
| <kbd>Ctrl</kbd>+<kbd>F</kbd> | Search the whole project |
| <kbd>F7</kbd> | Mouse on / off |
| <kbd>F5</kbd> | Compile to a single Markdown file |
| <kbd>F8</kbd> | Compile the novel template to a PDF |
| <kbd>F6</kbd> | Toggle continuous mode |
| <kbd>Ctrl</kbd>+<kbd>S</kbd> | Save |
| <kbd>Ctrl</kbd>+<kbd>Q</kbd> | Save and quit |

### In the tree

| Key | Action |
| --- | --- |
| <kbd>↑</kbd> <kbd>↓</kbd> or `k` `j` | Move through the tree |
| <kbd>→</kbd> <kbd>←</kbd> | Expand / collapse, or step out to the parent |
| <kbd>Space</kbd> | Fold or unfold a folder |
| <kbd>Enter</kbd> | Open the document and start writing |
| `n` / `f` | New document / new folder |
| `r` / `s` | Rename / edit synopsis |
| `t` / `l` / `w` | Status / label / keywords |
| `i` | Include or exclude from compiling |
| `c` | Compile just this subtree |
| `v` | Snapshots of this document |
| <kbd>Ctrl</kbd>+<kbd>B</kbd> | Book / PDF settings |
| `d` | Delete, with a confirmation |
| <kbd>Alt</kbd>+<kbd>↑</kbd> <kbd>↓</kbd>, or `K` `J` | Reorder among siblings |
| <kbd>Alt</kbd>+<kbd>→</kbd> <kbd>←</kbd>, or `>` `<` | Indent / outdent |

Desktop environments often reserve <kbd>Alt</kbd>+arrow keys for themselves, and
some terminals answer <kbd>F1</kbd> before the program sees it. The letter
alternatives above work everywhere.

A new document is created inside the selected folder when that folder is open,
and next to the selection otherwise. Keywords are entered as a comma separated
list.

On the card view the same commands apply. Move between cards with the arrow
keys, or `j` and `k` to change row.

### While writing

<kbd>Esc</kbd> returns to the tree. Otherwise the editor uses the standard
Emacs-style bindings: <kbd>Ctrl</kbd>+<kbd>A</kbd> and <kbd>Ctrl</kbd>+<kbd>E</kbd>
for start and end of line, <kbd>Ctrl</kbd>+<kbd>W</kbd> to delete a word,
<kbd>Ctrl</kbd>+<kbd>K</kbd> to cut to end of line. Undo is
<kbd>Ctrl</kbd>+<kbd>Z</kbd> or <kbd>Ctrl</kbd>+<kbd>U</kbd>, redo is
<kbd>Ctrl</kbd>+<kbd>R</kbd>.

Undo works a word at a time rather than a character at a time, which is the
right size of step for prose. A formatting toggle is a delete and an insert, so
it takes two presses to walk back off.

### Formatting

| Key | Action |
| --- | --- |
| <kbd>Ctrl</kbd>+<kbd>B</kbd> | Bold the selection, or the word under the cursor |
| <kbd>Ctrl</kbd>+<kbd>I</kbd> | Italic — or <kbd>Tab</kbd> while text is selected |
| <kbd>Ctrl</kbd>+<kbd>L</kbd> | Centre the current line, or every line the selection touches |
| <kbd>Ctrl</kbd>+<kbd>P</kbd> | Insert a page break |

Formatting is stored as plain Markdown in your document files: `**bold**`,
`*italic*`, a `::: center` / `:::` fence around the centred lines, and a lone
`\newpage` for a page break. The editor styles those spans in place and fades
the markers, so you read the prose, not the punctuation. Pressing the same key
again on formatted text takes the formatting back off — <kbd>Ctrl</kbd>+<kbd>L</kbd>
with the cursor anywhere inside a centred block lifts the whole fence.

Some terminals send <kbd>Ctrl</kbd>+<kbd>I</kbd> as <kbd>Tab</kbd> and cannot
tell the two apart. That is why <kbd>Tab</kbd> also italicises whenever text is
selected; with nothing selected it still inserts a tab.

In continuous mode, <kbd>PageUp</kbd> and <kbd>PageDown</kbd> scroll the whole
flow, and <kbd>Ctrl</kbd>+<kbd>↑</kbd> <kbd>↓</kbd> step between documents.
Those two override the editor's page-scroll and paragraph-movement bindings
while continuous mode is on; paragraph movement is still on
<kbd>Alt</kbd>+<kbd>[</kbd> and <kbd>Alt</kbd>+<kbd>]</kbd>.

## Compiling

<kbd>F5</kbd> walks the tree in order and joins everything into one Markdown
file next to the project. Folder titles become headings, nested by depth.
Document titles are left out, since scene names are usually scaffolding rather
than part of the finished text.

Anything marked excluded with `i` is skipped, and excluding a folder excludes
everything inside it. That is how the Research folder stays out of the
manuscript without touching the notes in it.

`c` compiles only the selected subtree, writing to a file named after that
node, so exporting a single chapter never overwrites the whole manuscript.

Formatting markup is copied through untouched, so the compiled file is valid
Markdown that a tool like Pandoc can turn into a PDF or an EPUB — `\newpage`
becomes a real page break, a `::: center` fence a centred block.

## The book

<kbd>F8</kbd> compiles the project as a novel: a print-ready PDF with the front
matter on its own pages, chapters that open on a fresh page, running heads and
page numbers that begin at the story.

Jqln does not typeset anything itself. It writes a [Typst](https://typst.app)
document — `<title>.typ`, next to the project — and, when the `typst` binary is
on your `PATH`, runs it to produce `<title>.pdf`. Typst is a single ~30 MB
binary, no TeX install; get it from <https://github.com/typst/typst/releases>
or `brew install typst`. Without it you still get the `.typ`, which you can
compile anywhere. The `.typ` is plain text: read it, tweak it, run
`typst compile` on it yourself.

### Front matter

A top-level folder named **Front Matter** (rename it with the `front_matter_folder`
key) is kept out of the ordinary manuscript, and its documents become the pages
before the story — in binder order, each on its own unnumbered page. Jqln looks
at each document's title:

- one containing **title** → a title-page layout;
- one containing **copyright** → a copyright-page layout;
- one containing **dedication** → centred italic, no heading;
- anything else (acknowledgements, epigraph, also-by…) → its title as a small
  centred heading, then the prose.

If there is no title or copyright document, Jqln generates one from `[book]`.
A dedication is generated from `book.dedication` when set and no dedication
document exists.

### Settings

<kbd>Ctrl</kbd>+<kbd>B</kbd> from the tree opens the book settings — the same
fields as a small in-editor list. <kbd>Enter</kbd> edits a field, the arrow keys
toggle the switches and cycle the trim size, <kbd>Esc</kbd> closes; save with
<kbd>Ctrl</kbd>+<kbd>S</kbd> as usual.

The fields all live in a `[book]` table in `jqln.toml`, so you can edit them
there too:

```toml
[book]
title = "The Salt Road"        # defaults to the project name
subtitle = "a novel"
author = "Your Name"
copyright_year = 2026           # 0 = the year you compile
copyright_holder = ""           # defaults to the author
publisher = "Salt Flats Press"
rights = "All rights reserved."
dedication = "For the road."
trim = "5.5x8.5"               # also 5x8, 5.25x8, 6x9, a5
body_font = "Libertinus Serif"
body_size = 11.0
chapter_label = "Chapter"      # "" for a bare numeral
scene_break = "•   •   •"
running_heads = true
chapters_on_recto = false      # true starts every chapter on a right-hand page
```

Folders become parts and chapters: a lone wrapper folder (a "Manuscript"
holding the chapters) is unwrapped, a folder of folders is a part, a folder of
documents is a chapter, and the documents inside are its scenes — separated in
the PDF by the `scene_break` glyphs, or by a `* * *` / `---` line in the prose.
A chapter folder titled plainly (`Chapter 4`, `Seven`, `XiV`) just gets its
number; any other title is printed as a subtitle.

Ordinary prose is set justified with wrapped lines reflowed. A `::: center`
block is set as verse instead: centred, unjustified, every line break kept and
a blank line between stanzas — so poems and song lyrics come out as written.

## The mouse

Click a row in the tree to select it, a card to pick it, or anywhere in the
text to put the cursor there — including inside a continuous flow, where
clicking a document focuses it at the point you clicked. Drag inside the editor
to select a run of text, which is what <kbd>Ctrl</kbd>+<kbd>B</kbd> and the rest
act on; the selection stays inside the editor and never spills into the tree.
The wheel scrolls whichever pane is under the pointer.

Capturing the mouse means the terminal hands those events to Jqln instead of
handling them itself. Jqln does its own click-and-drag selection, but the
terminal's own drag-to-select-and-copy — the one that reaches your system
clipboard — is off while capture is on. So it is a mode, not a fixture:
<kbd>F7</kbd> turns capture off and gives the terminal back its normal selection
behaviour, and turns it on again when you are done.

## Searching

<kbd>Ctrl</kbd>+<kbd>F</kbd> searches titles, synopses and prose across the
whole project, including text you have typed but not yet saved. Results list
the document and line; <kbd>Enter</kbd> opens the document with the cursor on
the matching line.

Queries are plain text by default, because prose is full of brackets and full
stops that would otherwise be read as syntax. Wrap a query in slashes for a
regular expression:

```
kestrel      matches the word, literally
/s[ae]lt/    matches "salt" or "selt"
/^The /      matches lines starting with "The "
```

Either way the match ignores case. A malformed pattern is reported rather than
silently finding nothing.

## Snapshots

Rewriting is easier when going back is cheap. `v` opens the snapshot list for
the selected document, where `t` takes a copy of the text as it stands and
<kbd>Enter</kbd> restores the highlighted one.

Restoring is not destructive: the text being replaced is snapshotted first, so
an accidental restore can itself be undone. `d` removes a snapshot, and asks
for a second press first, since a snapshot is the backup of last resort.

Snapshots are plain Markdown under `snapshots/`, named by date.

## Word counts

The status bar tracks the current document, the project total, progress
against your target, and how much you have added this session. Targets live in
`jqln.toml`:

```toml
[targets]
project_words = 50000
session_words = 500
```

## Not there yet

- Windows has never been run or tested
- Cards cannot be dragged to reorder; use the tree for that
- Markdown compile settings are fixed: folder titles become headings, document
  titles do not, and the separator is a blank line
- The book template has one layout. It is a generated `.typ` you can edit, but
  the knobs Jqln exposes are just the `[book]` table
- Emphasis does not span a line break; keep `*a phrase*` on one line

## Platforms

Developed on macOS. The full test suite is also run on Linux (aarch64, Rust
1.88) and the tree is type-checked for `x86_64-unknown-linux-gnu`, so both
common Linux architectures are covered. The code is plain portable Rust with
no platform-specific paths or system calls.

Windows is untested. It will most likely compile, since every dependency
supports it, but nobody has run it there.

## Licence

MIT. See [LICENSE](LICENSE).

## Developing

```sh
cargo test      # unit tests plus rendering tests against a headless terminal
cargo clippy --all-targets
cargo check --target x86_64-unknown-linux-gnu --all-targets   # portability

# the suite on real Linux
docker run --rm -v "$PWD":/w -w /w -e CARGO_TARGET_DIR=/tmp/target \
  rust:1.88 cargo test
```

The rendering tests draw into an off-screen terminal buffer and assert on the
characters that come out, so layout and wrapping are covered rather than
merely compiled.
