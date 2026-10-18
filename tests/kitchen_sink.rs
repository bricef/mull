//! The kitchen-sink fixture holds every common Markdown construct. Opening
//! and saving it must keep what it renders as, and the constructs that the
//! old line-by-line reader got wrong must come through whole.

use std::fs;

use mull::{
    markdown,
    tree::{BlockKind, Kind, Marker, Node},
};

fn fixture() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/kitchen-sink.md"
    );
    fs::read_to_string(path).expect("the fixture is readable")
}

/// What a document renders as, with whitespace evened out: joining an
/// item's lines or changing blank lines moves whitespace, not meaning.
fn rendered(source: &str) -> String {
    let options = ::markdown::Options {
        parse: markdown::parse_options(),
        compile: ::markdown::CompileOptions {
            allow_dangerous_html: true,
            allow_dangerous_protocol: true,
            ..::markdown::CompileOptions::gfm()
        },
    };
    let html = ::markdown::to_html_with_options(source, &options).expect("renders");
    html.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

fn all_nodes(node: &Node) -> Vec<&Node> {
    let mut out = vec![node];
    for child in &node.children {
        out.extend(all_nodes(child));
    }
    out
}

fn section<'a>(tree: &'a Node, title: &str) -> &'a Node {
    all_nodes(tree)
        .into_iter()
        .find(|n| n.is_heading() && n.text == title)
        .unwrap_or_else(|| panic!("no section {title:?}"))
}

#[test]
fn saving_keeps_what_the_file_renders_as() {
    let original = fixture();
    let saved = markdown::serialize(&markdown::parse(&original, "kitchen-sink"));
    let html = rendered(&original);
    for construct in [
        "<table>",
        "<pre><code class=\"language-rust\">",
        "<blockquote>",
        "data-footnotes",
    ] {
        assert!(html.contains(construct), "the oracle renders {construct}");
    }
    assert_eq!(rendered(&saved), html, "saved as:\n{saved}");
}

#[test]
fn saving_settles_after_one_save() {
    let once = markdown::serialize(&markdown::parse(&fixture(), "kitchen-sink"));
    let twice = markdown::serialize(&markdown::parse(&once, "kitchen-sink"));
    assert_eq!(once, twice);
}

#[test]
fn code_comments_stay_inside_their_code() {
    let tree = markdown::parse(&fixture(), "kitchen-sink");
    let headings: Vec<&str> = all_nodes(&tree)
        .into_iter()
        .filter(|n| n.is_heading())
        .map(|n| n.text.as_str())
        .collect();
    assert!(!headings.contains(&"A shell comment that looks like a heading"));
    assert!(!headings.contains(&"Tilde fences"));
    let code = section(&tree, "Code");
    assert!(
        code.children
            .iter()
            .all(|c| c.kind == Kind::Block(BlockKind::Code))
    );
    assert_eq!(
        code.children.len(),
        6,
        "rust, bash, python, markdown, indented, mermaid"
    );
}

#[test]
fn blocks_come_through_whole() {
    let tree = markdown::parse(&fixture(), "kitchen-sink");
    assert_eq!(
        tree.kind,
        Kind::Document,
        "front matter and two top-level headings"
    );
    assert_eq!(tree.children[0].kind, Kind::Block(BlockKind::FrontMatter));

    let tables = section(&tree, "Tables");
    assert_eq!(tables.children.len(), 1);
    assert_eq!(tables.children[0].kind, Kind::Block(BlockKind::Table));
    assert_eq!(tables.children[0].text.lines().count(), 5);

    let kinds: Vec<&Kind> = all_nodes(&tree).into_iter().map(|n| &n.kind).collect();
    for kind in [
        BlockKind::Definition,
        BlockKind::Footnote,
        BlockKind::Quote,
        BlockKind::Html,
        BlockKind::Maths,
        BlockKind::Rule,
    ] {
        assert!(kinds.contains(&&Kind::Block(kind)), "no {kind:?} block");
    }
}

#[test]
fn ordered_lists_keep_their_numbers_and_delimiters() {
    let tree = markdown::parse(&fixture(), "kitchen-sink");
    let three = all_nodes(&tree)
        .into_iter()
        .find(|n| n.text == "Parenthesis style, starting at three")
        .expect("the item is there");
    assert!(matches!(
        three.kind,
        Kind::Item(style) if style.marker == Marker::Ordered { number: 3, delimiter: ')' }
    ));
    let saved = markdown::serialize(&tree);
    assert!(saved.contains("\n3) Parenthesis style, starting at three\n4) Four\n"));
    assert!(saved.contains("\n[ref]: https://example.com/ref \"Reference title\"\n"));
    assert!(saved.contains("\n- [ ] A task to do\n"));
}
