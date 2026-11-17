//! How output is laid out: the "Quiet" style.
//!
//! No borders. The only symbols are a dot for official templates, a star
//! before the star count and a pointer in lists. Names are bold, labels are
//! dimmed, and results line up behind a right-aligned verb, the way cargo
//! prints. Every function takes the terminal width and returns finished
//! lines, so the layout can be tested without a terminal.

use console::{measure_text_width, Term};
use dialoguer::theme::ColorfulTheme;
use texshelf_core::manifest::Engine;

use crate::style;

const VERB: usize = 10;
const OFFICIAL: &str = "●";

/// Columns to lay out for: the terminal's width, or 100 when piped.
pub fn width() -> usize {
    columns(&Term::stdout())
}

fn columns(term: &Term) -> usize {
    let columns = if term.is_term() {
        usize::from(term.size().1)
    } else {
        std::env::var("COLUMNS")
            .ok()
            .and_then(|c| c.parse().ok())
            .unwrap_or(100)
    };
    columns.clamp(36, 120)
}

pub fn fit(text: &str, max: usize) -> String {
    if measure_text_width(text) <= max {
        return text.to_string();
    }
    let mut cut = String::new();
    for c in text.chars() {
        let mut next = cut.clone();
        next.push(c);
        if measure_text_width(&next) + 1 > max {
            break;
        }
        cut = next;
    }
    if max == 0 {
        return String::new();
    }
    format!("{}…", cut.trim_end())
}

fn pad(text: &str, to: usize) -> String {
    let used = measure_text_width(text);
    format!("{text}{}", " ".repeat(to.saturating_sub(used)))
}

pub fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && measure_text_width(&line) + 1 + measure_text_width(word) > max {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

pub fn show(lines: Vec<String>) {
    for line in lines {
        println!("{line}");
    }
}

/// One result line: a right-aligned verb, then what it applies to.
pub fn done(verb: &str, rest: &str) -> String {
    format!("{} {rest}", style::good(format!("{verb:>VERB$}")).bold())
}

fn message(label: &str, text: &str, w: usize) -> Vec<String> {
    let indent = label.len() + 1;
    wrap(text, w.saturating_sub(indent).max(20))
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                line
            } else {
                format!("{}{line}", " ".repeat(indent))
            }
        })
        .collect()
}

pub fn note(text: &str) {
    let lines = message("note:", text, width());
    println!(
        "{} {}",
        console::style("note:").yellow().bold(),
        lines.join("\n")
    );
}

pub fn warning(text: &str) {
    let lines = message("warning:", text, columns(&Term::stderr()));
    let label = console::style("warning:").yellow().bold().for_stderr();
    eprintln!("{label} {}", lines.join("\n"));
}

pub fn error(text: &str) {
    let lines = message("error:", text, columns(&Term::stderr()));
    let label = console::style("error:").red().bold().for_stderr();
    eprintln!("{label} {}", lines.join("\n"));
}

pub struct Row {
    pub name: String,
    pub engine: Engine,
    pub compiler: String,
    pub category: String,
    pub version: String,
    pub stars: u64,
    pub license: String,
    pub description: String,
    pub official: bool,
}

type Column<'a> = (&'a str, usize, &'a dyn Fn(&Row) -> String);

