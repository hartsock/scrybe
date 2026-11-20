// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Unit tests for the viewer app (moved out of `app.rs`).

use super::*;
use ratatui::backend::TestBackend;

// -----------------------------------------------------------------------
// Click-to-open (#244) — resolve_link_at against a real drawn Buffer.
// `on_mouse` itself isn't tested here: it shells out via `open::that`,
// which must never run in a test (no real subprocess / external
// service). `resolve_link_at` is the pure part; that's what's covered.
// -----------------------------------------------------------------------

fn drawn(app: &mut App, width: u16, height: u16) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let completed = terminal.draw(|f| app.draw(f)).unwrap();
    app.last_buffer = Some(completed.buffer.clone());
}

#[test]
fn click_on_rendered_link_resolves_its_href() {
    let mut app = App::from_source("[docs](https://example.com)\n", "t.md");
    drawn(&mut app, 40, 10);

    let area = app.pane_areas[0];
    let row = area.y;
    let buf = app.last_buffer.as_ref().unwrap();
    let link_col = (area.x..area.x + area.width)
        .find(|&x| {
            let cell = &buf[(x, row)];
            cell.fg == Color::Blue && cell.modifier.contains(Modifier::UNDERLINED)
        })
        .expect("link cell rendered somewhere on the content row");

    assert_eq!(
        app.resolve_link_at(link_col, row),
        Some("https://example.com".to_string())
    );
}

#[test]
fn click_on_plain_text_resolves_to_none() {
    let mut app = App::from_source("plain text, no links here\n", "t.md");
    drawn(&mut app, 40, 10);
    let area = app.pane_areas[0];
    assert_eq!(app.resolve_link_at(area.x, area.y), None);
}

#[test]
fn click_outside_any_pane_resolves_to_none() {
    let mut app = App::from_source("[docs](https://example.com)\n", "t.md");
    drawn(&mut app, 40, 10);
    // Row 9 of a 10-row backend is the footer hint line — outside every
    // pane's content area.
    assert_eq!(app.resolve_link_at(0, 9), None);
}

#[test]
fn two_links_with_identical_text_both_resolve_to_the_first_in_document_order() {
    // Documented, accepted limitation: resolution matches by visible
    // text, not by tracked position, so two links with byte-for-byte
    // identical text can't be told apart by which one was physically
    // clicked — both resolve to whichever comes first in the pane's
    // link list. Real-world duplicate-visible-text links on one screen
    // row are rare; this test exists so that behavior stays intentional
    // rather than silently changing.
    let mut app = App::from_source(
        "[dup](https://first.example) and [dup](https://second.example)\n",
        "t.md",
    );
    drawn(&mut app, 80, 10);
    let area = app.pane_areas[0];
    let row = area.y;
    let buf = app.last_buffer.as_ref().unwrap();
    let link_cols: Vec<u16> = (area.x..area.x + area.width)
        .filter(|&x| {
            let cell = &buf[(x, row)];
            cell.fg == Color::Blue && cell.modifier.contains(Modifier::UNDERLINED)
        })
        .collect();
    // Two separate "dup" runs, so there's a style gap between them.
    let first_run_start = link_cols[0];
    let second_run_start = *link_cols
        .iter()
        .find(|&&x| x > first_run_start + 3)
        .expect("second link run present");

    assert_eq!(
        app.resolve_link_at(first_run_start, row),
        Some("https://first.example".to_string())
    );
    assert_eq!(
        app.resolve_link_at(second_run_start, row),
        Some("https://first.example".to_string()),
        "known limitation: text-matching can't disambiguate identical link text"
    );
}

#[test]
fn j_scrolls_down_q_quits() {
    let mut app = App::from_source("# A\n\nbody\nmore\nlines\nhere\n", "t.md");
    app.panes[0].state.set_line_count(20);
    let before = app.panes[0].state.scroll();
    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert!(app.panes[0].state.scroll() >= before);
    app.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(app.quit);
}

#[test]
fn tab_cycles_focus_and_o_toggles_orientation() {
    let mut app = App::from_documents([("# A".to_string(), "a.md"), ("# B".to_string(), "b.md")]);
    assert_eq!(app.focus, 0);
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.focus, 1);
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.focus, 0); // wraps

    let before = app.orientation;
    app.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    assert_ne!(app.orientation, before);
}

