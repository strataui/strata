//! Integration tests for mdbook-strata interactive preprocessor.

use strata_mdbook::process_chapter_markdown;

#[test]
fn test_mdbook_preprocessor_live_rsx_transform() {
    let input = "# Chapter 1: Introduction\n\nHere is a live reactive component:\n\n```rsx live\n<div class=\"p-4 bg-[#08090a]\">\n    <button class=\"px-3 py-1 bg-emerald-500 font-bold\">\n        {\"Live Button\"}\n    </button>\n</div>\n```\n\nMore documentation text follows.";

    let output = process_chapter_markdown(input);

    assert!(output.contains("strata-live-preview"), "Output must contain live preview container");
    assert!(output.contains("STRATA LIVE RSX VIEWPORT"), "Output must contain live header banner");
    assert!(output.contains("Live Button"), "Output must preserve snippet content");
    assert!(output.contains("More documentation text follows."), "Output must preserve normal text");
}
