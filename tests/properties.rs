//! Properties that must hold for any map, not just the unit tests' examples.

use std::{collections::HashSet, path::PathBuf};

use mull::{
    app::{App, Mode},
    layout::{self, Options},
    markdown::{self, Map},
    tree::{BlockKind, Kind, Node, normalize_text},
};
use proptest::{prelude::*, sample::Index};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The name a document root reads back with.
const NAME: &str = "doc";

fn raw_text() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => "[a-z]{1,6}( [a-z]{1,6}){0,2}",
        // Characters that could be mistaken for Markdown structure.
        2 => "[ #*+\\-.)0-9a-z\\[\\]`>!_<$|=]{1,12}",
        1 => "\\PC{1,8}",
    ]
    .prop_map(|s| {
        let text = normalize_text(&s);
        if text.is_empty() {
            "x".to_string()
        } else {
            text
        }
    })
}

/// A heading as a user would leave it after typing it.
fn heading_text(level: u8) -> impl Strategy<Value = String> {
    raw_text().prop_map(move |t| markdown::safe_text(&Kind::Heading { level }, &t))
}

/// Blocks that read back as themselves.
const BLOCKS: &[(BlockKind, &str)] = &[
    (BlockKind::List, "- a\n- b\n  - nested"),
    (BlockKind::List, "1. one\n2. two"),
    (BlockKind::List, "* [ ] task\n* [x] done"),
    (BlockKind::List, "3) three\n\n4) four, loose"),
    (
        BlockKind::Code,
        "```rust\nfn main() {\n    println!(\"hi\");\n}\n```",
    ),
    (BlockKind::Code, "~~~\n# not a heading\n- not an item\n~~~"),
    (BlockKind::Table, "| a | b |\n|---|---|\n| 1 | 2 |"),
    (BlockKind::Quote, "> quoted\n> > nested"),
    (BlockKind::Html, "<div>\n<b>raw</b>\n</div>"),
    (BlockKind::Maths, "$$\nx^2\n$$"),
    (BlockKind::Definition, "[ref]: https://example.com"),
    (BlockKind::Footnote, "[^n]: A footnote."),
    (BlockKind::Rule, "***"),
];

fn block() -> impl Strategy<Value = Node> {
    prop_oneof![
        prop::sample::select(BLOCKS).prop_map(|(kind, source)| Node::block(kind, source)),
        "[a-z]{1,8}( [a-z]{1,8}){0,4}(\n[a-z]{1,8}( [a-z]{1,8}){0,3})?"
            .prop_map(|s| Node::block(BlockKind::Paragraph, &s)),
    ]
}

/// A section's blocks. Two lists in a row that would read back as one list
/// aren't a map anyone can make, so the second goes.
fn blocks() -> impl Strategy<Value = Vec<Node>> {
    prop::collection::vec(block(), 0..4).prop_map(|blocks| {
        let mut kept: Vec<Node> = Vec::new();
        for block in blocks {
            let merges = kept.last().is_some_and(|last| {
                let marker = |n: &Node| n.text.chars().find(|c| !c.is_ascii_digit());
                last.kind == Kind::Block(BlockKind::List)
                    && block.kind == Kind::Block(BlockKind::List)
                    && marker(last) == marker(&block)
            });
            if !merges {
                kept.push(block);
            }
        }
        kept
    })
}

fn section(level: u8, depth: u32) -> BoxedStrategy<Node> {
    let subsections = if level < 6 && depth > 0 {
        prop::collection::vec(section(level + 1, depth - 1), 0..3).boxed()
    } else {
        Just(Vec::new()).boxed()
    };
    (heading_text(level), blocks(), subsections)
        .prop_map(move |(text, mut children, subsections)| {
            children.extend(subsections);
            Node::heading(level, &text).with(children)
        })
        .boxed()
}

/// A well-formed map, as markdown::parse would give it.
fn map() -> impl Strategy<Value = Map> {
    let document = (blocks(), prop::collection::vec(section(1, 2), 0..3)).prop_filter_map(
        "a document that reads back as a document",
        |(blocks, sections)| {
            let mut children = blocks;
            children.extend(sections);
            let one_heading = children.len() == 1 && children[0].is_heading();
            (!one_heading).then(|| Node::document(NAME).with(children))
        },
    );
    let front_matter = prop::option::of(Just("---\ntitle: x\n---".to_string()));
    (prop_oneof![section(1, 3), document], front_matter)
        .prop_map(|(root, front_matter)| Map { front_matter, root })
}

