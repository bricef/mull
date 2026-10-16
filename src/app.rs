//! Everything that happens between key presses: selection, typing,
//! restructuring, undo, saving.

use std::{
    fs,
    io::{self, Write as _},
    mem,
    path::{Path as FilePath, PathBuf},
};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    edit::LineEditor,
    keymap::{self, Action, Context},
    layout::{self, Layout, Options},
    markdown,
    tree::{Node, Path, child_path, normalize_text},
};

pub enum Mode {
    /// Moving around and restructuring.
    Map,
    /// Typing the selected topic's text. `new_topic` marks a topic created
    /// for this edit, which cancelling removes again.
    Edit { editor: LineEditor, new_topic: bool },
    /// The full key list, scrolled down `scroll` rows when it doesn't fit.
    /// The UI keeps `scroll` within bounds.
    Help { scroll: usize },
}

/// What kind of news a status message is, which decides its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Something happened as asked: a cut, a copy.
    Info,
    /// The file is safe: saved.
    Success,
    /// The key did nothing, or there's something to know first.
    Warning,
    /// Something failed.
    Error,
}

/// A line for the status bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub tone: Tone,
    pub text: String,
}

/// A state to come back to with undo or redo.
struct Snapshot {
    tree: Node,
    selected: Path,
}

const UNDO_LIMIT: usize = 1000;

pub struct App {
    pub tree: Node,
    /// Always a visible node of `tree`.
    pub selected: Path,
    pub mode: Mode,
    pub file: PathBuf,
    pub options: Options,
    /// Shown in the status line until the next key press.
    pub message: Option<Message>,
    pub should_quit: bool,
    /// The map coordinates at the top left of the screen. The UI moves it
    /// to keep the selection in view.
    pub viewport: (isize, isize),
    /// The Markdown last read from or written to `file`.
    saved: String,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    clipboard: Option<Node>,
    /// Set by a first `q` with unsaved changes; a second one quits.
    quit_armed: bool,
}

impl App {
    pub fn new(tree: Node, file: PathBuf) -> Self {
        App {
            saved: markdown::serialize(&tree),
            tree,
            selected: Vec::new(),
            mode: Mode::Map,
            file,
            options: Options::default(),
            message: None,
            should_quit: false,
            viewport: (0, 0),
            undo: Vec::new(),
            redo: Vec::new(),
            clipboard: None,
            quit_armed: false,
        }
    }

    /// Open `file`, or start a new map named after it if it doesn't exist.
    pub fn open(file: &FilePath) -> io::Result<Self> {
        let name = file
            .file_stem()
            .map_or_else(|| "map".to_string(), |s| s.to_string_lossy().into_owned());
        match fs::read_to_string(file) {
            Ok(text) => {
                let mut app = App::new(markdown::parse(&text, &name), file.to_path_buf());
                if app.saved != text {
                    app.say(
                        Tone::Warning,
                        "Saving will rewrite this file in mull's Markdown layout",
                    );
                }
                Ok(app)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let mut app = App::new(Node::new(&name), file.to_path_buf());
                let text = format!("New map: {} is created when you save", file.display());
                app.say(Tone::Info, text);
                Ok(app)
            }
            Err(e) => Err(e),
        }
    }

    /// Show `text` in the status line until the next key press.
    pub fn say(&mut self, tone: Tone, text: impl Into<String>) {
        self.message = Some(Message {
            tone,
            text: text.into(),
        });
    }

    pub fn is_dirty(&self) -> bool {
        markdown::serialize(&self.tree) != self.saved
    }

    /// The map as it should look now, with the topic being typed shown in
    /// full.
    pub fn layout(&self) -> Layout {
        let editing = match &self.mode {
            Mode::Edit { editor, .. } => Some((self.selected.as_slice(), editor.text())),
            _ => None,
        };
        layout::layout(&self.tree, &self.options, editing)
    }

