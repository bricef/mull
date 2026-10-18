//! Drawing the screen: the map, a status line, and the key hints under it.
//! Every style comes from the [`Theme`].

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Position, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    app::{App, Mode},
    keymap::{self, Context, Group, Hint},
    layout::Layout,
    theme::Theme,
    tree::Node,
};

/// The hint bar grows to this many lines on narrow terminals, then drops
/// hints instead.
const MAX_HINT_LINES: usize = 3;

pub fn draw(frame: &mut Frame, app: &mut App, theme: &Theme) {
    let area = frame.area();
    let hints = hint_bar(app, theme, area.width as usize);
    let [map_area, status_area, hint_area] = ratatui::layout::Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(hints.len() as u16),
    ])
    .areas(area);

    let map = app.layout();
    follow_selection(app, &map, map_area);
    frame.render_widget(Block::new().style(theme.canvas), map_area);
    draw_map(frame.buffer_mut(), map_area, app, &map, theme);
    if let Some(cursor) = edit_cursor(app, &map, map_area) {
        frame.set_cursor_position(cursor);
    }
    let status = status_line(app, theme, status_area.width as usize);
    frame.render_widget(Paragraph::new(status).style(theme.bar), status_area);
    frame.render_widget(Paragraph::new(hints).style(theme.bar), hint_area);
    if let Mode::Help { .. } = app.mode {
        // Over everything but the hints, which say how to scroll and close.
        draw_help(frame, map_area.union(status_area), app, theme);
    }
}

/// Which bindings the keys follow in `mode`.
fn context(mode: &Mode) -> Context {
    match mode {
        Mode::Map => Context::Map,
        Mode::Edit { .. } => Context::Typing,
        Mode::Help { .. } => Context::Help,
    }
}

// The map

/// Move the viewport so the selection is on screen. A map that fits is
/// centred vertically with a column of margin on the left.
fn follow_selection(app: &mut App, map: &Layout, area: Rect) {
    let Some(node) = map.node(&app.selected) else {
        return;
    };
    let (view_width, view_height) = (area.width as isize, area.height as isize);
    let (map_width, map_height) = (map.width as isize, map.height as isize);
    let (x, y) = (node.x as isize, node.y as isize);
    // One column past the text, where the cursor sits while typing.
    let right = x + node.width() as isize + 1;
    let (view_x, view_y) = app.viewport;
    app.viewport = (
        if map_width + 2 <= view_width {
            -1
        } else {
            reveal(view_x, view_width, x, right, 4).clamp(-1, (map_width + 1 - view_width).max(-1))
        },
        if map_height <= view_height {
            -(view_height - map_height) / 2
        } else {
            reveal(view_y, view_height, y, y + 1, 2).clamp(0, (map_height - view_height).max(0))
        },
    );
}

/// Shift a view of `len` cells starting at `start` as little as possible to
/// show `from..to`, with `margin` cells to spare where there's room. When the
/// span is longer than the view, its start wins.
fn reveal(start: isize, len: isize, from: isize, to: isize, margin: isize) -> isize {
    let margin = margin.min((len - (to - from)) / 2).max(0);
    let mut start = start;
    if to + margin > start + len {
        start = to + margin - len;
    }
    if from - margin < start {
        start = from - margin;
    }
    start
}

/// Each first-level topic and everything under it share an accent: its
/// lines, its fold markers, and its highlight when selected. The root and
/// first-level topics stand out in bold; deeper topics are plain text.
fn draw_map(buf: &mut Buffer, area: Rect, app: &App, map: &Layout, theme: &Theme) {
    let editing = matches!(app.mode, Mode::Edit { .. });
    let view = app.viewport;
    for c in &map.connectors {
        let style = theme.branch(c.branch).line;
        put(
            buf,
            area,
            view,
            c.x,
            c.y,
            c.ch.encode_utf8(&mut [0; 4]),
            style,
        );
    }
    for node in &map.nodes {
        let branch = theme.branch(node.path.first().copied());
        let is_block = app.tree.get(&node.path).is_some_and(Node::is_block);
        let style = if node.path != app.selected {
            if is_block {
                theme.block
            } else if node.path.len() <= 1 {
                branch.label
            } else {
                theme.topic
            }
        } else if editing {
            theme.editing
        } else {
            branch.selected
        };
        put(buf, area, view, node.x, node.y, &node.label, style);
        if let Some(marker) = &node.note {
            let x = node.x + node.label_width() + 1;
            put(buf, area, view, x, node.y, marker, branch.line);
        }
    }
}

