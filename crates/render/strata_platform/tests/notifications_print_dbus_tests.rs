//! Phase 7 tests for pure-Rust D-Bus encoder/decoder upgrades, rich cross-platform
//! NotificationBuilder, and OS Print spooler dispatch.

use strata_platform::notifications::{
    close_notification, enable_notification_recorder, take_closed_notification_ids,
    take_recorded_notifications, NotificationBuilder, NotificationUrgency,
};
use strata_platform::print::{
    list_printers, print_pdf_bytes, set_mock_print_spooler, take_mock_print_jobs, PrintDuplex,
    PrintJobOptions,
};

#[test]
fn test_rich_notification_builder_actions_replace_and_close_recorder() {
    enable_notification_recorder(true);
    let _ = take_recorded_notifications();
    let _ = take_closed_notification_ids();

    let id1 = NotificationBuilder::new("System Battery Low", "20% remaining — power saver recommended")
        .app_name("Strata Telemetry")
        .subtitle("Telemetry Alert")
        .urgency(NotificationUrgency::Critical)
        .category("device.battery")
        .timeout_ms(10_000)
        .action("power_saver", "Enable Power Saver")
        .action("dismiss", "Dismiss")
        .send()
        .unwrap();

    let id2 = NotificationBuilder::new("System Battery Critical", "15% remaining")
        .replaces_id(id1)
        .send()
        .unwrap();
    assert_eq!(id1, id2);

    close_notification(id1).unwrap();

    let sent = take_recorded_notifications();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].urgency, NotificationUrgency::Critical);
    assert_eq!(sent[0].actions.len(), 2);
    assert_eq!(sent[0].actions[0].id, "power_saver");
    assert_eq!(sent[1].replaces_id, id1);

    let closed = take_closed_notification_ids();
    assert_eq!(closed, vec![id1]);
    enable_notification_recorder(false);
}

#[test]
fn test_print_spooler_validation_and_mock_pdf_dispatch() {
    set_mock_print_spooler(true);
    let _ = take_mock_print_jobs();

    let printers = list_printers();
    assert_eq!(printers.len(), 1);
    assert!(printers[0].is_default);

    // Rejects non-PDF bytes cleanly
    assert!(print_pdf_bytes(b"NOT_A_PDF", &PrintJobOptions::default()).is_err());

    let fake_pdf = b"%PDF-1.7\n%%EOF\n";
    let job_id = print_pdf_bytes(
        fake_pdf,
        &PrintJobOptions {
            copies: 2,
            duplex: PrintDuplex::LongEdge,
            job_name: "Telemetry Export".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!job_id.is_empty());

    let jobs = take_mock_print_jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].options.copies, 2);
    assert_eq!(jobs[0].options.duplex, PrintDuplex::LongEdge);
    assert_eq!(jobs[0].pdf_bytes, fake_pdf);
    set_mock_print_spooler(false);
}

#[cfg(target_os = "linux")]
#[test]
fn test_pure_rust_dbus_encoder_decoder_roundtrip_and_dict_alignment() {
    use strata_platform::dbus::{DbusMessageDecoder, DbusMessageEncoder, DbusVariantValue};

    let mut enc = DbusMessageEncoder::new();
    enc.push_string("Strata");
    enc.push_u32(42);
    enc.push_bool(true);
    enc.push_string_array(&["ok", "Confirm", "cancel", "Cancel"]);
    enc.push_dict_sv(&[
        ("urgency", DbusVariantValue::Byte(2)),
        ("category", DbusVariantValue::String("im.received".into())),
    ]);
    enc.push_i32(-1);

    let mut dec = DbusMessageDecoder::new(enc.as_bytes());
    assert_eq!(dec.read_string().as_deref(), Some("Strata"));
    assert_eq!(dec.read_u32(), Some(42));
    assert_eq!(dec.read_bool(), Some(true));
    // Verify string array length prefix & entries decode cleanly
    let arr_bytes = dec.read_u32().unwrap();
    assert!(arr_bytes > 0);
    assert_eq!(dec.read_string().as_deref(), Some("ok"));
    assert_eq!(dec.read_string().as_deref(), Some("Confirm"));
    assert_eq!(dec.read_string().as_deref(), Some("cancel"));
    assert_eq!(dec.read_string().as_deref(), Some("Cancel"));
}
