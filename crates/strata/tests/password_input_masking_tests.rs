use std::cell::RefCell;
use std::rc::Rc;

use strata::prelude::*;
use strata::render::compositor::DrawCommand;
use strata::runtime::EditCommand;
use strata_platform::PlatformEvent;

fn collect_drawn_texts(rt: &AppRuntime) -> Vec<String> {
    rt.display_list
        .commands()
        .iter()
        .filter_map(|cmd| match cmd {
            DrawCommand::DrawText { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn find_input_node<'a>(rt: &'a AppRuntime, idx: usize) -> &'a strata::layout::LayoutNode {
    let mut counter = 0;
    AppRuntime::find_input_layout(&rt.root_blueprint, &rt.layout_root, idx, None, &mut counter)
        .expect("input layout node must exist")
}

#[component]
fn PasswordFormApp(
    secret: String,
    input_type: String,
    placeholder: String,
    captured: Rc<RefCell<String>>,
) -> View {
    let on_in = move |val: String| {
        *captured.borrow_mut() = val;
    };
    rsx! {
        <div class="flex flex-col gap-2 w-[400px] h-[200px] p-4">
            <input
                id="pwd"
                type={input_type}
                value={secret}
                placeholder={placeholder}
                class="w-[300px] h-[36px] text-sm font-sans px-2"
                oninput=on_in
            />
            <input
                id="plain"
                type="text"
                value="visible"
                class="w-[300px] h-[36px] text-sm font-sans px-2"
            />
        </div>
    }
}

#[test]
fn password_input_masks_value_in_layout_and_display_list_while_preserving_placeholder() {
    let captured = Rc::new(RefCell::new(String::new()));
    let rt = AppRuntime::new(
        PasswordFormApp(
            "s3cr3t!".to_string(),
            "password".to_string(),
            "Enter password".to_string(),
            captured.clone(),
        ),
        400.0,
        200.0,
        1.0,
    );

    let pwd_layout = find_input_node(&rt, 0);
    let plain_layout = find_input_node(&rt, 1);

    assert_eq!(pwd_layout.text.as_deref(), Some("•••••••"));
    assert_eq!(plain_layout.text.as_deref(), Some("visible"));

    let drawn = collect_drawn_texts(&rt);
    assert!(
        drawn.iter().any(|t| t == "•••••••"),
        "display list should draw masked bullets, got: {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|t| t.contains("s3cr3t!")),
        "display list must never draw raw password plaintext, got: {drawn:?}"
    );
    assert!(drawn.iter().any(|t| t == "visible"));

    // Empty password input displays placeholder unmasked.
    let rt_empty = AppRuntime::new(
        PasswordFormApp(
            String::new(),
            "PASSWORD".to_string(),
            "Enter password".to_string(),
            captured.clone(),
        ),
        400.0,
        200.0,
        1.0,
    );
    let empty_layout = find_input_node(&rt_empty, 0);
    assert_eq!(empty_layout.text.as_deref(), Some("Enter password"));

    // Multibyte Unicode characters map 1-to-1 per scalar value, not UTF-8 byte length.
    let rt_unicode = AppRuntime::new(
        PasswordFormApp(
            "päss🔑".to_string(),
            " password ".to_string(),
            String::new(),
            captured,
        ),
        400.0,
        200.0,
        1.0,
    );
    let uni_layout = find_input_node(&rt_unicode, 0);
    assert_eq!(uni_layout.text.as_deref(), Some("•••••"));
}

#[test]
fn typing_and_toggling_password_visibility_stays_in_sync_while_focused() {
    let captured = Rc::new(RefCell::new(String::new()));
    let mut rt = AppRuntime::new(
        PasswordFormApp(
            "ab".to_string(),
            "password".to_string(),
            "Password".to_string(),
            captured.clone(),
        ),
        400.0,
        200.0,
        1.0,
    );

    // Click near the right edge of the password input to focus at the end.
    let b = find_input_node(&rt, 0).geometry.border_box;
    rt.dispatch_pointer_down(b.x + b.width - 4.0, b.y + b.height * 0.5);
    assert!(rt.active_focus.is_some());
    assert_eq!(find_input_node(&rt, 0).text.as_deref(), Some("••"));

    // Type 'c': oninput receives plaintext "abc", layout renders "•••".
    rt.dispatch_key_event(&PlatformEvent::ReceivedCharacter('c'));
    assert_eq!(*captured.borrow(), "abc");
    assert_eq!(find_input_node(&rt, 0).text.as_deref(), Some("•••"));
    rt.compile_display_list();
    let drawn = collect_drawn_texts(&rt);
    assert!(drawn.iter().any(|t| t == "•••"));
    assert!(!drawn.iter().any(|t| t.contains("abc")));

    // Toggle type="text" (like PasswordInput eye button) while still focused -> unmasked immediately.
    rt.update_view(PasswordFormApp(
        "abc".to_string(),
        "text".to_string(),
        "Password".to_string(),
        captured.clone(),
    ));
    assert_eq!(find_input_node(&rt, 0).text.as_deref(), Some("abc"));

    // Toggle back to type="password" while still focused -> re-masked immediately.
    rt.update_view(PasswordFormApp(
        "abc".to_string(),
        "password".to_string(),
        "Password".to_string(),
        captured,
    ));
    assert_eq!(find_input_node(&rt, 0).text.as_deref(), Some("•••"));
}

#[test]
fn password_caret_metrics_measure_masked_bullets_and_block_copy_cut() {
    let cap1 = Rc::new(RefCell::new(String::new()));
    let cap2 = Rc::new(RefCell::new(String::new()));
    let mut rt_narrow = AppRuntime::new(
        PasswordFormApp(
            "iiii".to_string(),
            "password".to_string(),
            String::new(),
            cap1,
        ),
        400.0,
        200.0,
        1.0,
    );
    let mut rt_wide = AppRuntime::new(
        PasswordFormApp(
            "WWWW".to_string(),
            "password".to_string(),
            String::new(),
            cap2,
        ),
        400.0,
        200.0,
        1.0,
    );

    let b1 = find_input_node(&rt_narrow, 0).geometry.border_box;
    rt_narrow.dispatch_pointer_down(b1.x + b1.width - 4.0, b1.y + b1.height * 0.5);
    let b2 = find_input_node(&rt_wide, 0).geometry.border_box;
    rt_wide.dispatch_pointer_down(b2.x + b2.width - 4.0, b2.y + b2.height * 0.5);

    let caret_narrow = rt_narrow
        .active_caret_rect()
        .expect("narrow password caret rect");
    let caret_wide = rt_wide
        .active_caret_rect()
        .expect("wide password caret rect");
    assert!(
        (caret_narrow.x - caret_wide.x).abs() < 0.01,
        "password caret must measure masked bullets identically regardless of plaintext glyph widths: {} vs {}",
        caret_narrow.x,
        caret_wide.x
    );

    // Copy and Cut are blocked on password inputs; SelectAll still works.
    assert!(rt_narrow.apply_edit_command(EditCommand::SelectAll));
    assert_eq!(
        rt_narrow
            .active_focus
            .as_ref()
            .unwrap()
            .state
            .selection_range(),
        Some((0, 4))
    );
    assert!(!rt_narrow.apply_edit_command(EditCommand::Copy));
    assert!(!rt_narrow.apply_edit_command(EditCommand::Cut));
    assert_eq!(
        rt_narrow.active_focus.as_ref().unwrap().state.text.get(),
        "iiii"
    );

    // Accessibility tree also masks password input value.
    rt_narrow.update_accessibility_tree();
    let tree = rt_narrow.a11y_tree.as_ref().expect("a11y tree");
    let pwd_a11y = tree
        .nodes
        .values()
        .find(|n| n.value.as_deref() == Some("••••"));
    assert!(
        pwd_a11y.is_some(),
        "a11y tree must expose masked value for password input"
    );
    assert!(
        !tree
            .nodes
            .values()
            .any(|n| n.value.as_deref() == Some("iiii")),
        "a11y tree must never expose plaintext password value"
    );
}
