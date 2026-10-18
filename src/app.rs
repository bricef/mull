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
    tree::{Kind, Node, Path, child_path, normalize_text},
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
    /// The file's front matter: metadata kept aside, written back first.
    pub front_matter: Option<String>,
    /// Always a node of `tree` that's on screen (see [`Node::is_shown`]).
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
        App::from_map(markdown::Map::from(tree), file)
    }

    pub fn from_map(map: markdown::Map, file: PathBuf) -> Self {
        App {
            saved: markdown::serialize(&map),
            tree: map.root,
            front_matter: map.front_matter,
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
                let mut app = App::from_map(markdown::parse(&text, &name), file.to_path_buf());
                if !markdown::readable(&text) {
                    app.say(
                        Tone::Warning,
                        "The Markdown parser can't read this file, so it's kept as one block",
                    );
                } else if app.saved != text {
                    app.say(
                        Tone::Warning,
                        "Saving will rewrite this file in mull's Markdown layout",
                    );
                }
                Ok(app)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let mut app = App::new(Node::heading(1, &name), file.to_path_buf());
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

    /// The file as saving would write it.
    pub fn markdown(&self) -> String {
        markdown::serialize(&markdown::Map {
            front_matter: self.front_matter.clone(),
            root: self.tree.clone(),
        })
    }

    pub fn is_dirty(&self) -> bool {
        self.markdown() != self.saved
    }

    /// Whether content blocks are on screen, or only sections.
    pub fn shows_blocks(&self) -> bool {
        self.options.show_blocks
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
        let mut parts = vec![node.label()];
        for &i in &self.selected {
            match node.children.get(i) {
                Some(child) => {
                    node = child;
                    parts.push(child.label());
                }
                None => break,
            }
        }
        if let (Mode::Edit { editor, .. }, Some(last)) = (&self.mode, parts.last_mut()) {
            *last = editor.text().to_string();
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
        self.keep_selection_on_screen();
    }

    /// If the selection has gone off screen (a hidden block after a cut, a
    /// paste or the toggle), move it up to the nearest section that's shown.
    fn keep_selection_on_screen(&mut self) {
        while !self.selected.is_empty() && !self.tree.is_shown(&self.selected, self.shows_blocks())
        {
            self.selected.pop();
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
            Action::ToggleBlocks => self.toggle_blocks(),
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

    /// Keep a change, selecting `selected`, if the tree can still be
    /// written as Markdown and read back the same; otherwise put everything
    /// back and say why. Returns whether the change was kept.
    fn settle(&mut self, before: Snapshot, selected: Path) -> bool {
        match self.tree.check() {
            Ok(()) => {
                self.record(before);
                self.selected = selected;
                true
            }
            Err(problem) => {
                self.restore(before);
                self.say(Tone::Warning, problem.to_string());
                false
            }
        }
    }

    /// A new sub-section of the selection, after the ones it has.
    fn add_child(&mut self) {
        let before = self.snapshot();
        let parent_path = self.selected.clone();
        let Some(parent) = self.tree.get_mut(&parent_path) else {
            return;
        };
        parent.collapsed = false;
        let node = Node::heading(sub_level(parent), "");
        if let Some(path) = self.tree.append(&parent_path, node)
            && self.settle(before, path)
        {
            self.start_typing("", true);
        }
    }

    /// A new section after the selection: at the same level after a
    /// section, or the section's first sub-section after a block.
    fn add_sibling(&mut self) {
        let Some((&index, parent)) = self.selected.split_last() else {
            // The root has no siblings; a child is the next best thing.
            return self.add_child();
        };
        let parent = parent.to_vec();
        let (Some(current), Some(parent_node)) =
            (self.tree.get(&self.selected), self.tree.get(&parent))
        else {
            return;
        };
        let node = match current.kind {
            Kind::Heading { level } => Node::heading(level, ""),
            Kind::Document | Kind::Block(_) => Node::heading(sub_level(parent_node), ""),
        };
        let before = self.snapshot();
        if let Some(path) = self.tree.insert(&parent, index + 1, node)
            && self.settle(before, path)
        {
            self.start_typing("", true);
        }
    }

    /// Run a structural change that returns the moved node's new path, or
    /// `None` (changing nothing) when it doesn't apply.
    fn restructure(&mut self, change: fn(&mut Node, &[usize]) -> Option<Path>, refusal: &str) {
        let before = self.snapshot();
        match change(&mut self.tree, &self.selected) {
            Some(path) => {
                self.settle(before, path);
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
        let siblings = self.tree.get(&parent).map_or(0, |p| p.children.len());
        let next = if index < siblings {
            child_path(&parent, index)
        } else if index > 0 {
            child_path(&parent, index - 1)
        } else {
            parent
        };
        // Taking out what stood between two lists would merge them.
        if self.settle(before, next) {
            self.say(Tone::Info, format!("Cut {}", describe(&node)));
            self.clipboard = Some(node);
        }
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
        let parent = self.selected.clone();
        if let Some(parent_node) = self.tree.get_mut(&parent) {
            parent_node.collapsed = false;
        }
        if let Some(path) = self.tree.append(&parent, node) {
            self.settle(before, path);
        }
    }

    /// Show content blocks, or only the outline of sections. View state,
    /// like folding.
    fn toggle_blocks(&mut self) {
        self.options.show_blocks = !self.options.show_blocks;
        if self.shows_blocks() {
            self.say(Tone::Info, "Showing content blocks");
        } else {
            self.say(
                Tone::Info,
                "Sections only: b shows the content blocks again",
            );
        }
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

    /// Topics can be typed into; content blocks only move, and the document
    /// takes its name from its file.
    fn begin_edit(&mut self, replace: bool) {
        let Some(node) = self.tree.get(&self.selected) else {
            return;
        };
        match node.kind {
            Kind::Block(_) => self.say(
                Tone::Warning,
                "mull moves content blocks but doesn't edit them",
            ),
            Kind::Document => self.say(Tone::Warning, "The document is named after its file"),
            Kind::Heading { .. } => {
                let text = if replace {
                    String::new()
                } else {
                    node.text.clone()
                };
                self.start_typing(&text, false);
            }
        }
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
        let Some(node) = self.tree.get(&self.selected) else {
            return;
        };
        let (kind, text) = typed(&node.kind, editor.text());
        if new_topic && text.is_empty() {
            // A new topic left empty goes, as it would with Esc.
            self.drop_new_topic();
            return;
        }
        if node.kind == kind && node.text == text {
            return;
        }
        let before = self.snapshot();
        if let Some(node) = self.tree.get_mut(&self.selected) {
            node.kind = kind;
            node.text = text;
        }
        match self.tree.check() {
            // A new topic's creation is already recorded; undoing it removes
            // the topic, text and all.
            Ok(()) if new_topic => {}
            Ok(()) => self.record(before),
            Err(problem) => {
                self.restore(before);
                self.say(Tone::Warning, problem.to_string());
            }
        }
    }

    fn cancel(&mut self) {
        if let Mode::Edit {
            new_topic: true, ..
        } = self.mode
        {
            self.drop_new_topic();
        }
        self.mode = Mode::Map;
    }

    /// Take back the topic just added for typing: its creation is the last
    /// change recorded, so undoing it removes the topic and leaves nothing
    /// to redo.
    fn drop_new_topic(&mut self) {
        if let Some(before) = self.undo.pop() {
            self.restore(before);
        }
    }

    // Files

    pub fn save(&mut self) -> io::Result<()> {
        let text = self.markdown();
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
        0 => format!("“{}”", node.label()),
        more => format!("“{}” and {more} more", node.label()),
    }
}

/// The level for a new sub-section of `parent`: one below a section, or
/// the level the document's other top-level sections have.
fn sub_level(parent: &Node) -> u8 {
    match parent.kind {
        Kind::Heading { level } => level.saturating_add(1),
        Kind::Document | Kind::Block(_) => parent
            .children
            .iter()
            .find_map(|c| match c.kind {
                Kind::Heading { level } => Some(level),
                _ => None,
            })
            .unwrap_or(1),
    }
}

/// What typing `typed` into a heading makes: one line, escaped where it
/// would otherwise read back as something else.
fn typed(kind: &Kind, typed: &str) -> (Kind, String) {
    (
        kind.clone(),
        markdown::safe_text(kind, &normalize_text(typed)),
    )
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
    use crate::tree::BlockKind;
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
    fn enter_on_an_empty_new_topic_drops_it() {
        let mut app = app(Node::new("Root"));
        press(&mut app, KeyCode::Tab);
        keys(&mut app, "  ");
        press(&mut app, KeyCode::Enter);
        assert!(app.tree.children.is_empty());
        assert_eq!(app.selected, Vec::<usize>::new());
        assert!(matches!(app.mode, Mode::Map));
        assert!(!app.is_dirty());
        keys(&mut app, "u");
        assert_eq!(
            app.message.as_ref().map(|m| m.text.as_str()),
            Some("Nothing to undo"),
            "dropping leaves no trace in the history"
        );
    }

    #[test]
    fn clearing_an_existing_topic_keeps_it() {
        let mut app = app(Node::with_children(
            "Root",
            vec![Node::new("A").with(vec![Node::new("a1")])],
        ));
        app.selected = vec![0];
        keys(&mut app, "c");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.tree.children[0].text, "");
        assert_eq!(app.tree.children[0].children.len(), 1, "its subtree stays");
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

    fn paragraph(text: &str) -> Node {
        Node::block(BlockKind::Paragraph, text)
    }

    #[test]
    fn blocks_move_but_are_not_edited() {
        let tree = Node::heading(1, "Root").with(vec![
            Node::block(BlockKind::Code, "```\ncode\n```"),
            paragraph("text"),
        ]);
        let mut app = app(tree.clone());
        keys(&mut app, "l");
        app.selected = vec![0];
        keys(&mut app, "e");
        assert!(matches!(app.mode, Mode::Map), "no typing into a block");
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.tree, tree, "blocks hold no sections");
        keys(&mut app, "J");
        assert_eq!(
            app.tree.children[1].kind,
            Kind::Block(BlockKind::Code),
            "but they move"
        );
    }

    #[test]
    fn content_stays_ahead_of_sub_sections() {
        let tree =
            Node::heading(1, "Root").with(vec![paragraph("prose"), Node::heading(2, "Part")]);
        let mut app = app(tree.clone());
        app.selected = vec![0];
        keys(&mut app, "J");
        assert_eq!(app.tree, tree);
        assert_eq!(
            app.message.as_ref().map(|m| m.text.as_str()),
            Some("Content has to come before a section's sub-sections")
        );
        // A copied block pasted into the root lands with its content, before Part.
        keys(&mut app, "yg");
        keys(&mut app, "p");
        assert_eq!(app.selected, vec![1]);
        assert_eq!(app.tree.children[2].text, "Part");
    }

    #[test]
    fn cutting_what_separates_two_lists_leaves_two_lists() {
        let mut app = app(Node::heading(1, "Root").with(vec![
            Node::block(BlockKind::List, "- a"),
            paragraph("between"),
            Node::block(BlockKind::List, "- b"),
        ]));
        app.selected = vec![1];
        keys(&mut app, "d");
        assert_eq!(app.tree.children.len(), 2);
        assert_eq!(app.markdown(), "# Root\n\n- a\n\n<!-- -->\n\n- b\n");
        let reread = markdown::parse(&app.markdown(), "x").root;
        assert_eq!(reread.children, app.tree.children);
    }

    #[test]
    fn tab_and_enter_make_sections_at_the_level_their_place_needs() {
        let mut app = app(Node::heading(1, "Root").with(vec![Node::heading(2, "Part")]));
        app.selected = vec![0];
        press(&mut app, KeyCode::Enter);
        keys(&mut app, "Next");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.tree.children[1], Node::heading(2, "Next"));
        press(&mut app, KeyCode::Tab);
        keys(&mut app, "Detail");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.tree.children[1].children[0], Node::heading(3, "Detail"));
    }

    #[test]
    fn b_shows_only_sections_and_brings_the_selection_up_to_one() {
        let tree = Node::heading(1, "Root").with(vec![
            paragraph("intro"),
            Node::heading(2, "A").with(vec![paragraph("about A")]),
        ]);
        let mut app = app(tree.clone());
        app.selected = vec![1, 0];
        keys(&mut app, "b");
        assert!(!app.shows_blocks());
        assert_eq!(
            app.selected,
            vec![1],
            "off the hidden block, onto its section"
        );
        let shown: Vec<String> = app.layout().nodes.iter().map(|n| n.label.clone()).collect();
        assert_eq!(shown, ["Root", "A"]);
        assert_eq!(app.tree, tree, "hiding is a view, not an edit");
        keys(&mut app, "b");
        assert!(app.shows_blocks());
        assert_eq!(app.layout().nodes.len(), 4);
    }

    #[test]
    fn typed_headings_are_escaped_where_they_would_change_meaning() {
        let mut app = app(Node::heading(1, "Root"));
        press(&mut app, KeyCode::Tab);
        keys(&mut app, "C #");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.tree.children[0].text, "C \\#");
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
