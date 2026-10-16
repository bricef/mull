//! Colour. The UI asks for a style by role (a topic, a branch's lines, the
//! status bar), never for a colour, and every role comes from one palette:
//! Catppuccin Mocha. Each top-level branch of the map gets its own accent,
//! as in a hand-drawn mind map.
//!
//! The palette is defined in 24-bit colour and shown that way when the
//! terminal says it can (`COLORTERM=truecolor`), as the nearest of the
//! standard 256 colours otherwise, and as plain bold and reverse video when
//! `NO_COLOR` is set.

use ratatui::style::{Color, Modifier, Style};

use crate::{app::Tone, keymap::Context, keymap::Group};

/// A 24-bit colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// Catppuccin Mocha: <https://catppuccin.com/palette>.
pub mod mocha {
    use super::Rgb;

    pub const PINK: Rgb = Rgb(0xf5, 0xc2, 0xe7);
    pub const MAUVE: Rgb = Rgb(0xcb, 0xa6, 0xf7);
    pub const RED: Rgb = Rgb(0xf3, 0x8b, 0xa8);
    pub const PEACH: Rgb = Rgb(0xfa, 0xb3, 0x87);
    pub const YELLOW: Rgb = Rgb(0xf9, 0xe2, 0xaf);
    pub const GREEN: Rgb = Rgb(0xa6, 0xe3, 0xa1);
    pub const TEAL: Rgb = Rgb(0x94, 0xe2, 0xd5);
    pub const SKY: Rgb = Rgb(0x89, 0xdc, 0xeb);
    pub const BLUE: Rgb = Rgb(0x89, 0xb4, 0xfa);
    pub const LAVENDER: Rgb = Rgb(0xb4, 0xbe, 0xfe);
    pub const TEXT: Rgb = Rgb(0xcd, 0xd6, 0xf4);
    pub const SUBTEXT1: Rgb = Rgb(0xba, 0xc2, 0xde);
    pub const SUBTEXT0: Rgb = Rgb(0xa6, 0xad, 0xc8);
    pub const OVERLAY1: Rgb = Rgb(0x7f, 0x84, 0x9c);
    pub const OVERLAY0: Rgb = Rgb(0x6c, 0x70, 0x86);
    pub const SURFACE1: Rgb = Rgb(0x45, 0x47, 0x5a);
    pub const SURFACE0: Rgb = Rgb(0x31, 0x32, 0x44);
    pub const BASE: Rgb = Rgb(0x1e, 0x1e, 0x2e);
    pub const MANTLE: Rgb = Rgb(0x18, 0x18, 0x25);
    pub const CRUST: Rgb = Rgb(0x11, 0x11, 0x1b);

    /// Top-level branches take these in turn: neighbours differ in hue, and
    /// red is left for errors.
    pub const BRANCHES: [Rgb; 8] = [BLUE, PEACH, GREEN, MAUVE, YELLOW, TEAL, PINK, SKY];
}

/// How much colour the terminal can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    Indexed,
    Mono,
}

impl ColorDepth {
    pub fn detect() -> Self {
        Self::from_env(|name| std::env::var(name).ok())
    }

    /// `NO_COLOR` (<https://no-color.org>) or a dumb terminal mean no
    /// colour, `COLORTERM=truecolor` (or `24bit`) means 24-bit, and anything
    /// else gets the 256-colour palette, which nearly every terminal has.
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Self {
        let set = |name| var(name).filter(|value| !value.is_empty());
        if set("NO_COLOR").is_some() || set("TERM").as_deref() == Some("dumb") {
            ColorDepth::Mono
        } else if matches!(set("COLORTERM").as_deref(), Some("truecolor" | "24bit")) {
            ColorDepth::TrueColor
        } else {
            ColorDepth::Indexed
        }
    }

    fn color(self, Rgb(r, g, b): Rgb) -> Color {
        match self {
            ColorDepth::TrueColor => Color::Rgb(r, g, b),
            ColorDepth::Indexed => Color::Indexed(nearest_256(Rgb(r, g, b))),
            ColorDepth::Mono => Color::Reset,
        }
    }
}

