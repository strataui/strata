//! Integration tests for cross-platform native dialogs.

use std::path::PathBuf;
use strata_platform::dialog::{
    open_file, open_multiple_files, pick_folder, save_file, set_dialog_mock_response,
    show_message_box, DialogFilter, DialogMockResponse, MessageBoxButtons, MessageBoxResult,
    MessageBoxType, OpenFileOptions, PickFolderOptions, SaveFileOptions,
};

#[test]
fn test_dialog_filter_sanitizes_extensions() {
    let filter = DialogFilter::new("Images", &[".png", "jpg", ".JPEG", "webp"]);
    assert_eq!(filter.name, "Images");
    assert_eq!(filter.extensions, vec!["png", "jpg", "JPEG", "webp"]);
}

#[test]
fn test_open_file_options_builder() {
    let options = OpenFileOptions::new()
        .with_title("Select Project")
        .with_directory("/home/user/projects")
        .with_filter("Rust Source", &[".rs"])
        .with_filter("All Files", &["*"])
        .with_multiple(true);

    assert_eq!(options.title.as_deref(), Some("Select Project"));
    assert_eq!(
        options.starting_directory,
        Some(PathBuf::from("/home/user/projects"))
    );
    assert_eq!(options.filters.len(), 2);
    assert!(options.multiple);
}

#[test]
fn test_save_file_options_builder() {
    let options = SaveFileOptions::new()
        .with_title("Export Report")
        .with_directory("/tmp")
        .with_default_name("audit.pdf")
        .with_filter("PDF Document", &["pdf"]);

    assert_eq!(options.title.as_deref(), Some("Export Report"));
    assert_eq!(options.starting_directory, Some(PathBuf::from("/tmp")));
    assert_eq!(options.default_name.as_deref(), Some("audit.pdf"));
    assert_eq!(options.filters.len(), 1);
}

#[test]
fn test_pick_folder_options_builder() {
    let options = PickFolderOptions::new()
        .with_title("Choose Workspace")
        .with_directory("/home/workspace");

    assert_eq!(options.title.as_deref(), Some("Choose Workspace"));
    assert_eq!(
        options.starting_directory,
        Some(PathBuf::from("/home/workspace"))
    );
}

static DIALOG_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_dialog_mock_responses() {
    let _guard = DIALOG_TEST_LOCK.lock().unwrap();
    // 1. Open file mock
    set_dialog_mock_response(Some(DialogMockResponse::OpenFile(Some(PathBuf::from(
        "/mock/path/main.rs",
    )))));
    let file = open_file(OpenFileOptions::new());
    assert_eq!(file, Some(PathBuf::from("/mock/path/main.rs")));

    // 2. Open multiple files mock
    set_dialog_mock_response(Some(DialogMockResponse::OpenFiles(vec![
        PathBuf::from("/mock/file1.png"),
        PathBuf::from("/mock/file2.png"),
    ])));
    let files = open_multiple_files(OpenFileOptions::new().with_multiple(true));
    assert_eq!(
        files,
        vec![
            PathBuf::from("/mock/file1.png"),
            PathBuf::from("/mock/file2.png")
        ]
    );

    // 3. Save file mock
    set_dialog_mock_response(Some(DialogMockResponse::SaveFile(Some(PathBuf::from(
        "/mock/saved.json",
    )))));
    let saved = save_file(SaveFileOptions::new());
    assert_eq!(saved, Some(PathBuf::from("/mock/saved.json")));

    // 4. Pick folder mock
    set_dialog_mock_response(Some(DialogMockResponse::PickFolder(Some(PathBuf::from(
        "/mock/workspace_dir",
    )))));
    let folder = pick_folder(PickFolderOptions::new());
    assert_eq!(folder, Some(PathBuf::from("/mock/workspace_dir")));

    // 5. Message box mock
    set_dialog_mock_response(Some(DialogMockResponse::MessageBox(MessageBoxResult::Yes)));
    let result = show_message_box(
        "Confirm Action",
        "Do you want to proceed?",
        MessageBoxType::Question,
        MessageBoxButtons::YesNo,
    );
    assert_eq!(result, MessageBoxResult::Yes);

    // 6. Clear mock
    set_dialog_mock_response(None);
}

