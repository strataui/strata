//! Cross-platform native OS dialogs (file pickers, folder selectors, and message boxes).
//!
//! Provides native system dialogs across Linux (GTK / XDG Desktop Portal),
//! Windows (`comdlg32` / Win32 `GetOpenFileNameW` / `MessageBoxW`), and
//! macOS (AppKit `NSOpenPanel`, `NSSavePanel`, `NSAlert`).
//!
//! Includes a thread-safe mock override system (`set_dialog_mock_response`)
//! allowing automated CI and headless tests to verify dialog logic deterministically.

use std::path::PathBuf;
use std::sync::Mutex;

/// File type filter used by file picker dialogs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

impl DialogFilter {
    /// Creates a new filter with a human-readable display name and allowed file extensions.
    pub fn new(name: impl Into<String>, extensions: &[&str]) -> Self {
        Self {
            name: name.into(),
            extensions: extensions
                .iter()
                .map(|ext| ext.trim_start_matches('.').to_string())
                .collect(),
        }
    }
}

/// Configuration options for opening files.
#[derive(Debug, Clone, Default)]
pub struct OpenFileOptions {
    pub title: Option<String>,
    pub starting_directory: Option<PathBuf>,
    pub filters: Vec<DialogFilter>,
    pub multiple: bool,
}

impl OpenFileOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.starting_directory = Some(directory.into());
        self
    }

    pub fn with_filter(mut self, name: impl Into<String>, extensions: &[&str]) -> Self {
        self.filters.push(DialogFilter::new(name, extensions));
        self
    }

    pub fn with_multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }
}

/// Configuration options for saving a file.
#[derive(Debug, Clone, Default)]
pub struct SaveFileOptions {
    pub title: Option<String>,
    pub starting_directory: Option<PathBuf>,
    pub default_name: Option<String>,
    pub filters: Vec<DialogFilter>,
}

impl SaveFileOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.starting_directory = Some(directory.into());
        self
    }

    pub fn with_default_name(mut self, default_name: impl Into<String>) -> Self {
        self.default_name = Some(default_name.into());
        self
    }

    pub fn with_filter(mut self, name: impl Into<String>, extensions: &[&str]) -> Self {
        self.filters.push(DialogFilter::new(name, extensions));
        self
    }
}

/// Configuration options for selecting a folder.
#[derive(Debug, Clone, Default)]
pub struct PickFolderOptions {
    pub title: Option<String>,
    pub starting_directory: Option<PathBuf>,
}

impl PickFolderOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.starting_directory = Some(directory.into());
        self
    }
}

/// Semantic type of message box or alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageBoxType {
    #[default]
    Info,
    Warning,
    Error,
    Question,
}

/// Set of buttons to present in a message box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageBoxButtons {
    #[default]
    Ok,
    OkCancel,
    YesNo,
    YesNoCancel,
}

/// Result returned when a user interacts with a message box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageBoxResult {
    Ok,
    Cancel,
    Yes,
    No,
}

/// Mock response for automated test suites.
#[derive(Debug, Clone)]
pub enum DialogMockResponse {
    OpenFile(Option<PathBuf>),
    OpenFiles(Vec<PathBuf>),
    SaveFile(Option<PathBuf>),
    PickFolder(Option<PathBuf>),
    MessageBox(MessageBoxResult),
}

static DIALOG_MOCK: Mutex<Option<DialogMockResponse>> = Mutex::new(None);

/// Sets an explicit mock response for testing.
pub fn set_dialog_mock_response(response: Option<DialogMockResponse>) {
    let mut lock = DIALOG_MOCK.lock().unwrap();
    *lock = response;
}

fn check_mock_response() -> Option<DialogMockResponse> {
    DIALOG_MOCK.lock().unwrap().clone()
}

/// Prompts user to select a single file to open.
pub fn open_file(options: OpenFileOptions) -> Option<PathBuf> {
    if let Some(DialogMockResponse::OpenFile(path)) = check_mock_response() {
        return path;
    }

    #[cfg(target_os = "linux")]
    {
        linux_backend::open_file(&options)
    }
    #[cfg(windows)]
    {
        windows_backend::open_file(&options)
    }
    #[cfg(target_os = "macos")]
    {
        macos_backend::open_file(&options)
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        let _ = options;
        None
    }
}