#[test]
fn scroll_keys_target_the_focused_pane() {
    let mut app =
        App::from_documents([("line\n".repeat(50), "a.md"), ("line\n".repeat(50), "b.md")]);
    app.panes[0].state.set_line_count(50);
    app.panes[1].state.set_line_count(50);
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)); // focus pane 1
    app.on_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
    assert_eq!(
        app.panes[0].state.scroll(),
        0,
        "unfocused pane must not move"
    );
    assert!(app.panes[1].state.scroll() > 0, "focused pane scrolled");
}

#[test]
fn set_source_re_renders_and_reclamps() {
    let mut pane = Pane::from_source("# One\n", "t.md");
    let before = pane.text.lines.len();
    pane.set_source("# One\n\n# Two\n\nmore body here\n");
    assert!(pane.text.lines.len() > before);
}

#[test]
fn reload_picks_up_file_changes() {
    let path = std::env::temp_dir().join(format!("scrybe-tui-reload-{}.md", std::process::id()));
    fs::write(&path, "# One\n").unwrap();
    let mut pane = Pane::from_file(path.clone()).unwrap();
    let before = pane.text.lines.len();

    fs::write(&path, "# One\n\n# Two\n\nmore\n").unwrap();
    // Force detection regardless of filesystem mtime resolution.
    pane.mtime = None;
    assert!(pane.reload_if_changed(), "change should be detected");
    assert!(pane.text.lines.len() > before, "content should have grown");

    let _ = fs::remove_file(&path);
}

// -----------------------------------------------------------------------
// CSV / TSV grid panes (#273)
// -----------------------------------------------------------------------

fn key(app: &mut App, c: KeyCode) {
    app.on_key(KeyEvent::new(c, KeyModifiers::NONE));
}

fn screen(app: &mut App, width: u16, height: u16) -> String {
    drawn(app, width, height);
    let buf = app.last_buffer.as_ref().unwrap();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn temp(name: &str, body: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("scrybe-tui-{}-{name}", std::process::id()));
    fs::write(&path, body).unwrap();
    path
}

#[test]
fn extension_picks_grid_or_markdown_case_insensitively() {
    for (name, grid) in [("g1.csv", true), ("g2.TSV", true), ("g3.md", false)] {
        let path = temp(name, "a,b\n1,2\n");
        let pane = Pane::from_file(path.clone()).unwrap();
        assert_eq!(pane.grid.is_some(), grid, "{name}");
        let _ = fs::remove_file(&path);
    }
}

#[test]
fn csv_pane_draws_as_a_grid() {
    let path = temp("draw.csv", "name,qty\napple,3\n");
    let mut app = App::from_files(vec![path.clone()]).unwrap();
    let out = screen(&mut app, 40, 8);
    assert!(out.contains("name") && out.contains("apple"), "{out}");
    assert!(out.contains("h/l"), "{out}");
    let _ = fs::remove_file(&path);
}

#[test]
fn h_l_move_the_cursor_across_columns_on_grid_and_noop_on_markdown() {
    let path = temp("cols.csv", "alpha,bravo,charlie\n1,2,3\n");
    let mut app = App::from_files(vec![path.clone()]).unwrap();
    // Narrow enough that columns do not all fit.
    screen(&mut app, 16, 6);
    key(&mut app, KeyCode::Char('l'));
    key(&mut app, KeyCode::Char('l')); // the cursor reaches the third column
    let out = screen(&mut app, 16, 6);
    assert!(!out.contains("alpha"), "leading column scrolled off: {out}");
    key(&mut app, KeyCode::Char('h'));
    key(&mut app, KeyCode::Char('h'));
    let out = screen(&mut app, 16, 6);
    assert!(out.contains("alpha"), "{out}");
    let _ = fs::remove_file(&path);

    let mut md = App::from_source("# T\n\nbody\n", "t.md");
    let before = md.panes[0].state.scroll();
    key(&mut md, KeyCode::Char('l'));
    key(&mut md, KeyCode::Char('h'));
    assert_eq!(md.panes[0].state.scroll(), before);
    assert!(md.panes[0].grid.is_none());
}

