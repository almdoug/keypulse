//! Persiste a última configuração em %APPDATA%\KeyPulse\config.json.

use std::fs;
use std::path::PathBuf;

fn config_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("KeyPulse").join("config.json"))
}

#[tauri::command]
pub fn load_config() -> Option<String> {
    fs::read_to_string(config_path()?).ok()
}

#[tauri::command]
pub fn save_config(data: String) -> Result<(), String> {
    let path = config_path().ok_or("APPDATA não definido")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(path, data).map_err(|e| e.to_string())
}
