//! Opening and saving real files.

use std::fs;

use mull::app::App;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::from(code));
}

/// Add a child of the selection called `text`.
fn add_child(app: &mut App, text: &str) {
    press(app, KeyCode::Tab);
    text.chars().for_each(|c| press(app, KeyCode::Char(c)));
    press(app, KeyCode::Enter);
}

#[test]
fn a_new_map_is_written_on_first_save() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ideas.md");
    let mut app = App::open(&file).unwrap();
    assert!(!file.exists());
    add_child(&mut app, "first");
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(fs::read_to_string(&file).unwrap(), "# ideas\n\n- first\n");
    assert!(!app.is_dirty());
}

#[test]
fn other_markdown_keeps_every_line_when_saved() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.md");
    fs::write(&file, "Intro\n# A\n- x\n").unwrap();
    let mut app = App::open(&file).unwrap();
    assert!(app.message.is_some(), "warns that saving will reformat");
    assert!(!app.is_dirty(), "reformatting alone isn't a change");
    app.save().unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "# notes\n\n- Intro\n- A\n  - x\n"
    );
}

#[cfg(unix)]
#[test]
fn saving_writes_through_symlinks_and_keeps_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real.md");
    let link = dir.path().join("link.md");
    fs::write(&real, "# R\n").unwrap();
    fs::set_permissions(&real, fs::Permissions::from_mode(0o640)).unwrap();
    symlink(&real, &link).unwrap();

    let mut app = App::open(&link).unwrap();
    add_child(&mut app, "x");
    app.save().unwrap();

    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_to_string(&real).unwrap(), "# R\n\n- x\n");
    let mode = fs::metadata(&real).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o640);
}

#[test]
fn an_unreadable_path_is_an_error_not_a_new_map() {
    let dir = tempfile::tempdir().unwrap();
    assert!(App::open(dir.path()).is_err());
}
