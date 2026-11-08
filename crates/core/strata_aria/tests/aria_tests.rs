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

#[test]
fn test_w3c_aria_live_regions_and_announcements() {
    let announcer = LiveRegionAnnouncer::global();
    announcer.clear();

    // 1. Direct speech announcements
    let id_polite = announce_polite("Build succeeded in 4.2s");
    let id_assertive = announce_assertive("CRITICAL: Power supply disconnect detected");
    assert!(id_polite > 0);
    assert!(id_assertive > id_polite);
    assert_eq!(announcer.pending_count(), 2);

    let drained = announcer.drain_announcements();
    assert_eq!(drained.len(), 2);
    assert_eq!(drained[0].politeness, LiveRegionPoliteness::Polite);
    assert_eq!(drained[0].message, "Build succeeded in 4.2s");
    assert_eq!(drained[1].politeness, LiveRegionPoliteness::Assertive);
    assert_eq!(drained[1].message, "CRITICAL: Power supply disconnect detected");
    assert_eq!(announcer.pending_count(), 0);

    // 2. Reactive live region state & attributes
    let status_region = use_live_region(LiveRegionConfig {
        id: "status-bar".to_string(),
        role: Some("status".to_string()),
        politeness: LiveRegionPoliteness::Polite,
        atomic: true,
        relevant: LiveRegionRelevant::AdditionsText,
    });

    let attrs = status_region.attrs();
    assert!(attrs.iter().any(|(k, v)| *k == "role" && v == "status"));
    assert!(attrs.iter().any(|(k, v)| *k == "aria-live" && v == "polite"));
    assert!(attrs.iter().any(|(k, v)| *k == "aria-atomic" && v == "true"));
    assert!(attrs.iter().any(|(k, v)| *k == "aria-busy" && v == "false"));
    assert!(attrs.iter().any(|(k, v)| *k == "aria-relevant" && v == "additions text"));

    // Setting content triggers announcement
    status_region.set_content("3 new notifications received");
    assert_eq!(status_region.content.get(), "3 new notifications received");
    assert_eq!(announcer.pending_count(), 1);

    let drained2 = announcer.drain_announcements();
    assert_eq!(drained2.len(), 1);
    assert_eq!(drained2[0].message, "3 new notifications received");

    // When busy is true, announcements are suppressed until busy is cleared
    status_region.set_busy(true);
    status_region.set_content("Downloading update (45%)...");
    assert_eq!(announcer.pending_count(), 0); // suppressed because busy

    status_region.set_busy(false); // clearing busy announces latest content
    assert_eq!(announcer.pending_count(), 1);
    let drained3 = announcer.drain_announcements();
    assert_eq!(drained3[0].message, "Downloading update (45%)...");
}

