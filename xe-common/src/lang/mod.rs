//! Translations. Chat messages use each player's game language; logs, command
//! descriptions and item names use the server language from config.toml.

mod en_us;
mod es_es;

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::{OnceLock, RwLock};

use pumpkin_plugin_api::command::CommandSender;
use pumpkin_plugin_api::text::TextComponent;
use pumpkin_plugin_api::Player;

pub const RESPALDO: &str = "en_us";
pub const IDIOMAS: [&str; 2] = ["en_us", "es_es"];

static SERVIDOR: RwLock<&'static str> = RwLock::new("en_us");

fn tablas() -> &'static HashMap<&'static str, HashMap<&'static str, &'static str>> {
    static TABLAS: OnceLock<HashMap<&'static str, HashMap<&'static str, &'static str>>> =
        OnceLock::new();
    TABLAS.get_or_init(|| {
        HashMap::from([
            ("en_us", en_us::TEXTOS.iter().copied().collect()),
            ("es_es", es_es::TEXTOS.iter().copied().collect()),
        ])
    })
}

/// Maps a Minecraft locale (es_mx, en_gb, ...) to one of [`IDIOMAS`]; anything else is English.
pub fn idioma(codigo: &str) -> &'static str {
    let codigo = codigo.to_lowercase();
    IDIOMAS
        .into_iter()
        .find(|disponible| codigo.starts_with(&disponible[..2]))
        .unwrap_or(RESPALDO)
}

pub fn establecer_idioma(codigo: &str) {
    *SERVIDOR.write().unwrap_or_else(|e| e.into_inner()) = idioma(codigo);
}

pub fn servidor() -> &'static str {
    *SERVIDOR.read().unwrap_or_else(|e| e.into_inner())
}

pub fn de_jugador(jugador: &Player) -> &'static str {
    idioma(&jugador.get_locale())
}

/// The console (and anything that isn't a player) uses the server language.
pub fn de_sender(sender: &CommandSender) -> &'static str {
    sender.as_player().map_or_else(servidor, |j| de_jugador(&j))
}

/// Text for `clave` in `idioma`, with `{name}` placeholders filled from `valores`.
pub fn t(idioma: &str, clave: &str, valores: &[(&str, &dyn Display)]) -> String {
    let texto = tablas()
        .get(idioma)
        .and_then(|tabla| tabla.get(clave))
        .or_else(|| tablas().get(RESPALDO).and_then(|tabla| tabla.get(clave)))
        .copied()
        .unwrap_or(clave);
    let mut resultado = texto.to_string();
    for (nombre, valor) in valores {
        resultado = resultado.replace(&format!("{{{nombre}}}"), &valor.to_string());
    }
    resultado
}

/// Text in the server language (logs, descriptions).
pub fn ts(clave: &str, valores: &[(&str, &dyn Display)]) -> String {
    t(servidor(), clave, valores)
}

pub fn texto(idioma: &str, clave: &str, valores: &[(&str, &dyn Display)]) -> TextComponent {
    TextComponent::text(&t(idioma, clave, valores))
}

/// Sends a translated message to a command sender, in its own language.
pub fn decir(sender: &CommandSender, clave: &str, valores: &[(&str, &dyn Display)]) {
    sender.send_message(texto(de_sender(sender), clave, valores));
}

/// Sends a translated chat message to a player, in their own language.
pub fn decir_jugador(jugador: &Player, clave: &str, valores: &[(&str, &dyn Display)]) {
    jugador.send_system_message(texto(de_jugador(jugador), clave, valores), false);
}
