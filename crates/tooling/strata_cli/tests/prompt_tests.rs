use std::io::Cursor;
use strata_cli::prelude::*;

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&next_ch) = chars.peek() {
                    chars.next();
                    if next_ch.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else if ch != '\r' {
            out.push(ch);
        }
    }
    out
}

#[test]
fn test_select_prompt_arrow_navigation() {
    let prompt = SelectPrompt::new("Which template would you like to start with?")
        .item(
            TemplateKind::Blank,
            "Blank Canvas",
            Some("Minimal reactive canvas"),
        )
        .item(
            TemplateKind::Saas,
            "SaaS Dashboard",
            Some("Cloud & telemetry portal"),
        )
        .item(
            TemplateKind::Ecommerce,
            "E-Commerce",
            Some("Storefront & cart drawer"),
        )
        .item(
            TemplateKind::Studio,
            "Creative Studio",
            Some("Vector canvas & 3D PBR editor"),
        );

    // Sequence: Down arrow (\x1b[B) -> 1, Down arrow (\x1b[B) -> 2, Up arrow (\x1b[A) -> 1, Enter (\r)
    let input_bytes = b"\x1b[B\x1b[B\x1b[A\r";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let result = prompt.run(&mut reader, &mut writer, true).unwrap();
    assert_eq!(result, TemplateKind::Saas);

    let output = String::from_utf8(writer).unwrap();
    let clean = strip_ansi(&output);
    assert!(clean.contains("√ Which template would you like to start with? › SaaS Dashboard"));
    // Verify in-place collapse escape sequence
    assert!(output.contains("\x1b[4A\r\x1b[J"));
}

#[test]
fn test_select_prompt_vim_keys_navigation() {
    let prompt = SelectPrompt::new("Which template would you like to start with?")
        .item(TemplateKind::Blank, "Blank Canvas", None::<String>)
        .item(TemplateKind::Saas, "SaaS Dashboard", None::<String>)
        .item(TemplateKind::Ecommerce, "E-Commerce", None::<String>)
        .item(TemplateKind::Studio, "Creative Studio", None::<String>);

    // Sequence: 'j' (down) -> 1, 'j' -> 2, 'j' -> 3, 'k' (up) -> 2, Enter (\n)
    let input_bytes = b"jjjk\n";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let result = prompt.run(&mut reader, &mut writer, true).unwrap();
    assert_eq!(result, TemplateKind::Ecommerce);

    let output = String::from_utf8(writer).unwrap();
    let clean = strip_ansi(&output);
    assert!(clean.contains("√ Which template would you like to start with? › E-Commerce"));
}

#[test]
fn test_select_prompt_digit_jump() {
    let prompt = SelectPrompt::new("Which template would you like to start with?")
        .item(TemplateKind::Blank, "Blank Canvas", None::<String>)
        .item(TemplateKind::Saas, "SaaS Dashboard", None::<String>)
        .item(TemplateKind::Ecommerce, "E-Commerce", None::<String>)
        .item(TemplateKind::Studio, "Creative Studio", None::<String>);

    // Jump to item 4 directly, then Enter
    let input_bytes = b"4\r";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let result = prompt.run(&mut reader, &mut writer, true).unwrap();
    assert_eq!(result, TemplateKind::Studio);

    let output = String::from_utf8(writer).unwrap();
    let clean = strip_ansi(&output);
    assert!(clean.contains("√ Which template would you like to start with? › Creative Studio"));
}

#[test]
fn test_select_prompt_wrap_around() {
    let prompt = SelectPrompt::new("Which template would you like to start with?")
        .item(TemplateKind::Blank, "Blank Canvas", None::<String>)
        .item(TemplateKind::Saas, "SaaS Dashboard", None::<String>)
        .item(TemplateKind::Ecommerce, "E-Commerce", None::<String>)
        .item(TemplateKind::Studio, "Creative Studio", None::<String>);

    // Up arrow at item 0 wraps to item 3 (Studio), then Enter
    let input_bytes = b"\x1b[A\r";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let result = prompt.run(&mut reader, &mut writer, true).unwrap();
    assert_eq!(result, TemplateKind::Studio);
}

#[test]
fn test_text_prompt_interactive_typing_and_backspace() {
    let prompt = TextPrompt::new("What is your project named?")
        .with_default("strata-app");

    // Types 'a', 'b', 'c', backspace (\x7f), 'd', Enter (\r) -> "abd"
    let input_bytes = b"abc\x7fd\r";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let result = prompt.run(&mut reader, &mut writer, true).unwrap();
    assert_eq!(result, "abd");

    let output = String::from_utf8(writer).unwrap();
    let clean = strip_ansi(&output);
    assert!(clean.contains("√ What is your project named? › abd"));
}

