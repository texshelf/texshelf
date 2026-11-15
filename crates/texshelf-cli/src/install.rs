use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use texshelf_core::manifest::Engine;

use crate::style;

/// How a distribution gets onto this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
enum How {
    /// A program and its arguments.
    Run(Vec<String>),
    /// A pipeline that needs a shell.
    Shell(String),
    /// Steps the user has to do by hand.
    Manual(String),
}

#[derive(Debug, Clone)]
struct Choice {
    id: &'static str,
    name: &'static str,
    about: &'static str,
    how: How,
    note: Option<String>,
}

fn run_args(args: &[&str]) -> How {
    How::Run(args.iter().map(|a| a.to_string()).collect())
}

fn manual(text: &str) -> How {
    How::Manual(text.to_string())
}

fn tinytex(os: &str, has: &dyn Fn(&str) -> bool) -> Choice {
    let script = "https://tinytex.yihui.org/install-bin-unix.sh";
    let (how, note) = match os {
        "windows" => (
            manual(
                "Download https://tinytex.yihui.org/install-bin-windows.bat and double-click it.",
            ),
            None,
        ),
        "macos" => (How::Shell(format!("curl -sL \"{script}\" | sh")), None),
        _ => {
            let how = if has("wget") {
                How::Shell(format!("wget -qO- \"{script}\" | sh"))
            } else {
                How::Shell(format!("curl -sL \"{script}\" | sh"))
            };
            let lacking: Vec<&str> = ["perl", "xz"].into_iter().filter(|t| !has(t)).collect();
            let note = (!lacking.is_empty()).then(|| {
                format!(
                    "TinyTeX needs {} first; install with your package manager.",
                    lacking.join(" and ")
                )
            });
            (how, note)
        }
    };
    Choice {
        id: "tinytex",
        name: "TinyTeX",
        about:
            "A small TeX Live in your home folder, no admin rights. Add packages later with tlmgr.",
        how,
        note,
    }
}

fn tectonic(os: &str, has: &dyn Fn(&str) -> bool) -> Choice {
    let how = if has("brew") {
        run_args(&["brew", "install", "tectonic"])
    } else if os == "linux" && has("pacman") {
        run_args(&["sudo", "pacman", "-S", "tectonic"])
    } else if has("cargo") {
        run_args(&["cargo", "install", "tectonic"])
    } else {
        manual("Follow https://tectonic-typesetting.github.io/book/latest/installation/")
    };
    Choice {
        id: "tectonic",
        name: "Tectonic",
        about:
            "One program that downloads the packages a document needs the first time it compiles.",
        how,
        note: None,
    }
}

fn full(os: &str, has: &dyn Fn(&str) -> bool) -> Choice {
    match os {
        "macos" => Choice {
            id: "mactex",
            name: "MacTeX",
            about: "All of TeX Live, several GB. Works offline and asks for your password.",
            how: if has("brew") {
                run_args(&["brew", "install", "--cask", "mactex-no-gui"])
            } else {
                manual("Download the installer from https://www.tug.org/mactex/")
            },
            note: None,
        },
        "windows" => Choice {
            id: "miktex",
            name: "MiKTeX",
            about: "Installs missing packages by itself while compiling.",
            how: if has("winget") {
                run_args(&["winget", "install", "-e", "--id", "MiKTeX.MiKTeX"])
            } else {
                manual("Download the installer from https://miktex.org/download")
            },
            note: None,
        },
        _ => Choice {
            id: "texlive",
            name: "TeX Live",
            about: "All of TeX Live from your distribution, several GB. Works offline.",
            how: if has("apt-get") {
                run_args(&["sudo", "apt-get", "install", "texlive-full"])
            } else if has("dnf") {
                run_args(&["sudo", "dnf", "install", "texlive-scheme-full"])
            } else {
                manual("Follow https://tug.org/texlive/quickinstall.html")
            },
            note: None,
        },
    }
}