/// Prompts user to select multiple files to open.
pub fn open_multiple_files(options: OpenFileOptions) -> Vec<PathBuf> {
    if let Some(DialogMockResponse::OpenFiles(paths)) = check_mock_response() {
        return paths;
    }

    #[cfg(target_os = "linux")]
    {
        linux_backend::open_multiple_files(&options)
    }
    #[cfg(windows)]
    {
        windows_backend::open_multiple_files(&options)
    }
    #[cfg(target_os = "macos")]
    {
        macos_backend::open_multiple_files(&options)
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        let _ = options;
        Vec::new()
    }
}

/// Prompts user to choose a destination path for saving a file.
pub fn save_file(options: SaveFileOptions) -> Option<PathBuf> {
    if let Some(DialogMockResponse::SaveFile(path)) = check_mock_response() {
        return path;
    }

    #[cfg(target_os = "linux")]
    {
        linux_backend::save_file(&options)
    }
    #[cfg(windows)]
    {
        windows_backend::save_file(&options)
    }
    #[cfg(target_os = "macos")]
    {
        macos_backend::save_file(&options)
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        let _ = options;
        None
    }
}

/// Prompts user to choose a directory / folder.
pub fn pick_folder(options: PickFolderOptions) -> Option<PathBuf> {
    if let Some(DialogMockResponse::PickFolder(path)) = check_mock_response() {
        return path;
    }

    #[cfg(target_os = "linux")]
    {
        linux_backend::pick_folder(&options)
    }
    #[cfg(windows)]
    {
        windows_backend::pick_folder(&options)
    }
    #[cfg(target_os = "macos")]
    {
        macos_backend::pick_folder(&options)
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        let _ = options;
        None
    }
}

/// Shows a native operating system message alert / confirmation box.
pub fn show_message_box(
    title: &str,
    message: &str,
    box_type: MessageBoxType,
    buttons: MessageBoxButtons,
) -> MessageBoxResult {
    if let Some(DialogMockResponse::MessageBox(result)) = check_mock_response() {
        return result;
    }

    #[cfg(target_os = "linux")]
    {
        linux_backend::show_message_box(title, message, box_type, buttons)
    }
    #[cfg(windows)]
    {
        windows_backend::show_message_box(title, message, box_type, buttons)
    }
    #[cfg(target_os = "macos")]
    {
        macos_backend::show_message_box(title, message, box_type, buttons)
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        let _ = (title, message, box_type, buttons);
        MessageBoxResult::Ok
    }
}

#[cfg(target_os = "linux")]
mod linux_backend {
    use super::*;
    use gtk::prelude::*;
    use std::path::PathBuf;

    pub fn open_file(options: &OpenFileOptions) -> Option<PathBuf> {
        if !gtk::is_initialized() {
            return None;
        }
        let dialog = gtk::FileChooserNative::new(
            Some(options.title.as_deref().unwrap_or("Open File")),
            None::<&gtk::Window>,
            gtk::FileChooserAction::Open,
            Some("Open"),
            Some("Cancel"),
        );
        apply_filters(&dialog, &options.filters);
        if let Some(dir) = &options.starting_directory {
            dialog.set_current_folder(dir);
        }
        let response = dialog.run();
        if response == gtk::ResponseType::Accept {
            dialog.filename()
        } else {
            None
        }
    }

    pub fn open_multiple_files(options: &OpenFileOptions) -> Vec<PathBuf> {
        if !gtk::is_initialized() {
            return Vec::new();
        }
        let dialog = gtk::FileChooserNative::new(
            Some(options.title.as_deref().unwrap_or("Open Files")),
            None::<&gtk::Window>,
            gtk::FileChooserAction::Open,
            Some("Open"),
            Some("Cancel"),
        );
        dialog.set_select_multiple(true);
        apply_filters(&dialog, &options.filters);
        if let Some(dir) = &options.starting_directory {
            dialog.set_current_folder(dir);
        }
        let response = dialog.run();
        if response == gtk::ResponseType::Accept {
            dialog.filenames()
        } else {
            Vec::new()
        }
    }

