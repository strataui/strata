//! mdBook preprocessor binary entrypoint (`mdbook-strata`).

use std::env;
use std::io::{self, Read, Write};
use strata_mdbook::process_chapter_markdown;

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();

    // Check renderer compatibility handshake
    if args.len() > 1 && args[1] == "supports" {
        // We support html output
        if args.len() > 2 && args[2] == "html" {
            std::process::exit(0);
        } else {
            std::process::exit(0); // accept standard renderers
        }
    }

    // Read full book JSON payload from stdin
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    // If input is empty or invalid JSON, return gracefully
    if input.trim().is_empty() {
        return Ok(());
    }

    // Process chapters within the payload
    // mdBook JSON schema: [context, { "sections": [ ... { "Chapter": { "content": "..." } } ] }]
    let processed = if input.contains("```rsx live") {
        // Fast streaming replace of live code blocks inside content fields
        process_chapter_markdown(&input)
    } else {
        input
    };

    io::stdout().write_all(processed.as_bytes())?;
    io::stdout().flush()?;

    Ok(())
}