#[test]
fn csv_reload_reparses_and_keeps_scroll() {
    let rows: String = (0..60).map(|i| format!("r{i},x\n")).collect();
    let path = temp("reload.csv", &rows);
    let mut app = App::from_files(vec![path.clone()]).unwrap();
    screen(&mut app, 30, 10);
    for _ in 0..12 {
        key(&mut app, KeyCode::Char('j')); // the cursor walks off the first page
    }
    screen(&mut app, 30, 10); // the viewport follows the cursor at render
    let scrolled = app.panes[0].percent();
    assert!(scrolled > 0);

    fs::write(&path, format!("{rows}extra,row\n")).unwrap();
    app.panes[0].mtime = None;
    assert!(app.panes[0].reload_if_changed());
    let out = screen(&mut app, 30, 10);
    assert!(app.panes[0].percent() > 0, "scroll kept: {out}");
    key(&mut app, KeyCode::Char('G'));
    let out = screen(&mut app, 30, 10);
    assert!(out.contains("extra"), "re-parsed: {out}");
    let _ = fs::remove_file(&path);
}

#[test]
fn split_of_markdown_and_csv_draws_both() {
    let md = temp("split.md", "# Heading\n\nbody\n");
    let csv = temp("split.csv", "name,qty\napple,3\n");
    let mut app = App::from_files(vec![md.clone(), csv.clone()]).unwrap();
    let out = screen(&mut app, 80, 8);
    assert!(out.contains("Heading") && out.contains("apple"), "{out}");
    let _ = fs::remove_file(&md);
    let _ = fs::remove_file(&csv);
}

// -----------------------------------------------------------------------
// CSV / TSV cell editing (#274)
// -----------------------------------------------------------------------

/// CRLF + quoted cell + BOM + a ragged last row.
const MESSY: &str = "\u{feff}name,note\r\n\"a,1\",x\r\nb\r\n";

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    app.on_key(KeyEvent::new(code, mods));
}

fn ctrl(app: &mut App, c: char) {
    press(app, KeyCode::Char(c), KeyModifiers::CONTROL);
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        key(app, KeyCode::Char(c));
    }
}

fn source_of(app: &App) -> String {
    app.panes[0]
        .grid
        .as_ref()
        .unwrap()
        .table
        .source()
        .to_owned()
}

fn grid_app(name: &str, body: &str) -> (App, PathBuf) {
    let path = temp(name, body);
    (App::from_files(vec![path.clone()]).unwrap(), path)
}

fn on_disk(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap()
}

#[test]
fn edit_changes_only_that_cells_bytes() {
    let (mut app, path) = grid_app("edit.csv", MESSY);
    key(&mut app, KeyCode::Char('j')); // row 1, col 0: the quoted "a,1"
    key(&mut app, KeyCode::Enter);
    for _ in 0..3 {
        key(&mut app, KeyCode::Backspace);
    }
    type_str(&mut app, "Z");
    key(&mut app, KeyCode::Enter);
    assert_eq!(source_of(&app), "\u{feff}name,note\r\n\"Z\",x\r\nb\r\n");
    assert!(app.panes[0].dirty());
    assert!(screen(&mut app, 60, 8).contains("[+]"));
    let _ = fs::remove_file(&path);
}

/// The dirty marker must stay visible when the path is wider than the pane
/// (a long Windows temp path hid a trailing marker, so unsaved edits looked saved).
#[test]
fn dirty_marker_survives_a_path_wider_than_the_pane() {
    let name = format!("{}.csv", "long-name-".repeat(8));
    let (mut app, path) = grid_app(&name, "a,b\n1,2\n");
    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Char('i'));
    type_str(&mut app, "9");
    key(&mut app, KeyCode::Enter);
    assert!(app.panes[0].dirty());
    assert!(
        screen(&mut app, 30, 8).contains("[+]"),
        "dirty marker hidden by a long path"
    );
    let _ = fs::remove_file(&path);
}

#[test]
fn esc_commits_and_ctrl_c_cancels() {
    let (mut app, path) = grid_app("esc.csv", "a,b\n1,2\n");
    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Char('i'));
    type_str(&mut app, "9");
    key(&mut app, KeyCode::Esc);
    assert!(!app.quit, "Esc in edit mode commits, it does not quit");
    assert_eq!(source_of(&app), "a,b\n19,2\n");

    key(&mut app, KeyCode::Enter);
    type_str(&mut app, "777");
    ctrl(&mut app, 'c');
    assert!(!app.quit);
    assert_eq!(source_of(&app), "a,b\n19,2\n", "cancel discards the buffer");
    assert!(app.panes[0].grid.as_ref().unwrap().edit.is_none());
    let _ = fs::remove_file(&path);
}

