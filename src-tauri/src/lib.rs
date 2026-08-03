mod analytics_types;
mod board;
mod commands;
mod db;
mod encoding;
mod gamelog_parse;
mod parse;
mod resolve;
mod run_desk;
mod site_id;
mod spawn_parse;
mod state;
mod timing;
mod types;
mod vanguard_payouts;
mod wallet_parse;
mod watch;

pub use board::*;
pub use parse::*;
pub use site_id::*;
pub use types::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    commands::run_app();
}
