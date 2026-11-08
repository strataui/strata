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

#[test]
fn test_dialog_mock_responses() {
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

#[test]
fn test_dialog_headless_fallback() {
    set_dialog_mock_response(None);
    // In headless test environments without active GTK display, open_file returns None safely
    let file = open_file(OpenFileOptions::new());
    // Should not panic, returns None or OS selection
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        assert_eq!(file, None);
    }
}
