//! Copies of items with progression. Each copy gets a creation id when it is made
//! (stored on the stack as `uid`); its tier, level, XP and rarity live in
//! `instances.json` in the data folder, keyed by that id:
//! `{"instancias": {"<uid>": {"item": "xeitems:emberfang", "tier": 7, "nivel": 18, "xp": 120,
//! "rareza": "epic", "origen": "craft", "creador": "Steve", "creado": 1790350000}}}`.
//! Changes stay in memory and are written every 30 seconds and when the plugin stops.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use xe_common::{datos, error};

use crate::progresion::Estado;

pub const ARCHIVO: &str = "instances.json";

#[derive(Clone, Serialize, Deserialize)]
pub struct Instancia {
    pub item: String,
    pub tier: u8,
    pub nivel: u8,
    pub xp: u32,
    pub rareza: String,
    /// craft, drop:minecraft:blaze, give, found (a copy made before creation ids).
    pub origen: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creador: Option<String>,
    /// Unix seconds.
    #[serde(default)]
    pub creado: u64,
}

impl Instancia {
    pub fn estado(&self) -> Estado {
        Estado::nuevo(self.tier, self.nivel, self.xp)
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Archivo {
    #[serde(default)]
    instancias: BTreeMap<String, Instancia>,
}

static INSTANCIAS: Mutex<BTreeMap<String, Instancia>> = Mutex::new(BTreeMap::new());
static SUCIO: AtomicBool = AtomicBool::new(false);

fn bloquear<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn cargar() {
    let Some(texto) = datos::leer(ARCHIVO) else { return };
    match serde_json::from_str::<Archivo>(&texto) {
        Ok(a) => *bloquear(&INSTANCIAS) = a.instancias,
        Err(e) => error(&xe_common::lang::ts("items.import.invalid", &[("file", &ARCHIVO), ("error", &e)])),
    }
}

pub fn guardar_todo() {
    if !SUCIO.swap(false, Ordering::Relaxed) {
        return;
    }
    let json = {
        let instancias = bloquear(&INSTANCIAS);
        serde_json::to_string_pretty(&serde_json::json!({ "instancias": &*instancias })).unwrap_or_default()
    };
    if !json.is_empty() {
        datos::escribir(ARCHIVO, &(json + "\n"));
    }
}

pub fn obtener(uid: &str) -> Option<Instancia> {
    bloquear(&INSTANCIAS).get(uid).cloned()
}

fn ahora() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// A new copy of `item`; returns its creation id.
pub fn crear(item: &str, rareza: &str, origen: &str, creador: Option<String>) -> String {
    let uid = pumpkin_plugin_api::uuid::to_string(pumpkin_plugin_api::uuid::generate());
    let inst = Instancia {
        item: item.to_string(),
        tier: 1,
        nivel: 1,
        xp: 0,
        rareza: rareza.to_string(),
        origen: origen.to_string(),
        creador,
        creado: ahora(),
    };
    bloquear(&INSTANCIAS).insert(uid.clone(), inst);
    SUCIO.store(true, Ordering::Relaxed);
    uid
}

pub fn fijar_estado(uid: &str, e: Estado) {
    if let Some(i) = bloquear(&INSTANCIAS).get_mut(uid) {
        (i.tier, i.nivel, i.xp) = (e.tier, e.nivel, e.xp);
        SUCIO.store(true, Ordering::Relaxed);
    }
}

pub fn fijar_rareza(uid: &str, rareza: &str) {
    if let Some(i) = bloquear(&INSTANCIAS).get_mut(uid) {
        i.rareza = rareza.to_string();
        SUCIO.store(true, Ordering::Relaxed);
    }
}

/// Creation ids that start with `prefijo` (for commands: `#1a2b3c4d`).
pub fn buscar(prefijo: &str) -> Vec<String> {
    let p = prefijo.to_lowercase();
    bloquear(&INSTANCIAS).keys().filter(|k| k.starts_with(&p)).cloned().collect()
}