/// The xterm 256-colour index nearest `rgb`, from the 6×6×6 colour cube or
/// the 24-step grey ramp.
pub fn nearest_256(rgb: Rgb) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let level = |c: u8| (0..6).min_by_key(|&i| LEVELS[i].abs_diff(c)).unwrap_or(0);
    let (r, g, b) = (level(rgb.0), level(rgb.1), level(rgb.2));
    let cube = Rgb(LEVELS[r], LEVELS[g], LEVELS[b]);

    let average = (u32::from(rgb.0) + u32::from(rgb.1) + u32::from(rgb.2)) / 3;
    let step = (average.saturating_sub(3) / 10).min(23) as u8;
    let grey = 8 + 10 * step;

    if distance(rgb, Rgb(grey, grey, grey)) < distance(rgb, cube) {
        232 + step
    } else {
        (16 + 36 * r + 6 * g + b) as u8
    }
}

fn distance(a: Rgb, b: Rgb) -> u32 {
    let d = |x: u8, y: u8| u32::from(x.abs_diff(y)).pow(2);
    d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)
}

/// `weight` of `a` mixed with the rest of `b`.
fn mix(a: Rgb, b: Rgb, weight: f32) -> Rgb {
    let channel =
        |x: u8, y: u8| (f32::from(x) * weight + f32::from(y) * (1.0 - weight)).round() as u8;
    Rgb(channel(a.0, b.0), channel(a.1, b.1), channel(a.2, b.2))
}

/// The styles for one branch of the map: the root's, or one first-level
/// topic's and everything under it.
#[derive(Debug, Clone, Copy)]
pub struct Branch {
    /// The root, or the branch's first-level topic.
    pub label: Style,
    /// Connectors and fold markers.
    pub line: Style,
    /// A selected topic.
    pub selected: Style,
}

/// The styles that show which mode the keys are in.
#[derive(Debug, Clone, Copy)]
pub struct ModeStyle {
    /// The mode's name at the left of the status line.
    pub badge: Style,
    /// Keys in the hint bar.
    pub key: Style,
}

/// Every style the UI draws with.
#[derive(Debug, Clone)]
pub struct Theme {
    /// The map's background.
    pub canvas: Style,
    /// Topics below the first level.
    pub topic: Style,
    /// The topic being typed.
    pub editing: Style,
    root: Branch,
    branches: Vec<Branch>,
    map_mode: ModeStyle,
    typing_mode: ModeStyle,
    help_mode: ModeStyle,
    /// The status line and hint rows.
    pub bar: Style,
    pub file: Style,
    /// The unsaved-changes mark after the file name.
    pub dirty: Style,
    pub crumb: Style,
    pub crumb_separator: Style,
    /// The selected topic, at the end of the breadcrumb.
    pub crumb_current: Style,
    pub hint_label: Style,
    info: Style,
    success: Style,
    warning: Style,
    error: Style,
    pub help_text: Style,
    pub help_border: Style,
    pub help_title: Style,
    pub help_more: Style,
    /// Section titles and keys on the help screen, one per [`Group::ALL`].
    groups: [Style; 6],
}

/// What a role's colours must be legible as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Legibility {
    /// Text people read: WCAG AA, 4.5:1.
    Text,
    /// Lines and marks: WCAG non-text contrast, 3:1.
    Graphic,
}

impl Theme {
    /// Catppuccin Mocha at whatever depth the terminal supports.
    pub fn detect() -> Theme {
        Theme::mocha(ColorDepth::detect())
    }

