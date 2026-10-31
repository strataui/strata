//! Strata Cloud SaaS Dashboard Example.

#![allow(non_snake_case, dead_code, unused_variables, unused_imports)]

mod app;
mod components;
mod state;

use app::App;
use strata::prelude::*;

fn main() {
    println!("[*] Starting Strata Cloud Enterprise Dashboard...");

    strata::launch(|| {
        App()
    });
}