/// Draw `text` at map position `(x, y)`, clipped to `area`.
fn put(
    buf: &mut Buffer,
    area: Rect,
    view: (isize, isize),
    x: usize,
    y: usize,
    text: &str,
    style: Style,
) {
    let row = y as isize - view.1;
    if !(0..area.height as isize).contains(&row) {
        return;
    }
    let mut column = x as isize - view.0;
    for c in text.chars() {
        let width = c.width().unwrap_or(0) as isize;
        if width > 0 && column >= 0 && column + width <= area.width as isize {
            buf.set_stringn(
                area.x + column as u16,
                area.y + row as u16,
                c.encode_utf8(&mut [0; 4]),
                width as usize,
                style,
            );
        }
        column += width;
    }
}

fn edit_cursor(app: &App, map: &Layout, area: Rect) -> Option<Position> {
    let Mode::Edit { editor, .. } = &app.mode else {
        return None;
    };
    let node = map.node(&app.selected)?;
    let x = (node.x + editor.cursor_column()) as isize - app.viewport.0;
    let y = node.y as isize - app.viewport.1;
    let on_screen = (0..area.width as isize).contains(&x) && (0..area.height as isize).contains(&y);
    on_screen.then(|| Position::new(area.x + x as u16, area.y + y as u16))
}

// The status line

/// The mode, the file (marked when unsaved), then the latest message or,
/// failing that, the path from the root to the selection.
fn status_line(app: &App, theme: &Theme, width: usize) -> Line<'static> {
    let mode = match app.mode {
        Mode::Map => " MAP ",
        Mode::Edit { .. } => " EDIT ",
        Mode::Help { .. } => " HELP ",
    };
    let name = app.file.file_name().map_or_else(
        || app.file.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let mut spans = vec![
        Span::styled(mode, theme.mode(context(&app.mode)).badge),
        Span::styled(format!(" {name} "), theme.file),
    ];
    if app.is_dirty() {
        spans.push(Span::styled("* ", theme.dirty));
    }
    spans.push(Span::raw(" "));
    let room = width.saturating_sub(spans.iter().map(Span::width).sum());
    match &app.message {
        Some(message) => spans.push(Span::styled(message.text.clone(), theme.tone(message.tone))),
        None => spans.extend(breadcrumb(&keep_end(&app.breadcrumb(), room), theme)),
    }
    Line::from(spans)
}

/// A breadcrumb in colour: dim ancestors and separators, then the selected
/// topic.
fn breadcrumb(text: &str, theme: &Theme) -> Vec<Span<'static>> {
    let parts: Vec<&str> = text.split(" › ").collect();
    let last = parts.len() - 1;
    let mut spans = Vec::new();
    for (i, part) in parts.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" › ", theme.crumb_separator));
        }
        let style = if i == last {
            theme.crumb_current
        } else {
            theme.crumb
        };
        spans.push(Span::styled(part.to_string(), style));
    }
    spans
}

/// The end of `text` that fits in `width` columns, after a `…` if cut.
fn keep_end(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let mut kept = Vec::new();
    let mut used = 1;
    for c in text.chars().rev() {
        let w = c.width().unwrap_or(0);
        if used + w > width {
            break;
        }
        used += w;
        kept.push(c);
    }
    std::iter::once('…').chain(kept.into_iter().rev()).collect()
}

// Key hints

