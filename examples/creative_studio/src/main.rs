//! Strata Creative Studio Entrypoint.

#![allow(non_snake_case, dead_code, unused_variables, unused_imports)]

mod app;
mod components;
mod state;

use app::App;
use strata::prelude::*;

fn main() {
    println!("[*] Starting Strata Creative Studio in Pure Rust...");

    strata::launch(|| {
        App()
    });
}
