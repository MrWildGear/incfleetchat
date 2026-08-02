mod board;
mod commands;
mod db;
mod encoding;
mod parse;
mod resolve;
mod site_id;
mod state;
mod types;
mod watch;

pub use board::*;
pub use parse::*;
pub use site_id::*;
pub use types::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    commands::run_app();
}
