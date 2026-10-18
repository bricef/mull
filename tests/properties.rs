//! Properties that must hold for any map, not just the unit tests' examples.

use std::{collections::HashSet, path::PathBuf};

use mull::{
    app::{App, Mode},
    layout::{self, Options},
    markdown,
    tree::{BlockKind, ItemStyle, Kind, Marker, Node, normalize_text},
};
use proptest::{prelude::*, sample::Index};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The name a document root reads back with.
const NAME: &str = "doc";

fn raw_text() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => "[a-z]{1,6}( [a-z]{1,6}){0,2}",
        // Characters that could be mistaken for Markdown structure.
        2 => "[ #*+\\-.)0-9a-z\\[\\]`>!_<$|]{1,12}",
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

/// Text as a user would leave it after typing it into a topic of `kind`.
fn typed(kind: Kind) -> impl Strategy<Value = String> {
    raw_text().prop_map(move |t| markdown::safe_text(&kind, &t))
}

/// Blocks that read back as themselves at the top level and inside items.
const BLOCKS: &[(BlockKind, &str)] = &[
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
    (BlockKind::Rule, "***"),
];

fn block() -> impl Strategy<Value = Node> {
    prop_oneof![
        prop::sample::select(BLOCKS).prop_map(|(kind, source)| Node::block(kind, source)),
        "[a-z]{1,8}( [a-z]{1,8}){0,4}(\n[a-z]{1,8}( [a-z]{1,8}){0,3})?"
            .prop_map(|s| Node::block(BlockKind::Paragraph, &s)),
    ]
}

fn marker() -> impl Strategy<Value = Marker> {
    prop_oneof![
        prop::sample::select(vec!['-', '*', '+']).prop_map(Marker::Bullet),
        (0u32..12, prop::sample::select(vec!['.', ')']))
            .prop_map(|(number, delimiter)| Marker::Ordered { number, delimiter }),
    ]
}

/// One list: items sharing a marker, each perhaps a task, perhaps holding
/// more.
fn list(depth: u32) -> BoxedStrategy<Vec<Node>> {
    let item = (
        raw_text(),
        "[a-z]{1,6}",
        prop::option::of(any::<bool>()),
        content(depth),
    );
    (marker(), any::<bool>(), prop::collection::vec(item, 1..4))
        .prop_map(|(marker, loose, items)| {
            items
                .into_iter()
                .map(|(text, task_text, checked, children)| {
                    let style = ItemStyle {
                        marker,
                        checked,
                        loose,
                    };
                    // A task's text follows its box, so keep it plain.
                    let text = match checked {
                        Some(_) => task_text,
                        None => markdown::safe_text(&Kind::Item(style), &text),
                    };
                    Node::item(style, &text).with(children)
                })
                .collect()
        })
        .boxed()
}

/// What an item or a section holds before any sub-sections: lists and
/// blocks.
fn content(depth: u32) -> BoxedStrategy<Vec<Node>> {
    if depth == 0 {
        return Just(Vec::new()).boxed();
    }
    let part = prop_oneof![block().prop_map(|b| vec![b]), list(depth - 1)];
    prop::collection::vec(part, 0..3)
        .prop_map(|parts| parts.concat())
        .boxed()
}

fn section(level: u8, depth: u32) -> BoxedStrategy<Node> {
    let subsections = if level < 6 && depth > 0 {
        prop::collection::vec(section(level + 1, depth - 1), 0..3).boxed()
    } else {
        Just(Vec::new()).boxed()
    };
    (typed(Kind::Heading { level }), content(depth), subsections)
        .prop_map(move |(text, mut children, subsections)| {
            children.extend(subsections);
            Node::heading(level, &text).with(children)
        })
        .boxed()
}

/// A well-formed map, as markdown::parse would give it.
fn map() -> impl Strategy<Value = Node> {
    let document = (
        any::<bool>(),
        content(2),
        prop::collection::vec(section(1, 2), 0..3),
    )
        .prop_filter_map(
            "a document that reads back as a document",
            |(front, content, sections)| {
                let mut children = Vec::new();
                if front {
                    children.push(Node::block(BlockKind::FrontMatter, "---\ntitle: x\n---"));
                }
                children.extend(content);
                children.extend(sections);
                let one_heading = children.len() == 1 && children[0].is_heading();
                (!children.is_empty() && !one_heading).then(|| Node::document(NAME).with(children))
            },
        );
    prop_oneof![section(1, 3), document].prop_map(|mut tree| {
        tidy(&mut tree);
        tree
    })
}