#[test]
fn unchanged_commit_stays_clean() {
    let (mut app, path) = grid_app("noop.csv", "a,b\n1,2\n");
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Enter);
    assert!(!app.panes[0].dirty());
    assert!(!app.panes[0].grid.as_ref().unwrap().history.can_undo());
    let _ = fs::remove_file(&path);
}

#[test]
fn edit_mode_swallows_global_keys() {
    let (mut app, path) = grid_app("swallow.csv", "a\n1\n");
    let before = app.orientation;
    key(&mut app, KeyCode::Enter);
    for c in ['q', 'o', 'j', 'k', 'g', 'G', ' ', 'd', 'u'] {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Tab);
    ctrl(&mut app, 'd');
    ctrl(&mut app, 'o');
    assert!(!app.quit);
    assert_eq!(app.orientation, before);
    assert_eq!(app.focus, 0);
    let g = app.panes[0].grid.as_ref().unwrap();
    assert_eq!(g.cursor(), (0, 0), "the cursor did not move");
    assert_eq!(g.edit.as_ref().unwrap().buf, "aqojkgG du");
    let _ = fs::remove_file(&path);
}

#[test]
fn caret_keys_edit_in_the_middle_and_the_caret_is_drawn() {
    let (mut app, path) = grid_app("caret.csv", "abc\n1\n");
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Left);
    key(&mut app, KeyCode::Left);
    key(&mut app, KeyCode::Char('X'));
    let out = screen(&mut app, 30, 6);
    assert!(out.contains(&format!("aX{CARET}bc")), "{out}");
    key(&mut app, KeyCode::Home);
    key(&mut app, KeyCode::Delete);
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Backspace);
    key(&mut app, KeyCode::Enter);
    assert_eq!(source_of(&app), "Xb\n1\n");
    let _ = fs::remove_file(&path);
}

#[test]
fn alt_enter_inserts_a_newline() {
    let (mut app, path) = grid_app("nl.csv", "a\n1\n");
    key(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter, KeyModifiers::ALT);
    type_str(&mut app, "z");
    key(&mut app, KeyCode::Enter);
    assert_eq!(source_of(&app), "\"a\nz\"\n1\n");
    let _ = fs::remove_file(&path);
}

#[test]
fn o_and_capital_o_insert_rows_and_move_onto_them() {
    let (mut app, path) = grid_app("rows.csv", "h\n1\n2\n");
    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Char('o'));
    assert_eq!(source_of(&app), "h\n1\n\"\"\n2\n");
    assert_eq!(app.panes[0].grid.as_ref().unwrap().cursor(), (2, 0));
    key(&mut app, KeyCode::Char('O'));
    assert_eq!(source_of(&app), "h\n1\n\"\"\n\"\"\n2\n");
    assert_eq!(app.panes[0].grid.as_ref().unwrap().cursor(), (2, 0));
    let _ = fs::remove_file(&path);
}

#[test]
fn dd_deletes_the_cursor_row_and_any_other_key_cancels_it() {
    let (mut app, path) = grid_app("dd.csv", "h\n1\n2\n");
    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Char('d'));
    assert_eq!(source_of(&app), "h\n1\n2\n", "one d deletes nothing");
    key(&mut app, KeyCode::Char('d'));
    assert_eq!(source_of(&app), "h\n2\n");

    key(&mut app, KeyCode::Char('d'));
    key(&mut app, KeyCode::Char('k')); // another key in between cancels
    key(&mut app, KeyCode::Char('d'));
    assert_eq!(source_of(&app), "h\n2\n");
    let _ = fs::remove_file(&path);
}

