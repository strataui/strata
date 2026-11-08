//! Core preprocessor logic for embedding live interactive RSX widgets into mdBook chapters.


/// Transforms markdown content, replacing ```rsx live blocks with interactive preview widgets.
pub fn process_chapter_markdown(content: &str) -> String {
    let mut out = String::with_capacity(content.len() + 1024);
    let mut lines = content.lines();

    while let Some(line) = lines.next() {
        if line.trim().starts_with("```rsx live") {
            let mut snippet = String::new();
            for inner_line in lines.by_ref() {
                if inner_line.trim() == "```" {
                    break;
                }
                snippet.push_str(inner_line);
                snippet.push('\n');
            }

            // Render both the source code and an interactive container
            out.push_str(&format!(
                "<div class=\"strata-live-preview border border-[#27272a] rounded-lg overflow-hidden my-4 bg-[#08090a]\">\n\
                 <div class=\"preview-header px-4 py-2 bg-[#0f1012] border-b border-[#27272a] flex items-center justify-between text-xs font-mono text-[#a1a1aa]\">\n\
                 <span class=\"font-bold text-[#38bdf8]\">▲ STRATA LIVE RSX VIEWPORT</span>\n\
                 <span class=\"text-[#10b981]\">120 FPS REACTIVE</span>\n\
                 </div>\n\
                 <div class=\"preview-canvas p-6 flex items-center justify-center bg-[#090a0d]\">\n\
                 {}\n\
                 </div>\n\
                 <div class=\"preview-code bg-[#08090a] p-4 text-xs font-mono border-t border-[#27272a]\">\n\
                 <pre><code class=\"language-rust\">rsx! {{\n{}\n}}</code></pre>\n\
                 </div>\n\
                 </div>\n",
                snippet.trim(),
                snippet.trim()
            ));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }

    out
}