/// The keys for the current mode, in the mode's colour.
fn hint_bar(app: &App, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    let context = context(&app.mode);
    let key_style = theme.mode(context).key;
    let items: Vec<(String, &str)> = keymap::hints(context).map(entry).collect();
    let widths: Vec<usize> = items
        .iter()
        .map(|(k, l)| k.width() + 1 + l.width())
        .collect();
    pack(&widths, width, MAX_HINT_LINES)
        .into_iter()
        .map(|line| {
            let mut spans = vec![Span::raw(" ")];
            for (n, i) in line.into_iter().enumerate() {
                if n > 0 {
                    spans.push(Span::raw("  "));
                }
                let (keys, label) = &items[i];
                spans.push(Span::styled(keys.clone(), key_style));
                spans.push(Span::styled(format!(" {label}"), theme.hint_label));
            }
            Line::from(spans)
        })
        .collect()
}

fn entry(hint: &Hint) -> (String, &'static str) {
    (hint.keys(), hint.label)
}

/// Arrange items of the given widths into at most `max_lines` lines of
/// `width` columns, as indices into `widths`. The last item always stays;
/// the ones before it drop off the end until the rest fit.
fn pack(widths: &[usize], width: usize, max_lines: usize) -> Vec<Vec<usize>> {
    let Some(last) = widths.len().checked_sub(1) else {
        return Vec::new();
    };
    for keep in (0..=last).rev() {
        let chosen: Vec<usize> = (0..keep).chain([last]).collect();
        let lines = flow(&chosen, widths, width);
        if lines.len() <= max_lines {
            return lines;
        }
    }
    vec![vec![last]]
}

/// Fill lines greedily: a leading space, then items two spaces apart.
fn flow(chosen: &[usize], widths: &[usize], width: usize) -> Vec<Vec<usize>> {
    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut used = 0;
    for &i in chosen {
        match lines.last_mut() {
            Some(line) if used + 2 + widths[i] <= width => {
                line.push(i);
                used += 2 + widths[i];
            }
            _ => {
                lines.push(vec![i]);
                used = 1 + widths[i];
            }
        }
    }
    lines
}

// The help screen

/// Space after each help column.
const GUTTER: usize = 4;

/// Every binding, grouped, in as many columns as fit across the screen. A
/// page taller than the screen scrolls.
fn draw_help(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let inner = Block::bordered().inner(area);
    let columns = help_columns(inner.width.saturating_sub(1) as usize, theme);
    let page_height = columns.iter().map(Vec::len).max().unwrap_or(0);
    let max_scroll = page_height.saturating_sub(inner.height as usize);
    let scroll = match &mut app.mode {
        Mode::Help { scroll } => {
            *scroll = (*scroll).min(max_scroll);
            *scroll
        }
        _ => 0,
    };

    let mut block = Block::bordered()
        .style(theme.help_text)
        .border_style(theme.help_border)
        .title(Span::styled(" All keys ", theme.help_title));
    if scroll < max_scroll {
        block = block.title_bottom(Span::styled(" more below ", theme.help_more));
    }
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    let mut x = inner.x + 1;
    for column in columns {
        if x >= inner.right() {
            break;
        }
        let width = (column.iter().map(Line::width).max().unwrap_or(0) + GUTTER) as u16;
        let rect = Rect {
            x,
            width: width.min(inner.right() - x),
            ..inner
        };
        frame.render_widget(Paragraph::new(column).scroll((scroll as u16, 0)), rect);
        x = x.saturating_add(width);
    }
}

/// The help sections in the fewest, shortest columns that fit `width`.
fn help_columns(width: usize, theme: &Theme) -> Vec<Vec<Line<'static>>> {
    let sections: Vec<Vec<Line<'static>>> = Group::ALL
        .into_iter()
        .map(|group| help_section(group, theme))
        .collect();
    let column_width = sections
        .iter()
        .flatten()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        + GUTTER;
    let max_columns = (width / column_width).max(1);
    let lines = sections.iter().map(|s| s.len() + 1).sum::<usize>() - 1;
    (lines.div_ceil(max_columns)..=lines)
        .map(|height| stack(&sections, height))
        .find(|columns| columns.len() <= max_columns)
        .unwrap_or_else(|| stack(&sections, usize::MAX))
}