#[test]
fn undo_redo_restore_exact_bytes_and_dirty_follows_the_saved_text() {
    let (mut app, path) = grid_app("undo.csv", MESSY);
    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Enter);
    type_str(&mut app, "!");
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Char('o'));
    key(&mut app, KeyCode::Char('d'));
    key(&mut app, KeyCode::Char('d'));
    let edited = source_of(&app);
    assert_ne!(edited, MESSY);
    assert!(app.panes[0].dirty());

    key(&mut app, KeyCode::Char('u'));
    key(&mut app, KeyCode::Char('u'));
    key(&mut app, KeyCode::Char('u'));
    assert_eq!(source_of(&app), MESSY);
    assert!(
        !app.panes[0].dirty(),
        "undone back to the saved text is clean"
    );
    key(&mut app, KeyCode::Char('u'));
    assert_eq!(app.status.as_deref(), Some("nothing to undo"));

    ctrl(&mut app, 'r');
    assert!(app.panes[0].dirty());
    ctrl(&mut app, 'r');
    ctrl(&mut app, 'r');
    assert_eq!(source_of(&app), edited);
    ctrl(&mut app, 'r');
    assert_eq!(app.status.as_deref(), Some("nothing to redo"));
    let _ = fs::remove_file(&path);
}

#[test]
fn empty_table_has_no_cells_to_edit_but_o_adds_the_first_row() {
    let (mut app, path) = grid_app("empty.csv", "");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.status.as_deref(), Some("no cells"));
    key(&mut app, KeyCode::Char('o'));
    assert_eq!(source_of(&app), "\"\"\n");
    let _ = fs::remove_file(&path);
}

#[test]
fn o_still_toggles_on_markdown_and_ctrl_o_toggles_on_both() {
    let mut md = App::from_source("# T\n", "t.md");
    let before = md.orientation;
    key(&mut md, KeyCode::Char('o'));
    assert_ne!(md.orientation, before);
    ctrl(&mut md, 'o');
    assert_eq!(md.orientation, before);

    let (mut app, path) = grid_app("ctrlo.csv", "a\n1\n");
    let before = app.orientation;
    ctrl(&mut app, 'o');
    assert_ne!(app.orientation, before);
    assert_eq!(source_of(&app), "a\n1\n", "Ctrl-o is not insert-row");
    let _ = fs::remove_file(&path);
}

#[test]
fn page_keys_move_the_cursor() {
    let rows: String = (0..60).map(|i| format!("r{i}\n")).collect();
    let (mut app, path) = grid_app("page.csv", &rows);
    screen(&mut app, 30, 12);
    key(&mut app, KeyCode::PageDown);
    let (r, _) = app.panes[0].grid.as_ref().unwrap().cursor();
    assert!(r > 3, "page moved the cursor: {r}");
    key(&mut app, KeyCode::Char('G'));
    assert_eq!(app.panes[0].grid.as_ref().unwrap().cursor().0, 59);
    key(&mut app, KeyCode::Char('g'));
    assert_eq!(app.panes[0].grid.as_ref().unwrap().cursor().0, 0);
    let _ = fs::remove_file(&path);
}

// ---- save -------------------------------------------------------------

fn edit_first_cell(app: &mut App, text: &str) {
    key(app, KeyCode::Enter);
    key(app, KeyCode::End);
    type_str(app, text);
    key(app, KeyCode::Enter);
}

#[test]
fn ctrl_s_writes_the_exact_bytes_and_causes_no_false_conflict() {
    let (mut app, path) = grid_app("save.csv", MESSY);
    edit_first_cell(&mut app, "Q");
    ctrl(&mut app, 's');
    let expected = "\u{feff}nameQ,note\r\n\"a,1\",x\r\nb\r\n";
    assert_eq!(
        on_disk(&path),
        expected.as_bytes(),
        "only the edited cell changed"
    );
    assert!(app.status.as_deref().unwrap().starts_with("saved "));
    assert!(!app.panes[0].dirty());
    assert!(
        app.panes[0].grid.as_ref().unwrap().history.can_undo(),
        "undo survives a save"
    );

    app.reload_changed();
    assert!(app.panes[0].grid.as_ref().unwrap().conflict.is_none());
    assert!(app.panes[0].notice.is_none());
    key(&mut app, KeyCode::Char('u'));
    assert!(app.panes[0].dirty(), "undo after save re-dirties");
    let _ = fs::remove_file(&path);
}

