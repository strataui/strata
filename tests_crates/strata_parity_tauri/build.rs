fn main() {
    #[cfg(feature = "tauri-backend")]
    tauri_build::build();
}
