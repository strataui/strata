//! Integration tests for WindowManager, Native Menus, Accelerators, and System Tray Badges.

use strata_platform::{
    AcceleratorModifier, KeyCode, MenuAccelerator, MenuBar, MenuItemRole, PlatformEvent, ThemeMode,
    TrayIcon, WindowConfig, WindowManager,
};

#[test]
fn test_window_manager_lifecycle_and_focus() {
    let mut wm = WindowManager::new();
    assert_eq!(wm.window_count(), 0);
    assert_eq!(wm.active_window(), None);

    let config1 = WindowConfig {
        title: "Main Dashboard".to_string(),
        width: 1280,
        height: 720,
        ..Default::default()
    };
    let w1 = wm.create_window(config1);
    assert_eq!(wm.window_count(), 1);
    assert_eq!(wm.active_window(), Some(w1));

    let config2 = WindowConfig {
        title: "Telemetry Monitor".to_string(),
        width: 800,
        height: 600,
        ..Default::default()
    };
    let w2 = wm.create_window(config2);
    assert_eq!(wm.window_count(), 2);
    // Active window remained w1 until changed
    assert_eq!(wm.active_window(), Some(w1));

    wm.set_active_window(w2).expect("Focus should switch to w2");
    assert_eq!(wm.active_window(), Some(w2));

    assert!(wm.get_window(w1).is_some());
    assert!(wm.get_window(w2).is_some());
}

#[test]
fn test_window_manager_modal_modality_and_cascade() {
    let mut wm = WindowManager::new();
    let parent = wm.create_window(WindowConfig::default());

    assert!(!wm.is_modal_blocked(parent));

    let modal_cfg = WindowConfig {
        title: "Confirm Cluster Migration".to_string(),
        width: 480,
        height: 320,
        ..Default::default()
    };
    let modal = wm.create_modal(parent, modal_cfg).expect("Modal creation should succeed");

    // Parent is now modal-blocked
    assert!(wm.is_modal_blocked(parent));
    // Modal is active window
    assert_eq!(wm.active_window(), Some(modal));

    // Attempting to focus parent should fail while modal is active
    let focus_res = wm.set_active_window(parent);
    assert!(focus_res.is_err());

    // Closing modal restores parent focus and unblocks parent
    wm.close_window(modal).expect("Close modal should succeed");
    assert!(!wm.is_modal_blocked(parent));
    assert_eq!(wm.active_window(), Some(parent));
}

#[test]
fn test_window_manager_event_broadcast_and_poll() {
    let mut wm = WindowManager::new();
    let w1 = wm.create_window(WindowConfig::default());
    let w2 = wm.create_window(WindowConfig::default());

    // Broadcast theme change
    wm.broadcast_event(PlatformEvent::ThemeChanged(ThemeMode::Dark));

    let polled = wm.poll_all_events();
    assert_eq!(polled.len(), 2);

    let w1_events: Vec<_> = polled.iter().filter(|(id, _)| *id == w1).collect();
    let w2_events: Vec<_> = polled.iter().filter(|(id, _)| *id == w2).collect();
    assert_eq!(w1_events.len(), 1);
    assert_eq!(w2_events.len(), 1);
    assert_eq!(w1_events[0].1, PlatformEvent::ThemeChanged(ThemeMode::Dark));
}

#[test]
fn test_menu_accelerators_and_roles() {
    let acc1 = MenuAccelerator::parse("CmdOrCtrl+S").expect("Parsing CmdOrCtrl+S should succeed");
    assert_eq!(acc1.modifiers, vec![AcceleratorModifier::CmdOrCtrl]);
    assert_eq!(acc1.key, KeyCode::KeyS);
    assert_eq!(acc1.to_display_string(true), "⌘S");
    assert_eq!(acc1.to_display_string(false), "Ctrl+S");

    let acc2 = MenuAccelerator::parse("Ctrl+Shift+P").expect("Parsing Ctrl+Shift+P should succeed");
    assert_eq!(acc2.modifiers, vec![AcceleratorModifier::Ctrl, AcceleratorModifier::Shift]);
    assert_eq!(acc2.key, KeyCode::KeyP);
    assert_eq!(acc2.to_display_string(false), "Ctrl+Shift+P");

    let bar = MenuBar::standard_app_menu("Strata Enterprise");
    assert_eq!(bar.menus.len(), 5); // File, Edit, View, Window, Help

    let file_menu = &bar.menus[0];
    assert_eq!(file_menu.title, "File");
    assert!(file_menu.find_item(103).is_some()); // Save item
    assert_eq!(file_menu.find_item(103).unwrap().label, "Save");

    let edit_menu = &bar.menus[1];
    assert_eq!(edit_menu.title, "Edit");
    assert!(edit_menu.items.iter().any(|i| i.role == MenuItemRole::Undo));
    assert!(edit_menu.items.iter().any(|i| i.role == MenuItemRole::Copy));
}

#[test]
fn test_tray_icon_badges_and_notifications() {
    let mut tray = TrayIcon::new("Strata Agent Status", vec![0u8; 16], 2, 2);
    assert_eq!(tray.badge(), None);

    tray.set_badge(Some("ALERT".to_string()));
    assert_eq!(tray.badge(), Some("ALERT"));

    tray.set_badge_count(Some(5));
    assert_eq!(tray.badge(), Some("5"));

    tray.set_badge_count(Some(0));
    assert_eq!(tray.badge(), None);
}