#[test]
fn save_leaves_no_temp_file_behind() {
    let (mut app, path) = grid_app("tmpfile.csv", "a\n1\n");
    edit_first_cell(&mut app, "x");
    ctrl(&mut app, 's');
    let dir = path.parent().unwrap();
    let stray = fs::read_dir(dir).unwrap().filter_map(Result::ok).any(|e| {
        e.file_name().to_string_lossy().starts_with(".scrybe-tui-")
            && e.file_name().to_string_lossy().ends_with(".tmp")
    });
    assert!(!stray);
    let _ = fs::remove_file(&path);
}

#[cfg(unix)]
#[test]
fn save_into_a_read_only_directory_fails_without_losing_edits() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("scrybe-tui-ro-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ro.csv");
    fs::write(&path, "a\n1\n").unwrap();
    let mut app = App::from_files(vec![path.clone()]).unwrap();
    edit_first_cell(&mut app, "x");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
    ctrl(&mut app, 's');
    let status = app.status.clone().unwrap_or_default();
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o755));
    assert!(status.starts_with("save failed"), "{status}");
    assert!(app.panes[0].dirty(), "edits kept");
    assert_eq!(source_of(&app), "ax\n1\n");
    assert_eq!(on_disk(&path), b"a\n1\n");
    let _ = fs::remove_dir_all(&dir);
}

// ---- reload never destroys edits -------------------------------------

fn dirty_app(name: &str) -> (App, PathBuf) {
    let (mut app, path) = grid_app(name, "a,b\n1,2\n");
    edit_first_cell(&mut app, "!");
    (app, path)
}

#[test]
fn external_write_to_a_dirty_pane_prompts_and_k_keeps_mine() {
    let (mut app, path) = dirty_app("ext-k.csv");
    let mine = source_of(&app);
    fs::write(&path, "a,b\n1,2\n3,4\n").unwrap();
    app.reload_changed();
    assert_eq!(source_of(&app), mine, "edits not replaced");
    let out = screen(&mut app, 100, 6);
    assert!(out.contains("File changed on disk: r reload"), "{out}");
    assert!(out.contains("[conflict]"), "{out}");

    key(&mut app, KeyCode::Char('j')); // modal: ignored
    key(&mut app, KeyCode::Char('q'));
    assert!(!app.quit);
    key(&mut app, KeyCode::Char('k'));
    assert_eq!(source_of(&app), mine);
    app.reload_changed();
    assert!(
        app.panes[0].grid.as_ref().unwrap().conflict.is_none(),
        "no re-prompt"
    );
    assert!(app.panes[0].dirty());

    fs::write(&path, "a,b\n9,9\n").unwrap(); // the NEXT external change prompts again
    app.reload_changed();
    assert!(app.panes[0].grid.as_ref().unwrap().conflict.is_some());
    let _ = fs::remove_file(&path);
}

#[test]
fn external_write_then_r_discards_my_edits() {
    let (mut app, path) = dirty_app("ext-r.csv");
    fs::write(&path, "a,b\n1,2\n3,4\n").unwrap();
    app.reload_changed();
    key(&mut app, KeyCode::Char('r'));
    assert_eq!(source_of(&app), "a,b\n1,2\n3,4\n");
    assert!(!app.panes[0].dirty());
    assert!(
        !app.panes[0].grid.as_ref().unwrap().history.can_undo(),
        "history cleared"
    );
    let _ = fs::remove_file(&path);
}

#[test]
fn external_write_equal_to_my_text_is_not_a_conflict() {
    let (mut app, path) = dirty_app("ext-eq.csv");
    let mine = source_of(&app);
    fs::write(&path, &mine).unwrap();
    app.reload_changed();
    assert!(app.panes[0].grid.as_ref().unwrap().conflict.is_none());
    assert!(!app.panes[0].dirty(), "disk now equals the buffer");
    let _ = fs::remove_file(&path);
}

#[test]
fn deleted_or_invalid_utf8_file_keeps_a_dirty_pane() {
    let (mut app, path) = dirty_app("ext-gone.csv");
    let mine = source_of(&app);
    fs::remove_file(&path).unwrap();
    app.reload_changed();
    assert_eq!(source_of(&app), mine);
    assert!(app.status.as_deref().unwrap().contains("cannot read"));
    app.status = None;
    app.reload_changed();
    assert!(app.status.is_none(), "reported once, not every tick");

    fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
    app.reload_changed();
    assert_eq!(source_of(&app), mine);
    assert!(app.panes[0].dirty());
    assert!(app.panes[0].grid.as_ref().unwrap().conflict.is_none());
    let _ = fs::remove_file(&path);
}

