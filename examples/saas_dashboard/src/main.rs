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

    #[cfg(target_os = "linux")]
    {
        let _ = strata::launch_gtk(App, config);
    }

    #[cfg(not(target_os = "linux"))]
    {
        strata::app().with_window(config).build(App).run();
    }
}
