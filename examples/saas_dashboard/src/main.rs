//! Strata Cloud SaaS Dashboard Example.

#![allow(non_snake_case, dead_code, unused_variables, unused_imports)]

mod app;
mod components;
mod state;

use app::App;
use strata::prelude::*;

fn main() {
    println!("[*] Starting Strata Cloud Enterprise Dashboard...");

    let config = WindowConfig {
        title: "Strata Cloud Enterprise Dashboard".to_string(),
        width: 1440,
        height: 900,
        ..Default::default()
    };

    if let Err(e) = strata::launch_with_config(App, config) {
        eprintln!("[Strata] Failed to launch SaaS Dashboard: {}", e);
    }
}