#[test]
fn same_mtime_tick_with_different_content_is_still_noticed() {
    let (mut app, path) = dirty_app("ext-tick.csv");
    fs::write(&path, "a,b\n7,7\n").unwrap();
    app.panes[0].mtime = file_mtime(&path); // as if nothing changed
    app.reload_changed();
    assert!(app.panes[0].grid.as_ref().unwrap().conflict.is_some());
    let _ = fs::remove_file(&path);
}

#[test]
fn an_open_edit_is_not_replaced_by_a_reload_either() {
    let (mut app, path) = grid_app("ext-open.csv", "a,b\n1,2\n");
    key(&mut app, KeyCode::Enter);
    type_str(&mut app, "typed");
    fs::write(&path, "a,b\n9,9\n").unwrap();
    app.reload_changed();
    let g = app.panes[0].grid.as_ref().unwrap();
    assert_eq!(g.edit.as_ref().unwrap().buf, "atyped");
    assert!(g.conflict.is_some());
    let _ = fs::remove_file(&path);
}

#[test]
fn an_open_edit_stays_modal_over_the_conflict_prompt() {
    // R2-R2-BEH-1: a typed 'r' used to answer the prompt and reset the pane.
    let (mut app, path) = grid_app("ext-open-r.csv", "a,b\n1,2\n");
    key(&mut app, KeyCode::Enter);
    type_str(&mut app, "x");
    fs::write(&path, "a,b\n9,9\n").unwrap();
    app.reload_changed();
    key(&mut app, KeyCode::Char('r'));
    let g = app.panes[0].grid.as_ref().unwrap();
    assert_eq!(g.edit.as_ref().unwrap().buf, "axr");
    assert!(g.conflict.is_some());
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('r'));
    let g = app.panes[0].grid.as_ref().unwrap();
    assert!(g.conflict.is_none());
    let _ = fs::remove_file(&path);
}

#[test]
fn ctrl_s_over_an_unnoticed_external_change_prompts_instead_of_overwriting() {
    let (mut app, path) = dirty_app("save-conflict.csv");
    fs::write(&path, "a,b\n5,5\n").unwrap();
    ctrl(&mut app, 's'); // before any reload tick saw the change
    assert_eq!(on_disk(&path), b"a,b\n5,5\n", "no silent overwrite");
    assert!(app.panes[0].grid.as_ref().unwrap().conflict.is_some());
    key(&mut app, KeyCode::Char('k'));
    ctrl(&mut app, 's'); // now an explicit choice
    assert_eq!(on_disk(&path), source_of(&app).as_bytes());
    let _ = fs::remove_file(&path);
}

#[test]
fn ctrl_s_never_overwrites_an_unreadable_file() {
    // R1-DL-1: a non-UTF-8 replacement skipped the pre-write check.
    let (mut app, path) = dirty_app("save-unreadable.csv");
    fs::write(&path, [0xffu8, 0xfe]).unwrap();
    ctrl(&mut app, 's');
    assert_eq!(on_disk(&path), vec![0xffu8, 0xfe]);
    assert!(app.panes[0].dirty());
    let _ = fs::remove_file(&path);
}

#[test]
fn after_keep_mine_undo_to_baseline_stays_dirty_not_stale_clean() {
    // R1-BEH-2 / R1-CONV-2: k left baseline old, so undo looked clean while disk differed.
    let (mut app, path) = dirty_app("keep-undo.csv");
    fs::write(&path, "a,b\n9,9\n").unwrap();
    app.reload_changed();
    key(&mut app, KeyCode::Char('k'));
    key(&mut app, KeyCode::Char('u'));
    assert!(app.panes[0].dirty(), "differs from disk: not clean");
    let _ = fs::remove_file(&path);
}

#[test]
fn a_long_cell_shows_the_caret_and_typed_text() {
    // R1-BEH-1: the caret past column 30 was truncated away.
    let long = "x".repeat(40);
    let (mut app, path) = grid_app("long.csv", &format!("a\n{long}\n"));
    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Char('i'));
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Char('Z'));
    let out = screen(&mut app, 60, 6);
    assert!(out.contains(&format!("Z{CARET}")), "{out}");
    let _ = fs::remove_file(&path);
}