#[test]
fn test_text_prompt_default_on_empty_enter() {
    let prompt = TextPrompt::new("What is your project named?")
        .with_default("my-default-app");

    let input_bytes = b"\r";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let result = prompt.run(&mut reader, &mut writer, true).unwrap();
    assert_eq!(result, "my-default-app");

    let output = String::from_utf8(writer).unwrap();
    let clean = strip_ansi(&output);
    assert!(clean.contains("√ What is your project named? › my-default-app"));
}

#[test]
fn test_confirm_prompt_interactive_yes_no_and_default() {
    let prompt = ConfirmPrompt::new("Would you like to use Tailwind CSS?")
        .with_default(true);

    // Enter uses default (true)
    let mut reader = Cursor::new(b"\r");
    let mut writer = Vec::new();
    assert!(prompt.run(&mut reader, &mut writer, true).unwrap());
    let out = String::from_utf8(writer).unwrap();
    let clean = strip_ansi(&out);
    assert!(clean.contains("√ Would you like to use Tailwind CSS? › Yes"));

    // 'n' selects false
    let mut reader = Cursor::new(b"n");
    let mut writer = Vec::new();
    assert!(!prompt.run(&mut reader, &mut writer, true).unwrap());
    let out = String::from_utf8(writer).unwrap();
    let clean = strip_ansi(&out);
    assert!(clean.contains("√ Would you like to use Tailwind CSS? › No"));

    // 'y' selects true
    let mut reader = Cursor::new(b"y");
    let mut writer = Vec::new();
    assert!(prompt.run(&mut reader, &mut writer, true).unwrap());
    let out = String::from_utf8(writer).unwrap();
    let clean = strip_ansi(&out);
    assert!(clean.contains("√ Would you like to use Tailwind CSS? › Yes"));
}

#[test]
fn test_multiselect_prompt_space_toggle_and_arrows() {
    let prompt = MultiSelectPrompt::new("Which target platform do you want to target?")
        .item(BuildTarget::Desktop, "Desktop", Some("Native"), true)
        .item(BuildTarget::WebWasm, "Web", Some("Wasm"), false)
        .item(BuildTarget::Android, "Mobile", Some("NDK"), false);

    // Sequence: Down arrow (\x1b[B) to Web, Space to toggle [x], Enter (\r)
    let input_bytes = b"\x1b[B \r";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let result = prompt.run(&mut reader, &mut writer, true).unwrap();
    assert_eq!(result, vec![BuildTarget::Desktop, BuildTarget::WebWasm]);

    let output = String::from_utf8(writer).unwrap();
    let clean = strip_ansi(&output);
    assert!(clean.contains("√ Which target platform do you want to target? › Desktop, Web"));
}

#[test]
fn test_multiselect_prompt_digit_toggle_and_stream() {
    let prompt = MultiSelectPrompt::new("Which target platform do you want to target?")
        .item(BuildTarget::Desktop, "Desktop", None::<String>, true)
        .item(BuildTarget::WebWasm, "Web", None::<String>, false)
        .item(BuildTarget::Android, "Mobile", None::<String>, false);

    // Stream mode with "2,3\n"
    let input_bytes = b"2,3\n";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let result = prompt.run(&mut reader, &mut writer, false).unwrap();
    assert_eq!(result, vec![BuildTarget::WebWasm, BuildTarget::Android]);

    let output = String::from_utf8(writer).unwrap();
    let clean = strip_ansi(&output);
    assert!(clean.contains("√ Which target platform do you want to target? › Web, Mobile"));
}

#[test]
fn test_multiselect_prompt_universal_selection() {
    let prompt = MultiSelectPrompt::new("Which target platform do you want to target?")
        .item(BuildTarget::Desktop, "Desktop", None::<String>, true)
        .item(BuildTarget::WebWasm, "Web", None::<String>, false)
        .item(BuildTarget::Android, "Mobile", None::<String>, false)
        .item(BuildTarget::Desktop, "Universal", None::<String>, false);

    // Press '4' (toggles Universal), then Enter
    let input_bytes = b"4\r";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let result = prompt.run(&mut reader, &mut writer, true).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(
        result,
        vec![
            BuildTarget::Desktop,
            BuildTarget::WebWasm,
            BuildTarget::Android
        ]
    );

    let output = String::from_utf8(writer).unwrap();
    let clean = strip_ansi(&output);
    assert!(clean.contains("√ Which target platform do you want to target? › Universal"));
    assert!(!clean.contains("Desktop, Web, Mobile, Universal"));
}

