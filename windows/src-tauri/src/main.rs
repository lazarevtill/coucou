// Coucou runs without a console window: Mochi is the whole UI.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args_os().nth(1).is_some_and(|arg| arg == "mcp") {
        if std::env::args_os().len() != 2 {
            eprintln!("Usage: coucou mcp");
            std::process::exit(2);
        }
        if let Err(error) = coucou_lib::run_mcp() {
            eprintln!("{error}");
            std::process::exit(1);
        }
    } else {
        coucou_lib::run()
    }
}
