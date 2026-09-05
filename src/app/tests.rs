//! Key-and-mouse-level tests driving a real App through synthesised events.

use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tui_textarea::CursorMove;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    }
}

fn key_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    }
}

/// A private directory per test. These run in parallel, and a clock-based
/// name is not safe: macOS timestamp granularity is coarse enough for two
/// tests to land on the same path, after which one test's cleanup deletes
/// the project another is still building. A counter cannot collide.
fn app() -> App {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("jqln-app-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    let p = Project::create(&dir, "T").unwrap();
    App::new(p)
}

#[test]
fn navigates_and_opens_only_text_documents() {
    let mut a = app();
    // Row 0 is the "Manuscript" folder.
    assert_eq!(a.rows().len(), 4);
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.focus, Focus::Binder), "folders must not open the editor");

    // Move to "Opening Scene" (row 2) and open it.
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    assert!(a.editor_doc().is_some());
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.focus, Focus::Editor));

    // Typing lands in the document, Esc returns to the binder.
    a.on_key(key(KeyCode::Char('H')));
    a.on_key(key(KeyCode::Char('i')));
    assert_eq!(a.current_words(), 1);
    assert!(a.dirty);
    a.on_key(key(KeyCode::Esc));
    assert!(matches!(a.focus, Focus::Binder));

    // 'i' is a binder command again, not text.
    let before = a.current_words();
    a.on_key(key(KeyCode::Char('i')));
    assert_eq!(a.current_words(), before);
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn collapse_hides_children() {
    let mut a = app();
    assert_eq!(a.rows().len(), 4);
    a.on_key(key(KeyCode::Char(' ')));  // collapse "Manuscript"
    assert_eq!(a.rows().len(), 2, "collapsing must hide the subtree");
    a.on_key(key(KeyCode::Char(' ')));
    assert_eq!(a.rows().len(), 4);
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn new_document_goes_inside_an_open_folder() {
    let mut a = app();
    a.on_key(key(KeyCode::Char('n')));
    for c in "Scene Two".chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    a.on_key(key(KeyCode::Enter));
    let id = a.selected_id().unwrap();
    assert_eq!(a.project.nodes[&id].title, "Scene Two");
    // "Manuscript" was selected and expanded, so it becomes the parent, and the
    // new node is its first child — right under the Manuscript row.
    let parent = a.project.parent_of(&id);
    assert_eq!(a.project.nodes[&parent].title, "Manuscript");
    assert_eq!(a.project.children[&parent].first().unwrap(), &id);
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn a_new_folder_beside_a_chapter_not_inside_it() {
    let mut a = app();
    // "Chapter One" is open and holds "Opening Scene" — a chapter, not a part.
    a.on_key(key(KeyCode::Down));
    let chapter_one = a.selected_id().unwrap();
    let manuscript = a.project.parent_of(&chapter_one);
    let before = a.project.index_in_parent(&chapter_one);

    a.on_key(key(KeyCode::Char('f')));
    type_str(&mut a, "Chapter Two");
    a.on_key(key(KeyCode::Enter));
    let new = a.selected_id().unwrap();

    assert_eq!(a.project.parent_of(&new), manuscript, "a sibling of Chapter One");
    assert_eq!(a.project.index_in_parent(&new), before + 1, "immediately after it");

    // A new *document* on that same chapter still goes inside it.
    a.select_id(&chapter_one);
    a.on_key(key(KeyCode::Char('n')));
    type_str(&mut a, "Second Scene");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.project.parent_of(&a.selected_id().unwrap()), chapter_one);
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn a_new_folder_nests_into_a_folder_of_folders() {
    let mut a = app();
    // "Manuscript" holds "Chapter One" (a folder) and no loose documents.
    let manuscript = a.selected_id().unwrap();
    assert_eq!(a.project.nodes[&manuscript].title, "Manuscript");

    a.on_key(key(KeyCode::Char('f')));
    type_str(&mut a, "Chapter Two");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(
        a.project.parent_of(&a.selected_id().unwrap()),
        manuscript,
        "a folder of folders takes new chapters inside it"
    );
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn delete_moves_to_trash_and_restores() {
    let mut a = app();
    // Row 1 is "Chapter One" (a folder holding "Opening Scene").
    a.on_key(key(KeyCode::Down));
    let chapter = a.selected_id().unwrap();
    let manuscript = a.project.parent_of(&chapter);

    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('n'))); // cancels
    assert_eq!(a.project.parent_of(&chapter), manuscript);

    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('y')));
    assert!(a.project.is_trashed(&chapter), "moved to Trash, not erased");
    assert_eq!(a.project.trash_count(), 1);
    // Still in the model, and the Trash starts expanded so it shows in the rows.
    assert!(a.rows().iter().any(|(id, _)| id == &chapter));
    // The Trash node is the last top-level row.
    let rows = a.rows();
    let last_top = rows.iter().rev().find(|(_, d)| *d == 0).map(|(id, _)| id.as_str());
    assert_eq!(last_top, Some("__trash__"));

    // Select the trashed chapter and restore it with Enter.
    a.select_id(&chapter);
    a.on_key(key(KeyCode::Enter));
    assert!(!a.project.is_trashed(&chapter));
    assert_eq!(a.project.parent_of(&chapter), manuscript);
    assert_eq!(a.project.trash_count(), 0);
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn empty_trash_erases_for_good() {
    let mut a = app();
    a.on_key(key(KeyCode::Down)); // "Chapter One"
    let chapter = a.selected_id().unwrap();
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('y')));
    assert!(a.project.is_trashed(&chapter));

    // `X` targets the Trash node; `y` empties it.
    a.on_key(key(KeyCode::Char('X')));
    assert!(matches!(a.modal, Modal::ConfirmDelete));
    a.on_key(key(KeyCode::Char('y')));
    assert!(!a.project.nodes.contains_key(&chapter), "gone from the model");
    assert!(!a.project.nodes.contains_key("__trash__"), "Trash node pruned when empty");
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn alt_arrows_restructure_the_tree() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));  // "Chapter One"
    let id = a.selected_id().unwrap();
    assert_eq!(a.project.nodes[&id].title, "Chapter One");

    a.on_key(key_mod(KeyCode::Left, KeyModifiers::ALT));  // outdent
    assert_eq!(a.project.parent_of(&id), ROOT);
    // Selection follows the node it moved.
    assert_eq!(a.selected_id().as_deref(), Some(id.as_str()));

    a.on_key(key_mod(KeyCode::Right, KeyModifiers::ALT));  // indent back
    let parent = a.project.parent_of(&id);
    assert_eq!(a.project.nodes[&parent].title, "Manuscript");
    let _ = std::fs::remove_dir_all(&a.project.root);
}