    pub fn save_file(options: &SaveFileOptions) -> Option<PathBuf> {
        if !gtk::is_initialized() {
            return None;
        }
        let dialog = gtk::FileChooserNative::new(
            Some(options.title.as_deref().unwrap_or("Save File")),
            None::<&gtk::Window>,
            gtk::FileChooserAction::Save,
            Some("Save"),
            Some("Cancel"),
        );
        dialog.set_do_overwrite_confirmation(true);
        apply_filters(&dialog, &options.filters);
        if let Some(dir) = &options.starting_directory {
            dialog.set_current_folder(dir);
        }
        if let Some(name) = &options.default_name {
            dialog.set_current_name(name);
        }
        let response = dialog.run();
        if response == gtk::ResponseType::Accept {
            dialog.filename()
        } else {
            None
        }
    }

    pub fn pick_folder(options: &PickFolderOptions) -> Option<PathBuf> {
        if !gtk::is_initialized() {
            return None;
        }
        let dialog = gtk::FileChooserNative::new(
            Some(options.title.as_deref().unwrap_or("Select Folder")),
            None::<&gtk::Window>,
            gtk::FileChooserAction::SelectFolder,
            Some("Select"),
            Some("Cancel"),
        );
        if let Some(dir) = &options.starting_directory {
            dialog.set_current_folder(dir);
        }
        let response = dialog.run();
        if response == gtk::ResponseType::Accept {
            dialog.filename()
        } else {
            None
        }
    }

    pub fn show_message_box(
        title: &str,
        message: &str,
        box_type: MessageBoxType,
        buttons: MessageBoxButtons,
    ) -> MessageBoxResult {
        if !gtk::is_initialized() {
            return MessageBoxResult::Ok;
        }
        let message_type = match box_type {
            MessageBoxType::Info => gtk::MessageType::Info,
            MessageBoxType::Warning => gtk::MessageType::Warning,
            MessageBoxType::Error => gtk::MessageType::Error,
            MessageBoxType::Question => gtk::MessageType::Question,
        };
        let buttons_type = match buttons {
            MessageBoxButtons::Ok => gtk::ButtonsType::Ok,
            MessageBoxButtons::OkCancel => gtk::ButtonsType::OkCancel,
            MessageBoxButtons::YesNo => gtk::ButtonsType::YesNo,
            MessageBoxButtons::YesNoCancel => gtk::ButtonsType::None,
        };
        let dialog = gtk::MessageDialog::new(
            None::<&gtk::Window>,
            gtk::DialogFlags::MODAL,
            message_type,
            buttons_type,
            message,
        );
        dialog.set_title(title);
        if buttons == MessageBoxButtons::YesNoCancel {
            dialog.add_button("Yes", gtk::ResponseType::Yes);
            dialog.add_button("No", gtk::ResponseType::No);
            dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        }
        let response = dialog.run();
        dialog.close();
        match response {
            gtk::ResponseType::Ok | gtk::ResponseType::Accept => MessageBoxResult::Ok,
            gtk::ResponseType::Yes => MessageBoxResult::Yes,
            gtk::ResponseType::No => MessageBoxResult::No,
            _ => MessageBoxResult::Cancel,
        }
    }

    fn apply_filters(dialog: &gtk::FileChooserNative, filters: &[DialogFilter]) {
        for f in filters {
            let filter = gtk::FileFilter::new();
            filter.set_name(Some(&f.name));
            for ext in &f.extensions {
                filter.add_pattern(&format!("*.{}", ext));
                filter.add_pattern(&format!("*.{}", ext.to_uppercase()));
            }
            dialog.add_filter(filter);
        }
    }
}