pub fn search(rows: &[Row], more: usize, bin: &str, w: usize) -> Vec<String> {
    let longest = |get: &dyn Fn(&Row) -> usize, head: &str| {
        rows.iter().map(get).max().unwrap_or(0).max(head.len()) + 2
    };
    let name_w = longest(&|r| measure_text_width(&r.name), "NAME").min(26);
    let engine_w = longest(&|r| r.compiler.len(), "ENGINE");
    let optional: [Column; 4] = [
        (
            "CATEGORY",
            longest(&|r| r.category.len(), "CATEGORY"),
            &|r| r.category.clone(),
        ),
        ("VERSION", longest(&|r| r.version.len(), "VERSION"), &|r| {
            r.version.clone()
        }),
        (
            "STARS",
            longest(&|r| stars(r.stars).chars().count(), "STARS"),
            &|r| stars(r.stars),
        ),
        ("LICENSE", longest(&|r| r.license.len(), "LICENSE"), &|r| {
            r.license.clone()
        }),
    ];
    // The last two columns of every row are kept for the official dot, so
    // the dots line up whether or not a row has one.
    let mut kept = optional.len();
    let room = |kept: usize| {
        let used: usize = optional[..kept].iter().map(|c| c.1).sum();
        w.saturating_sub(name_w + engine_w + used + 2)
    };
    while kept > 0 && room(kept) < 34 {
        kept -= 1;
    }
    let mark = |row: &Row| {
        if row.official {
            format!(" {}", style::good(OFFICIAL))
        } else {
            String::new()
        }
    };
    let mut lines = Vec::new();

    if room(kept) >= 34 {
        let mut head = format!("{}{}", pad("NAME", name_w), pad("ENGINE", engine_w));
        for column in &optional[..kept] {
            head.push_str(&pad(column.0, column.1));
        }
        head.push_str("DESCRIPTION");
        lines.push(style::faint(head).to_string());
        for row in rows {
            let mut middle = String::new();
            for column in &optional[..kept] {
                middle.push_str(&pad(&column.2(row), column.1));
            }
            let description = fit(&row.description, room(kept));
            lines.push(format!(
                "{}{}{middle}{}{}",
                style::strong(pad(&fit(&row.name, name_w - 2), name_w)),
                style::engine(row.engine, pad(&row.compiler, engine_w)),
                if row.official {
                    pad(&description, room(kept))
                } else {
                    description
                },
                mark(row)
            ));
        }
    } else {
        let compiler_w = rows.iter().map(|r| r.compiler.len()).max().unwrap_or(0);
        for row in rows {
            let name = fit(&row.name, w.saturating_sub(compiler_w + 3));
            lines.push(format!(
                "{}{}{}",
                style::strong(pad(&name, w - compiler_w - 2)),
                style::engine(row.engine, format!("{:>compiler_w$}", row.compiler)),
                mark(row)
            ));
            lines.push(style::faint(fit(&row.description, w)).to_string());
        }
    }

    lines.push(String::new());
    let count = format!(
        "{} template{}",
        rows.len(),
        if rows.len() == 1 { "" } else { "s" }
    );
    // Styled in pieces: a reset after the green dot would end the dim text.
    let count = if rows.iter().any(|r| r.official) {
        format!(
            "{}{}{}",
            style::faint(format!("{count}, ")),
            style::good(OFFICIAL),
            style::faint(" official.")
        )
    } else {
        style::faint(format!("{count}.")).to_string()
    };
    let details = format!("{bin} info <name>");
    if measure_text_width(&count) + 10 + details.len() <= w {
        lines.push(format!(
            "{count} {} {}",
            style::faint("Details:"),
            style::command(details)
        ));
    } else {
        lines.push(count);
        lines.push(style::command(fit(&details, w)).to_string());
    }
    if more > 0 {
        lines.push(style::faint(format!("{more} more; raise --limit to see them")).to_string());
    }
    lines
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Plain,
    Good,
    Bad,
}