/// Text shaped like Markdown: headings, lists, fences, quotes, tables,
/// indentation, and anything at all.
fn document() -> impl Strategy<Value = String> {
    let line = prop_oneof![
        "[ \t]{0,6}([-*+>]|#{1,7}|[0-9]{1,2}[.)]|```|~~~|\\|)?[ \t]?[a-z#é |-]{0,8}",
        Just("---".to_string()),
        Just(String::new()),
        any::<String>(),
    ];
    prop::collection::vec(line, 0..20).prop_map(|lines| lines.join("\n"))
}

/// The words of a text, sorted: whatever moves or changes markup, these
/// stay.
fn words(text: &str) -> Vec<String> {
    let mut words: Vec<String> = text
        .split_whitespace()
        .filter(|w| w.chars().any(char::is_alphabetic))
        .map(str::to_lowercase)
        .collect();
    words.sort();
    words
}

fn preorder<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    out.push(node);
    for child in &node.children {
        preorder(child, out);
    }
}

fn paths(node: &Node, path: &mut Vec<usize>, visible_only: bool, out: &mut Vec<Vec<usize>>) {
    out.push(path.clone());
    if visible_only && node.collapsed {
        return;
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        paths(child, path, visible_only, out);
        path.pop();
    }
}

fn all_paths(node: &Node, visible_only: bool) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    paths(node, &mut Vec::new(), visible_only, &mut out);
    out
}

fn sorted_texts(node: &Node) -> Vec<String> {
    let mut nodes = Vec::new();
    preorder(node, &mut nodes);
    let mut texts: Vec<String> = nodes.iter().map(|n| n.text.clone()).collect();
    texts.sort();
    texts
}

