//! Strata Native Browser Example.
//!
//! A high-performance, multi-tab web browser written in pure Rust that renders
//! web documents directly through Strata's native layout engine and WebTranspiler,
//! completely free of Chromium, WebKit, or JavaScript engine overhead.

#![allow(non_snake_case, dead_code, unused_variables, unused_imports)]

mod components;
mod mock_pages;
mod state;

use components::BrowserApp;
use strata::prelude::*;

fn main() {
    println!("[*] Launching Strata Native Zero-Chromium Web Browser...");

    strata::launch(|| {
        BrowserApp()
    });
}