fn type_str(a: &mut App, s: &str) {
    for c in s.chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
}

#[test]
fn metadata_fields_are_editable_and_persist() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));  // "Opening Scene"
    let id = a.selected_id().unwrap();

    a.on_key(key(KeyCode::Char('t')));
    type_str(&mut a, "First draft");
    a.on_key(key(KeyCode::Enter));

    a.on_key(key(KeyCode::Char('l')));
    type_str(&mut a, "Act One");
    a.on_key(key(KeyCode::Enter));

    a.on_key(key(KeyCode::Char('w')));
    type_str(&mut a, "salt, road,  desert ");
    a.on_key(key(KeyCode::Enter));

    assert_eq!(a.project.nodes[&id].status, "First draft");
    assert_eq!(a.project.nodes[&id].label, "Act One");
    // Comma separated, trimmed, and blanks dropped.
    assert_eq!(a.project.nodes[&id].keywords, ["salt", "road", "desert"]);

    a.save();
    let root = a.project.root.clone();
    let q = Project::open(&root).unwrap();
    assert_eq!(q.nodes[&id].keywords, ["salt", "road", "desert"]);
    assert_eq!(q.nodes[&id].status, "First draft");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn subtree_compile_writes_its_own_file() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));  // "Chapter One"
    let id = a.selected_id().unwrap();
    assert_eq!(a.project.nodes[&id].title, "Chapter One");

    // Give the scene inside some text.
    a.on_key(key(KeyCode::Down));
    let scene = a.selected_id().unwrap();
    a.ensure_editor(&scene);
    a.editors.get_mut(&scene).unwrap().insert_str("Chapter text.");

    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Char('c')));

    let path = a.project.root.join("chapter-one.md");
    assert!(path.exists(), "subtree compile should write chapter-one.md");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("Chapter text."));
    // The whole-project file is a different name, so neither clobbers the other.
    assert!(!a.project.root.join("t.md").exists());
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn letters_reach_the_editor_not_the_binder() {
    // The metadata bindings t/l/w must not fire while writing.
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    type_str(&mut a, "twl");
    let id = a.editor_doc().unwrap();
    assert_eq!(a.editors[&id].lines().join(""), "twl");
    assert!(matches!(a.modal, Modal::None), "no prompt should have opened");
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn search_jumps_to_the_matching_document() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    let scene = a.selected_id().unwrap();
    a.ensure_editor(&scene);
    a.editors
        .get_mut(&scene)
        .unwrap()
        .insert_str("line one\nthe kestrel turned\nline three");

    // Move away, then search from the tree.
    a.on_key(key(KeyCode::Home));
    a.on_key(key_mod(KeyCode::Char('f'), KeyModifiers::CONTROL));
    type_str(&mut a, "kestrel");
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Modal::Results));
    assert_eq!(a.hits.len(), 1);

    a.on_key(key(KeyCode::Enter));  // jump
    assert!(matches!(a.modal, Modal::None));
    assert_eq!(a.selected_id().as_deref(), Some(scene.as_str()));
    assert!(matches!(a.focus, Focus::Editor));
    // Cursor landed on the matching line (0-based row 1).
    assert_eq!(a.editors[&scene].cursor().0, 1);
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn a_search_with_no_matches_reports_instead_of_opening_a_list() {
    let mut a = app();
    a.on_key(key_mod(KeyCode::Char('f'), KeyModifiers::CONTROL));
    type_str(&mut a, "zzzz");
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Modal::None));
    assert!(a.status.contains("No matches"));
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn snapshot_and_restore_through_the_interface() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    type_str(&mut a, "original");
    a.on_key(key(KeyCode::Esc));

    a.on_key(key(KeyCode::Char('v')));
    assert!(matches!(a.modal, Modal::Snapshots));
    a.on_key(key(KeyCode::Char('t')));   // take one
    assert_eq!(a.snaps.len(), 1);
    a.on_key(key(KeyCode::Esc));

    // Rewrite the document.
    a.on_key(key(KeyCode::Enter));
    let id = a.editor_doc().unwrap();
    a.editors.get_mut(&id).unwrap().select_all();
    a.editors.get_mut(&id).unwrap().insert_str("replaced");
    assert_eq!(a.editors[&id].lines().join(""), "replaced");
    a.on_key(key(KeyCode::Esc));

    // Restore brings the original text back into the live editor.
    a.on_key(key(KeyCode::Char('v')));
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.editors[&id].lines().join(""), "original");
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn snapshots_are_refused_on_folders() {
    let mut a = app();  // row 0 is a folder
    a.on_key(key(KeyCode::Char('v')));
    assert!(matches!(a.modal, Modal::None));
    assert!(a.status.contains("folders have no text"));
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn deleting_a_snapshot_takes_two_presses() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    type_str(&mut a, "words");
    a.on_key(key(KeyCode::Esc));

    a.on_key(key(KeyCode::Char('v')));
    a.on_key(key(KeyCode::Char('t')));
    assert_eq!(a.snaps.len(), 1);

    // One press only arms it.
    a.on_key(key(KeyCode::Char('d')));
    assert_eq!(a.snaps.len(), 1, "a single press must not delete");
    assert!(a.snap_confirm);

    // Anything else disarms it.
    a.on_key(key(KeyCode::Down));
    assert!(!a.snap_confirm);
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('d')));
    assert!(a.snaps.is_empty(), "two presses should delete");
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn a_broken_regex_reports_instead_of_crashing() {
    let mut a = app();
    a.on_key(key_mod(KeyCode::Char('f'), KeyModifiers::CONTROL));
    type_str(&mut a, "/oops(/");
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Modal::None));
    assert!(a.status.starts_with("Bad search:"), "got: {}", a.status);
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn ctrl_b_wraps_and_unwraps_the_selection() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    type_str(&mut a, "the salt road");
    let id = a.editor_doc().unwrap();

    // Select "salt".
    let ta = a.editors.get_mut(&id).unwrap();
    ta.move_cursor(CursorMove::Jump(0, 4));
    ta.start_selection();
    ta.move_cursor(CursorMove::Jump(0, 8));

    a.on_key(key_mod(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert_eq!(a.editors[&id].lines().join("\n"), "the **salt** road");
    assert!(a.dirty);

    // The word stayed selected, so the same key strips it again.
    a.on_key(key_mod(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert_eq!(a.editors[&id].lines().join("\n"), "the salt road");
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn ctrl_b_with_no_selection_bolds_the_word_under_the_cursor() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    type_str(&mut a, "hello world");
    let id = a.editor_doc().unwrap();

    a.on_key(key_mod(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert_eq!(a.editors[&id].lines().join("\n"), "hello **world**");
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn tab_italicises_a_selection_but_still_indents_otherwise() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    type_str(&mut a, "word");
    let id = a.editor_doc().unwrap();

    // No selection: Tab inserts whitespace as before.
    a.on_key(key(KeyCode::Tab));
    assert!(a.editors[&id].lines().join("\n").ends_with("    "));

    // With a selection: Tab wraps it in italic markers.
    let ta = a.editors.get_mut(&id).unwrap();
    ta.move_cursor(CursorMove::Jump(0, 0));
    ta.start_selection();
    ta.move_cursor(CursorMove::Jump(0, 4));
    a.on_key(key(KeyCode::Tab));
    assert_eq!(a.editors[&id].lines().join("\n"), "*word*    ");
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn ctrl_l_toggles_a_centered_fence_and_ctrl_p_adds_a_page_break() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    type_str(&mut a, "middle");
    let id = a.editor_doc().unwrap();

    a.on_key(key_mod(KeyCode::Char('l'), KeyModifiers::CONTROL));
    assert_eq!(a.editors[&id].lines(), ["::: center", "middle", ":::"]);

    a.on_key(key_mod(KeyCode::Char('l'), KeyModifiers::CONTROL));
    assert_eq!(a.editors[&id].lines(), ["middle"]);

    a.on_key(key_mod(KeyCode::Char('p'), KeyModifiers::CONTROL));
    assert_eq!(a.editors[&id].lines(), ["middle", "", "\\newpage", ""]);
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn ctrl_l_centres_every_line_the_selection_touches() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    type_str(&mut a, "one\ntwo\nthree");
    let id = a.editor_doc().unwrap();

    // Select from the middle of line 0 to the middle of line 2.
    let ta = a.editors.get_mut(&id).unwrap();
    ta.move_cursor(CursorMove::Jump(0, 1));
    ta.start_selection();
    ta.move_cursor(CursorMove::Jump(2, 2));

    a.on_key(key_mod(KeyCode::Char('l'), KeyModifiers::CONTROL));
    assert_eq!(
        a.editors[&id].lines(),
        ["::: center", "one", "two", "three", ":::"]
    );

    // Toggling again from inside the block removes the fence.
    a.editors.get_mut(&id).unwrap().move_cursor(CursorMove::Jump(2, 0));
    a.on_key(key_mod(KeyCode::Char('l'), KeyModifiers::CONTROL));
    assert_eq!(a.editors[&id].lines(), ["one", "two", "three"]);
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn formatting_is_undoable() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    type_str(&mut a, "the salt road");
    let id = a.editor_doc().unwrap();

    let ta = a.editors.get_mut(&id).unwrap();
    ta.move_cursor(CursorMove::Jump(0, 4));
    ta.start_selection();
    ta.move_cursor(CursorMove::Jump(0, 8));
    a.on_key(key_mod(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert_eq!(a.editors[&id].lines().join("\n"), "the **salt** road");

    // A wrap is a delete-then-insert, so it lifts in two Ctrl-Z presses,
    // leaving the prose exactly as it was.
    a.on_key(key_mod(KeyCode::Char('z'), KeyModifiers::CONTROL));
    a.on_key(key_mod(KeyCode::Char('z'), KeyModifiers::CONTROL));
    assert_eq!(a.editors[&id].lines().join("\n"), "the salt road");
    assert!(a.dirty);
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn edits_survive_a_save_and_reload() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    for c in "Once upon a time".chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    a.save();
    assert!(!a.dirty);
    let root = a.project.root.clone();

    let mut re = Project::open(&root).unwrap();
    assert_eq!(re.total_words(), 4);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn enter_inserts_a_newline_in_the_editor() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.focus, Focus::Editor));
    type_str(&mut a, "abc");
    a.on_key(key(KeyCode::Enter));
    type_str(&mut a, "def");
    let id = a.editor_doc().unwrap();
    assert_eq!(a.editors[&id].lines(), ["abc", "def"]);
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn question_mark_is_a_question_mark_while_writing() {
    let mut a = app();
    // From the tree, `?` opens help.
    a.on_key(key(KeyCode::Char('?')));
    assert!(matches!(a.modal, Modal::Help));
    a.on_key(key(KeyCode::Esc));

    // In the editor, it is text.
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    type_str(&mut a, "why?");
    assert!(matches!(a.modal, Modal::None));
    assert_eq!(a.editors[&a.editor_doc().unwrap()].lines().join(""), "why?");
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn a_double_hyphen_becomes_an_em_dash() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    type_str(&mut a, "wait -- no");
    let id = a.editor_doc().unwrap();
    assert_eq!(a.editors[&id].lines().join(""), "wait — no");

    // A third hyphen does not undo the dash.
    type_str(&mut a, " a---b");
    assert!(a.editors[&id].lines().join("").ends_with("a—-b"));
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn cards_show_a_level_and_alt_arrows_reorder_it() {
    use crate::project::Kind;
    let mut a = app();
    // Two more chapters beside "Chapter One".
    let ms = a.rows()[0].0.clone();
    let c1 = a.rows()[1].0.clone();
    let c2 = a.project.insert(&ms, None, "Chapter Two", Kind::Folder);
    a.project.insert(&c2, None, "s", Kind::Text);
    let c3 = a.project.insert(&ms, None, "Chapter Three", Kind::Folder);
    a.project.insert(&c3, None, "s", Kind::Text);

    // On a chapter, F3 shows all the chapters.
    a.select_id(&c2);
    a.on_key(key(KeyCode::F(3)));
    assert_eq!(a.card_root, ms);
    assert_eq!(a.cards(), vec![c1.clone(), c2.clone(), c3.clone()]);

    // Alt-Down pushes Chapter Two past Chapter Three.
    a.on_key(key_mod(KeyCode::Down, KeyModifiers::ALT));
    assert_eq!(a.cards(), vec![c1.clone(), c3.clone(), c2.clone()]);
    assert_eq!(a.selected_id().as_deref(), Some(c2.as_str()));
    assert!(a.dirty);

    // Capital K works too, and against a terminal that has no Alt.
    a.on_key(key(KeyCode::Char('K')));
    assert_eq!(a.cards(), vec![c1, c2.clone(), c3]);
    assert_eq!(a.selected_id().as_deref(), Some(c2.as_str()));
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn book_settings_edit_toggle_and_cycle() {
    use crate::app::BookField;
    let mut a = app();

    // Ctrl-B from the tree opens the settings list.
    a.on_key(key_mod(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert!(matches!(a.modal, Modal::BookSettings));

    // Row 0 is Title: Enter opens a prompt, type a value, Enter commits and
    // returns to the list.
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Modal::Input(_)));
    type_str(&mut a, "The Salt Road");
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Modal::BookSettings));
    assert_eq!(a.project.book.title, "The Salt Road");
    assert!(a.dirty);

    // Move to "Running heads" and toggle it with the arrow keys.
    let rh = BookField::ALL.iter().position(|f| f.label() == "Running heads").unwrap();
    for _ in 0..rh {
        a.on_key(key(KeyCode::Down));
    }
    let before = a.project.book.running_heads;
    a.on_key(key(KeyCode::Right));
    assert_eq!(a.project.book.running_heads, !before);

    // Trim size cycles.
    let tr = BookField::ALL.iter().position(|f| f.label() == "Trim size").unwrap();
    a.on_key(key(KeyCode::Home));
    for _ in 0..tr {
        a.on_key(key(KeyCode::Down));
    }
    a.project.book.trim = "5x8".into();
    a.on_key(key(KeyCode::Right));
    assert_eq!(a.project.book.trim, "5.25x8");

    a.on_key(key(KeyCode::Esc));
    assert!(matches!(a.modal, Modal::None));

    // Persists through a save + reload.
    a.save();
    let root = a.project.root.clone();
    let q = Project::open(&root).unwrap();
    assert_eq!(q.book.title, "The Salt Road");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn h_sets_a_chapter_heading_override() {
    let mut a = app();
    // Row 1 is "Chapter One" (a folder).
    a.on_key(key(KeyCode::Down));
    let id = a.selected_id().unwrap();

    a.on_key(key(KeyCode::Char('h')));
    assert!(matches!(a.modal, Modal::Input(_)));
    type_str(&mut a, "Prologue");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.project.nodes[&id].heading, "Prologue");
    assert!(a.dirty);

    // "title" and "numbered" fold to their canonical forms.
    a.on_key(key(KeyCode::Char('h')));
    type_str(&mut a, "titled");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.project.nodes[&id].heading, "title");

    a.on_key(key(KeyCode::Char('h')));
    type_str(&mut a, "numbered");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.project.nodes[&id].heading, "");
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn capital_n_edits_a_nodes_notes() {
    let mut a = app();
    a.on_key(key(KeyCode::Down)); // "Chapter One" (a folder — folders get notes)
    let id = a.selected_id().unwrap();

    a.on_key(key(KeyCode::Char('N')));
    assert!(matches!(a.modal, Modal::Notes));
    type_str(&mut a, "cut if the pacing drags");
    a.on_key(key_mod(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(matches!(a.modal, Modal::None));
    assert!(a.dirty);
    assert!(a.project.has_note(&id));
    assert_eq!(a.project.note(&id), "cut if the pacing drags");

    // Esc discards an edit.
    a.on_key(key(KeyCode::Char('N')));
    type_str(&mut a, "XXX");
    a.on_key(key(KeyCode::Esc));
    assert_eq!(a.project.note(&id), "cut if the pacing drags");
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn ctrl_n_adds_edits_and_clears_an_inline_comment() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down)); // "Opening Scene"
    a.on_key(key(KeyCode::Enter));
    let id = a.editor_doc().unwrap();
    type_str(&mut a, "The road was long.");

    // No selection: a bare comment is inserted at the cursor.
    a.on_key(key_mod(KeyCode::Char('n'), KeyModifiers::CONTROL));
    assert!(matches!(a.modal, Modal::Input(Prompt::Comment)));
    type_str(&mut a, "too flat?");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.editors[&id].lines()[0], "The road was long.{>>too flat?<<}");
    assert!(a.dirty);

    // Cursor sits just after the marker — Ctrl-N re-edits it.
    a.on_key(key_mod(KeyCode::Char('n'), KeyModifiers::CONTROL));
    assert_eq!(a.input.lines()[0], "too flat?");
    type_str(&mut a, "fix the rhythm");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.editors[&id].lines()[0], "The road was long.{>>fix the rhythm<<}");

    // Clearing the text removes the comment.
    a.on_key(key_mod(KeyCode::Char('n'), KeyModifiers::CONTROL));
    a.on_key(key(KeyCode::Backspace)); // the prefilled text starts selected
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.editors[&id].lines()[0], "The road was long.");
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn ctrl_n_wraps_a_selection_with_a_highlight() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    let id = a.editor_doc().unwrap();
    type_str(&mut a, "keep the salt road");

    // Select "salt" (chars 9..13) then comment it.
    let ta = a.editors.get_mut(&id).unwrap();
    ta.move_cursor(CursorMove::Jump(0, 9));
    ta.start_selection();
    ta.move_cursor(CursorMove::Jump(0, 13));
    a.on_key(key_mod(KeyCode::Char('n'), KeyModifiers::CONTROL));
    type_str(&mut a, "vivid");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.editors[&id].lines()[0], "keep the {==salt==}{>>vivid<<} road");

    // Clearing the comment keeps the phrase it flagged.
    a.on_key(key_mod(KeyCode::Char('n'), KeyModifiers::CONTROL));
    a.on_key(key(KeyCode::Backspace)); // prefilled text starts selected
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.editors[&id].lines()[0], "keep the salt road");
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn ctrl_c_copies_the_selection_out_of_the_editor() {
    let mut a = app();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    let id = a.editor_doc().unwrap();
    type_str(&mut a, "the salt road");

    // Nothing selected: no copy, just a nudge.
    a.on_key(key_mod(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(a.clipboard.is_none());
    assert!(a.status.contains("Select some text"));

    // Select "salt" and copy it.
    let ta = a.editors.get_mut(&id).unwrap();
    ta.move_cursor(CursorMove::Jump(0, 4));
    ta.start_selection();
    ta.move_cursor(CursorMove::Jump(0, 8));
    a.on_key(key_mod(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(a.clipboard.as_deref(), Some("salt"));
    // The selection is left intact — copy reads, it does not cut.
    assert!(a.editors[&id].selection_range().is_some());
    assert_eq!(a.editors[&id].lines()[0], "the salt road");
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[test]
fn ctrl_g_corrects_a_misspelling_and_learns_a_word() {
    let mut a = app();
    // Open "Opening Scene" and type a misspelling.
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.focus, Focus::Editor));
    type_str(&mut a, "teh");

    // Ctrl-G on the word opens the corrections list; Enter takes the first.
    a.on_key(key_mod(KeyCode::Char('g'), KeyModifiers::CONTROL));
    assert!(matches!(a.modal, Modal::Spell));
    assert_eq!(a.spell_suggestions.first().map(String::as_str), Some("the"));
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Modal::None));
    let id = a.editor_doc().unwrap();
    assert_eq!(a.editors[&id].lines()[0], "the");

    // A made-up name: learn it, and it lands in the project's word list.
    a.on_key(key(KeyCode::Char(' ')));
    type_str(&mut a, "Zybex");
    a.on_key(key_mod(KeyCode::Char('g'), KeyModifiers::CONTROL));
    assert!(matches!(a.modal, Modal::Spell));
    a.on_key(key(KeyCode::Char('a')));
    assert!(a.project.spelling.words.contains(&"Zybex".to_string()));
    assert!(a.spell.is_correct("Zybex"));

    // From the tree, Ctrl-G toggles the whole feature off.
    a.on_key(key(KeyCode::Esc));
    a.on_key(key_mod(KeyCode::Char('g'), KeyModifiers::CONTROL));
    assert!(!a.spell_on);
    assert!(!a.project.spelling.enabled);
    let _ = std::fs::remove_dir_all(&a.project.root);
}


#[cfg(feature = "assistant")]
#[test]
fn assistant_toggles_and_runs_slash_commands() {
    use crate::assistant::context::Scope;
    let mut a = app();
    a.ai_available = true;

    a.on_key(key(KeyCode::F(9)));
    assert!(a.assistant.open && a.assistant.focused);

    type_str(&mut a, "/model gpt-4o");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.assistant.model, "gpt-4o");
    assert_eq!(a.project.assistant.model, "gpt-4o");
    assert!(a.dirty);

    type_str(&mut a, "/context manuscript");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.assistant.scope, Scope::Manuscript);

    type_str(&mut a, "/comments on");
    a.on_key(key(KeyCode::Enter));
    assert!(a.project.assistant.allow_comments);

    // Esc unfocuses but keeps the pane; F9 closes it.
    a.on_key(key(KeyCode::Esc));
    assert!(a.assistant.open && !a.assistant.focused);
    a.on_key(key(KeyCode::F(9)));
    assert!(a.assistant.focused);
    a.on_key(key(KeyCode::F(9)));
    assert!(!a.assistant.open);
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[cfg(feature = "assistant")]
#[test]
fn assistant_applies_a_comment_proposal_to_the_open_document() {
    use crate::assistant::comments::Proposal;
    let mut a = app();
    a.ai_available = true;
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down)); // "Opening Scene"
    a.on_key(key(KeyCode::Enter));
    let id = a.editor_doc().unwrap();
    type_str(&mut a, "The road was long and grey.");

    a.on_key(key(KeyCode::F(9)));
    a.assistant.proposals = vec![Proposal {
        quote: "long and grey".into(),
        note: "cliché".into(),
    }];
    type_str(&mut a, "/apply");
    a.on_key(key(KeyCode::Enter));

    assert_eq!(
        a.editors[&id].lines()[0],
        "The road was {==long and grey==}{>>AI: cliché<<}."
    );
    assert!(a.assistant.proposals.is_empty());
    let _ = std::fs::remove_dir_all(&a.project.root);
}

#[cfg(feature = "assistant")]
#[test]
fn assistant_prompts_for_a_key_when_none_is_set() {
    let _g = crate::assistant::keyring::env_lock();
    // Make sure no env key shadows the flow.
    unsafe {
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("OPENAI_API_KEY");
        std::env::set_var("XDG_CONFIG_HOME", std::env::temp_dir().join("jqln-no-such-config"));
    }
    let mut a = app();
    a.ai_available = true;
    a.on_key(key(KeyCode::F(9)));

    type_str(&mut a, "hello");
    a.on_key(key(KeyCode::Enter));
    // No key -> the paste popup opens and the message is held.
    assert!(matches!(a.modal, Modal::AssistantKey));
    assert_eq!(a.assistant.pending.as_deref(), Some("hello"));

    // Esc cancels without saving.
    a.on_key(key(KeyCode::Esc));
    assert!(matches!(a.modal, Modal::None));
    assert!(a.assistant.pending.is_none());

    unsafe {
        std::env::remove_var("XDG_CONFIG_HOME");
    }
    let _ = std::fs::remove_dir_all(&a.project.root);
}
