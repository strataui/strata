//! Proves that tray and single-instance platform events reach the user event hook through the
//! shared dispatch seam, and that the native tray handle emits the matching platform events.

use std::sync::{Arc, Mutex};

use strata::prelude::*;
use strata::runtime::invoke_user_event_hook;
use strata_platform::{NativeTrayHandle, PlatformEvent, TrayEventSink, TrayIcon, TrayMenu};

fn trivial_view() -> View {
    rsx! {
        <div class="w-full h-full">
            <span>"app"</span>
        </div>
    }
}

#[test]
fn test_user_event_hook_receives_tray_and_single_instance_events() {
    let mut runtime = AppRuntime::new(trivial_view(), 320.0, 240.0, 1.0);
    let mut recorded: Vec<PlatformEvent> = Vec::new();

    {
        let mut hook = |event: &PlatformEvent, _runtime: &mut AppRuntime| -> bool {
            recorded.push(event.clone());
            matches!(event, PlatformEvent::TrayIconClicked)
        };

        let icon_result =
            invoke_user_event_hook(&PlatformEvent::TrayIconClicked, &mut runtime, &mut hook);
        assert!(
            icon_result,
            "hook return value must propagate for the tray click"
        );

        let menu_result = invoke_user_event_hook(
            &PlatformEvent::TrayMenuItemClicked(42),
            &mut runtime,
            &mut hook,
        );
        assert!(
            !menu_result,
            "hook return value must propagate for the menu action"
        );

        let instance_result = invoke_user_event_hook(
            &PlatformEvent::SingleInstanceTriggered(vec!["--open".to_string()]),
            &mut runtime,
            &mut hook,
        );
        assert!(
            !instance_result,
            "hook return value must propagate for single instance"
        );
    }

    assert_eq!(recorded.len(), 3);
    assert!(matches!(recorded[0], PlatformEvent::TrayIconClicked));
    assert!(matches!(
        recorded[1],
        PlatformEvent::TrayMenuItemClicked(42)
    ));
    match &recorded[2] {
        PlatformEvent::SingleInstanceTriggered(args) => {
            assert_eq!(args, &vec!["--open".to_string()]);
        }
        other => panic!("unexpected event: {:?}", other),
    }
}

#[test]
fn test_native_tray_handle_emits_platform_events() {
    let events = Arc::new(Mutex::new(Vec::<PlatformEvent>::new()));
    let sink_events = events.clone();
    let sink: TrayEventSink = Arc::new(move |event: PlatformEvent| {
        sink_events.lock().unwrap().push(event);
    });

    let mut menu = TrayMenu::new();
    menu.add_item(42, "Open");

    let mut icon = TrayIcon::new("Strata", Vec::new(), 16, 16);
    icon.set_menu(Some(menu));

    let mut handle = NativeTrayHandle::new(icon, sink);
    handle.trigger_click();
    assert!(handle.trigger_menu_action(42));
    assert!(!handle.trigger_menu_action(99));

    let recorded = events.lock().unwrap().clone();
    assert_eq!(
        recorded,
        vec![
            PlatformEvent::TrayIconClicked,
            PlatformEvent::TrayMenuItemClicked(42),
        ]
    );
}
