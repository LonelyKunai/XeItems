//! The plugin's data folder (`plugins/data/<Plugin>/`). Needs the `fs.write.data`
//! permission in the plugin metadata. If the server doesn't grant it, reads return
//! nothing and writes log one warning, so the plugin keeps working from memory.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::lang::ts;

static CARPETA: OnceLock<String> = OnceLock::new();
static AVISADO: AtomicBool = AtomicBool::new(false);

pub fn iniciar(carpeta: String) {
    let _ = std::fs::create_dir_all(&carpeta);
    let _ = CARPETA.set(carpeta);
}

pub fn ruta(archivo: &str) -> Option<String> {
    CARPETA.get().map(|c| format!("{}/{archivo}", c.trim_end_matches('/')))
}

pub fn leer(archivo: &str) -> Option<String> {
    std::fs::read_to_string(ruta(archivo)?).ok()
}

pub fn existe(archivo: &str) -> bool {
    ruta(archivo).is_some_and(|r| std::path::Path::new(&r).exists())
}

pub fn crear_carpeta(carpeta: &str) -> bool {
    ruta(carpeta).is_some_and(|r| std::fs::create_dir_all(r).is_ok())
}

/// Names of the files in a folder of the data folder, sorted.
pub fn listar(carpeta: &str) -> Vec<String> {
    let Some(r) = ruta(carpeta) else { return vec![] };
    let Ok(entradas) = std::fs::read_dir(r) else { return vec![] };
    let mut nombres: Vec<String> = entradas
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    nombres.sort();
    nombres
}

pub fn mover(de: &str, a: &str) -> bool {
    match (ruta(de), ruta(a)) {
        (Some(de), Some(a)) => std::fs::rename(de, a).is_ok(),
        _ => false,
    }
}

/// Writes through a temporary file so a crash never leaves a half-written file.
pub fn escribir(archivo: &str, contenido: &str) -> bool {
    let Some(destino) = ruta(archivo) else { return false };
    let temporal = format!("{destino}.tmp");
    let resultado = std::fs::write(&temporal, contenido).and_then(|()| std::fs::rename(&temporal, &destino));
    match resultado {
        Ok(()) => true,
        Err(e) => {
            if !AVISADO.swap(true, Ordering::Relaxed) {
                crate::warn(&ts("data.fs_failed", &[("file", &archivo), ("error", &e)]));
            }
            false
        }
    }
}
