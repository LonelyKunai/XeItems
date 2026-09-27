//! Code shared by XeEssentials and XeItems: translations, config.toml loading,
//! data-folder files and logging.

pub mod comandos;
pub mod config;
pub mod datos;
pub mod lang;
pub mod permisos;

use pumpkin_plugin_api::logging::{LogLevel, log};

pub fn info(mensaje: &str) {
    log(LogLevel::Info, mensaje);
}

pub fn warn(mensaje: &str) {
    log(LogLevel::Warn, mensaje);
}

pub fn error(mensaje: &str) {
    log(LogLevel::Error, mensaje);
}
