// No console window on Windows; the shell launches us for every right-click.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod shell;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--register") => std::process::exit(cli_result(shell::register())),
        Some("--unregister") => std::process::exit(cli_result(shell::unregister())),
        Some("--version") => {
            println!("LocalPDF {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        _ => {}
    }
    app::run(args);
}

fn cli_result(r: std::io::Result<()>) -> i32 {
    match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("LocalPDF: {e}");
            1
        }
    }
}