#[cfg(windows)]
mod windows_backend {
    use super::*;
    use std::path::PathBuf;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, GetSaveFileNameW, OFN_ALLOWMULTISELECT, OFN_EXPLORER, OFN_FILEMUSTEXIST,
        OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDCANCEL, IDNO, IDOK, IDYES, MB_ICONERROR, MB_ICONINFORMATION,
        MB_ICONQUESTION, MB_ICONWARNING, MB_OK, MB_OKCANCEL, MB_YESNO, MB_YESNOCANCEL,
    };

    pub fn open_file(options: &OpenFileOptions) -> Option<PathBuf> {
        let mut paths = run_open_dialog(options, false);
        paths.pop()
    }

    pub fn open_multiple_files(options: &OpenFileOptions) -> Vec<PathBuf> {
        run_open_dialog(options, true)
    }

    fn run_open_dialog(options: &OpenFileOptions, multiple: bool) -> Vec<PathBuf> {
        unsafe {
            let mut buf = vec![0u16; 65536];
            let filter = build_filter_string(&options.filters);
            let title = options
                .title
                .as_ref()
                .map(|t| t.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>());
            let dir = options
                .starting_directory
                .as_ref()
                .and_then(|d| d.to_str())
                .map(|s| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>());

            let mut flags = OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST;
            if multiple {
                flags |= OFN_ALLOWMULTISELECT;
            }

            let mut ofn: OPENFILENAMEW = std::mem::zeroed();
            ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
            ofn.hwndOwner = 0 as HWND;
            ofn.lpstrFilter = filter.as_ptr();
            ofn.lpstrFile = buf.as_mut_ptr();
            ofn.nMaxFile = buf.len() as u32;
            ofn.lpstrInitialDir = dir.as_ref().map(|d| d.as_ptr()).unwrap_or(std::ptr::null());
            ofn.lpstrTitle = title.as_ref().map(|t| t.as_ptr()).unwrap_or(std::ptr::null());
            ofn.Flags = flags;

            if GetOpenFileNameW(&mut ofn) == 0 {
                return Vec::new();
            }

            parse_open_file_buffer(&buf)
        }
    }

    pub fn save_file(options: &SaveFileOptions) -> Option<PathBuf> {
        unsafe {
            let mut buf = vec![0u16; 4096];
            if let Some(default_name) = &options.default_name {
                let name_wide: Vec<u16> = default_name.encode_utf16().collect();
                let len = name_wide.len().min(buf.len().saturating_sub(1));
                buf[..len].copy_from_slice(&name_wide[..len]);
            }

            let filter = build_filter_string(&options.filters);
            let title = options
                .title
                .as_ref()
                .map(|t| t.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>());
            let dir = options
                .starting_directory
                .as_ref()
                .and_then(|d| d.to_str())
                .map(|s| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>());

            let mut ofn: OPENFILENAMEW = std::mem::zeroed();
            ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
            ofn.hwndOwner = 0 as HWND;
            ofn.lpstrFilter = filter.as_ptr();
            ofn.lpstrFile = buf.as_mut_ptr();
            ofn.nMaxFile = buf.len() as u32;
            ofn.lpstrInitialDir = dir.as_ref().map(|d| d.as_ptr()).unwrap_or(std::ptr::null());
            ofn.lpstrTitle = title.as_ref().map(|t| t.as_ptr()).unwrap_or(std::ptr::null());
            ofn.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_OVERWRITEPROMPT;

            if GetSaveFileNameW(&mut ofn) == 0 {
                return None;
            }

            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            let path_str = String::from_utf16_lossy(&buf[..end]);
            if path_str.is_empty() {
                None
            } else {
                Some(PathBuf::from(path_str))
            }
        }
    }

    pub fn pick_folder(options: &PickFolderOptions) -> Option<PathBuf> {
        // Fallback folder picker via Windows standard directory selection
        let mut open_opt = OpenFileOptions::new();
        open_opt.title = options.title.clone();
        open_opt.starting_directory = options.starting_directory.clone();
        open_file(&open_opt).and_then(|p| {
            if p.is_dir() {
                Some(p)
            } else {
                p.parent().map(|d| d.to_path_buf())
            }
        })
    }

    pub fn show_message_box(
        title: &str,
        message: &str,
        box_type: MessageBoxType,
        buttons: MessageBoxButtons,
    ) -> MessageBoxResult {
        unsafe {
            let wide_msg: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
            let wide_title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();

            let mut flags = match box_type {
                MessageBoxType::Info => MB_ICONINFORMATION,
                MessageBoxType::Warning => MB_ICONWARNING,
                MessageBoxType::Error => MB_ICONERROR,
                MessageBoxType::Question => MB_ICONQUESTION,
            };

            flags |= match buttons {
                MessageBoxButtons::Ok => MB_OK,
                MessageBoxButtons::OkCancel => MB_OKCANCEL,
                MessageBoxButtons::YesNo => MB_YESNO,
                MessageBoxButtons::YesNoCancel => MB_YESNOCANCEL,
            };

            let res = MessageBoxW(0 as HWND, wide_msg.as_ptr(), wide_title.as_ptr(), flags);
            match res {
                IDOK => MessageBoxResult::Ok,
                IDCANCEL => MessageBoxResult::Cancel,
                IDYES => MessageBoxResult::Yes,
                IDNO => MessageBoxResult::No,
                _ => MessageBoxResult::Ok,
            }
        }
    }

    fn build_filter_string(filters: &[DialogFilter]) -> Vec<u16> {
        let mut out = Vec::new();
        for f in filters {
            out.extend(f.name.encode_utf16());
            out.push(0);
            let exts: Vec<String> = f.extensions.iter().map(|e| format!("*.{}", e)).collect();
            let ext_str = exts.join(";");
            out.extend(ext_str.encode_utf16());
            out.push(0);
        }
        if filters.is_empty() {
            out.extend("All Files (*.*)".encode_utf16());
            out.push(0);
            out.extend("*.*".encode_utf16());
            out.push(0);
        }
        out.push(0);
        out
    }

    fn parse_open_file_buffer(buf: &[u16]) -> Vec<PathBuf> {
        let mut chunks = Vec::new();
        let mut start = 0;
        for i in 0..buf.len() {
            if buf[i] == 0 {
                if start == i {
                    break;
                }
                chunks.push(String::from_utf16_lossy(&buf[start..i]));
                start = i + 1;
            }
        }
        if chunks.is_empty() {
            return Vec::new();
        }
        if chunks.len() == 1 {
            return vec![PathBuf::from(&chunks[0])];
        }
        let base_dir = PathBuf::from(&chunks[0]);
        chunks[1..]
            .iter()
            .map(|filename| base_dir.join(filename))
            .collect()
    }
}

