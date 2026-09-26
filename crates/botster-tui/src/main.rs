mod acceptance;
mod app;
mod entity_options;
mod hub_io;
mod projection_paint;
mod renderer;
mod route_trace;
mod terminal_input;

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
            std::process::exit(2);
        }
    };
    if args.smoke {
        println!("{}", app::smoke_message());
        return Ok(());
    }

    route_trace::init_from_env();
    app::run(args)
}