/// Make a tree's lists look the way they read back: one list per run of
/// items with the same marker, numbered from its first item, loose all
/// through or not at all.
fn tidy(node: &mut Node) {
    let mut i = 0;
    while i < node.children.len() {
        let Kind::Item(first) = node.children[i].kind else {
            i += 1;
            continue;
        };
        let end = node.children[i..]
            .iter()
            .position(|n| !matches!(n.kind, Kind::Item(s) if s.marker.same_list(first.marker)))
            .map_or(node.children.len(), |n| i + n);
        let run = &node.children[i..end];
        let loose = (run
            .iter()
            .any(|n| matches!(n.kind, Kind::Item(s) if s.loose))
            || run.iter().any(markdown::needs_gap))
            && (run.len() > 1 || run.iter().any(|n| !n.children.is_empty()));
        for (n, item) in node.children[i..end].iter_mut().enumerate() {
            if let Kind::Item(style) = &mut item.kind {
                style.loose = loose;
                if let (Marker::Ordered { number, .. }, Marker::Ordered { number: start, .. }) =
                    (&mut style.marker, first.marker)
                {
                    *number = start + n as u32;
                }
            }
        }
        i = end;
    }
    for child in &mut node.children {
        tidy(child);
    }
}

/// The tree with every list marked tight, so that only what the writer
/// itself requires makes a list loose.
fn tightened(node: &Node) -> Node {
    let mut node = node.clone();
    if let Kind::Item(style) = &mut node.kind {
        style.loose = false;
    }
    node.children = node.children.iter().map(tightened).collect();
    node
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
    let mut keys: Vec<KeyEvent> = "hjklgecdyp JKHLuU?x漢#-[] 1.".chars().map(plain).collect();
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
    fn saving_then_reading_gives_back_the_same_map(tree in map()) {
        let text = markdown::serialize(&tree);
        prop_assert_eq!(markdown::parse(&text, NAME), tree, "written as:\n{}", text);
    }

    #[test]
    fn typed_text_always_reads_back(text in raw_text(), heading in any::<bool>()) {
        let kind = if heading { Kind::Heading { level: 1 } } else { Kind::default() };
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
        // A file with no words at all may gain the map's name as a title.
        if input.chars().any(char::is_alphabetic) {
            prop_assert_eq!(words(&input), words(&saved), "saved as:\n{}", saved);
        }
    }

    #[test]
    fn layout_shows_each_visible_topic_once_without_overlaps(
        mut tree in map(),
        folds in prop::collection::vec(any::<bool>(), 0..64),
        max_width in 2usize..30,
    ) {
        let mut folds = folds.into_iter();
        for path in all_paths(&tree.clone(), false) {
            if let Some(node) = tree.get_mut(&path) {
                node.collapsed = folds.next().unwrap_or(false);
            }
        }
        let map = layout::layout(&tree, &Options { max_text_width: max_width }, None);

        let placed: Vec<Vec<usize>> = map.nodes.iter().map(|n| n.path.clone()).collect();
        prop_assert_eq!(placed, all_paths(&tree, true));

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
        mut tree in map(),
        moves in prop::collection::vec((0..4usize, any::<Index>()), 0..30),
    ) {
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
        let mut app = App::new(start.clone(), PathBuf::from("unused.md"));
        for key in keys {
            app.handle_key(key);
            prop_assert!(app.tree.is_visible(&app.selected), "selection {:?} isn't on screen", app.selected);
            prop_assert_eq!(app.tree.check(), Ok(()), "{:#?}", app.tree);
            let mut nodes = Vec::new();
            preorder(&app.tree, &mut nodes);
            prop_assert!(nodes.iter().filter(|n| !n.is_block()).all(|n| n.text == normalize_text(&n.text)));
            prop_assert!(!app.should_quit);
        }
        // Leave whatever mode the keys ended in.
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        prop_assert!(matches!(app.mode, Mode::Map));

        // What would be saved reads back as the same map, then settles. Only
        // list looseness may change on the way: markdown-rs marks an item
        // loose when a sub-list ends in an empty item and a blank line.
        let saved = markdown::serialize(&app.tree);
        let reloaded = markdown::parse(&saved, NAME);
        prop_assert_eq!(markdown::serialize(&tightened(&reloaded)), markdown::serialize(&tightened(&app.tree)));
        let resaved = markdown::serialize(&reloaded);
        prop_assert_eq!(markdown::serialize(&markdown::parse(&resaved, NAME)), resaved);

        // The session may itself end on an undo, so go forward to the
        // newest state first; redo-everything must come back to exactly it.
        for _ in 0..200 {
            app.handle_key(plain('U'));
        }
        let newest = markdown::serialize(&app.tree);
        for _ in 0..200 {
            app.handle_key(plain('u'));
        }
        prop_assert_eq!(markdown::serialize(&app.tree), markdown::serialize(&start));
        prop_assert!(!app.is_dirty());
        for _ in 0..200 {
            app.handle_key(plain('U'));
        }
        prop_assert_eq!(markdown::serialize(&app.tree), newest);
    }
}