    pub fn mocha(depth: ColorDepth) -> Theme {
        use mocha::*;

        if depth == ColorDepth::Mono {
            return Theme::mono();
        }
        let on = |fg: Rgb, bg: Rgb| Style::new().fg(depth.color(fg)).bg(depth.color(bg));
        let bold = |style: Style| style.add_modifier(Modifier::BOLD);
        let branch = |accent: Rgb| Branch {
            label: bold(on(accent, BASE)),
            line: on(mix(accent, BASE, 0.6), BASE),
            selected: bold(on(CRUST, accent)),
        };
        let mode = |accent: Rgb| ModeStyle {
            badge: bold(on(CRUST, accent)),
            key: bold(on(accent, MANTLE)),
        };
        Theme {
            canvas: on(TEXT, BASE),
            topic: on(TEXT, BASE),
            editing: on(TEXT, SURFACE1),
            root: Branch {
                label: bold(on(LAVENDER, BASE)),
                line: on(OVERLAY0, BASE),
                selected: bold(on(CRUST, LAVENDER)),
            },
            branches: BRANCHES.map(branch).to_vec(),
            map_mode: mode(BLUE),
            typing_mode: mode(GREEN),
            help_mode: mode(MAUVE),
            bar: on(SUBTEXT0, MANTLE),
            file: on(TEXT, SURFACE0),
            dirty: bold(on(PEACH, SURFACE0)),
            crumb: on(OVERLAY1, MANTLE),
            crumb_separator: on(OVERLAY0, MANTLE),
            crumb_current: on(SUBTEXT1, MANTLE),
            hint_label: on(SUBTEXT0, MANTLE),
            info: on(SKY, MANTLE),
            success: on(GREEN, MANTLE),
            warning: on(YELLOW, MANTLE),
            error: bold(on(RED, MANTLE)),
            help_text: on(TEXT, BASE),
            help_border: on(OVERLAY0, BASE),
            help_title: bold(on(MAUVE, BASE)),
            help_more: on(SUBTEXT0, BASE),
            groups: [BLUE, GREEN, PEACH, YELLOW, TEAL, MAUVE].map(|accent| bold(on(accent, BASE))),
        }
    }

    /// No colour: emphasis from bold, underline and reverse video alone.
    pub fn mono() -> Theme {
        let plain = Style::new();
        let bold = plain.add_modifier(Modifier::BOLD);
        let reversed = plain.add_modifier(Modifier::REVERSED);
        let branch = Branch {
            label: bold,
            line: plain,
            selected: reversed.add_modifier(Modifier::BOLD),
        };
        let mode = ModeStyle {
            badge: reversed.add_modifier(Modifier::BOLD),
            key: reversed,
        };
        Theme {
            canvas: plain,
            topic: plain,
            editing: plain.add_modifier(Modifier::UNDERLINED),
            root: branch,
            branches: vec![branch],
            map_mode: mode,
            typing_mode: mode,
            help_mode: mode,
            bar: plain,
            file: bold,
            dirty: bold,
            crumb: plain,
            crumb_separator: plain,
            crumb_current: bold,
            hint_label: plain,
            info: plain,
            success: plain,
            warning: bold,
            error: bold,
            help_text: plain,
            help_border: plain,
            help_title: bold,
            help_more: plain,
            groups: [bold; 6],
        }
    }

    /// The styles for the branch under first-level topic `index`, or for
    /// the root when `index` is `None`. Accents repeat after the eighth.
    pub fn branch(&self, index: Option<usize>) -> &Branch {
        match index {
            None => &self.root,
            Some(i) => &self.branches[i % self.branches.len()],
        }
    }

    pub fn mode(&self, context: Context) -> &ModeStyle {
        match context {
            Context::Map => &self.map_mode,
            Context::Typing => &self.typing_mode,
            Context::Help => &self.help_mode,
        }
    }

    pub fn tone(&self, tone: Tone) -> Style {
        match tone {
            Tone::Info => self.info,
            Tone::Success => self.success,
            Tone::Warning => self.warning,
            Tone::Error => self.error,
        }
    }

    pub fn group(&self, group: Group) -> Style {
        let index = Group::ALL.iter().position(|&g| g == group).unwrap_or(0);
        self.groups[index]
    }

    /// Every style with a foreground, and how legible it must be against
    /// its own background.
    pub fn roles(&self) -> Vec<(String, Style, Legibility)> {
        use Legibility::{Graphic, Text};
        let mut roles = vec![
            ("canvas".to_string(), self.canvas, Text),
            ("topic".to_string(), self.topic, Text),
            ("editing".to_string(), self.editing, Text),
            ("bar".to_string(), self.bar, Text),
            ("file".to_string(), self.file, Text),
            ("dirty".to_string(), self.dirty, Text),
            ("crumb".to_string(), self.crumb, Text),
            ("crumb separator".to_string(), self.crumb_separator, Graphic),
            ("crumb current".to_string(), self.crumb_current, Text),
            ("hint label".to_string(), self.hint_label, Text),
            ("info".to_string(), self.info, Text),
            ("success".to_string(), self.success, Text),
            ("warning".to_string(), self.warning, Text),
            ("error".to_string(), self.error, Text),
            ("help text".to_string(), self.help_text, Text),
            ("help border".to_string(), self.help_border, Graphic),
            ("help title".to_string(), self.help_title, Text),
            ("help more".to_string(), self.help_more, Text),
        ];
        let branches = std::iter::once(("root".to_string(), &self.root)).chain(
            self.branches
                .iter()
                .enumerate()
                .map(|(i, b)| (format!("branch {i}"), b)),
        );
        for (name, branch) in branches {
            roles.push((format!("{name} label"), branch.label, Text));
            roles.push((format!("{name} line"), branch.line, Graphic));
            roles.push((format!("{name} selected"), branch.selected, Text));
        }
        for (name, mode) in [
            ("map", &self.map_mode),
            ("typing", &self.typing_mode),
            ("help", &self.help_mode),
        ] {
            roles.push((format!("{name} badge"), mode.badge, Text));
            roles.push((format!("{name} key"), mode.key, Text));
        }
        for (group, style) in Group::ALL.iter().zip(self.groups) {
            roles.push((format!("{} group", group.title()), style, Text));
        }
        roles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
    }

