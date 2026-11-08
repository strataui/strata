//! Integration tests for the desktop system tray controller and the
//! single-instance IPC lock.

use std::sync::{Arc, Mutex};
use strata_platform::events::PlatformEvent;
use strata_platform::single_instance::{SingleInstanceLock, SingleInstanceStatus};
use strata_platform::tray::{TrayIcon, TrayMenu, TrayMenuItem};

fn temp_runtime_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "strata-single-instance-{}-{}-{}",
        std::process::id(),
        tag,
        sequence
    ))
}

#[cfg(unix)]
#[test]
fn unix_single_instance_recovers_stale_socket_and_forwards_arguments() {
    use std::os::unix::net::UnixListener;
    use std::sync::mpsc;
    use std::time::Duration;

    let runtime_dir = temp_runtime_dir("stale");
    std::fs::create_dir_all(&runtime_dir).expect("create private runtime dir");

    let app_id = "com.strata.test.stale";
    let socket_path = runtime_dir.join(format!("{}.sock", app_id));
    {
        let _stale = UnixListener::bind(&socket_path).expect("bind stale listener");
    }
    assert!(
        socket_path.exists(),
        "dropping a UnixListener must leave the socket file behind for this test"
    );

    let (sender, receiver) = mpsc::channel::<Vec<String>>();
    let first = SingleInstanceLock::acquire_in_dir(app_id, &[], &runtime_dir, move |forwarded| {
        let _ = sender.send(forwarded);
    })
    .expect("primary acquire");
    let lock = match first {
        SingleInstanceStatus::Primary(lock) => lock,
        SingleInstanceStatus::SecondaryExited => {
            panic!("stale socket should be reclaimed by the first instance")
        }
    };

    let second_args = vec!["--open".to_string(), "circuit.eda".to_string()];
    let second = SingleInstanceLock::acquire_in_dir(app_id, &second_args, &runtime_dir, |_| {})
        .expect("secondary acquire");
    assert!(matches!(second, SingleInstanceStatus::SecondaryExited));

    let forwarded = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("primary should receive forwarded arguments");
    assert_eq!(forwarded, second_args);
    assert_eq!(lock.app_id(), app_id);

    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(&socket_path).expect("socket metadata");
    assert_eq!(meta.permissions().mode() & 0o777, 0o600, "socket permissions must be 0600");

    drop(lock);
    let _ = std::fs::remove_dir_all(&runtime_dir);
}

#[cfg(unix)]
#[test]
fn unix_single_instance_second_acquire_is_secondary() {
    let runtime_dir = temp_runtime_dir("duplicate");
    std::fs::create_dir_all(&runtime_dir).expect("create private runtime dir");

    let app_id = "com.strata.test.duplicate";
    let first = SingleInstanceLock::acquire_in_dir(app_id, &[], &runtime_dir, |_| {})
        .expect("primary acquire");
    let lock = match first {
        SingleInstanceStatus::Primary(lock) => lock,
        SingleInstanceStatus::SecondaryExited => panic!("first instance must be primary"),
    };

    let second = SingleInstanceLock::acquire_in_dir(app_id, &[], &runtime_dir, |_| {})
        .expect("second acquire");
    assert!(matches!(second, SingleInstanceStatus::SecondaryExited));

    drop(lock);
    let _ = std::fs::remove_dir_all(&runtime_dir);
}

