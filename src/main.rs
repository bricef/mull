use std::{io, path::PathBuf};

use anyhow::Context as _;
use clap::Parser;
use mull::{app::App, keymap, theme::Theme, ui};
use ratatui::crossterm::event;

/// A keyboard-driven mind mapper for the terminal. Maps are plain Markdown
/// files.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// The Markdown file to open. A new one is created when you first save.
    #[arg(required_unless_present = "keys")]
    file: Option<PathBuf>,

    /// Print the map as text and exit.
    #[arg(long)]
    print: bool,

    /// Print every key binding as Markdown and exit.
    #[arg(long)]
    keys: bool,

    /// Widest a topic is shown before it's cut short with "…".
    #[arg(long, default_value_t = 40)]
    max_width: usize,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.keys {
        print!("{}", keymap::reference_markdown());
        return Ok(());
    }
    let file = cli.file.context("no file given")?;
    let mut app = App::open(&file).with_context(|| format!("couldn't open {}", file.display()))?;
    app.options.max_text_width = cli.max_width;
    if cli.print {
        print!("{}", app.layout().to_text());
        return Ok(());
    }
    let theme = Theme::detect();
    ratatui::run(|terminal| -> io::Result<()> {
        while !app.should_quit {
            terminal.draw(|frame| ui::draw(frame, &mut app, &theme))?;
            if let Some(key) = event::read()?.as_key_press_event() {
                app.handle_key(key);
            }
        }
        Ok(())
    })?;
    Ok(())
}
