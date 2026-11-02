//! Strata Native Web Browser Entry Point.
//!
//! A high-performance, multi-tab web browser written in pure Rust that renders
//! web documents directly through Strata's native layout engine and WebTranspiler,
//! completely free of Chromium, WebKit, or JavaScript engine overhead.

#![allow(non_snake_case, dead_code, unused_variables, unused_imports)]

use std::io::{self, BufRead, Write};
use strata::prelude::*;
use strata_browser::components::BrowserApp;
use strata_browser::state::BrowserState;
use strata_browser::terminal_view::render_terminal_browser;

fn main() {
    println!("[*] Starting Strata Native Zero-Chromium Web Browser...");

    // 1. Initialize Strata Application Runtime and verify layout compilation
    strata::launch(|| BrowserApp());

    let state = BrowserState::new();

    // Check if command line arguments were provided (e.g. initial URL)
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && !args[1].starts_with('-') {
        state.navigate(&args[1]);
    }

    // 2. Render initial terminal view
    print!("{}", render_terminal_browser(&state));
    io::stdout().flush().unwrap();

    // 3. Check for demonstration flag or non-interactive mode
    if args.iter().any(|a| a == "--demo") {
        run_showcase_demo(&state);
        return;
    }

    // If running in an automated non-interactive terminal (e.g. piped or EOF)
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    println!("\nAvailable commands:");
    println!("  goto <url>     - Navigate to URL (or enter any search query)");
    println!("  newtab [url]   - Open a new tab");
    println!("  switch <id>    - Switch active tab by ID");
    println!("  close [id]     - Close tab by ID");
    println!("  back / fwd     - History navigation");
    println!("  theme <name>   - Change theme: dark, amoled, sepia, light");
    println!("  1..5           - Bookmark shortcuts (1:Google, 2:Wikipedia, 3:HN, 4:Docs, 5:About)");
    println!("  exit / quit    - Exit browser\n");

    loop {
        print!("strata-browser> ");
        if io::stdout().flush().is_err() {
            break;
        }

        let line = match lines.next() {
            Some(Ok(l)) => l,
            _ => break, // EOF reached
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed == "exit" || trimmed == "quit" || trimmed == "q" {
            println!("[*] Exiting Strata Native Browser.");
            break;
        }

        process_command(&state, trimmed);

        // Re-render browser interface
        print!("{}", render_terminal_browser(&state));
        io::stdout().flush().unwrap();
    }
}

fn process_command(state: &BrowserState, cmd: &str) {
    if let Some(url) = cmd.strip_prefix("goto ") {
        state.navigate(url);
    } else if let Some(url) = cmd.strip_prefix("navigate ") {
        state.navigate(url);
    } else if let Some(target) = cmd.strip_prefix("newtab ") {
        state.new_tab(target);
    } else if cmd == "newtab" {
        state.new_tab("strata://newtab");
    } else if let Some(id_str) = cmd.strip_prefix("switch ") {
        if let Ok(id) = id_str.trim().parse::<usize>() {
            state.switch_tab(id);
        }
    } else if let Some(id_str) = cmd.strip_prefix("tab ") {
        if let Ok(id) = id_str.trim().parse::<usize>() {
            state.switch_tab(id);
        }
    } else if let Some(id_str) = cmd.strip_prefix("close ") {
        if let Ok(id) = id_str.trim().parse::<usize>() {
            state.close_tab(id);
        }
    } else if cmd == "close" {
        let active_id = state.active_tab_id.get();
        state.close_tab(active_id);
    } else if cmd == "back" || cmd == "<" {
        state.go_back();
    } else if cmd == "fwd" || cmd == "forward" || cmd == ">" {
        state.go_forward();
    } else if cmd == "reload" || cmd == "r" {
        state.reload();
    } else if cmd == "home" || cmd == "h" {
        state.go_home();
    } else if let Some(theme) = cmd.strip_prefix("theme ") {
        state.set_theme(theme.trim());
    } else if cmd == "1" {
        state.navigate("strata://newtab");
    } else if cmd == "2" {
        state.navigate("https://en.wikipedia.org/wiki/Rust_(programming_language)");
    } else if cmd == "3" {
        state.navigate("https://news.ycombinator.com");
    } else if cmd == "4" {
        state.navigate("https://strata.larvance.com");
    } else if cmd == "5" {
        state.navigate("strata://about");
    } else if cmd == "help" {
        println!("Commands: goto <url>, newtab [url], switch <id>, close [id], back, fwd, reload, home, theme <name>, 1..5, exit");
    } else {
        // Direct query or URL entry
        state.navigate(cmd);
    }
}

fn run_showcase_demo(state: &BrowserState) {
    println!("\n[*] Executing Strata Native Browser Showcase Tour...");

    // Step 1: Open Wikipedia in Tab 2
    println!("\n[Tour 1/3] Opening Wikipedia: Rust (programming language) in Tab 2...");
    state.new_tab("https://en.wikipedia.org/wiki/Rust_(programming_language)");
    print!("{}", render_terminal_browser(state));

    // Step 2: Open Hacker News in Tab 3
    println!("\n[Tour 2/3] Opening Hacker News in Tab 3...");
    state.new_tab("https://news.ycombinator.com");
    print!("{}", render_terminal_browser(state));

    // Step 3: Switch back to Tab 1 (Google New Tab)
    println!("\n[Tour 3/3] Switching back to Tab 1 (Google New Tab Start Page)...");
    state.switch_tab(1);
    print!("{}", render_terminal_browser(state));

    println!("\n[OK] Strata Browser Showcase Tour complete: Multi-tab layout and native WebTranspiler operational.");
}
