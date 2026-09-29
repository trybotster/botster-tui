//! The Workspaces acceptance driver: the same client as `botster-tui`, run
//! from the scenario and evidence files named in the environment (see the
//! README). It is a separate binary so the product binary carries no
//! acceptance machinery, and `cargo build` and `cargo test` always compile it.

use botster_tui::app;

fn main() -> std::io::Result<()> {
    match app::AppArgs::parse(std::env::args().skip(1)) {
        Ok(app::ParsedCommand::Run(args)) => app::run_acceptance(args),
        Ok(app::ParsedCommand::Help | app::ParsedCommand::Version) => {
            println!("{}", app::usage());
            Ok(())
        }
        Err(error) => {
            eprintln!("error: {error}\n\n{}", app::usage());
            std::process::exit(2);
        }
    }
}