#[test]
fn tray_handle_only_dispatches_click_and_menu_events() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let worker_events = Arc::clone(&events);

    // The handle is created on a dedicated worker thread so the test remains
    // valid even when another test has initialised GTK: the GTK main thread is
    // process-wide and the worker is never it.
    let backend = std::thread::spawn(move || {
        let mut menu = TrayMenu::new();
        menu.add_item(7, "Open Window")
            .add_separator()
            .add_submenu(
                8,
                "Settings",
                vec![
                    TrayMenuItem::item(81, "Dark Theme").with_checked(true),
                    TrayMenuItem::item(82, "Quiet Mode").with_enabled(false),
                ],
            )
            .add_item(9, "Quit");

        let mut icon = TrayIcon::new("Strata Tray Test", vec![255u8; 16 * 16 * 4], 16, 16);
        icon.set_menu(Some(menu));

        let mut handle = icon.spawn_native(move |event| {
            worker_events.lock().unwrap().push(event);
        });

        assert_eq!(handle.backend_name(), "handle-only");
        handle.set_tooltip("Updated tooltip");
        handle.set_badge(Some("NEW".to_string()));
        handle.set_visible(false);

        handle.trigger_click();
        assert!(handle.trigger_menu_action(7));
        assert!(handle.trigger_menu_action(81));
        assert!(!handle.trigger_menu_action(82));
        assert!(!handle.trigger_menu_action(404));

        assert_eq!(handle.icon().click_count, 1);
        assert_eq!(handle.icon().last_action_id, Some(81));

        handle.backend_name()
    })
    .join()
    .expect("tray worker thread");

    assert_eq!(backend, "handle-only");
    let recorded = events.lock().unwrap().clone();
    assert_eq!(
        recorded,
        vec![
            PlatformEvent::TrayIconClicked,
            PlatformEvent::TrayMenuItemClicked(7),
            PlatformEvent::TrayMenuItemClicked(81),
        ]
    );
}

#[cfg(target_os = "linux")]
fn list_session_bus_names() -> Option<Vec<String>> {
    let output = std::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.DBus",
            "--object-path",
            "/org/freedesktop/DBus",
            "--method",
            "org.freedesktop.DBus.ListNames",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(parse_gdbus_quoted(&String::from_utf8_lossy(&output.stdout)))
}

#[cfg(target_os = "linux")]
fn registered_status_notifier_items() -> Option<Vec<String>> {
    let output = std::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.kde.StatusNotifierWatcher",
            "--object-path",
            "/StatusNotifierWatcher",
            "--method",
            "org.freedesktop.DBus.Properties.Get",
            "org.kde.StatusNotifierWatcher",
            "RegisteredStatusNotifierItems",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(parse_gdbus_quoted(&String::from_utf8_lossy(&output.stdout)))
}

