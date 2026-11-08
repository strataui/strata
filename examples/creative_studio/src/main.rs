//! Strata Creative Studio Entrypoint.

#![allow(non_snake_case, dead_code, unused_variables, unused_imports)]

mod app;
mod components;
mod state;

use app::App;
use strata::prelude::*;

fn main() {
    println!("[*] Starting Strata Creative Studio in Pure Rust...");

    let config = WindowConfig {
        title: "Strata Creative Studio".to_string(),
        width: 1440,
        height: 900,
        ..Default::default()
    };

    #[cfg(target_os = "linux")]
    {
        let _ = strata::launch_gtk(App, config);
    }

    #[cfg(not(target_os = "linux"))]
    {
        strata::app().with_window(config).build(App).run();
    }
}