    #[test]
    fn depth_comes_from_the_environment() {
        let detect = |vars: &[(&str, &str)]| ColorDepth::from_env(env(vars));
        assert_eq!(detect(&[]), ColorDepth::Indexed);
        assert_eq!(detect(&[("COLORTERM", "truecolor")]), ColorDepth::TrueColor);
        assert_eq!(detect(&[("COLORTERM", "24bit")]), ColorDepth::TrueColor);
        assert_eq!(
            detect(&[("COLORTERM", "truecolor"), ("NO_COLOR", "1")]),
            ColorDepth::Mono
        );
        assert_eq!(
            detect(&[("NO_COLOR", ""), ("COLORTERM", "truecolor")]),
            ColorDepth::TrueColor,
            "an empty NO_COLOR doesn't count"
        );
        assert_eq!(detect(&[("TERM", "dumb")]), ColorDepth::Mono);
    }

    #[test]
    fn nearest_256_picks_the_cube_or_the_grey_ramp() {
        assert_eq!(nearest_256(Rgb(0, 0, 0)), 16);
        assert_eq!(nearest_256(Rgb(255, 255, 255)), 231);
        assert_eq!(nearest_256(Rgb(0x87, 0xaf, 0xff)), 111);
        assert_eq!(nearest_256(Rgb(128, 128, 128)), 244);
        assert_eq!(nearest_256(mocha::BASE), 235);
    }

    /// WCAG relative luminance.
    fn luminance(Rgb(r, g, b): Rgb) -> f64 {
        let linear = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
    }

    fn contrast(a: Rgb, b: Rgb) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn every_role_is_legible_on_its_background() {
        for (name, style, legibility) in Theme::mocha(ColorDepth::TrueColor).roles() {
            let (Some(Color::Rgb(fr, fg, fb)), Some(Color::Rgb(br, bg, bb))) = (style.fg, style.bg)
            else {
                panic!("{name} doesn't set both colours: {style:?}");
            };
            let ratio = contrast(Rgb(fr, fg, fb), Rgb(br, bg, bb));
            let minimum = match legibility {
                Legibility::Text => 4.5,
                Legibility::Graphic => 3.0,
            };
            assert!(
                ratio >= minimum,
                "{name}: {ratio:.2}:1 is under {minimum}:1"
            );
        }
    }

    #[test]
    fn neighbouring_branches_differ() {
        let theme = Theme::mocha(ColorDepth::TrueColor);
        for i in 0..mocha::BRANCHES.len() {
            assert_ne!(
                theme.branch(Some(i)).label.fg,
                theme.branch(Some(i + 1)).label.fg
            );
        }
    }

    #[test]
    fn indexed_themes_use_only_the_256_palette() {
        for (name, style, _) in Theme::mocha(ColorDepth::Indexed).roles() {
            for color in [style.fg, style.bg] {
                assert!(
                    matches!(color, Some(Color::Indexed(_))),
                    "{name}: {color:?}"
                );
            }
        }
    }

    #[test]
    fn mono_themes_use_no_colour_and_still_mark_the_selection() {
        let theme = Theme::mocha(ColorDepth::Mono);
        for (name, style, _) in theme.roles() {
            assert_eq!((style.fg, style.bg), (None, None), "{name}");
        }
        assert!(
            theme
                .branch(Some(0))
                .selected
                .add_modifier
                .contains(Modifier::REVERSED)
        );
    }
}
