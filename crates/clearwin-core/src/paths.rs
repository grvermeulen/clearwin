use std::env;
use std::path::PathBuf;

/// Well-known Windows folders the catalog is built from.
///
/// Kept as plain data so tests can point every folder at a temporary directory.
#[derive(Clone, Debug, Default)]
pub struct Paths {
    /// `%TEMP%` of the current user.
    pub temp: Option<PathBuf>,
    /// `%LOCALAPPDATA%`, e.g. `C:\Users\me\AppData\Local`.
    pub local_app_data: Option<PathBuf>,
    /// `%APPDATA%`, e.g. `C:\Users\me\AppData\Roaming`.
    pub app_data: Option<PathBuf>,
    /// `%WINDIR%`, e.g. `C:\Windows`.
    pub windows: Option<PathBuf>,
    /// `%ProgramData%`, e.g. `C:\ProgramData`.
    pub program_data: Option<PathBuf>,
}

impl Paths {
    pub fn from_env() -> Self {
        let var = |name: &str| {
            env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        Self {
            temp: var("TEMP").or_else(|| var("TMP")),
            local_app_data: var("LOCALAPPDATA"),
            app_data: var("APPDATA"),
            windows: var("WINDIR").or_else(|| var("SystemRoot")),
            program_data: var("ProgramData"),
        }
    }
}
