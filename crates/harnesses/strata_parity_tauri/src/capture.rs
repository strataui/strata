//! Window-scoped screenshot capture utility isolating application surface from OS elements.

use std::process::Command;

/// Captures a screenshot scoped exclusively to the specified window by title.
/// Filters out OS top panels, desktop docks, window decorations, and wallpapers.
pub fn capture_window_by_title(title: &str, output_path: &str) -> bool {
    let out = Command::new("xwininfo").args(["-root", "-tree"]).output();

    if let Ok(output) = out {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if line.contains(title) {
                if let Some(id) = line.split_whitespace().next() {
                    println!(
                        "[Capture] Target window resolved: ID {} for \"{}\"",
                        id, title
                    );
                    let status = Command::new("import")
                        .args(["-window", id, output_path])
                        .status();
                    if let Ok(s) = status {
                        if s.success() {
                            println!(
                                "[Capture] Successfully captured window-scoped screenshot to {}",
                                output_path
                            );
                            return true;
                        }
                    }
                }
            }
        }
    }

    // Direct fallback by name
    println!("[Capture] Fallback capturing by title name: {}", title);
    let status = Command::new("import")
        .args(["-window", title, output_path])
        .status();

    status.map(|s| s.success()).unwrap_or(false)
}