#[derive(Clone)]
pub struct Card {
    pub name: String,
    pub version: String,
    pub official: bool,
    pub description: String,
    pub fields: Vec<(&'static str, String, Tone)>,
    pub variables: Vec<(String, String)>,
    pub init: String,
    pub link: Option<String>,
}

fn toned(text: String, tone: Tone) -> String {
    match tone {
        Tone::Plain => text,
        Tone::Good => style::good(text).to_string(),
        Tone::Bad => style::bad(text).to_string(),
    }
}

pub fn info(card: &Card, w: usize) -> Vec<String> {
    let badge = if card.official { 11 } else { 0 };
    let name_w = w.saturating_sub(card.version.len() + 1 + badge);
    let mut lines = vec![format!(
        "{} {}{}",
        style::strong(fit(&card.name, name_w)),
        card.version,
        if card.official {
            format!(" {} official", style::good(OFFICIAL))
        } else {
            String::new()
        }
    )];
    lines.extend(wrap(&card.description, w));
    lines.push(String::new());
    for (label, value, tone) in &card.fields {
        if measure_text_width(value) + 12 <= w {
            lines.push(format!(
                "{}{}",
                style::faint(pad(label, 12)),
                toned(value.clone(), *tone)
            ));
        } else if is_link(value) {
            lines.push(style::faint(label).to_string());
            lines.push(format!("  {}", toned(value.clone(), *tone)));
        } else {
            lines.push(style::faint(label).to_string());
            for line in wrap(value, w - 2) {
                lines.push(format!("  {}", toned(fit(&line, w - 2), *tone)));
            }
        }
    }
    if !card.variables.is_empty() {
        lines.push(String::new());
        lines.push(style::faint("Variables").to_string());
        for (key, about) in &card.variables {
            if key.len() <= 8 {
                lines.push(format!(
                    "  {}{}",
                    style::strong(pad(key, 10)),
                    fit(about, w.saturating_sub(12))
                ));
            } else {
                lines.push(format!("  {}", style::strong(fit(key, w - 2))));
                lines.push(format!("    {}", fit(about, w - 4)));
            }
        }
    }
    lines.push(String::new());
    lines.push(style::command(fit(&card.init, w)).to_string());
    if let Some(link) = &card.link {
        lines.push(style::faint(link).to_string());
    }
    lines
}

/// Printed once the archive is downloaded and its checksum matches.
pub fn fetched(name: &str, version: &str, _engine: Engine, w: usize) -> Vec<String> {
    let name = fit(name, w.saturating_sub(VERB + 2 + version.len()));
    vec![
        done("Fetched", &format!("{name} {version}")),
        done("Verified", &fit("sha256 matches the index", w - VERB - 1)),
    ]
}

pub fn created(dir: &str, details: &str, next: &str, w: usize) -> Vec<String> {
    let room = w - VERB - 1;
    let place = format!("{dir}/");
    let created = if measure_text_width(&place) + 3 + details.len() <= room {
        vec![done(
            "Created",
            &format!("{} ({details})", style::strong(place)),
        )]
    } else {
        vec![
            done("Created", &style::strong(fit(&place, room)).to_string()),
            format!("{}{}", " ".repeat(VERB + 1), fit(details, room)),
        ]
    };
    // The next step is a command to copy, so it becomes one command per
    // line before anything is cut.
    let mut lines = created;
    let parts: Vec<&str> = if measure_text_width(next) <= room {
        vec![next]
    } else {
        next.split(" && ").collect()
    };
    for (i, line) in parts.into_iter().enumerate() {
        let line = style::command(fit(line, room)).to_string();
        if i == 0 {
            lines.push(done("Next", &line));
        } else {
            lines.push(format!("{}{line}", " ".repeat(VERB + 1)));
        }
    }
    lines
}

pub fn stars(count: u64) -> String {
    format!("★ {count}")
}

/// Links are never cut short: a cut link is useless, and a terminal wraps a
/// long one so that it still opens.
fn is_link(text: &str) -> bool {
    text.starts_with("https://") || text.starts_with("http://")
}

pub struct Tool {
    pub name: &'static str,
    pub version: Option<String>,
    pub purpose: &'static str,
}

pub fn tools(tools: &[Tool], w: usize) -> Vec<String> {
    tools
        .iter()
        .map(|tool| {
            let name = style::strong(pad(tool.name, 10));
            match &tool.version {
                Some(version) => {
                    format!(
                        "{}{name}{}",
                        style::good(pad("ok", 9)),
                        fit(version, w - 19)
                    )
                }
                None => format!(
                    "{}{name}{}",
                    style::bad(pad("missing", 9)),
                    style::faint(fit(tool.purpose, w - 19))
                ),
            }
        })
        .collect()
}

pub fn packages(
    file: &str,
    total: usize,
    missing: &[String],
    fix: Option<&str>,
    w: usize,
) -> Vec<String> {
    let mut lines = vec![String::new()];
    lines.extend(wrap(
        &format!("{file} uses {total} packages and classes."),
        w,
    ));
    if missing.is_empty() {
        lines.push(style::good("All of them are installed.").to_string());
        return lines;
    }
    for (i, line) in wrap(&missing.join(", "), w - 9).into_iter().enumerate() {
        let label = if i == 0 { "Missing: " } else { "         " };
        lines.push(format!("{}{line}", style::bad(label)));
    }
    if let Some(fix) = fix {
        lines.push(format!(
            "{}{}",
            style::faint("Try      "),
            style::command(fit(fix, w - 9))
        ));
        lines.push(
            style::faint(fit(
                "A file name is not always the package name; `tlmgr search --file <name>` finds the owner.",
                w,
            ))
            .to_string(),
        );
    }
    lines
}

pub fn taps(taps: &[(String, String)], w: usize) -> Vec<String> {
    let name_w = taps.iter().map(|t| t.0.len()).max().unwrap_or(0) + 2;
    taps.iter()
        .map(|(name, url)| {
            format!(
                "{}{}",
                style::strong(pad(name, name_w)),
                if is_link(url) {
                    url.clone()
                } else {
                    fit(url, w - name_w)
                }
            )
        })
        .collect()
}

pub struct Option_ {
    pub id: &'static str,
    pub about: &'static str,
    pub installed: bool,
    pub command: String,
}

/// The lines of a list of things to install, one entry per option.
pub fn options(list: &[Option_], w: usize) -> Vec<String> {
    list.iter()
        .map(|option| {
            let tag = if option.installed { "installed" } else { "" };
            if w >= 70 {
                format!(
                    "{}{}{}",
                    style::strong(pad(option.id, 10)),
                    style::good(pad(tag, 11)),
                    fit(option.about, w - 25)
                )
            } else {
                let gap = w.saturating_sub(4 + option.id.len() + tag.len());
                format!(
                    "{}{}{}",
                    style::strong(option.id),
                    " ".repeat(gap),
                    style::good(tag)
                )
            }
        })
        .collect()
}

/// The same list for output that nobody can answer: every option with its command.
pub fn options_listing(list: &[Option_], bin: &str, w: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for (label, option) in options(list, w + 2).into_iter().zip(list) {
        lines.push(label);
        lines.push(format!(
            "{}{}",
            " ".repeat(10),
            style::command(fit(&option.command, w - 10))
        ));
    }
    lines.push(String::new());
    lines.push(format!(
        "Install one with: {}",
        style::command(format!("{bin} tex install <name>"))
    ));
    lines
}

pub fn about_to_run(name: &str, about: &str, command: &str, w: usize) -> Vec<String> {
    let mut lines: Vec<String> = wrap(&format!("{name}: {about}"), w);
    lines[0] = lines[0].replacen(name, &style::strong(name).to_string(), 1);
    lines.push(format!(
        "{} {}",
        style::faint(format!("{:>VERB$}", "Runs")),
        style::command(fit(command, w - VERB - 1))
    ));
    lines
}

pub fn select_prompt() -> &'static str {
    "Install which one?"
}

