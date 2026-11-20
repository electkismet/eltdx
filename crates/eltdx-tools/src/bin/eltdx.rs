#[path = "cli/mod.rs"]
mod cli;

fn main() -> std::process::ExitCode {
    cli::main()
}