    /// The topics from the root down to the selection: `Root › A › A1`.
    /// While typing, the selection's part is the text as typed so far.
    pub fn breadcrumb(&self) -> String {
        let mut node = &self.tree;
        let mut parts = vec![node.text.as_str()];
        for &i in &self.selected {
            match node.children.get(i) {
                Some(child) => {
                    node = child;
                    parts.push(&child.text);
                }
                None => break,
            }
        }
        if let (Mode::Edit { editor, .. }, Some(last)) = (&self.mode, parts.last_mut()) {
            *last = editor.text();
        }
        parts.join(" › ")
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        self.message = None;
        let quit_armed = mem::take(&mut self.quit_armed);
        match self.mode {
            Mode::Help { .. } => match keymap::lookup(Context::Help, key) {
                Some(action) => self.apply(action),
                None => self.mode = Mode::Map,
            },
            Mode::Edit { .. } => match keymap::lookup(Context::Typing, key) {
                Some(action) => self.apply(action),
                None => self.type_char(key),
            },
            Mode::Map => match keymap::lookup(Context::Map, key) {
                Some(Action::Quit) => self.quit(quit_armed),
                Some(action) => self.apply(action),
                None => {}
            },
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Parent => {
                self.selected.pop();
            }
            Action::Child => self.go_to_child(),
            Action::Down => self.go_along_level(true),
            Action::Up => self.go_along_level(false),
            Action::Root => self.selected.clear(),

            Action::AddChild => self.add_child(),
            Action::AddSibling => self.add_sibling(),
            Action::Edit => self.begin_edit(false),
            Action::Change => self.begin_edit(true),
            Action::Cut => self.cut(),
            Action::Copy => self.copy(),
            Action::Paste => self.paste(),

            Action::Fold => self.toggle_fold(),
            Action::MoveDown => self.restructure(Node::move_down, "Already the last sibling"),
            Action::MoveUp => self.restructure(Node::move_up, "Already the first sibling"),
            Action::Outdent => {
                self.restructure(Node::outdent, "Can't move out: the map has one root")
            }
            Action::Indent => self.restructure(Node::indent, "No sibling above to move under"),

            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::Save => self.save_and_report(),
            Action::Quit => self.quit(false),
            Action::Help => self.mode = Mode::Help { scroll: 0 },

            Action::Commit => self.commit(),
            Action::Cancel => self.cancel(),
            Action::CursorLeft => self.with_editor(LineEditor::left),
            Action::CursorRight => self.with_editor(LineEditor::right),
            Action::CursorHome => self.with_editor(LineEditor::home),
            Action::CursorEnd => self.with_editor(LineEditor::end),
            Action::Backspace => self.with_editor(LineEditor::backspace),
            Action::DeleteForward => self.with_editor(LineEditor::delete_forward),
            Action::DeleteWord => self.with_editor(LineEditor::delete_word),
            Action::DeleteToStart => self.with_editor(LineEditor::delete_to_start),

            Action::ScrollDown => self.scroll_help(|scroll| scroll.saturating_add(1)),
            Action::ScrollUp => self.scroll_help(|scroll| scroll.saturating_sub(1)),
            Action::CloseHelp => self.mode = Mode::Map,
        }
    }

    fn scroll_help(&mut self, change: impl FnOnce(usize) -> usize) {
        if let Mode::Help { scroll } = &mut self.mode {
            *scroll = change(*scroll);
        }
    }

    // Moving

    /// Unfold the selection and go to its child nearest on screen.
    fn go_to_child(&mut self) {
        let Some(node) = self.tree.get_mut(&self.selected) else {
            return;
        };
        if node.children.is_empty() {
            return;
        }
        node.collapsed = false;
        let map = self.layout();
        let Some(here) = map.node(&self.selected).map(|n| n.y) else {
            return;
        };
        let depth = self.selected.len() + 1;
        let nearest = map
            .nodes
            .iter()
            .filter(|n| n.path.len() == depth && n.path.starts_with(&self.selected))
            .min_by_key(|n| n.y.abs_diff(here));
        if let Some(child) = nearest {
            self.selected = child.path.clone();
        }
    }