/// A group's title and bindings, keys in the group's accent.
fn help_section(group: Group, theme: &Theme) -> Vec<Line<'static>> {
    let key_width = keymap::BINDINGS
        .iter()
        .map(|b| b.keys_label().width())
        .max()
        .unwrap_or(0);
    let accent = theme.group(group);
    let mut section = vec![Line::styled(group.title(), accent)];
    for binding in keymap::BINDINGS.iter().filter(|b| b.group == group) {
        let keys = binding.keys_label();
        let pad = " ".repeat(key_width - keys.width());
        section.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(keys, accent),
            Span::styled(format!("{pad}  {}", binding.description), theme.help_text),
        ]));
    }
    section
}

/// Stack whole sections into columns of at most `height` lines, with a
/// blank line between sections. A section taller than `height` gets a
/// column of its own.
fn stack(sections: &[Vec<Line<'static>>], height: usize) -> Vec<Vec<Line<'static>>> {
    let mut columns = Vec::new();
    let mut column: Vec<Line<'static>> = Vec::new();
    for section in sections {
        if !column.is_empty() && column.len() + 1 + section.len() > height {
            columns.push(std::mem::take(&mut column));
        }
        if !column.is_empty() {
            column.push(Line::default());
        }
        column.extend(section.iter().cloned());
    }
    if !column.is_empty() {
        columns.push(column);
    }
    columns
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::Tone,
        theme::{ColorDepth, Theme},
        tree::Node,
    };
    use ratatui::{
        Terminal,
        backend::TestBackend,
        style::{Color, Modifier},
    };
    use std::path::PathBuf;

    fn render(app: &mut App, theme: &Theme, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, app, theme)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn screen(app: &mut App, width: u16, height: u16) -> Vec<String> {
        let buffer = render(app, &Theme::mocha(ColorDepth::TrueColor), width, height);
        (0..height)
            .map(|y| {
                let row: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
                row.trim_end().to_string()
            })
            .collect()
    }

    /// Every cell of the screen.
    fn cells(buffer: &Buffer) -> impl Iterator<Item = &ratatui::buffer::Cell> {
        buffer.content.iter()
    }

    fn sample() -> App {
        let tree =
            Node::with_children("Root", vec![Node::new("A"), Node::new("B"), Node::new("C")]);
        App::new(tree, PathBuf::from("ideas.md"))
    }

    #[test]
    fn the_screen_shows_the_map_the_file_and_the_keys() {
        let mut app = sample();
        let rows = screen(&mut app, 100, 10);
        // A map that fits is centred vertically, one column in from the left.
        assert_eq!(rows[3], " Root ─┼─ B", "{rows:#?}");
        assert_eq!(rows[7], " MAP  ideas.md  Root", "{rows:#?}");
        let hints = rows[8..].join("\n");
        for expected in [
            "hjkl move",
            "Tab child",
            "Enter sibling",
            "q quit",
            "? all keys",
        ] {
            assert!(
                hints.contains(expected),
                "{expected:?} missing from\n{hints}"
            );
        }
    }

    #[test]
    fn typing_shows_the_typing_keys() {
        let mut app = sample();
        app.handle_key(ratatui::crossterm::event::KeyEvent::from(
            ratatui::crossterm::event::KeyCode::Tab,
        ));
        let rows = screen(&mut app, 100, 10);
        assert!(rows[8].starts_with(" EDIT  ideas.md * "), "{rows:#?}");
        let hints = &rows[9];
        assert!(hints.contains("Enter done"), "{hints}");
        assert!(hints.contains("Esc cancel"), "{hints}");
    }

    #[test]
    fn the_help_screen_lists_every_binding() {
        let mut app = sample();
        app.mode = Mode::Help { scroll: 0 };
        let rows = screen(&mut app, 100, 30).join("\n");
        assert!(
            rows.ends_with(" jk scroll  Esc close"),
            "hints stay visible"
        );
        for binding in keymap::BINDINGS {
            assert!(
                rows.contains(binding.description),
                "{:?} missing",
                binding.description
            );
        }
    }

    #[test]
    fn a_short_help_screen_scrolls_to_every_binding() {
        let mut app = sample();
        app.mode = Mode::Help { scroll: usize::MAX };
        screen(&mut app, 90, 16);
        let Mode::Help { scroll: max } = app.mode else {
            panic!("help closed");
        };
        assert!(max > 0, "a 16-row terminal can't show every key at once");
        let mut seen = String::new();
        for scroll in 0..=max {
            app.mode = Mode::Help { scroll };
            seen += &screen(&mut app, 90, 16).join("\n");
        }
        for binding in keymap::BINDINGS {
            assert!(
                seen.contains(binding.description),
                "{:?} unreachable",
                binding.description
            );
        }
    }

    #[test]
    fn narrow_terminals_keep_the_last_hint() {
        // A line is a leading space, then items two spaces apart.
        let widths = [10, 10, 10, 10, 6];
        assert_eq!(pack(&widths, 31, 1), vec![vec![0, 1, 4]]);
        assert_eq!(pack(&widths, 30, 1), vec![vec![0, 4]]);
        assert_eq!(pack(&widths, 30, 3), vec![vec![0, 1], vec![2, 3], vec![4]]);
    }

    #[test]
    fn reveal_moves_the_view_as_little_as_it_can() {
        assert_eq!(reveal(0, 10, 3, 4, 2), 0);
        assert_eq!(reveal(0, 10, 12, 13, 2), 5);
        assert_eq!(reveal(20, 10, 3, 4, 2), 1);
    }

    #[test]
    fn long_breadcrumbs_keep_their_end() {
        assert_eq!(keep_end("Root › A › A1", 6), "… › A1");
    }

    #[test]
    fn each_branch_draws_in_its_own_accent() {
        let theme = Theme::mocha(ColorDepth::TrueColor);
        let buffer = render(&mut sample(), &theme, 100, 10);
        // The map is centred: A, B and C sit at column 10, rows 2 to 4, and
        // their stubs at column 8.
        for (i, row) in [2, 3, 4].into_iter().enumerate() {
            let branch = theme.branch(Some(i));
            assert_eq!(Some(buffer[(10, row)].fg), branch.label.fg, "label {i}");
            assert_eq!(Some(buffer[(8, row)].fg), branch.line.fg, "stub {i}");
        }
        assert_ne!(buffer[(10, 2)].fg, buffer[(10, 3)].fg);
        let root = theme.branch(None);
        assert_eq!(
            Some(buffer[(1, 3)].bg),
            root.selected.bg,
            "the root is selected"
        );
        assert_eq!(Some(buffer[(7, 3)].fg), root.line.fg, "the shared junction");
    }

    #[test]
    fn the_theme_paints_every_cell() {
        let theme = Theme::mocha(ColorDepth::TrueColor);
        let mut app = sample();
        app.selected = vec![1];
        let buffer = render(&mut app, &theme, 100, 10);
        assert!(cells(&buffer).all(|c| c.bg != Color::Reset && c.fg != Color::Reset));
    }

    #[test]
    fn messages_take_the_colour_of_their_tone() {
        let theme = Theme::mocha(ColorDepth::TrueColor);
        let mut app = sample();
        app.say(Tone::Error, "boom");
        let buffer = render(&mut app, &theme, 100, 10);
        let row = 7;
        let x = (0..100)
            .find(|&x| buffer[(x, row)].symbol() == "b")
            .unwrap();
        assert_eq!(Some(buffer[(x, row)].fg), theme.tone(Tone::Error).fg);
    }

    #[test]
    fn without_colour_the_selection_is_reversed() {
        let mut app = sample();
        let buffer = render(&mut app, &Theme::mocha(ColorDepth::Mono), 100, 10);
        assert!(cells(&buffer).all(|c| c.fg == Color::Reset && c.bg == Color::Reset));
        assert!(buffer[(1, 3)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(10, 2)].modifier.contains(Modifier::REVERSED));
    }
}
