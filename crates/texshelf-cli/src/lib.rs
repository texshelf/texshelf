mod author;
mod commands;
mod doctor;
mod install;
mod style;
mod taps;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use texshelf_core::index::Sort;
use texshelf_core::manifest::Engine;

#[derive(Parser)]
#[command(version, about = "Start LaTeX and Typst projects from templates")]
struct Cli {
    /// Which templates to show; defaults to the engine this command is named for
    #[arg(long, global = true, value_enum)]
    engine: Option<EngineArg>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum EngineArg {
    Latex,
    Typst,
    All,
}

#[derive(Clone, Copy, ValueEnum)]
enum SortArg {
    Relevance,
    Downloads,
    Stars,
    Newest,
    Name,
    Official,
}

impl From<SortArg> for Sort {
    fn from(s: SortArg) -> Self {
        match s {
            SortArg::Relevance => Sort::Relevance,
            SortArg::Downloads => Sort::Downloads,
            SortArg::Stars => Sort::Stars,
            SortArg::Newest => Sort::Newest,
            SortArg::Name => Sort::Name,
            SortArg::Official => Sort::Official,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Create a project from a template
    Init {
        /// Template name, optionally with a version: article or article@1.2.0
        template: String,
        /// Folder to create; defaults to the template name
        dir: Option<PathBuf>,
        /// Set a template variable: --var title="My report"
        #[arg(long = "var", value_name = "KEY=VALUE")]
        vars: Vec<String>,
        /// Use defaults for variables without asking
        #[arg(short, long)]
        yes: bool,
        /// Do not run `git init` in the new folder
        #[arg(long)]
        no_git: bool,
        /// Write into a folder that already has files
        #[arg(long)]
        force: bool,
    },
    /// Search the template index
    Search {
        query: Vec<String>,
        #[arg(long, value_enum)]
        sort: Option<SortArg>,
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        license: Option<String>,
        /// Only templates maintained by the texshelf project
        #[arg(long)]
        official: bool,
        /// Only templates made for these universities, e.g. --university "UC Chile" Cambridge
        #[arg(long, num_args = 1.., value_name = "NAME")]
        university: Vec<String>,
        #[arg(long, default_value_t = 30)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Show a template's details
    Info {
        template: String,
        #[arg(long)]
        json: bool,
    },
    /// Open the prebuilt PDF preview of a template
    View {
        template: String,
        /// Open the template's web page instead of the PDF
        #[arg(long)]
        web: bool,
    },
    /// Open a template in Overleaf or the Typst web app
    Open {
        template: String,
        /// Print the link instead of opening a browser
        #[arg(long)]
        print: bool,
    },
    /// Refresh the local copy of the template index
    Update,
    /// Check the TeX and Typst tools on this machine, and a document's packages
    Doctor {
        /// A .tex file whose packages should be checked
        file: Option<PathBuf>,
    },
    /// Check a registry manifest before submitting it
    Validate { manifest: PathBuf },
    /// Start a template of your own: a folder with a starter document and texshelf.toml
    NewTemplate {
        /// Folder to create
        dir: PathBuf,
        /// Template name; defaults to the folder name
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value = "article")]
        category: String,
    },
    /// Submit the template in this folder to the registry
    Publish {
        /// Folder holding texshelf.toml; defaults to the current one
        dir: Option<PathBuf>,
        /// Print the manifest and the link without opening a browser
        #[arg(long)]
        print: bool,
    },
    /// Install a TeX distribution or Typst
    Tex {
        #[command(subcommand)]
        command: TexCommand,
    },
    /// Use templates from another registry, such as your university's
    Tap {
        #[command(subcommand)]
        command: TapCommand,
    },
    /// Print a shell completion script
    Completions { shell: clap_complete::Shell },
}

#[derive(Subcommand)]
enum TexCommand {
    /// Show what can be installed on this system, or install one of them
    Install {
        /// tinytex, tectonic, mactex, texlive, miktex or typst
        name: Option<String>,
        /// Print the install command without running it
        #[arg(long)]
        print: bool,
        /// Run the installer without asking first
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum TapCommand {
    /// Add a registry: tap add uc https://example.org/v1/index.json
    Add {
        /// Short name; its templates are listed as <name>/<template>
        name: String,
        /// Address of the registry's index.json, or a local folder that holds one
        url: String,
    },
    /// Stop using a registry
    Remove { name: String },
    /// Show the registries in use
    List,
}

/// Lets `texshelf search | head` end quietly instead of failing on a closed pipe.
#[cfg(unix)]
fn quiet_on_closed_pipe() {
    // SAFETY: called once at startup, before any other thread exists.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn quiet_on_closed_pipe() {}

pub fn run(bin: &'static str, default_engine: Engine) -> ExitCode {
    quiet_on_closed_pipe();
    let matches = Cli::command().name(bin).bin_name(bin).get_matches();
    let cli = match <Cli as clap::FromArgMatches>::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(err) => err.exit(),
    };
    let engine = match cli.engine {
        None => Some(default_engine),
        Some(EngineArg::Latex) => Some(Engine::Latex),
        Some(EngineArg::Typst) => Some(Engine::Typst),
        Some(EngineArg::All) => None,
    };
    let result = match cli.command {
        Command::Init {
            template,
            dir,
            vars,
            yes,
            no_git,
            force,
        } => commands::init(&template, dir, &vars, yes, no_git, force),
        Command::Search {
            query,
            sort,
            category,
            license,
            official,
            university,
            limit,
            json,
        } => commands::search(
            &query.join(" "),
            engine,
            sort.map(Sort::from),
            category.as_deref(),
            license.as_deref(),
            official,
            &university,
            limit,
            json,
        ),
        Command::Info { template, json } => commands::info(bin, &template, json),
        Command::View { template, web } => commands::view(&template, web),
        Command::Open { template, print } => commands::open(&template, print),
        Command::Update => commands::update(),
        Command::Doctor { file } => doctor::run(file),
        Command::Validate { manifest } => commands::validate(&manifest),
        Command::NewTemplate {
            dir,
            name,
            category,
        } => author::new_template(bin, &dir, engine.unwrap_or(default_engine), name, &category),
        Command::Publish { dir, print } => author::publish(dir, print),
        Command::Tex {
            command: TexCommand::Install { name, print, yes },
        } => install::run(bin, engine.unwrap_or(default_engine), name, print, yes),
        Command::Tap { command } => commands::cache_dir().and_then(|cache| match command {
            TapCommand::Add { name, url } => taps::add(&cache, &name, &url),
            TapCommand::Remove { name } => taps::remove(&cache, &name),
            TapCommand::List => taps::list(),
        }),
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), bin, &mut std::io::stdout());
            Ok(())
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            style::error(format!("{err:#}"));
            ExitCode::FAILURE
        }
    }
}