fn plain(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

/// Every key except the ones that save or quit, and some text that needs
/// escaping.
fn key() -> impl Strategy<Value = KeyEvent> {
    let mut keys: Vec<KeyEvent> = "hjklgecdyp JKHLuU?bx漢# ".chars().map(plain).collect();
    keys.extend([
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
        KeyEvent::from(KeyCode::Tab),
        KeyEvent::from(KeyCode::Enter),
        KeyEvent::from(KeyCode::Esc),
        KeyEvent::from(KeyCode::Backspace),
        KeyEvent::from(KeyCode::Delete),
        KeyEvent::from(KeyCode::Left),
        KeyEvent::from(KeyCode::Home),
    ]);
    prop::sample::select(keys)
}

proptest! {
    #[test]
    fn saving_then_reading_gives_back_the_same_map(map in map()) {
        let text = markdown::serialize(&map);
        prop_assert_eq!(markdown::parse(&text, NAME), map, "written as:\n{}", text);
    }

    #[test]
    fn generated_maps_follow_the_rules(map in map()) {
        prop_assert_eq!(map.root.check(), Ok(()));
    }

    #[test]
    fn typed_headings_always_read_back(text in raw_text()) {
        let kind = Kind::Heading { level: 1 };
        let safe = markdown::safe_text(&kind, &text);
        prop_assert!(markdown::reads_back(&kind, &safe), "{:?} became {:?}", text, safe);
        prop_assert_eq!(markdown::safe_text(&kind, &safe), safe.clone(), "escaping twice changes it");
    }

    #[test]
    fn any_document_settles_after_one_save(input in document()) {
        let once = markdown::serialize(&markdown::parse(&input, NAME));
        let twice = markdown::serialize(&markdown::parse(&once, NAME));
        prop_assert_eq!(once, twice);
    }

    #[test]
    fn saving_neither_loses_nor_invents_words(input in document()) {
        let saved = markdown::serialize(&markdown::parse(&input, NAME));
        prop_assert_eq!(words(&input), words(&saved), "saved as:\n{}", saved);
    }

    #[test]
    fn layout_shows_each_visible_node_once_without_overlaps(
        map in map(),
        folds in prop::collection::vec(any::<bool>(), 0..64),
        max_width in 2usize..30,
        show_blocks in any::<bool>(),
    ) {
        let mut tree = map.root;
        let mut folds = folds.into_iter();
        for path in all_paths(&tree.clone(), false) {
            if let Some(node) = tree.get_mut(&path) {
                node.collapsed = folds.next().unwrap_or(false);
            }
        }
        let options = Options { max_text_width: max_width, show_blocks };
        let map = layout::layout(&tree, &options, None);

        let placed: Vec<Vec<usize>> = map.nodes.iter().map(|n| n.path.clone()).collect();
        let expected: Vec<Vec<usize>> = all_paths(&tree, true)
            .into_iter()
            .filter(|p| tree.is_shown(p, show_blocks))
            .collect();
        prop_assert_eq!(placed, expected);

        let mut cells = HashSet::new();
        for node in &map.nodes {
            prop_assert!(node.label_width() <= max_width);
            for x in node.x..node.x + node.width() {
                prop_assert!(cells.insert((x, node.y)), "{:?} overlaps at {x},{}", node.path, node.y);
            }
        }
        for c in &map.connectors {
            prop_assert!(cells.insert((c.x, c.y)), "connector overlaps at {},{}", c.x, c.y);
        }
        prop_assert!(cells.iter().all(|&(x, y)| x < map.width && y < map.height));

        let mut leaves = 0;
        for node in &map.nodes {
            let children: Vec<_> = map
                .nodes
                .iter()
                .filter(|c| c.path.len() == node.path.len() + 1 && c.path.starts_with(&node.path))
                .collect();
            let (Some(top), Some(bottom)) = (children.iter().map(|c| c.y).min(), children.iter().map(|c| c.y).max()) else {
                leaves += 1;
                continue;
            };
            prop_assert!(top <= node.y && node.y <= bottom, "{:?} isn't beside its children", node.path);
            prop_assert!(children.iter().all(|c| c.x == node.x + node.label_width() + 5));
        }
        prop_assert_eq!(map.height, leaves, "one row per visible leaf");
    }

    #[test]
    fn restructuring_moves_whole_subtrees_and_loses_nothing(
        map in map(),
        moves in prop::collection::vec((0..4usize, any::<Index>()), 0..30),
    ) {
        let mut tree = map.root;
        let texts = sorted_texts(&tree);
        for (op, pick) in moves {
            let paths = all_paths(&tree, false);
            let path = pick.get(&paths).clone();
            let before = tree.clone();
            let subtree = tree.get(&path).cloned();
            let moved = match op {
                0 => tree.move_up(&path),
                1 => tree.move_down(&path),
                2 => tree.indent(&path),
                _ => tree.outdent(&path),
            };
            match moved {
                Some(new_path) => prop_assert_eq!(tree.get(&new_path).cloned(), subtree),
                None => prop_assert_eq!(&tree, &before),
            }
        }
        prop_assert_eq!(sorted_texts(&tree), texts);
    }

    #[test]
    fn random_sessions_keep_the_map_writable_and_undo_completely(
        start in map(),
        keys in prop::collection::vec(key(), 0..80),
    ) {
        let mut app = App::from_map(start.clone(), PathBuf::from("unused.md"));
        for key in keys {
            app.handle_key(key);
            prop_assert!(
                app.tree.is_shown(&app.selected, app.shows_blocks()),
                "selection {:?} isn't on screen", app.selected
            );
            prop_assert_eq!(app.tree.check(), Ok(()), "{:#?}", app.tree);
            let mut nodes = Vec::new();
            preorder(&app.tree, &mut nodes);
            prop_assert!(nodes.iter().filter(|n| !n.is_block()).all(|n| n.text == normalize_text(&n.text)));
            prop_assert!(!app.should_quit);
        }
        // Leave whatever mode the keys ended in.
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        prop_assert!(matches!(app.mode, Mode::Map));

        // What would be saved reads back as itself.
        let saved = app.markdown();
        prop_assert_eq!(markdown::serialize(&markdown::parse(&saved, NAME)), saved.clone());
        prop_assert_eq!(markdown::parse(&saved, NAME).front_matter, start.front_matter.clone());

        // The session may itself end on an undo, so go forward to the
        // newest state first; redo-everything must come back to exactly it.
        for _ in 0..200 {
            app.handle_key(plain('U'));
        }
        let newest = app.markdown();
        for _ in 0..200 {
            app.handle_key(plain('u'));
        }
        prop_assert_eq!(app.markdown(), markdown::serialize(&start));
        prop_assert!(!app.is_dirty());
        for _ in 0..200 {
            app.handle_key(plain('U'));
        }
        prop_assert_eq!(app.markdown(), newest);
    }
}
