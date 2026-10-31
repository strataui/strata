use strata_aria::prelude::*;

#[test]
fn test_aria_modal_dialog_focus_integration() {
    let dialog = use_dialog(DialogConfig {
        id: "settings-dialog".to_string(),
        close_on_escape: true,
        close_on_outside_click: true,
    });

    let mut trap = FocusTrap::new(vec![
        "name-field".to_string(),
        "save-button".to_string(),
        "close-button".to_string(),
    ]);

    // Open modal and activate focus trap
    dialog.open_dialog();
    trap.activate(Some("open-trigger".to_string()));

    assert!(dialog.open.get());
    assert_eq!(trap.current_target(), Some("name-field".to_string()));

    // Tab moves focus within trap
    assert_eq!(trap.handle_key("Tab", false), Some("save-button".to_string()));
    assert_eq!(trap.handle_key("Tab", false), Some("close-button".to_string()));
    assert_eq!(trap.handle_key("Tab", false), Some("name-field".to_string()));

    // Escape closes dialog and restores focus
    assert!(dialog.handle_key("Escape"));
    assert!(!dialog.open.get());

    let restored = trap.deactivate();
    assert_eq!(restored, Some("open-trigger".to_string()));
}

#[test]
fn test_aria_dropdown_floating_collision_integration() {
    let anchor = FloatRect::new(500.0, 980.0, 120.0, 36.0);
    let popup = FloatRect::new(0.0, 0.0, 200.0, 150.0);
    let viewport = FloatRect::new(0.0, 0.0, 1920.0, 1080.0);

    let result = compute_position(
        anchor,
        popup,
        ComputePositionOptions {
            placement: Placement::BottomStart,
            offset: 6.0,
            flip: true,
            shift: true,
            viewport,
        },
    );

    // Should flip to TopStart because 980 + 36 + 6 + 150 = 1172 > 1080
    assert_eq!(result.actual_placement, Placement::TopStart);
    assert_eq!(result.x, 500.0);
    assert_eq!(result.y, 980.0 - 150.0 - 6.0); // 824.0
}
