//! Every key binding lives in [`BINDINGS`]. Key handling, the hint bar, the
//! help screen and `mull --keys` all read that one table, so what the screen
//! says is what the keys do.

use std::fmt;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Something a key can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    // Moving around the map
    Parent,
    Child,
    Down,
    Up,
    Root,
    // Topics
    AddChild,
    AddSibling,
    Edit,
    Change,
    Cut,
    Copy,
    Paste,
    // Structure
    Fold,
    MoveDown,
    MoveUp,
    Outdent,
    Indent,
    // File and session
    Undo,
    Redo,
    Save,
    Quit,
    Help,
    // Typing a topic
    Commit,
    Cancel,
    CursorLeft,
    CursorRight,
    CursorHome,
    CursorEnd,
    Backspace,
    DeleteForward,
    DeleteWord,
    DeleteToStart,
    // The help screen
    ScrollDown,
    ScrollUp,
    CloseHelp,
}

/// Which bindings are live: the map's, the text field's while a topic is
/// being typed, or the help screen's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Map,
    Typing,
    Help,
}

/// How bindings are grouped on the help screen and in `--keys`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Move,
    Topics,
    Structure,
    File,
    Typing,
    Help,
}

impl Group {
    pub const ALL: [Group; 6] = [
        Group::Move,
        Group::Topics,
        Group::Structure,
        Group::File,
        Group::Typing,
        Group::Help,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Group::Move => "Move",
            Group::Topics => "Topics",
            Group::Structure => "Structure",
            Group::File => "File",
            Group::Typing => "While typing",
            Group::Help => "On this screen",
        }
    }

    pub fn context(self) -> Context {
        match self {
            Group::Typing => Context::Typing,
            Group::Help => Context::Help,
            _ => Context::Map,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl Key {
    /// The table's view of a key press. Letters carry Shift in their case
    /// (`J` vs `j`) and terminals disagree on whether they also report the
    /// Shift modifier, so it's dropped for character keys; Ctrl-letters are
    /// matched in lower case whatever Caps Lock says.
    pub fn from_event(event: KeyEvent) -> Key {
        let mut modifiers =
            event.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        let mut code = event.code;
        if let KeyCode::Char(c) = code {
            modifiers.remove(KeyModifiers::SHIFT);
            if modifiers.contains(KeyModifiers::CONTROL) {
                code = KeyCode::Char(c.to_ascii_lowercase());
            }
        }
        Key { code, modifiers }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            f.write_str("^")?;
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            f.write_str("M-")?;
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("Space"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::Left => f.write_str("←"),
            KeyCode::Right => f.write_str("→"),
            KeyCode::Up => f.write_str("↑"),
            KeyCode::Down => f.write_str("↓"),
            KeyCode::Enter => f.write_str("Enter"),
            KeyCode::Tab => f.write_str("Tab"),
            KeyCode::Esc => f.write_str("Esc"),
            KeyCode::Backspace => f.write_str("Bksp"),
            KeyCode::Delete => f.write_str("Del"),
            KeyCode::Home => f.write_str("Home"),
            KeyCode::End => f.write_str("End"),
            other => write!(f, "{other:?}"),
        }
    }
}

pub struct Binding {
    pub group: Group,
    pub action: Action,
    /// The first key is the one the hint bar shows.
    pub keys: &'static [Key],
    pub description: &'static str,
}

impl Binding {
    pub fn context(&self) -> Context {
        self.group.context()
    }

    /// All the keys, slash-separated: `h/←`.
    pub fn keys_label(&self) -> String {
        let keys: Vec<String> = self.keys.iter().map(Key::to_string).collect();
        keys.join("/")
    }
}

const fn key(c: char) -> Key {
    Key {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::NONE,
    }
}

const fn ctrl(c: char) -> Key {
    Key {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::CONTROL,
    }
}

const fn special(code: KeyCode) -> Key {
    Key {
        code,
        modifiers: KeyModifiers::NONE,
    }
}

const fn bind(
    group: Group,
    action: Action,
    keys: &'static [Key],
    description: &'static str,
) -> Binding {
    Binding {
        group,
        action,
        keys,
        description,
    }
}

#[rustfmt::skip]
pub const BINDINGS: &[Binding] = &[
    bind(Group::Move, Action::Parent, &[key('h'), special(KeyCode::Left)], "Go to the parent"),
    bind(Group::Move, Action::Child, &[key('l'), special(KeyCode::Right)], "Go to a child, unfolding"),
    bind(Group::Move, Action::Down, &[key('j'), special(KeyCode::Down)], "Next topic at this level"),
    bind(Group::Move, Action::Up, &[key('k'), special(KeyCode::Up)], "Previous topic at this level"),
    bind(Group::Move, Action::Root, &[key('g'), special(KeyCode::Home)], "Go to the root"),

    bind(Group::Topics, Action::AddChild, &[special(KeyCode::Tab)], "Add a child"),
    bind(Group::Topics, Action::AddSibling, &[special(KeyCode::Enter)], "Add a sibling below"),
    bind(Group::Topics, Action::Edit, &[key('e'), key('i')], "Edit the text"),
    bind(Group::Topics, Action::Change, &[key('c')], "Replace the text"),
    bind(Group::Topics, Action::Cut, &[key('d'), special(KeyCode::Delete)], "Cut, with everything under it"),
    bind(Group::Topics, Action::Copy, &[key('y')], "Copy, with everything under it"),
    bind(Group::Topics, Action::Paste, &[key('p')], "Paste as the last child"),

    bind(Group::Structure, Action::Fold, &[key(' ')], "Fold or unfold"),
    bind(Group::Structure, Action::MoveDown, &[key('J')], "Swap with the sibling below"),
    bind(Group::Structure, Action::MoveUp, &[key('K')], "Swap with the sibling above"),
    bind(Group::Structure, Action::Outdent, &[key('H')], "Move out, after the parent"),
    bind(Group::Structure, Action::Indent, &[key('L')], "Move in, under the sibling above"),

    bind(Group::File, Action::Undo, &[key('u')], "Undo"),
    bind(Group::File, Action::Redo, &[ctrl('r'), key('U')], "Redo"),
    bind(Group::File, Action::Save, &[key('s'), ctrl('s')], "Save"),
    bind(Group::File, Action::Quit, &[key('q'), ctrl('c')], "Quit (twice if unsaved)"),
    bind(Group::File, Action::Help, &[key('?')], "Show all keys"),

    bind(Group::Typing, Action::Commit, &[special(KeyCode::Enter)], "Keep the text"),
    bind(Group::Typing, Action::Cancel, &[special(KeyCode::Esc), ctrl('c')], "Discard; drops a new topic"),
    bind(Group::Typing, Action::CursorLeft, &[special(KeyCode::Left), ctrl('b')], "Cursor left"),
    bind(Group::Typing, Action::CursorRight, &[special(KeyCode::Right), ctrl('f')], "Cursor right"),
    bind(Group::Typing, Action::CursorHome, &[special(KeyCode::Home), ctrl('a')], "Cursor to the start"),
    bind(Group::Typing, Action::CursorEnd, &[special(KeyCode::End), ctrl('e')], "Cursor to the end"),
    bind(Group::Typing, Action::Backspace, &[special(KeyCode::Backspace)], "Delete before the cursor"),
    bind(Group::Typing, Action::DeleteForward, &[special(KeyCode::Delete)], "Delete under the cursor"),
    bind(Group::Typing, Action::DeleteWord, &[ctrl('w')], "Delete the word before"),
    bind(Group::Typing, Action::DeleteToStart, &[ctrl('u')], "Delete back to the start"),

    bind(Group::Help, Action::ScrollDown, &[key('j'), special(KeyCode::Down)], "Scroll down"),
    bind(Group::Help, Action::ScrollUp, &[key('k'), special(KeyCode::Up)], "Scroll up"),
    bind(Group::Help, Action::CloseHelp, &[special(KeyCode::Esc), key('q'), key('?')], "Close (any other key does too)"),
];

/// An entry in the hint bar: the first key of each action, then a label.
pub struct Hint {
    pub context: Context,
    pub actions: &'static [Action],
    pub label: &'static str,
}

const fn hint(context: Context, actions: &'static [Action], label: &'static str) -> Hint {
    Hint {
        context,
        actions,
        label,
    }
}

/// The hint bar, most useful first. The last hint of each context always
/// stays on screen; the others drop off the end when the terminal is narrow.
#[rustfmt::skip]
pub const HINTS: &[Hint] = &[
    hint(Context::Map, &[Action::Parent, Action::Down, Action::Up, Action::Child], "move"),
    hint(Context::Map, &[Action::AddChild], "child"),
    hint(Context::Map, &[Action::AddSibling], "sibling"),
    hint(Context::Map, &[Action::Edit], "edit"),
    hint(Context::Map, &[Action::Cut], "cut"),
    hint(Context::Map, &[Action::Paste], "paste"),
    hint(Context::Map, &[Action::Fold], "fold"),
    hint(Context::Map, &[Action::MoveDown, Action::MoveUp], "reorder"),
    hint(Context::Map, &[Action::Outdent, Action::Indent], "out/in"),
    hint(Context::Map, &[Action::Undo], "undo"),
    hint(Context::Map, &[Action::Save], "save"),
    hint(Context::Map, &[Action::Quit], "quit"),
    hint(Context::Map, &[Action::Help], "all keys"),

    hint(Context::Help, &[Action::ScrollDown, Action::ScrollUp], "scroll"),
    hint(Context::Help, &[Action::CloseHelp], "close"),

    hint(Context::Typing, &[Action::Commit], "done"),
    hint(Context::Typing, &[Action::CursorLeft, Action::CursorRight], "move"),
    hint(Context::Typing, &[Action::CursorHome, Action::CursorEnd], "start/end"),
    hint(Context::Typing, &[Action::DeleteWord], "delete word"),
    hint(Context::Typing, &[Action::Cancel], "cancel"),
];

impl Hint {
    /// The first key of each action: run together when they're all single
    /// characters (`hjkl`), slash-separated otherwise (`Home/End`).
    pub fn keys(&self) -> String {
        let keys: Vec<String> = self
            .actions
            .iter()
            .filter_map(|&action| first_key(self.context, action))
            .map(|key| key.to_string())
            .collect();
        if keys.iter().all(|k| k.chars().count() == 1) {
            keys.concat()
        } else {
            keys.join("/")
        }
    }
}

pub fn hints(context: Context) -> impl Iterator<Item = &'static Hint> {
    HINTS.iter().filter(move |h| h.context == context)
}

/// The action bound to `event` in `context`, if any.
pub fn lookup(context: Context, event: KeyEvent) -> Option<Action> {
    let key = Key::from_event(event);
    BINDINGS
        .iter()
        .find(|b| b.context() == context && b.keys.contains(&key))
        .map(|b| b.action)
}

fn first_key(context: Context, action: Action) -> Option<Key> {
    BINDINGS
        .iter()
        .find(|b| b.context() == context && b.action == action)?
        .keys
        .first()
        .copied()
}

/// Every binding as Markdown tables, one per group: what `mull --keys`
/// prints and the README shows.
pub fn reference_markdown() -> String {
    let mut out = String::new();
    for group in Group::ALL {
        out.push_str(&format!(
            "**{}**\n\n| Keys | Action |\n|---|---|\n",
            group.title()
        ));
        for binding in BINDINGS.iter().filter(|b| b.group == group) {
            let keys: Vec<String> = binding.keys.iter().map(|k| format!("`{k}`")).collect();
            out.push_str(&format!(
                "| {} | {} |\n",
                keys.join(" "),
                binding.description
            ));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_key_does_two_things_in_one_context() {
        for (i, a) in BINDINGS.iter().enumerate() {
            for b in &BINDINGS[i + 1..] {
                if a.context() != b.context() {
                    continue;
                }
                for key in a.keys {
                    assert!(
                        !b.keys.contains(key),
                        "{key} is bound to both {:?} and {:?}",
                        a.action,
                        b.action
                    );
                }
            }
        }
    }

    #[test]
    fn every_hinted_action_is_bound_in_its_context() {
        for hint in HINTS {
            for &action in hint.actions {
                assert!(
                    first_key(hint.context, action).is_some(),
                    "hint {:?} names unbound {action:?}",
                    hint.label
                );
            }
        }
    }

    #[test]
    fn hints_name_their_keys() {
        let keys: Vec<String> = hints(Context::Map).map(Hint::keys).collect();
        assert_eq!(keys[0], "hjkl");
        assert!(keys.contains(&"Tab".to_string()));
        let typing: Vec<String> = hints(Context::Typing).map(Hint::keys).collect();
        assert!(typing.contains(&"Home/End".to_string()));
    }

    #[test]
    fn shifted_letters_match_whatever_the_terminal_reports() {
        let plain = KeyEvent::new(KeyCode::Char('J'), KeyModifiers::NONE);
        let shifted = KeyEvent::new(KeyCode::Char('J'), KeyModifiers::SHIFT);
        assert_eq!(lookup(Context::Map, plain), Some(Action::MoveDown));
        assert_eq!(lookup(Context::Map, shifted), Some(Action::MoveDown));
    }

    #[test]
    fn ctrl_letters_ignore_caps_lock() {
        let event = KeyEvent::new(KeyCode::Char('R'), KeyModifiers::CONTROL);
        assert_eq!(lookup(Context::Map, event), Some(Action::Redo));
    }

    #[test]
    fn the_same_key_means_different_things_in_each_context() {
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(lookup(Context::Map, enter), Some(Action::AddSibling));
        assert_eq!(lookup(Context::Typing, enter), Some(Action::Commit));
    }
}
