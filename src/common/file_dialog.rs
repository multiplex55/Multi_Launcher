#[cfg(target_os = "windows")]
pub use rfd::FileDialog;

#[cfg(not(target_os = "windows"))]
pub struct FileDialog;

#[cfg(not(target_os = "windows"))]
impl FileDialog {
    pub fn new() -> Self { FileDialog }
    pub fn add_filter(self, _name: &str, _exts: &[&str]) -> Self { self }
    pub fn set_directory<P: AsRef<std::path::Path>>(self, _path: P) -> Self { self }
    pub fn pick_file(self) -> Option<std::path::PathBuf> { None }
    pub fn save_file(self) -> Option<std::path::PathBuf> { None }
    pub fn pick_folder(self) -> Option<std::path::PathBuf> { None }
}
