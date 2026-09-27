//! config.toml in the data folder, read when the plugin loads. If it doesn't exist it
//! is created from the plugin's commented template; if it has an error, the defaults
//! are used and the error is logged.

use serde::de::DeserializeOwned;

use crate::datos;
use crate::lang::ts;

pub const ARCHIVO: &str = "config.toml";

pub fn cargar<T: DeserializeOwned + Default>(plantilla: &str) -> T {
    if !datos::existe(ARCHIVO) && datos::escribir(ARCHIVO, plantilla) {
        crate::info(&ts("config.created", &[("file", &ARCHIVO)]));
    }
    let contenido = datos::leer(ARCHIVO).unwrap_or_else(|| plantilla.to_string());
    match toml::from_str(&contenido) {
        Ok(config) => config,
        Err(e) => {
            crate::warn(&ts("config.invalid", &[("file", &ARCHIVO), ("error", &e)]));
            toml::from_str(plantilla).unwrap_or_default()
        }
    }
}