#[test]
fn dirty_split_grid_footer_keeps_quit_hint_at_80_columns() {
    // R1-CONV-1: the 94-char hint cut off q:quit.
    let p1 = temp("foot1.csv", "a\n1\n");
    let p2 = temp("foot2.csv", "a\n1\n");
    let mut app = App::from_files(vec![p1.clone(), p2.clone()]).unwrap();
    edit_first_cell(&mut app, "!");
    let out = screen(&mut app, 80, 8);
    assert!(out.contains("q:quit"), "{out}");
    let _ = fs::remove_file(&p1);
    let _ = fs::remove_file(&p2);
}

// ---- quit guard ------------------------------------------------------

#[test]
fn quit_with_nothing_dirty_quits_at_once() {
    let (mut app, path) = grid_app("q-clean.csv", "a\n1\n");
    key(&mut app, KeyCode::Char('q'));
    assert!(app.quit);
    let _ = fs::remove_file(&path);
}

#[test]
fn quit_with_a_dirty_pane_prompts_and_c_cancels() {
    let (mut app, path) = dirty_app("q-c.csv");
    for quit_key in [
        (KeyCode::Char('q'), KeyModifiers::NONE),
        (KeyCode::Esc, KeyModifiers::NONE),
        (KeyCode::Char('c'), KeyModifiers::CONTROL),
    ] {
        press(&mut app, quit_key.0, quit_key.1);
        assert!(!app.quit);
        let out = screen(&mut app, 100, 6);
        assert!(
            out.contains("Unsaved changes: s save all and quit"),
            "{out}"
        );
        key(&mut app, KeyCode::Char('x')); // anything else is ignored
        assert!(app.quit_prompt && !app.quit);
        key(&mut app, KeyCode::Char('c'));
        assert!(!app.quit_prompt && !app.quit);
    }
    assert!(app.panes[0].dirty());
    let _ = fs::remove_file(&path);
}

#[test]
fn quit_prompt_d_discards_and_quits() {
    let (mut app, path) = dirty_app("q-d.csv");
    key(&mut app, KeyCode::Char('q'));
    key(&mut app, KeyCode::Char('d'));
    assert!(app.quit);
    assert_eq!(on_disk(&path), b"a,b\n1,2\n", "nothing written");
    let _ = fs::remove_file(&path);
}

#[test]
fn quit_prompt_s_saves_then_quits() {
    let (mut app, path) = dirty_app("q-s.csv");
    key(&mut app, KeyCode::Char('q'));
    key(&mut app, KeyCode::Char('s'));
    assert!(app.quit);
    assert_eq!(on_disk(&path), source_of(&app).as_bytes());
    let _ = fs::remove_file(&path);
}

#[test]
fn quit_prompt_esc_cancels() {
    let (mut app, path) = dirty_app("q-esc.csv");
    key(&mut app, KeyCode::Char('q'));
    key(&mut app, KeyCode::Esc);
    assert!(!app.quit && !app.quit_prompt);
    let _ = fs::remove_file(&path);
}

#[cfg(unix)]
#[test]
fn a_failed_save_cancels_the_quit() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("scrybe-tui-roq-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("q.csv");
    fs::write(&path, "a\n1\n").unwrap();
    let mut app = App::from_files(vec![path.clone()]).unwrap();
    edit_first_cell(&mut app, "x");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
    key(&mut app, KeyCode::Char('q'));
    key(&mut app, KeyCode::Char('s'));
    let status = app.status.clone().unwrap_or_default();
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o755));
    assert!(!app.quit, "the quit was cancelled");
    assert!(status.starts_with("save failed"), "{status}");
    assert!(app.panes[0].dirty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn clean_grid_pane_still_reloads_and_clears_history() {
    let (mut app, path) = grid_app("clean-reload.csv", "a\n1\n");
    edit_first_cell(&mut app, "x");
    ctrl(&mut app, 's');
    fs::write(&path, "a\n1\n2\n").unwrap();
    app.panes[0].mtime = None;
    assert!(app.panes[0].reload_if_changed());
    assert_eq!(source_of(&app), "a\n1\n2\n");
    assert!(!app.panes[0].grid.as_ref().unwrap().history.can_undo());
    let _ = fs::remove_file(&path);
}
