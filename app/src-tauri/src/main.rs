// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // run() has released the clipboard and flushed the log file by now.
    let code = navaja_lib::run()?;
    std::process::exit(code)
}
