use texshelf_core::manifest::Engine;

fn main() -> std::process::ExitCode {
    texshelf::run("typshelf", Engine::Typst)
}