fn typst(os: &str, has: &dyn Fn(&str) -> bool) -> Choice {
    let how = if os == "windows" && has("winget") {
        run_args(&["winget", "install", "--id", "Typst.Typst"])
    } else if has("brew") {
        run_args(&["brew", "install", "typst"])
    } else if os == "linux" && has("pacman") {
        run_args(&["sudo", "pacman", "-S", "typst"])
    } else if has("cargo") {
        run_args(&["cargo", "install", "--locked", "typst-cli"])
    } else {
        manual("Download a release from https://github.com/typst/typst/releases and put it on your PATH.")
    };
    Choice {
        id: "typst",
        name: "Typst",
        about: "The Typst compiler.",
        how,
        note: None,
    }
}

/// The choices for this system, with the recommended one first.
fn choices(os: &str, engine: Engine, has: &dyn Fn(&str) -> bool) -> Vec<Choice> {
    let mut latex = vec![tinytex(os, has), tectonic(os, has), full(os, has)];
    if os == "windows" {
        latex.swap(0, 2);
    }
    let typst = typst(os, has);
    match engine {
        Engine::Typst => std::iter::once(typst).chain(latex).collect(),
        Engine::Latex => latex.into_iter().chain([typst]).collect(),
    }
}

fn on_path(tool: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let file = dir.join(tool);
        file.is_file() || (cfg!(windows) && is_windows_program(&file))
    })
}

fn is_windows_program(file: &Path) -> bool {
    ["exe", "cmd", "bat"]
        .iter()
        .any(|ext| file.with_extension(ext).is_file())
}

fn command_line(how: &How) -> String {
    match how {
        How::Run(args) => args.join(" "),
        How::Shell(line) | How::Manual(line) => line.clone(),
    }
}

/// The folder a program really lives in, with links followed.
fn home_of(tool: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(tool))
        .find(|file| file.is_file())
        .and_then(|file| std::fs::canonicalize(file).ok())
}

/// Which choice the LaTeX on this machine came from, judged by where pdflatex lives.
fn latex_origin(os: &str, pdflatex: Option<&Path>) -> Option<&'static str> {
    let place = pdflatex?.to_string_lossy().to_lowercase();
    Some(if place.contains("tinytex") {
        "tinytex"
    } else if place.contains("miktex") {
        "miktex"
    } else if os == "macos" {
        "mactex"
    } else {
        "texlive"
    })
}

fn installed(os: &str) -> Vec<&'static str> {
    let mut found = Vec::new();
    found.extend(latex_origin(os, home_of("pdflatex").as_deref()));
    if on_path("tectonic") {
        found.push("tectonic");
    }
    if on_path("typst") {
        found.push("typst");
    }
    found
}

fn pick<'a>(bin: &str, list: &'a [Choice], have: &[&str]) -> Result<Option<&'a Choice>> {
    let width = list.iter().map(|c| c.id.len()).max().unwrap_or(0);
    let label = |choice: &Choice| {
        let status = if have.is_empty() {
            String::new()
        } else if have.contains(&choice.id) {
            format!("{}  ", style::good("installed"))
        } else {
            format!("{:9}  ", "")
        };
        format!(
            "{}  {status}{}",
            style::strong(format!("{:<width$}", choice.id)),
            choice.about
        )
    };
    if !style::interactive() {
        for choice in list {
            println!("{}", label(choice));
            println!(
                "{:width$}  {}",
                "",
                style::command(command_line(&choice.how))
            );
        }
        println!("\nInstall one with: {bin} tex install <name>");
        return Ok(None);
    }
    let labels: Vec<String> = list.iter().map(label).collect();
    let start = list.iter().position(|c| !have.contains(&c.id)).unwrap_or(0);
    let chosen = style::choose("Install which one? (Esc to cancel)", &labels, start)?;
    Ok(chosen.map(|index| &list[index]))
}

fn execute(how: &How) -> Result<()> {
    let status = match how {
        How::Run(args) => Command::new(&args[0]).args(&args[1..]).status(),
        How::Shell(line) => Command::new("sh").arg("-c").arg(line).status(),
        How::Manual(_) => return Ok(()),
    }
    .with_context(|| format!("could not start `{}`", command_line(how)))?;
    if !status.success() {
        bail!("the installer stopped with {status}");
    }
    Ok(())
}