    /// Go to the nearest topic below (or above) at the same depth, whatever
    /// its parent.
    fn go_along_level(&mut self, down: bool) {
        let map = self.layout();
        let Some(here) = map.node(&self.selected).map(|n| n.y) else {
            return;
        };
        let depth = self.selected.len();
        let level = map.nodes.iter().filter(|n| n.path.len() == depth);
        let next = if down {
            level.filter(|n| n.y > here).min_by_key(|n| n.y)
        } else {
            level.filter(|n| n.y < here).max_by_key(|n| n.y)
        };
        if let Some(node) = next {
            self.selected = node.path.clone();
        }
    }

    // Changing the map. Every change records the state before it, so undo
    // can bring it back.

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            tree: self.tree.clone(),
            selected: self.selected.clone(),
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.tree = snapshot.tree;
        self.selected = snapshot.selected;
    }

    fn record(&mut self, before: Snapshot) {
        self.undo.push(before);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn add_child(&mut self) {
        let before = self.snapshot();
        let Some(parent) = self.tree.get_mut(&self.selected) else {
            return;
        };
        parent.collapsed = false;
        parent.children.push(Node::default());
        let index = parent.children.len() - 1;
        self.record(before);
        self.selected = child_path(&self.selected, index);
        self.start_typing("", true);
    }

    fn add_sibling(&mut self) {
        let Some((&index, parent)) = self.selected.split_last() else {
            // The root has no siblings; a child is the next best thing.
            return self.add_child();
        };
        let parent = parent.to_vec();
        let before = self.snapshot();
        if let Some(path) = self.tree.insert(&parent, index + 1, Node::default()) {
            self.record(before);
            self.selected = path;
            self.start_typing("", true);
        }
    }

    /// Run a structural change that returns the moved node's new path, or
    /// `None` (changing nothing) when it doesn't apply.
    fn restructure(&mut self, change: fn(&mut Node, &[usize]) -> Option<Path>, refusal: &str) {
        let before = self.snapshot();
        match change(&mut self.tree, &self.selected) {
            Some(path) => {
                self.record(before);
                self.selected = path;
            }
            None => self.say(Tone::Warning, refusal),
        }
    }

    fn cut(&mut self) {
        let Some((&index, parent)) = self.selected.split_last() else {
            self.say(Tone::Warning, "The root can't be cut");
            return;
        };
        let parent = parent.to_vec();
        let before = self.snapshot();
        let Some(node) = self.tree.remove(&self.selected) else {
            return;
        };
        self.record(before);
        let siblings = self.tree.get(&parent).map_or(0, |p| p.children.len());
        self.selected = if index < siblings {
            child_path(&parent, index)
        } else if index > 0 {
            child_path(&parent, index - 1)
        } else {
            parent
        };
        self.say(Tone::Info, format!("Cut {}", describe(&node)));
        self.clipboard = Some(node);
    }

    fn copy(&mut self) {
        if let Some(node) = self.tree.get(&self.selected).cloned() {
            self.say(Tone::Info, format!("Copied {}", describe(&node)));
            self.clipboard = Some(node);
        }
    }

    fn paste(&mut self) {
        let Some(node) = self.clipboard.clone() else {
            self.say(Tone::Warning, "Nothing to paste: cut or copy a topic first");
            return;
        };
        let before = self.snapshot();
        let Some(parent) = self.tree.get_mut(&self.selected) else {
            return;
        };
        parent.collapsed = false;
        parent.children.push(node);
        let index = parent.children.len() - 1;
        self.record(before);
        self.selected = child_path(&self.selected, index);
    }

    /// Folding is view state: it isn't saved, so it isn't undone either.
    fn toggle_fold(&mut self) {
        match self.tree.get_mut(&self.selected) {
            Some(node) if !node.children.is_empty() => node.collapsed = !node.collapsed,
            _ => self.say(Tone::Warning, "Nothing to fold"),
        }
    }

    fn undo(&mut self) {
        match self.undo.pop() {
            Some(snapshot) => {
                self.redo.push(self.snapshot());
                self.restore(snapshot);
            }
            None => self.say(Tone::Warning, "Nothing to undo"),
        }
    }

    fn redo(&mut self) {
        match self.redo.pop() {
            Some(snapshot) => {
                self.undo.push(self.snapshot());
                self.restore(snapshot);
            }
            None => self.say(Tone::Warning, "Nothing to redo"),
        }
    }

    // Typing

    fn start_typing(&mut self, text: &str, new_topic: bool) {
        self.mode = Mode::Edit {
            editor: LineEditor::new(text),
            new_topic,
        };
    }

    fn begin_edit(&mut self, replace: bool) {
        let text = match self.tree.get(&self.selected) {
            Some(node) if !replace => node.text.clone(),
            _ => String::new(),
        };
        self.start_typing(&text, false);
    }

    fn with_editor(&mut self, change: impl FnOnce(&mut LineEditor)) {
        if let Mode::Edit { editor, .. } = &mut self.mode {
            change(editor);
        }
    }

    fn type_char(&mut self, key: KeyEvent) {
        if let KeyCode::Char(c) = key.code
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            self.with_editor(|editor| editor.insert(c));
        }
    }

    fn commit(&mut self) {
        let Mode::Edit { editor, new_topic } = mem::replace(&mut self.mode, Mode::Map) else {
            return;
        };
        let text = normalize_text(editor.text());
        if self.tree.get(&self.selected).is_none_or(|n| n.text == text) {
            return;
        }
        // A new topic's creation is already recorded; undoing it removes the
        // topic, text and all.
        let before = (!new_topic).then(|| self.snapshot());
        if let Some(node) = self.tree.get_mut(&self.selected) {
            node.text = text;
        }
        if let Some(before) = before {
            self.record(before);
        }
    }

    fn cancel(&mut self) {
        if let Mode::Edit {
            new_topic: true, ..
        } = self.mode
            && let Some(before) = self.undo.pop()
        {
            self.restore(before);
        }
        self.mode = Mode::Map;
    }

    // Files

    pub fn save(&mut self) -> io::Result<()> {
        let text = markdown::serialize(&self.tree);
        write_file(&self.file, &text)?;
        self.saved = text;
        Ok(())
    }

    fn save_and_report(&mut self) {
        match self.save() {
            Ok(()) => self.say(Tone::Success, format!("Saved {}", self.file.display())),
            Err(e) => self.say(
                Tone::Error,
                format!("Couldn't save {}: {e}", self.file.display()),
            ),
        }
    }

    fn quit(&mut self, confirmed: bool) {
        if self.is_dirty() && !confirmed {
            self.quit_armed = true;
            self.say(
                Tone::Warning,
                "Unsaved changes: s saves, q again quits without saving",
            );
        } else {
            self.should_quit = true;
        }
    }
}