#[cfg(target_os = "macos")]
mod macos_backend {
    use super::*;
    use std::ffi::{c_char, c_void, CString};
    use std::path::PathBuf;

    extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend(receiver: *mut c_void, op: *mut c_void, ...) -> *mut c_void;
    }

    pub fn open_file(options: &OpenFileOptions) -> Option<PathBuf> {
        let mut files = run_open_panel(options, false);
        files.pop()
    }

    pub fn open_multiple_files(options: &OpenFileOptions) -> Vec<PathBuf> {
        run_open_panel(options, true)
    }

    fn run_open_panel(options: &OpenFileOptions, multiple: bool) -> Vec<PathBuf> {
        unsafe {
            let panel_cls = objc_getClass(b"NSOpenPanel\0".as_ptr() as *const c_char);
            if panel_cls.is_null() {
                return Vec::new();
            }
            let open_sel = sel_registerName(b"openPanel\0".as_ptr() as *const c_char);
            let msg_open: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let panel = msg_open(panel_cls, open_sel);
            if panel.is_null() {
                return Vec::new();
            }

            let set_files_sel = sel_registerName(b"setCanChooseFiles:\0".as_ptr() as *const c_char);
            let msg_set_bool: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_set_bool(panel, set_files_sel, true);

            let set_dirs_sel =
                sel_registerName(b"setCanChooseDirectories:\0".as_ptr() as *const c_char);
            msg_set_bool(panel, set_dirs_sel, false);

            let set_multi_sel =
                sel_registerName(b"setAllowsMultipleSelection:\0".as_ptr() as *const c_char);
            msg_set_bool(panel, set_multi_sel, multiple);

            if let Some(title) = &options.title {
                let set_title_sel = sel_registerName(b"setTitle:\0".as_ptr() as *const c_char);
                let ns_title = ns_string(title);
                let msg_set_title: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_set_title(panel, set_title_sel, ns_title);
            }

            let run_modal_sel = sel_registerName(b"runModal\0".as_ptr() as *const c_char);
            let msg_run: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
                std::mem::transmute(objc_msgSend as *const ());
            let response = msg_run(panel, run_modal_sel);

            // NSModalResponseOK is 1
            if response != 1 {
                return Vec::new();
            }

            let urls_sel = sel_registerName(b"URLs\0".as_ptr() as *const c_char);
            let msg_urls: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let urls_array = msg_urls(panel, urls_sel);
            parse_ns_array_urls(urls_array)
        }
    }

    pub fn save_file(options: &SaveFileOptions) -> Option<PathBuf> {
        unsafe {
            let panel_cls = objc_getClass(b"NSSavePanel\0".as_ptr() as *const c_char);
            if panel_cls.is_null() {
                return None;
            }
            let save_sel = sel_registerName(b"savePanel\0".as_ptr() as *const c_char);
            let msg_save: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let panel = msg_save(panel_cls, save_sel);
            if panel.is_null() {
                return None;
            }

            if let Some(title) = &options.title {
                let set_title_sel = sel_registerName(b"setTitle:\0".as_ptr() as *const c_char);
                let ns_title = ns_string(title);
                let msg_set_str: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_set_str(panel, set_title_sel, ns_title);
            }

            if let Some(name) = &options.default_name {
                let set_name_sel =
                    sel_registerName(b"setNameFieldStringValue:\0".as_ptr() as *const c_char);
                let ns_name = ns_string(name);
                let msg_set_str: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_set_str(panel, set_name_sel, ns_name);
            }

            let run_modal_sel = sel_registerName(b"runModal\0".as_ptr() as *const c_char);
            let msg_run: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
                std::mem::transmute(objc_msgSend as *const ());
            let response = msg_run(panel, run_modal_sel);

            if response != 1 {
                return None;
            }

            let url_sel = sel_registerName(b"URL\0".as_ptr() as *const c_char);
            let msg_url: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let url = msg_url(panel, url_sel);
            path_from_ns_url(url)
        }
    }

    pub fn pick_folder(options: &PickFolderOptions) -> Option<PathBuf> {
        unsafe {
            let panel_cls = objc_getClass(b"NSOpenPanel\0".as_ptr() as *const c_char);
            if panel_cls.is_null() {
                return None;
            }
            let open_sel = sel_registerName(b"openPanel\0".as_ptr() as *const c_char);
            let msg_open: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let panel = msg_open(panel_cls, open_sel);
            if panel.is_null() {
                return None;
            }

            let set_files_sel = sel_registerName(b"setCanChooseFiles:\0".as_ptr() as *const c_char);
            let msg_set_bool: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_set_bool(panel, set_files_sel, false);

            let set_dirs_sel =
                sel_registerName(b"setCanChooseDirectories:\0".as_ptr() as *const c_char);
            msg_set_bool(panel, set_dirs_sel, true);

            let set_multi_sel =
                sel_registerName(b"setAllowsMultipleSelection:\0".as_ptr() as *const c_char);
            msg_set_bool(panel, set_multi_sel, false);

            if let Some(title) = &options.title {
                let set_title_sel = sel_registerName(b"setTitle:\0".as_ptr() as *const c_char);
                let ns_title = ns_string(title);
                let msg_set_title: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_set_title(panel, set_title_sel, ns_title);
            }

            let run_modal_sel = sel_registerName(b"runModal\0".as_ptr() as *const c_char);
            let msg_run: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
                std::mem::transmute(objc_msgSend as *const ());
            let response = msg_run(panel, run_modal_sel);

            if response != 1 {
                return None;
            }

            let url_sel = sel_registerName(b"URL\0".as_ptr() as *const c_char);
            let msg_url: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let url = msg_url(panel, url_sel);
            path_from_ns_url(url)
        }
    }

    pub fn show_message_box(
        title: &str,
        message: &str,
        _box_type: MessageBoxType,
        buttons: MessageBoxButtons,
    ) -> MessageBoxResult {
        unsafe {
            let alert_cls = objc_getClass(b"NSAlert\0".as_ptr() as *const c_char);
            if alert_cls.is_null() {
                return MessageBoxResult::Ok;
            }
            let alloc_sel = sel_registerName(b"alloc\0".as_ptr() as *const c_char);
            let init_sel = sel_registerName(b"init\0".as_ptr() as *const c_char);
            let msg_alloc: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let alert_inst = msg_alloc(alert_cls, alloc_sel);
            let alert = msg_alloc(alert_inst, init_sel);

            let set_msg_sel = sel_registerName(b"setMessageText:\0".as_ptr() as *const c_char);
            let ns_title = ns_string(title);
            let msg_set_str: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_set_str(alert, set_msg_sel, ns_title);

            let set_info_sel =
                sel_registerName(b"setInformativeText:\0".as_ptr() as *const c_char);
            let ns_msg = ns_string(message);
            msg_set_str(alert, set_info_sel, ns_msg);

            let add_btn_sel =
                sel_registerName(b"addButtonWithTitle:\0".as_ptr() as *const c_char);
            match buttons {
                MessageBoxButtons::Ok => {
                    msg_set_str(alert, add_btn_sel, ns_string("OK"));
                }
                MessageBoxButtons::OkCancel => {
                    msg_set_str(alert, add_btn_sel, ns_string("OK"));
                    msg_set_str(alert, add_btn_sel, ns_string("Cancel"));
                }
                MessageBoxButtons::YesNo => {
                    msg_set_str(alert, add_btn_sel, ns_string("Yes"));
                    msg_set_str(alert, add_btn_sel, ns_string("No"));
                }
                MessageBoxButtons::YesNoCancel => {
                    msg_set_str(alert, add_btn_sel, ns_string("Yes"));
                    msg_set_str(alert, add_btn_sel, ns_string("No"));
                    msg_set_str(alert, add_btn_sel, ns_string("Cancel"));
                }
            }

            let run_modal_sel = sel_registerName(b"runModal\0".as_ptr() as *const c_char);
            let msg_run: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
                std::mem::transmute(objc_msgSend as *const ());
            let response = msg_run(alert, run_modal_sel);

            // First button added returns 1000, second 1001, third 1002
            match buttons {
                MessageBoxButtons::Ok => MessageBoxResult::Ok,
                MessageBoxButtons::OkCancel => {
                    if response == 1000 {
                        MessageBoxResult::Ok
                    } else {
                        MessageBoxResult::Cancel
                    }
                }
                MessageBoxButtons::YesNo => {
                    if response == 1000 {
                        MessageBoxResult::Yes
                    } else {
                        MessageBoxResult::No
                    }
                }
                MessageBoxButtons::YesNoCancel => {
                    if response == 1000 {
                        MessageBoxResult::Yes
                    } else if response == 1001 {
                        MessageBoxResult::No
                    } else {
                        MessageBoxResult::Cancel
                    }
                }
            }
        }
    }

    unsafe fn ns_string(s: &str) -> *mut c_void {
        let cls = objc_getClass(b"NSString\0".as_ptr() as *const c_char);
        let sel = sel_registerName(b"stringWithUTF8String:\0".as_ptr() as *const c_char);
        let cstr = CString::new(s).unwrap_or_default();
        let msg: unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        msg(cls, sel, cstr.as_ptr())
    }

    unsafe fn path_from_ns_url(url: *mut c_void) -> Option<PathBuf> {
        if url.is_null() {
            return None;
        }
        let path_sel = sel_registerName(b"path\0".as_ptr() as *const c_char);
        let msg_path: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let path_str = msg_path(url, path_sel);
        if path_str.is_null() {
            return None;
        }
        let utf8_sel = sel_registerName(b"UTF8String\0".as_ptr() as *const c_char);
        let msg_utf8: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *const c_char =
            std::mem::transmute(objc_msgSend as *const ());
        let c_str = msg_utf8(path_str, utf8_sel);
        if c_str.is_null() {
            return None;
        }
        let slice = std::ffi::CStr::from_ptr(c_str).to_str().ok()?;
        Some(PathBuf::from(slice))
    }

    unsafe fn parse_ns_array_urls(array: *mut c_void) -> Vec<PathBuf> {
        if array.is_null() {
            return Vec::new();
        }
        let count_sel = sel_registerName(b"count\0".as_ptr() as *const c_char);
        let msg_count: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
            std::mem::transmute(objc_msgSend as *const ());
        let count = msg_count(array, count_sel);
        let obj_at_sel = sel_registerName(b"objectAtIndex:\0".as_ptr() as *const c_char);
        let msg_obj: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());

        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let url = msg_obj(array, obj_at_sel, i);
            if let Some(path) = path_from_ns_url(url) {
                out.push(path);
            }
        }
        out
    }
}