/// How prompts look. dialoguer draws them on stderr.
pub fn theme() -> ColorfulTheme {
    use console::{style as paint, Style};
    ColorfulTheme {
        prompt_prefix: paint("?".to_string()).for_stderr().cyan().bold(),
        prompt_suffix: paint("›".to_string()).for_stderr().dim(),
        success_prefix: paint("✓".to_string()).for_stderr().green(),
        success_suffix: paint("·".to_string()).for_stderr().dim(),
        prompt_style: Style::new().for_stderr().bold(),
        values_style: Style::new().for_stderr(),
        active_item_prefix: paint("❯".to_string()).for_stderr().cyan().bold(),
        inactive_item_prefix: paint(" ".to_string()).for_stderr(),
        active_item_style: Style::new().for_stderr(),
        inactive_item_style: Style::new().for_stderr().dim(),
        ..ColorfulTheme::default()
    }
}

/// Colours for `--help`.
pub fn help_styles() -> clap::builder::Styles {
    use clap::builder::styling::{AnsiColor, Style};
    clap::builder::Styles::styled()
        .header(Style::new().dimmed())
        .usage(Style::new().dimmed())
        .literal(AnsiColor::Cyan.on_default())
        .placeholder(Style::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Row> {
        ["article", "a-template-with-a-very-long-name-indeed"]
            .into_iter()
            .map(|name| Row {
                name: name.into(),
                engine: Engine::Latex,
                compiler: "pdflatex".into(),
                category: "article".into(),
                version: "1.0.0".into(),
                stars: 12,
                license: "MIT".into(),
                description: "Short article with sections, a table and a BibTeX bibliography."
                    .into(),
                official: true,
            })
            .collect()
    }

    #[test]
    fn every_layout_stays_inside_the_window() {
        let card = Card {
            name: "aastex".into(),
            version: "1.0.0".into(),
            official: true,
            description: "AAS journals manuscript (ApJ, AJ, ApJS) with the aastex701 class.".into(),
            fields: vec![(
                "Source",
                "https://github.com/texshelf/templates/tree/main/aastex".into(),
                Tone::Plain,
            )],
            variables: vec![("title".into(), "Title (default: An Example Title)".into())],
            init: "texshelf init aastex".into(),
            link: None,
        };
        let long = Card {
            name: "uc/a-template-with-a-very-long-name".into(),
            version: "2026.10.28".into(),
            official: true,
            variables: vec![(
                "supervisor_department".into(),
                "Department of the supervisor (default: Astronomy)".into(),
            )],
            init: "texshelf init uc/a-template-with-a-very-long-name".into(),
            ..card.clone()
        };
        let list = [Option_ {
            id: "tinytex",
            about: "A small TeX Live in your home folder, no admin rights. Add packages later with tlmgr.",
            installed: true,
            command: "curl -sL \"https://tinytex.yihui.org/install-bin-unix.sh\" | sh".into(),
        }];
        for w in [36, 44, 60, 80, 100, 120] {
            let mut all = search(&rows(), 3, "texshelf", w);
            all.extend(info(&card, w));
            all.extend(created(
                "my-paper",
                "2 files, MIT",
                "cd my-paper && latexmk -pdf main.tex",
                w,
            ));
            all.extend(created(
                "papers/2026/a-paper-with-a-long-folder-name",
                "14 files, LPPL-1.3c, git repository",
                "cd papers/2026/a-paper-with-a-long-folder-name && latexmk -pdf main.tex",
                w,
            ));
            all.extend(fetched(
                "uc/a-template-with-a-long-name",
                "2026.10.28",
                Engine::Latex,
                w,
            ));
            all.extend(info(&long, w));
            all.extend(options(&list, w - 4));
            all.extend(packages(
                "main.tex",
                14,
                &["minted.sty".into()],
                Some("tlmgr install minted"),
                w,
            ));
            all.extend(taps(
                &[(
                    "uc".into(),
                    "https://example.org/a/very/long/path/to/templates/v1/index.json".into(),
                )],
                w,
            ));
            for line in all.into_iter().filter(|l| !l.contains("://")) {
                assert!(measure_text_width(&line) <= w, "{w}: {line}");
            }
        }
    }

    #[test]
    fn narrow_windows_get_two_lines_per_template() {
        let wide = search(&rows(), 0, "texshelf", 110);
        assert!(wide[0].contains("LICENSE") && wide[1].ends_with('●'));
        assert!(wide[1].contains("★ 12"));
        let medium = search(&rows(), 0, "texshelf", 80);
        assert!(medium[0].contains("DESCRIPTION") && !medium[0].contains("LICENSE"));
        let narrow = search(&rows(), 0, "texshelf", 44);
        assert!(!narrow[0].contains("NAME") && narrow[1].starts_with("Short article"));
    }

    #[test]
    fn compilers_and_dots_line_up_in_narrow_windows() {
        let mut rows = rows();
        rows[1].compiler = "typst".into();
        rows[1].official = false;
        let lines = search(&rows, 0, "texshelf", 44);
        assert!(lines[0].ends_with("pdflatex ●") && measure_text_width(&lines[0]) == 44);
        assert!(lines[2].ends_with("   typst") && measure_text_width(&lines[2]) == 42);
    }

    #[test]
    fn wraps_on_words() {
        assert_eq!(wrap("one two three", 7), ["one two", "three"]);
        assert_eq!(wrap("", 7), [""]);
    }
}
