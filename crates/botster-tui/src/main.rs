mod acceptance;
mod app;
mod entity_options;
mod hub_io;
mod projection_paint;
mod renderer;
mod terminal_input;

fn main() -> std::io::Result<()> {
    let args = app::AppArgs::parse(std::env::args().skip(1));
    if args.smoke {
        println!("{}", app::smoke_message());
        return Ok(());
    }

    app::run(args)
}
