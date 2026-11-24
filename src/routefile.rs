//! Salvar e importar a rota de caminhada em arquivo, para compartilhar a hunt.
//! Usa os diálogos padrão do Windows ("Salvar como" / "Abrir").

use std::fs;
use std::mem::{size_of, zeroed};
use std::path::Path;

use serde::Serialize;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST,
    OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};

/// Arquivos maiores que isso não são rotas.
const MAX_SIZE: u64 = 1024 * 1024;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Abre o diálogo e devolve o caminho escolhido (`None` se o usuário cancelou).
fn dialog(owner: HWND, save: bool, suggested: &str) -> Result<Option<String>, String> {
    let filter: Vec<u16> = "Rota do KeyPulse (*.json)\0*.json\0Todos os arquivos\0*.*\0\0".encode_utf16().collect();
    let title = wide(if save { "Salvar rota" } else { "Importar rota" });
    let ext = wide("json");
    let mut file = vec![0u16; 1024];
    for (i, c) in suggested.encode_utf16().take(file.len() - 1).enumerate() {
        file[i] = c;
    }
    unsafe {
        let mut ofn: OPENFILENAMEW = zeroed();
        ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = owner;
        ofn.lpstrFilter = filter.as_ptr();
        ofn.nFilterIndex = 1;
        ofn.lpstrFile = file.as_mut_ptr();
        ofn.nMaxFile = file.len() as u32;
        ofn.lpstrTitle = title.as_ptr();
        ofn.lpstrDefExt = ext.as_ptr();
        ofn.Flags = OFN_EXPLORER | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST
            | if save { OFN_OVERWRITEPROMPT } else { OFN_FILEMUSTEXIST };
        let ok = if save { GetSaveFileNameW(&mut ofn) } else { GetOpenFileNameW(&mut ofn) };
        if ok == 0 {
            return match CommDlgExtendedError() {
                0 => Ok(None),
                code => Err(format!("Não foi possível abrir a janela de arquivos (erro {code:#x})")),
            };
        }
    }
    let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
    Ok(Some(String::from_utf16_lossy(&file[..len])))
}

fn owner_of(window: &tauri::WebviewWindow) -> HWND {
    window.hwnd().map(|h| h.0 as HWND).unwrap_or(std::ptr::null_mut())
}

/// Nome do arquivo sem a extensão (o nome da hunt).
fn stem(path: &str) -> String {
    Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Mantém só caracteres válidos em nome de arquivo.
fn safe_file_name(name: &str) -> String {
    let clean: String = name.chars().filter(|c| !r#"<>:"/\|?*"#.contains(*c) && !c.is_control()).collect();
    let clean = clean.trim().trim_end_matches('.').to_string();
    if clean.is_empty() { "Minha hunt".into() } else { clean }
}

/// Salva a rota (JSON já montado pela interface). Devolve o nome escolhido, ou `None` se cancelou.
#[tauri::command]
pub fn export_route(window: tauri::WebviewWindow, name: String, content: String) -> Result<Option<String>, String> {
    let suggested = format!("{}.json", safe_file_name(&name));
    let Some(path) = dialog(owner_of(&window), true, &suggested)? else { return Ok(None) };
    fs::write(&path, content).map_err(|e| format!("Não foi possível salvar o arquivo: {e}"))?;
    Ok(Some(stem(&path)))
}

#[derive(Serialize)]
pub struct Imported {
    name: String,
    content: String,
}

/// Lê um arquivo de rota escolhido pelo usuário (`None` se cancelou). A validação fica na interface.
#[tauri::command]
pub fn import_route(window: tauri::WebviewWindow) -> Result<Option<Imported>, String> {
    let Some(path) = dialog(owner_of(&window), false, "")? else { return Ok(None) };
    let size = fs::metadata(&path).map_err(|e| format!("Não foi possível ler o arquivo: {e}"))?.len();
    if size > MAX_SIZE {
        return Err("Esse arquivo é grande demais para ser uma rota".into());
    }
    let content = fs::read_to_string(&path).map_err(|_| "O arquivo não é uma rota do KeyPulse".to_string())?;
    Ok(Some(Imported { name: stem(&path), content }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_cleaned() {
        assert_eq!(safe_file_name("Cave: rotworms/2"), "Cave rotworms2");
        assert_eq!(safe_file_name("  ..."), "Minha hunt");
        assert_eq!(stem(r"C:\x\Hunt dos ratos.json"), "Hunt dos ratos");
    }
}
