//! Colours and prompts. Colour is only used when the output is a terminal,
//! and `NO_COLOR` turns it off.

use std::fmt::Display;
use std::io::{self, IsTerminal};

use anyhow::Result;
use console::{style, StyledObject};
use texshelf_core::manifest::Engine;

use crate::ui;

pub fn good<D: Display>(text: D) -> StyledObject<D> {
    style(text).green()
}

pub fn bad<D: Display>(text: D) -> StyledObject<D> {
    style(text).red()
}

pub fn strong<D: Display>(text: D) -> StyledObject<D> {
    style(text).bold()
}

pub fn faint<D: Display>(text: D) -> StyledObject<D> {
    style(text).dim()
}

pub fn command<D: Display>(text: D) -> StyledObject<D> {
    style(text).cyan()
}

/// LaTeX is yellow and Typst is blue, as on the website.
pub fn engine<D: Display>(engine: Engine, text: D) -> StyledObject<D> {
    match engine {
        Engine::Latex => style(text).yellow(),
        Engine::Typst => style(text).blue(),
    }
}

pub fn note(message: impl Display) {
    ui::note(&message.to_string());
}

pub fn warning(message: impl Display) {
    ui::warning(&message.to_string());
}

pub fn error(message: impl Display) {
    ui::error(&message.to_string());
}

/// True when questions can be asked: both ends are a terminal.
pub fn interactive() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}

/// A list to move through with the arrow keys. `None` when the user presses Esc.
pub fn choose(prompt: &str, items: &[String], start: usize) -> Result<Option<usize>> {
    Ok(dialoguer::Select::with_theme(&ui::theme())
        .with_prompt(prompt)
        .items(items)
        .default(start)
        .report(false)
        .interact_opt()?)
}

pub fn confirm(prompt: &str) -> Result<bool> {
    Ok(dialoguer::Confirm::with_theme(&ui::theme())
        .with_prompt(prompt)
        .default(false)
        .interact()?)
}

pub fn input(prompt: &str, default: &str) -> Result<String> {
    Ok(dialoguer::Input::with_theme(&ui::theme())
        .with_prompt(prompt)
        .default(default.to_string())
        .interact_text()?)
}