#[cfg(target_os = "linux")]
fn parse_gdbus_quoted(output: &str) -> Vec<String> {
    output
        .split('\'')
        .enumerate()
        .filter_map(|(index, token)| {
            if index % 2 == 1 {
                Some(token.to_string())
            } else {
                None
            }
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn pump_gtk() {
    while strata_platform::gtk::events_pending() {
        strata_platform::gtk::main_iteration_do(false);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn linux_ayatana_indicator_registers_status_notifier_item() {
    use std::time::{Duration, Instant};

    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("[skip] live tray test: DISPLAY and WAYLAND_DISPLAY are both unset");
        return;
    }
    if strata_platform::gtk::init().is_err() {
        eprintln!("[skip] live tray test: gtk::init failed, no usable display");
        return;
    }

    let has_session_bus = std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some();
    let names_before = if has_session_bus {
        list_session_bus_names()
    } else {
        None
    };
    let items_before = if has_session_bus {
        registered_status_notifier_items()
    } else {
        None
    };

    let mut rgba = Vec::with_capacity(32 * 32 * 4);
    for y in 0..32u8 {
        for x in 0..32u8 {
            let accent = if (8..24).contains(&x) && (8..24).contains(&y) {
                220
            } else {
                48
            };
            rgba.extend_from_slice(&[accent, 120, 90, 255]);
        }
    }

    let mut menu = TrayMenu::new();
    menu.add_item(101, "Show Window")
        .add_separator()
        .add_submenu(
            102,
            "Settings",
            vec![
                TrayMenuItem::item(1021, "Dark Theme").with_checked(true),
                TrayMenuItem::item(1022, "Quiet Mode"),
            ],
        )
        .add_item(103, "Quit");

    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let mut icon = TrayIcon::new("Strata Tray Test", rgba, 32, 32);
    icon.set_menu(Some(menu));
    let mut handle = icon.spawn_native(move |event| {
        sink.lock().unwrap().push(event);
    });

    let live_backend = handle.backend_name() == "ayatana-appindicator";
    if !live_backend {
        eprintln!(
            "[skip] live tray check: the Ayatana AppIndicator library could not be loaded on this host"
        );
    }

    handle.set_badge(Some("3".to_string()));
    handle.set_tooltip("Strata Tray Test Updated");
    handle.set_visible(true);
    pump_gtk();

    if !live_backend {
        eprintln!("[skip] session bus check: no live indicator backend");
    } else if !has_session_bus {
        eprintln!("[skip] session bus check: DBUS_SESSION_BUS_ADDRESS is unset");
    } else if let Some(items_before) = items_before {
        // libayatana-appindicator 0.5.93 registers its object under
        // /org/ayatana/NotificationItem/<clean-id> through the watcher and does
        // not own a well-known bus name. Older releases own
        // org.freedesktop.StatusNotifierItem-<pid>-<n>; both are checked.
        let clean_id = format!("strata-tray-{}", std::process::id())
            .replace(|ch: char| !ch.is_ascii_alphanumeric(), "_");
        let expected_suffix = format!("/org/ayatana/NotificationItem/{}", clean_id);

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut registered: Option<String> = None;
        while Instant::now() < deadline {
            pump_gtk();
            if let Some(items) = registered_status_notifier_items() {
                if let Some(item) = items
                    .iter()
                    .find(|item| item.ends_with(&expected_suffix) && !items_before.contains(item))
                {
                    registered = Some(format!("watcher item {}", item));
                    break;
                }
            }
            if let Some(names) = list_session_bus_names() {
                if let Some(name) = names.iter().find(|name| {
                    name.contains("StatusNotifierItem")
                        && !names_before
                            .as_ref()
                            .is_none_or(|before| before.contains(name))
                }) {
                    registered = Some(format!("bus name {}", name));
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        match registered {
            Some(mechanism) => {
                eprintln!("[ok] live indicator registered through {}", mechanism);
            }
            None => {
                let items = registered_status_notifier_items().unwrap_or_default();
                let names = list_session_bus_names().unwrap_or_default();
                panic!(
                    "indicator did not register on the session bus; expected {}; watcher items: {:?}; bus names: {:?}",
                    expected_suffix, items, names
                );
            }
        }
    } else {
        eprintln!(
            "[skip] session bus check: org.kde.StatusNotifierWatcher is unavailable on this session bus"
        );
    }

    // Native maximize round trip. It shares this test's gtk::init() and GTK
    // thread, so no second GTK test thread is created.
    {
        use strata_platform::gtk::prelude::*;

        let window = strata_platform::gtk::Window::new(strata_platform::gtk::WindowType::Toplevel);
        window.set_title("Strata Maximize Round Trip");
        window.set_default_size(640, 480);
        window.show_all();
        strata_platform::gtk_window::gtk_register_window(window.clone());
        pump_gtk();

        let maximized = strata_platform::gtk_toggle_maximize();
        pump_gtk();
        let observed_maximized = strata_platform::gtk_is_maximized();
        eprintln!(
            "[info] first gtk_toggle_maximize returned {}; gtk_is_maximized={} (window.is_maximized={})",
            maximized,
            observed_maximized,
            window.is_maximized()
        );
        assert!(
            maximized,
            "first gtk_toggle_maximize should report maximized"
        );
        assert!(
            observed_maximized,
            "gtk_is_maximized should be true after maximizing"
        );

        let restored = strata_platform::gtk_toggle_maximize();
        // The window manager applies unmaximize asynchronously; allow a bounded
        // settle window before checking the restored state.
        let restore_deadline = Instant::now() + Duration::from_secs(2);
        while strata_platform::gtk_is_maximized() && Instant::now() < restore_deadline {
            pump_gtk();
            std::thread::sleep(Duration::from_millis(20));
        }
        pump_gtk();
        let observed_restored = strata_platform::gtk_is_maximized();
        eprintln!(
            "[info] second gtk_toggle_maximize returned {}; gtk_is_maximized={} (window.is_maximized={})",
            restored,
            observed_restored,
            window.is_maximized()
        );
        assert!(
            !restored,
            "second gtk_toggle_maximize should report restored"
        );
        assert!(
            !observed_restored,
            "gtk_is_maximized should be false after restoring"
        );

        window.hide();
        drop(window);
        pump_gtk();
    }

    drop(handle);
    pump_gtk();

    let icon_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let prefix = format!("strata-tray-{}-", std::process::id());
    let leftovers: Vec<std::path::PathBuf> = std::fs::read_dir(&icon_dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
                .map(|entry| entry.path())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        leftovers.is_empty(),
        "tray icon PNG files should be removed when the handle is dropped: {:?}",
        leftovers
    );
}
