//! Rendering tests: draw into an off-screen buffer and assert on the output.

use super::*;
use crate::app::{App, Focus, Modal, View};
use crate::project::Project;
use ratatui::backend::TestBackend;
use ratatui::style::Modifier;
use ratatui::Terminal;

/// Each test gets its own directory; these run in parallel and would
/// otherwise race on creating and deleting the same path.
fn scratch_app(tag: &str) -> App {
    let dir = std::env::temp_dir().join(format!("jqln-ui-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&dir);
    App::new(Project::create(&dir, "The Salt Road").unwrap())
}

fn render(app: &mut App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| draw(f, app)).unwrap();
    let buf = t.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn renders_binder_and_editor() {
    let mut app = scratch_app("binder");
    let out = render(&mut app, 90, 18);
    println!("{out}");
    assert!(out.contains("The Salt Road"), "project name in binder title");
    assert!(out.contains("Manuscript") && out.contains("Opening Scene"));
    assert!(out.contains("total"), "status bar word counts");
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn a_full_trash_shows_a_count_in_the_tree_and_status_bar() {
    use crate::project::{Kind, ROOT};
    let mut app = scratch_app("trash-ui");
    for i in 0..21 {
        let id = app.project.insert(ROOT, None, &format!("junk {i}"), Kind::Text);
        app.project.trash(&id);
    }
    let out = render(&mut app, 90, 24);
    println!("{out}");
    assert!(out.contains("Trash · 21"), "the Trash row shows its count");
    assert!(out.contains("🗑 21"), "and the status bar nudges once it is full");
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn renders_prose_with_soft_wrap() {
    let mut app = scratch_app("wrap");
    // Select "Opening Scene" and type a long paragraph.
    app.sel = 2;
    let id = app.editor_doc().expect("row 2 is a document");
    app.ensure_editor(&id);
    app.editors.get_mut(&id).unwrap().insert_str(
        "The road out of the salt flats was white and it went on for a very long way indeed.",
    );
    app.focus = Focus::Editor;
    let out = render(&mut app, 90, 14);
    println!("{out}");
    // The paragraph is longer than the editor pane, so it must occupy >1 row
    // and must not be truncated at the pane edge.
    assert!(out.contains("The road out of the salt flats"));
    assert!(out.contains("indeed."), "tail of the paragraph must wrap into view");
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn continuous_mode_stitches_documents_into_one_flow() {
    use crate::project::Kind;
    let mut app = scratch_app("flow");
    // Add two more scenes beside the starter one.
    let chapter = {
        let id = app.rows()[1].0.clone();  // "Chapter One"
        id
    };
    let a = app.rows()[2].0.clone();       // "Opening Scene"
    let b = app.project.insert(&chapter, None, "Second Scene", Kind::Text);
    let c = app.project.insert(&chapter, None, "Third Scene", Kind::Text);
    app.project.set_body(&a, "Alpha prose.".into());
    app.project.set_body(&b, "Beta prose.".into());
    app.project.set_body(&c, "Gamma prose.".into());

    app.sel = 2;              // select "Opening Scene"
    app.continuous = true;
    app.focus = Focus::Editor;

    let out = render(&mut app, 90, 20);
    println!("{out}");
    assert!(out.contains("continuous"), "pane should announce the mode");
    // All three documents appear together, each under its own title.
    for expected in ["Opening Scene", "Alpha prose.", "Second Scene", "Beta prose.", "Third Scene", "Gamma prose."] {
        assert!(out.contains(expected), "continuous flow missing {expected:?}");
    }
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn continuous_flow_scrolls_and_keeps_the_edited_document_visible() {
    use crate::project::Kind;
    let mut app = scratch_app("scroll");
    let chapter = app.rows()[1].0.clone();
    // Enough documents to overflow a short pane several times over.
    let mut ids = vec![app.rows()[2].0.clone()];
    for i in 0..12 {
        let id = app.project.insert(&chapter, None, &format!("Scene {i}"), Kind::Text);
        app.project.set_body(&id, format!("Body of scene {i}."));
        ids.push(id);
    }
    app.continuous = true;
    app.focus = Focus::Editor;

    // Land on the last document; the view must scroll down to reveal it.
    let last = ids.last().unwrap().clone();
    app.select_id(&last);
    let out = render(&mut app, 60, 10);
    println!("{out}");
    assert!(app.scroll > 0, "flow should have scrolled to reach the last document");
    assert!(out.contains("Body of scene 11."), "edited document must be on screen");
    assert!(!out.contains("Body of scene 0."), "far-off documents should be scrolled away");
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn card_view_shows_synopses_and_navigates_the_grid() {
    use crate::project::Kind;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    fn k(code: KeyCode) -> KeyEvent {
        KeyEvent { code, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE }
    }

    let mut app = scratch_app("cards");
    let chapter = app.rows()[1].0.clone();
    let first = app.rows()[2].0.clone();
    let second = app.project.insert(&chapter, None, "Second Scene", Kind::Text);
    app.project.nodes.get_mut(&second).unwrap().synopsis = "They reach the coast.".into();

    // From a scene, the card view shows that scene's siblings.
    app.sel = 2;
    app.on_key(k(KeyCode::F(3)));
    assert_eq!(app.card_root, chapter);
    let out = render(&mut app, 90, 16);
    println!("{out}");
    assert!(out.contains("cards"));
    assert!(out.contains("The one where it begins."), "starter synopsis");
    assert!(out.contains("They reach the coast."), "second card synopsis");

    // Right moves along the row of cards.
    app.on_key(k(KeyCode::Right));
    assert_eq!(app.selected_id().as_deref(), Some(second.as_str()));

    // Backspace steps out to the chapter among its peers; Enter descends back.
    app.on_key(k(KeyCode::Backspace));
    assert_eq!(app.card_root, app.project.parent_of(&chapter));
    assert_eq!(app.selected_id().as_deref(), Some(chapter.as_str()));
    app.on_key(k(KeyCode::Enter));
    assert_eq!(app.card_root, chapter);
    assert_eq!(app.selected_id().as_deref(), Some(first.as_str()));
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn card_grid_pages_instead_of_dropping_cards() {
    use crate::project::Kind;
    let mut app = scratch_app("paging");
    let chapter = app.rows()[1].0.clone();
    let mut ids = vec![app.rows()[2].0.clone()];
    for i in 0..11 {
        let id = app.project.insert(&chapter, None, &format!("Scene {i}"), Kind::Text);
        app.project.nodes.get_mut(&id).unwrap().synopsis = format!("Synopsis {i}");
        ids.push(id);
    }
    app.view = View::Corkboard;
    app.sel = 2;
    app.enter_cards();

    // A window this size fits two rows of cards, not six.
    let out = render(&mut app, 92, 20);
    assert!(out.contains("more row(s) below"), "should signal hidden rows");
    assert!(!out.contains("Synopsis 10"), "later cards are off this page");

    // Selecting a late card scrolls it into view rather than dropping it.
    let last = ids.last().unwrap().clone();
    app.select_id(&last);
    let out = render(&mut app, 92, 20);
    println!("{out}");
    assert!(app.card_scroll > 0, "grid should have scrolled");
    assert!(out.contains("Synopsis 10"), "selected card must be drawn");
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn outline_lists_word_counts_and_compile_flags() {
    let mut app = scratch_app("outline");
    let scene = app.rows()[2].0.clone();
    app.ensure_editor(&scene);
    app.editors.get_mut(&scene).unwrap().insert_str("one two three four five");

    app.view = View::Outliner;
    let out = render(&mut app, 80, 12);
    println!("{out}");
    assert!(out.contains("outline"));
    assert!(out.contains("5 w"), "word count must reflect unsaved typing");
    // "Research" is excluded from compile in the starter project.
    assert!(out.contains("Research"));
    assert!(out.contains("·"), "excluded rows marked");
    assert!(out.contains("✓"), "included rows marked");
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn renders_search_results_and_snapshot_list() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    fn k(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent { code, modifiers: m, kind: KeyEventKind::Press, state: KeyEventState::NONE }
    }
    let mut app = scratch_app("find");
    app.sel = 2;
    let id = app.editor_doc().unwrap();
    app.ensure_editor(&id);
    app.editors.get_mut(&id).unwrap().insert_str("the white road\nand the salt");

    app.on_key(k(KeyCode::Char('f'), KeyModifiers::CONTROL));
    for c in "the".chars() {
        app.on_key(k(KeyCode::Char(c), KeyModifiers::NONE));
    }
    app.on_key(k(KeyCode::Enter, KeyModifiers::NONE));
    let out = render(&mut app, 90, 22);
    println!("{out}");
    assert!(out.contains("matches for"));
    assert!(out.contains("the white road"));
    assert!(out.contains("enter to jump"));
    app.on_key(k(KeyCode::Esc, KeyModifiers::NONE));

    app.on_key(k(KeyCode::Char('v'), KeyModifiers::NONE));
    app.on_key(k(KeyCode::Char('t'), KeyModifiers::NONE));
    let out = render(&mut app, 90, 22);
    println!("{out}");
    assert!(out.contains("Snapshots — Opening Scene"));
    assert!(out.contains("d delete"), "delete hint should be offered");
    // The snapshot is listed as a readable date, not a raw stamp.
    assert!(out.contains("-") && out.contains(":"));
    let _ = std::fs::remove_dir_all(&app.project.root);
}

fn click(app: &mut App, x: u16, y: u16) {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    });
}

fn wheel(app: &mut App, x: u16, y: u16, down: bool) {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    app.on_mouse(MouseEvent {
        kind: if down { MouseEventKind::ScrollDown } else { MouseEventKind::ScrollUp },
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    });
}

fn mouse(app: &mut App, kind: crossterm::event::MouseEventKind, x: u16, y: u16) {
    use crossterm::event::{KeyModifiers, MouseEvent};
    app.on_mouse(MouseEvent { kind, column: x, row: y, modifiers: KeyModifiers::NONE });
}

#[test]
fn dragging_selects_text_within_the_editor_only() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut app = scratch_app("drag");
    app.sel = 2;
    let id = app.editor_doc().unwrap();
    app.ensure_editor(&id);
    app.editors
        .get_mut(&id)
        .unwrap()
        .insert_str("first line\nsecond line\nthird line");
    render(&mut app, 90, 16);

    let pane = app.pane_editor;
    let sel_before = app.sel;

    // Press on line 0, drag down onto line 2 — and keep going left, off the
    // pane and over where the binder sits.
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), pane.x + 2, pane.y);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), pane.x + 5, pane.y + 1);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 0, pane.y + 2);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 0, pane.y + 2);

    let ((sr, _), (er, _)) = app.editors[&id]
        .selection_range()
        .expect("a multi-line drag should leave a selection");
    assert_eq!((sr, er), (0, 2), "selection should span the dragged rows");
    assert_eq!(app.sel, sel_before, "the binder selection must not move");
    assert!(matches!(app.focus, Focus::Editor));

    // A plain click (down then up, no movement) clears the selection again.
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), pane.x + 1, pane.y + 1);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), pane.x + 1, pane.y + 1);
    assert!(app.editors[&id].selection_range().is_none());
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn clicking_the_tree_selects_that_row() {
    let mut app = scratch_app("mouse-tree");
    render(&mut app, 90, 16);   // establishes pane geometry
    assert_eq!(app.sel, 0);

    // Row 2 of the tree is "Opening Scene"; the pane starts below its border.
    let (bx, by) = (app.pane_binder.x + 4, app.pane_binder.y + 2);
    click(&mut app, bx, by);
    assert_eq!(app.sel, 2);
    let id = app.selected_id().unwrap();
    assert_eq!(app.project.nodes[&id].title, "Opening Scene");
    assert!(matches!(app.focus, Focus::Binder));
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn clicking_the_page_places_the_cursor() {
    let mut app = scratch_app("mouse-text");
    app.sel = 2;
    let id = app.editor_doc().unwrap();
    app.ensure_editor(&id);
    app.editors.get_mut(&id).unwrap().insert_str("first line\nsecond line\nthird line");
    render(&mut app, 90, 16);

    // Click into the middle of the second line of prose.
    let x = app.pane_editor.x + 3;
    let y = app.pane_editor.y + 1;
    click(&mut app, x, y);
    assert!(matches!(app.focus, Focus::Editor));
    assert_eq!(app.editors[&id].cursor(), (1, 3), "cursor should land where clicked");
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn clicking_a_card_selects_it() {
    use crate::project::Kind;
    let mut app = scratch_app("mouse-card");
    let chapter = app.rows()[1].0.clone();
    let second = app.project.insert(&chapter, None, "Second Scene", Kind::Text);
    app.view = View::Corkboard;
    app.sel = 2;
    app.enter_cards();
    render(&mut app, 92, 16);

    let (_, rect) = app
        .card_hits
        .iter()
        .find(|(id, _)| *id == second)
        .cloned()
        .expect("second card should have been drawn");
    click(&mut app, rect.x + 2, rect.y + 1);
    assert_eq!(app.selected_id().as_deref(), Some(second.as_str()));
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn dragging_a_card_reorders_the_corkboard() {
    use crate::project::Kind;
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut app = scratch_app("card-drag");
    let chapter = app.rows()[1].0.clone();
    let first = app.rows()[2].0.clone(); // "Opening Scene"
    let mid = app.project.insert(&chapter, None, "Middle", Kind::Text);
    let last = app.project.insert(&chapter, None, "Last", Kind::Text);
    app.view = View::Corkboard;
    app.sel = 2;
    app.enter_cards();
    render(&mut app, 100, 20);

    let rect = |app: &App, id: &str| {
        app.card_hits.iter().find(|(c, _)| c == id).map(|(_, r)| *r).unwrap()
    };
    let a = rect(&app, &first);
    let c = rect(&app, &last);

    // Grab the first card, drag it onto the last, release.
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), a.x + 2, a.y + 1);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), c.x + 2, c.y + 1);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), c.x + 2, c.y + 1);

    assert_eq!(
        app.project.children[&chapter],
        [mid.clone(), last.clone(), first.clone()],
        "the grabbed card moved to the drop position"
    );
    assert_eq!(app.selected_id().as_deref(), Some(first.as_str()));
    assert!(app.dirty);
    assert!(app.drag_card.is_none() && app.drag_over.is_none());
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn a_card_reorders_even_without_drag_events() {
    // Some terminals send only Down then Up, no Drag in between.
    use crate::project::Kind;
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut app = scratch_app("card-drag-nodrag");
    let chapter = app.rows()[1].0.clone();
    let first = app.rows()[2].0.clone();
    let mid = app.project.insert(&chapter, None, "Middle", Kind::Text);
    let last = app.project.insert(&chapter, None, "Last", Kind::Text);
    app.view = View::Corkboard;
    app.sel = 2;
    app.enter_cards();
    render(&mut app, 100, 20);
    let rc = |app: &App, id: &str| {
        app.card_hits.iter().find(|(c, _)| c == id).map(|(_, r)| *r).unwrap()
    };
    let a = rc(&app, &first);
    let c = rc(&app, &last);

    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), a.x + 2, a.y + 1);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), c.x + 2, c.y + 1);

    assert_eq!(app.project.children[&chapter], [mid, last, first]);
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn the_wheel_scrolls_the_pane_under_the_pointer() {
    use crate::project::Kind;
    let mut app = scratch_app("mouse-wheel");
    let chapter = app.rows()[1].0.clone();
    for i in 0..10 {
        let id = app.project.insert(&chapter, None, &format!("Scene {i}"), Kind::Text);
        app.project.set_body(&id, format!("Body {i}."));
    }
    app.continuous = true;
    app.sel = 2;
    render(&mut app, 90, 14);

    // Wheeling over the tree moves the selection, not the flow.
    let before_scroll = app.scroll;
    let (bx, by) = (app.pane_binder.x + 2, app.pane_binder.y + 1);
    wheel(&mut app, bx, by, true);
    assert!(app.sel > 2, "wheel over the tree should move the selection");
    assert_eq!(app.scroll, before_scroll, "the flow should not have moved");

    // Wheeling over the flow scrolls it instead.
    app.sel = 2;
    render(&mut app, 90, 14);
    let before_sel = app.sel;
    let (fx, fy) = (app.flow_inner.x + 5, app.flow_inner.y + 2);
    wheel(&mut app, fx, fy, true);
    assert_eq!(app.sel, before_sel, "the selection should not have moved");
    assert!(app.scroll > 0, "wheel over the flow should scroll it");
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn clicking_a_document_in_the_flow_focuses_it() {
    use crate::project::Kind;
    let mut app = scratch_app("mouse-flow");
    let chapter = app.rows()[1].0.clone();
    let second = app.project.insert(&chapter, None, "Second Scene", Kind::Text);
    app.project.set_body(&second, "Beta prose here.".into());
    app.continuous = true;
    app.sel = 2;
    render(&mut app, 90, 18);

    let (_, rect) = app
        .flow_hits
        .iter()
        .find(|(id, _)| *id == second)
        .cloned()
        .expect("second document should be visible in the flow");
    click(&mut app, rect.x + 5, rect.y);
    assert_eq!(app.selected_id().as_deref(), Some(second.as_str()));
    assert!(matches!(app.focus, Focus::Editor));
    // Offscreen-to-screen translation put the cursor on the clicked column.
    assert_eq!(app.editors[&second].cursor(), (0, 5));
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn the_lighthouse_appears_on_the_empty_pane_when_there_is_room() {
    let mut app = scratch_app("lighthouse");
    // Tall terminal: the art fits.
    let out = render(&mut app, 90, 30);
    println!("{out}");
    assert!(out.contains("O"), "lamp");
    assert!(out.contains("~~~~~~~~~~~~~"), "water line");
    assert!(out.contains("Manuscript"));

    // Short terminal: the art is dropped rather than clipped.
    let out = render(&mut app, 90, 12);
    assert!(!out.contains("~~~~~~~~~~~~~"), "art must not be drawn when cramped");
    assert!(out.contains("Manuscript"), "the useful text still shows");
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn the_editor_styles_markup_in_place() {
    let mut app = scratch_app("markup");
    app.sel = 2;
    let id = app.editor_doc().unwrap();
    app.ensure_editor(&id);
    app.editors.get_mut(&id).unwrap().insert_str("say **now** ok");
    app.focus = Focus::Editor;

    let mut t = Terminal::new(TestBackend::new(90, 12)).unwrap();
    t.draw(|f| draw(f, &mut app)).unwrap();
    let buf = t.backend().buffer().clone();

    let area = app.pane_editor;
    let mut bold_runs: Vec<String> = Vec::new();
    let mut dim_star = false;
    for y in area.y..area.y + area.height {
        let mut run = String::new();
        for x in area.x..area.x + area.width {
            let cell = &buf[(x, y)];
            if cell.modifier.contains(Modifier::BOLD) {
                run.push_str(cell.symbol());
            } else if !run.is_empty() {
                bold_runs.push(std::mem::take(&mut run));
            }
            if cell.symbol() == "*" && cell.modifier.contains(Modifier::DIM) {
                dim_star = true;
            }
        }
        if !run.is_empty() {
            bold_runs.push(std::mem::take(&mut run));
        }
    }
    assert!(bold_runs.iter().any(|r| r == "now"), "the word between ** ** must render bold, got {bold_runs:?}");
    assert!(dim_star, "the ** markers must be dimmed rather than shown at full weight");
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn renders_help_overlay() {
    let mut app = scratch_app("help");
    app.modal = Modal::Help;

    // Wide-but-short terminal: two columns, and every row must fit — including
    // the last entry of each column, which used to be clipped by the border.
    let out = render(&mut app, 90, 24);
    println!("{out}");
    assert!(out.contains("Jqln"));
    assert!(out.contains("reorder up / down"), "left column not fully drawn");
    assert!(out.contains("press any key to close"), "right column not fully drawn");
    // Descriptions are not truncated at the box edge.
    assert!(out.contains("status / label / keywords"));

    // Tall terminal: one column, still complete.
    let out = render(&mut app, 60, 50);
    assert!(out.contains("bold selection / word"));
    assert!(out.contains("press any key to close"));
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn renders_book_settings() {
    let mut app = scratch_app("booksettings");
    app.project.book.author = "A. Writer".into();
    app.modal = Modal::BookSettings;
    let out = render(&mut app, 90, 26);
    println!("{out}");
    assert!(out.contains("Book settings"));
    assert!(out.contains("Title") && out.contains("(project name)"));
    assert!(out.contains("A. Writer"));
    assert!(out.contains("Trim size") && out.contains("5.5x8.5"));
    assert!(out.contains("Running heads") && out.contains("yes"));
    assert!(out.contains("enter edit"));
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn notes_show_in_the_tree_and_above_the_prose() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    fn k(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent { code, modifiers: m, kind: KeyEventKind::Press, state: KeyEventState::NONE }
    }
    let none = KeyModifiers::NONE;

    let mut app = scratch_app("notes");
    app.sel = 2; // "Opening Scene"

    // Write a note the way a user would: N, type, Ctrl-S.
    app.on_key(k(KeyCode::Char('N'), none));
    assert!(matches!(app.modal, Modal::Notes));
    let out = render(&mut app, 100, 20);
    assert!(out.contains("Notes — Opening Scene"));
    assert!(out.contains("ctrl-s to save"));
    for c in "watch the timeline here".chars() {
        app.on_key(k(KeyCode::Char(c), none));
    }
    app.on_key(k(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(matches!(app.modal, Modal::None));

    app.focus = Focus::Editor;
    let out = render(&mut app, 100, 20);
    println!("{out}");
    assert!(out.contains("✎"), "the tree marks a noted node");
    assert!(out.contains("✎ notes"), "the editor labels the note strip");
    assert!(out.contains("watch the timeline here"));
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn renders_spell_corrections_and_underlines_a_misspelling() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use ratatui::style::Color;
    fn k(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent { code, modifiers: m, kind: KeyEventKind::Press, state: KeyEventState::NONE }
    }
    let none = KeyModifiers::NONE;

    let mut app = scratch_app("spell");
    // Open "Opening Scene" and type a misspelling.
    app.on_key(k(KeyCode::Down, none));
    app.on_key(k(KeyCode::Down, none));
    app.on_key(k(KeyCode::Enter, none));
    for c in "teh".chars() {
        app.on_key(k(KeyCode::Char(c), none));
    }

    // The word carries a red underline in the editor.
    let mut term = Terminal::new(TestBackend::new(90, 24)).unwrap();
    term.draw(|f| draw(f, &mut app)).unwrap();
    let buf = term.backend().buffer().clone();
    let underlined = (0..buf.area.height).any(|y| {
        (0..buf.area.width).any(|x| {
            let c = &buf[(x, y)];
            c.modifier.contains(Modifier::UNDERLINED) && c.fg == Color::Red
        })
    });
    assert!(underlined, "the misspelled word should be red-underlined");

    // Ctrl-G opens the corrections list.
    app.on_key(k(KeyCode::Char('g'), KeyModifiers::CONTROL));
    let out = render(&mut app, 90, 24);
    println!("{out}");
    assert!(out.contains("teh"));
    assert!(out.contains("the"));
    assert!(out.contains("add to dictionary"));
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn a_dragged_card_marks_its_drop_target() {
    use crate::project::Kind;
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut app = scratch_app("card-drop-mark");
    let chapter = app.rows()[1].0.clone();
    let first = app.rows()[2].0.clone();
    let last = app.project.insert(&chapter, None, "Last", Kind::Text);
    app.view = View::Corkboard;
    app.sel = 2;
    app.enter_cards();
    render(&mut app, 100, 20);
    let c = app.card_hits.iter().find(|(id, _)| *id == last).map(|(_, r)| *r).unwrap();
    let a = app.card_hits.iter().find(|(id, _)| *id == first).map(|(_, r)| *r).unwrap();

    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), a.x + 2, a.y + 1);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), c.x + 2, c.y + 1);
    assert_eq!(app.drag_over.as_deref(), Some(last.as_str()));

    let mut t = Terminal::new(TestBackend::new(100, 20)).unwrap();
    t.draw(|f| draw(f, &mut app)).unwrap();
    let buf = t.backend().buffer().clone();
    // The drop-target card's top-left corner is drawn in yellow.
    let corner = &buf[(c.x, c.y)];
    assert_eq!(corner.fg, ratatui::style::Color::Yellow, "drop target should be highlighted");
    let _ = std::fs::remove_dir_all(&app.project.root);
}

#[test]
fn card_view_hints_at_f7_when_mouse_is_off() {
    let mut app = scratch_app("card-f7-hint");
    app.view = View::Corkboard;
    app.sel = 2;
    app.enter_cards();
    app.mouse = true;
    assert!(!render(&mut app, 90, 16).contains("F7 to drag"));
    app.mouse = false;
    assert!(render(&mut app, 90, 16).contains("F7 to drag"), "off-state should prompt F7");
    let _ = std::fs::remove_dir_all(&app.project.root);
}


#[cfg(feature = "assistant")]
#[test]
fn assistant_pane_shows_the_header_and_transcript() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    fn k(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }
    let mut app = scratch_app("assistant");
    app.ai_available = true;
    app.on_key(k(KeyCode::F(9)));

    let out = render(&mut app, 130, 30);
    println!("{out}");
    assert!(out.contains("assistant"));
    assert!(out.contains("claude-sonnet-5"), "header names the model");
    assert!(out.contains("/help"), "prompt hint is shown");
    assert!(out.contains("enter sends") || out.contains("Type a message"));
    let _ = std::fs::remove_dir_all(&app.project.root);
}
