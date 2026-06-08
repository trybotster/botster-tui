mod app;

fn main() -> std::io::Result<()> {
    if std::env::args().skip(1).any(|arg| arg == "--smoke") {
        println!("{}", app::smoke_message());
        return Ok(());
    }

    app::run()
}
