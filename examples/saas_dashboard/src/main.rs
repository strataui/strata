//! Strata Cloud SaaS Dashboard Example.

#![allow(non_snake_case, dead_code, unused_variables, unused_imports)]

mod app;
mod components;
mod state;

use app::App;
use strata::prelude::*;

fn main() {
    println!("[*] Starting Strata Cloud Enterprise Dashboard...");

    #[cfg(target_os = "linux")]
    {
        let mut config = WindowConfig::default();
        config.title = "Strata Cloud Enterprise Dashboard".to_string();
        config.width = 1440;
        config.height = 900;
        let _ = strata::launch_gtk(|| App(), config);
    }

    #[cfg(not(target_os = "linux"))]
    {
        strata::launch(|| {
            App()
        });
    }
}