/// `“text”`, or `“text” and N more` for a topic with others under it.
fn describe(node: &Node) -> String {
    match node.size() - 1 {
        0 => format!("“{}”", node.text),
        more => format!("“{}” and {more} more", node.text),
    }
}

/// Replace `file`'s contents without ever leaving it half-written: write a
/// temporary file beside it, then rename it into place. Symlinks are
/// followed rather than replaced, and the file keeps its permissions.
fn write_file(file: &FilePath, text: &str) -> io::Result<()> {
    let Ok(target) = fs::canonicalize(file) else {
        // Nothing there yet, so nothing to protect.
        return fs::write(file, text);
    };
    let dir = target.parent().unwrap_or(FilePath::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    temp.write_all(text.as_bytes())?;
    temp.as_file()
        .set_permissions(fs::metadata(&target)?.permissions())?;
    temp.as_file().sync_all()?;
    temp.persist(&target).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn press(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    /// Press each character in turn.
    fn keys(app: &mut App, chars: &str) {
        chars.chars().for_each(|c| press(app, KeyCode::Char(c)));
    }

    fn app(tree: Node) -> App {
        App::new(tree, PathBuf::from("test.md"))
    }

    fn sample() -> Node {
        Node::with_children(
            "Root",
            vec![
                Node::with_children("A", vec![Node::new("A1")]),
                Node::with_children("B", vec![Node::new("B1")]),
                Node::new("C"),
            ],
        )
    }

    fn text_at(app: &App, path: &[usize]) -> String {
        app.tree
            .get(path)
            .map(|n| n.text.clone())
            .unwrap_or_default()
    }

    #[test]
    fn tab_adds_a_child_and_starts_typing_it() {
        let mut app = app(Node::new("Root"));
        press(&mut app, KeyCode::Tab);
        keys(&mut app, "Idea");
        press(&mut app, KeyCode::Enter);
        assert_eq!(text_at(&app, &[0]), "Idea");
        assert_eq!(app.selected, vec![0]);
        assert!(matches!(app.mode, Mode::Map));
        assert!(app.is_dirty());
    }

    #[test]
    fn escape_drops_a_new_topic() {
        let mut app = app(Node::new("Root"));
        press(&mut app, KeyCode::Tab);
        keys(&mut app, "oops");
        press(&mut app, KeyCode::Esc);
        assert!(app.tree.children.is_empty());
        assert_eq!(app.selected, Vec::<usize>::new());
        assert!(!app.is_dirty());
    }

    #[test]
    fn editing_can_be_undone_and_redone() {
        let mut app = app(sample());
        keys(&mut app, "l");
        keys(&mut app, "c");
        keys(&mut app, "New");
        press(&mut app, KeyCode::Enter);
        assert_eq!(text_at(&app, &[1]), "New");
        keys(&mut app, "u");
        assert_eq!(text_at(&app, &[1]), "B");
        keys(&mut app, "U");
        assert_eq!(text_at(&app, &[1]), "New");
    }

    #[test]
    fn l_goes_to_the_child_nearest_on_screen() {
        let mut app = app(sample());
        keys(&mut app, "l");
        assert_eq!(app.selected, vec![1]);
    }

    #[test]
    fn j_and_k_cross_between_parents_at_the_same_depth() {
        let mut app = app(sample());
        app.selected = vec![0, 0];
        keys(&mut app, "j");
        assert_eq!(app.selected, vec![1, 0]);
        keys(&mut app, "j");
        assert_eq!(app.selected, vec![1, 0], "nothing further down at depth 2");
        keys(&mut app, "k");
        assert_eq!(app.selected, vec![0, 0]);
    }

    #[test]
    fn cutting_selects_the_next_sibling_and_paste_puts_it_back_as_a_child() {
        let mut app = app(sample());
        app.selected = vec![0];
        keys(&mut app, "d");
        assert_eq!(text_at(&app, &[0]), "B");
        assert_eq!(app.selected, vec![0]);
        keys(&mut app, "p");
        assert_eq!(app.selected, vec![0, 1]);
        assert_eq!(text_at(&app, &[0, 1, 0]), "A1");
    }

    #[test]
    fn the_root_cannot_be_cut() {
        let mut app = app(sample());
        keys(&mut app, "d");
        assert_eq!(app.tree, sample());
        assert_eq!(app.message.map(|m| m.tone), Some(Tone::Warning));
    }

    #[test]
    fn folding_hides_children_and_l_unfolds() {
        let mut app = app(sample());
        app.selected = vec![0];
        keys(&mut app, " ");
        assert!(app.tree.children[0].collapsed);
        keys(&mut app, "l");
        assert!(!app.tree.children[0].collapsed);
        assert_eq!(app.selected, vec![0, 0]);
    }

    #[test]
    fn quitting_with_unsaved_changes_takes_a_second_q() {
        let mut app = app(sample());
        app.selected = vec![2];
        keys(&mut app, "dq");
        assert!(!app.should_quit);
        keys(&mut app, "q");
        assert!(app.should_quit);
    }

    #[test]
    fn any_other_key_disarms_quit() {
        let mut app = app(sample());
        app.selected = vec![2];
        keys(&mut app, "dqjq");
        assert!(!app.should_quit);
    }

    #[test]
    fn a_clean_map_quits_at_once() {
        let mut app = app(sample());
        keys(&mut app, "q");
        assert!(app.should_quit);
    }

    #[test]
    fn the_breadcrumb_names_the_path_to_the_selection() {
        let mut app = app(sample());
        app.selected = vec![1, 0];
        assert_eq!(app.breadcrumb(), "Root › B › B1");
        press(&mut app, KeyCode::Tab);
        keys(&mut app, "new");
        assert_eq!(
            app.breadcrumb(),
            "Root › B › B1 › new",
            "shows what's typed"
        );
    }
}