pub fn run(bin: &str, engine: Engine, name: Option<String>, print: bool, yes: bool) -> Result<()> {
    let os = std::env::consts::OS;
    let list = choices(os, engine, &on_path);
    let have = installed(os);
    let choice = match name {
        Some(name) => list.iter().find(|c| c.id == name).with_context(|| {
            let ids: Vec<&str> = list.iter().map(|c| c.id).collect();
            format!(
                "`{name}` is not available on this system (choices: {})",
                ids.join(", ")
            )
        })?,
        None => match pick(bin, &list, &have)? {
            Some(choice) => choice,
            None => return Ok(()),
        },
    };

    if print {
        println!("{}", command_line(&choice.how));
        return Ok(());
    }
    println!("{}: {}", style::strong(choice.name), choice.about);
    if have.contains(&choice.id) {
        style::note(format!("{} is already installed here", choice.name));
    }
    if let Some(note) = &choice.note {
        style::note(note);
    }
    if let How::Manual(steps) = &choice.how {
        println!("{steps}");
        println!("texshelf cannot run this one for you.");
        return Ok(());
    }
    println!(
        "This runs:\n\n  {}\n",
        style::command(command_line(&choice.how))
    );
    if !yes {
        if !style::interactive() {
            bail!("pass --yes to run it without being asked");
        }
        if !style::confirm("Run it now?")? {
            println!("Nothing was installed.");
            return Ok(());
        }
    }
    execute(&choice.how)?;
    println!(
        "\n{} Open a new terminal, then run `{bin} doctor` to check the result.",
        style::good("Done.")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[Choice]) -> Vec<&str> {
        list.iter().map(|c| c.id).collect()
    }

    #[test]
    fn recommends_by_system_and_engine() {
        let none = |_: &str| false;
        assert_eq!(
            ids(&choices("macos", Engine::Latex, &none)),
            ["tinytex", "tectonic", "mactex", "typst"]
        );
        assert_eq!(
            ids(&choices("windows", Engine::Latex, &none)),
            ["miktex", "tectonic", "tinytex", "typst"]
        );
        assert_eq!(ids(&choices("linux", Engine::Typst, &none))[0], "typst");
    }

    #[test]
    fn uses_the_package_manager_that_is_present() {
        let brew = |tool: &str| tool == "brew";
        let mac = choices("macos", Engine::Latex, &brew);
        assert_eq!(
            command_line(&mac[2].how),
            "brew install --cask mactex-no-gui"
        );
        assert_eq!(command_line(&mac[3].how), "brew install typst");

        let apt = |tool: &str| tool == "apt-get" || tool == "wget";
        let linux = choices("linux", Engine::Latex, &apt);
        assert!(command_line(&linux[0].how).starts_with("wget -qO- "));
        assert!(linux[0].note.as_deref().unwrap().contains("perl and xz"));
        assert_eq!(
            command_line(&linux[2].how),
            "sudo apt-get install texlive-full"
        );
        assert!(matches!(linux[3].how, How::Manual(_)));

        let winget = |tool: &str| tool == "winget";
        let windows = choices("windows", Engine::Latex, &winget);
        assert_eq!(
            command_line(&windows[0].how),
            "winget install -e --id MiKTeX.MiKTeX"
        );
        assert!(matches!(windows[2].how, How::Manual(_)));
    }

    #[test]
    fn tells_where_latex_came_from() {
        let origin = |os, path: &str| latex_origin(os, Some(Path::new(path)));
        assert_eq!(
            origin(
                "macos",
                "/Users/a/Library/TinyTeX/bin/universal-darwin/pdftex"
            ),
            Some("tinytex")
        );
        assert_eq!(
            origin(
                "macos",
                "/usr/local/texlive/2025/bin/universal-darwin/pdftex"
            ),
            Some("mactex")
        );
        assert_eq!(origin("linux", "/usr/bin/pdftex"), Some("texlive"));
        assert_eq!(
            origin(
                "windows",
                "C:/Users/a/AppData/Local/Programs/MiKTeX/miktex/bin/x64/pdflatex.exe"
            ),
            Some("miktex")
        );
        assert_eq!(latex_origin("linux", None), None);
    }
}