#[cfg(target_os = "linux")]
#[test]
fn test_dialog_headless_fallback() {
    let _guard = DIALOG_TEST_LOCK.lock().unwrap();
    set_dialog_mock_response(None);
    // In headless Linux test environments without active GTK display, open_file returns None safely.
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        let file = open_file(OpenFileOptions::new());
        assert_eq!(file, None);
    }
}

#[test]
fn test_dialog_async_apis_deliver_mock_and_headless_responses() {
    use std::sync::{Arc, Mutex};
    use strata_platform::dialog::{
        open_file_async, open_multiple_files_async, pick_folder_async, save_file_async,
    };

    let _guard = DIALOG_TEST_LOCK.lock().unwrap();

    // 1. open_file_async
    set_dialog_mock_response(Some(DialogMockResponse::OpenFile(Some(PathBuf::from(
        "/mock/async/design.svg",
    )))));
    let opened = Arc::new(Mutex::new(None));
    let opened_cb = Arc::clone(&opened);
    open_file_async(OpenFileOptions::new(), move |res| {
        *opened_cb.lock().unwrap() = Some(res);
    });
    assert_eq!(
        *opened.lock().unwrap(),
        Some(Some(PathBuf::from("/mock/async/design.svg")))
    );

    // 2. open_multiple_files_async
    set_dialog_mock_response(Some(DialogMockResponse::OpenFiles(vec![
        PathBuf::from("/mock/async/a.pcb"),
        PathBuf::from("/mock/async/b.pcb"),
    ])));
    let opened_multi = Arc::new(Mutex::new(None));
    let opened_multi_cb = Arc::clone(&opened_multi);
    open_multiple_files_async(OpenFileOptions::new().with_multiple(true), move |res| {
        *opened_multi_cb.lock().unwrap() = Some(res);
    });
    assert_eq!(
        *opened_multi.lock().unwrap(),
        Some(vec![
            PathBuf::from("/mock/async/a.pcb"),
            PathBuf::from("/mock/async/b.pcb"),
        ])
    );

    // 3. save_file_async
    set_dialog_mock_response(Some(DialogMockResponse::SaveFile(Some(PathBuf::from(
        "/mock/async/export.json",
    )))));
    let saved = Arc::new(Mutex::new(None));
    let saved_cb = Arc::clone(&saved);
    save_file_async(SaveFileOptions::new().with_default_name("export.json"), move |res| {
        *saved_cb.lock().unwrap() = Some(res);
    });
    assert_eq!(
        *saved.lock().unwrap(),
        Some(Some(PathBuf::from("/mock/async/export.json")))
    );

    // 4. pick_folder_async
    set_dialog_mock_response(Some(DialogMockResponse::PickFolder(Some(PathBuf::from(
        "/mock/async/project",
    )))));
    let picked = Arc::new(Mutex::new(None));
    let picked_cb = Arc::clone(&picked);
    pick_folder_async(PickFolderOptions::new(), move |res| {
        *picked_cb.lock().unwrap() = Some(res);
    });
    assert_eq!(
        *picked.lock().unwrap(),
        Some(Some(PathBuf::from("/mock/async/project")))
    );

    // 5. Headless / uninitialized GTK fallback invokes callback cleanly without blocking or panicking
    set_dialog_mock_response(None);
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        let headless_res = Arc::new(Mutex::new(None));
        let headless_cb = Arc::clone(&headless_res);
        open_file_async(OpenFileOptions::new(), move |res| {
            *headless_cb.lock().unwrap() = Some(res);
        });
        assert_eq!(*headless_res.lock().unwrap(), Some(None));
    }
}

