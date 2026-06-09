mod app;
mod renderer;
mod socket_client;

fn main() -> std::io::Result<()> {
    let args = app::AppArgs::parse(std::env::args().skip(1));
    if args.smoke {
        println!("{}", app::smoke_message());
        return Ok(());
    }

    app::run(args)
}