#[test]
fn test_multiselect_unselecting_item_unselects_universal() {
    let prompt = MultiSelectPrompt::new("Which target platform do you want to target?")
        .item(BuildTarget::Desktop, "Desktop", None::<String>, true)
        .item(BuildTarget::WebWasm, "Web", None::<String>, true)
        .item(BuildTarget::Android, "Mobile", None::<String>, true)
        .item(BuildTarget::Desktop, "Universal", None::<String>, true);

    // Initial state: all selected. Cursor is at 0 (Desktop).
    // Sequence: Down arrow (\x1b[B) to Web (index 1), Space to toggle [ ] (unselects Web and Universal), Enter (\r)
    let input_bytes = b"\x1b[B \r";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let result = prompt.run(&mut reader, &mut writer, true).unwrap();
    assert_eq!(result, vec![BuildTarget::Desktop, BuildTarget::Android]);

    let output = String::from_utf8(writer).unwrap();
    let clean = strip_ansi(&output);
    assert!(clean.contains("√ Which target platform do you want to target? › Desktop, Mobile"));
    assert!(!clean.contains("√ Which target platform do you want to target? › Universal"));
    assert!(!clean.contains("Desktop, Mobile, Universal"));
}

#[test]
fn test_multiselect_selecting_all_items_selects_universal() {
    let prompt = MultiSelectPrompt::new("Which target platform do you want to target?")
        .item(BuildTarget::Desktop, "Desktop", None::<String>, true)
        .item(BuildTarget::WebWasm, "Web", None::<String>, true)
        .item(BuildTarget::Android, "Mobile", None::<String>, false)
        .item(BuildTarget::Desktop, "Universal", None::<String>, false);

    // Initial: Desktop [x], Web [x], Mobile [ ], Universal [ ]
    // Cursor at 0. Down 2 times to Mobile, Space to toggle [x] Mobile -> should automatically sync Universal to [x]
    let input_bytes = b"\x1b[B\x1b[B \r";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let result = prompt.run(&mut reader, &mut writer, true).unwrap();
    assert_eq!(
        result,
        vec![
            BuildTarget::Desktop,
            BuildTarget::WebWasm,
            BuildTarget::Android
        ]
    );

    let output = String::from_utf8(writer).unwrap();
    let clean = strip_ansi(&output);
    assert!(clean.contains("√ Which target platform do you want to target? › Universal"));
}

#[test]
fn test_select_prompt_ctrl_c_cancellation() {
    let prompt = SelectPrompt::new("Which template would you like to start with?")
        .item(TemplateKind::Blank, "Blank Canvas", None::<String>)
        .item(TemplateKind::Saas, "SaaS Dashboard", None::<String>);

    // Send Ctrl+C (byte 0x03)
    let input_bytes = b"\x03";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let err = prompt.run(&mut reader, &mut writer, true).unwrap_err();
    assert!(err.contains("cancelled by user"));
}

#[test]
fn test_text_prompt_ctrl_c_cancellation() {
    let prompt = TextPrompt::new("What is your project named?")
        .with_default("strata-app");

    // Send Ctrl+C (byte 0x03)
    let input_bytes = b"\x03";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let err = prompt.run(&mut reader, &mut writer, true).unwrap_err();
    assert!(err.contains("cancelled by user"));
}

#[test]
fn test_multiselect_prompt_ctrl_c_cancellation() {
    let prompt = MultiSelectPrompt::new("Which target platform do you want to target?")
        .item(BuildTarget::Desktop, "Desktop", None::<String>, true);

    // Send Ctrl+C (byte 0x03)
    let input_bytes = b"\x03";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let err = prompt.run(&mut reader, &mut writer, true).unwrap_err();
    assert!(err.contains("cancelled by user"));
}

#[test]
fn test_confirm_prompt_ctrl_c_cancellation() {
    let prompt = ConfirmPrompt::new("Proceed with operation?");

    // Send Ctrl+C (byte 0x03)
    let input_bytes = b"\x03";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let err = prompt.run(&mut reader, &mut writer, true).unwrap_err();
    assert!(err.contains("cancelled by user"));
}

