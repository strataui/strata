use strata_cli::prelude::*;

fn is_emoji(ch: char) -> bool {
    let code = ch as u32;
    // Box drawing & blocks are allowed clean ASCII/Unicode art
    if (0x2500..=0x259F).contains(&code) || (0x25A0..=0x25FF).contains(&code) {
        return false;
    }
    (0x1F300..=0x1F6FF).contains(&code)
        || (0x1F900..=0x1FAFF).contains(&code)
        || (0x2600..=0x26FF).contains(&code)
        || (0x2700..=0x27BF).contains(&code)
        || (0x1F1E6..=0x1F1FF).contains(&code)
        || (0xFE0E..=0xFE0F).contains(&code)
        || code == 0x200D
        || code == 0x20E3
}

#[test]
fn test_cli_help_text_is_strictly_emoji_free() {
    let subcommands = [
        None,
        Some("new"),
        Some("dev"),
        Some("build"),
        Some("package"),
        Some("test"),
        Some("sim"),
    ];

    for sc in subcommands {
        let text = CliArgs::help_text(sc);
        for ch in text.chars() {
            assert!(
                !is_emoji(ch),
                "Help text for subcommand '{:?}' contained forbidden emoji: '{}' (U+{:04X})",
                sc,
                ch,
                ch as u32
            );
        }
    }
}
