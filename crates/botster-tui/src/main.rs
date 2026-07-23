mod app;
mod renderer;

fn main() -> std::io::Result<()> {
    let args = match app::AppArgs::parse(std::env::args().skip(1)) {
        Ok(app::ParsedCommand::Run(args)) => args,
        Ok(app::ParsedCommand::Help) => {
            println!("{}", app::usage());
            return Ok(());
        }
        Ok(app::ParsedCommand::Version) => {
            println!("botster-tui {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Err(error) => {
            eprintln!("error: {error}\n\n{}", app::usage());
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, error));
        }
    };
    if args.smoke {
        println!("{}", app::smoke_message());
        return Ok(());
    }

    app::run(args)
}
