//! Item definitions: loaded from every .json and .toml file in the `items` folder of the
//! data folder. Changes made with /xeitems or the editor are written back to the file the
//! item came from (new items go to custom.json).

use std::collections::BTreeMap;
use std::sync::Mutex;

use pumpkin_plugin_api::Server;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use xe_common::lang::ts;
use xe_common::{datos, error, info, warn};

use crate::modelo::{disparador_de, ItemDef, Receta, DISPARADORES};
use crate::{api, nombres, progresion, rareza};

/// Folder (in the data folder) with the item files.
pub const CARPETA: &str = "items";
/// File in `CARPETA` for items created in game or defined in code and then edited.
const NUEVOS: &str = "custom.json";
/// Where items were kept before the folder; moved to `NUEVOS` the first time.
const ANTIGUO: &str = "items.json";
/// Files written the first time the folder is created.
const PLANTILLAS: [(&str, &str); 1] = [("examples.json", include_str!("../items/examples.json"))];

static DEFINICIONES: Mutex<BTreeMap<String, ItemDef>> = Mutex::new(BTreeMap::new());
/// Recipe id -> id of the item it makes (for the crafted-item fix).
static RECETAS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
/// File name (in `CARPETA`) -> what it holds, in order. Items not in any file (defined in
/// code) aren't saved unless edited.
static ARCHIVOS: Mutex<BTreeMap<String, ArchivoItems>> = Mutex::new(BTreeMap::new());

#[derive(Default)]
struct ArchivoItems {
    entradas: Vec<Entrada>,
    /// Couldn't be read: never written, so fixing it by hand loses nothing.
    ilegible: bool,
}

enum Entrada {
    Item(String),
    /// An item that failed to load (or was overridden by a later one with the same id),
    /// written back as it was.
    Crudo(Value),
}

#[derive(Clone, Copy)]
enum Formato {
    Json,
    Toml,
}

fn formato(archivo: &str) -> Option<Formato> {
    let (_, extension) = archivo.rsplit_once('.')?;
    match extension.to_ascii_lowercase().as_str() {
        "json" => Some(Formato::Json),
        "toml" => Some(Formato::Toml),
        _ => None,
    }
}

fn ruta(archivo: &str) -> String {
    format!("{CARPETA}/{archivo}")
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Origen {
    /// Loaded from an item file.
    Archivo,
    /// Defined in the plugin's code.
    Codigo,
    /// Created or changed with a command or the editor: saved to its file.
    Usuario,
}

fn bloquear<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn id_de(def: &ItemDef) -> String {
    let base = def
        .id
        .clone()
        .or_else(|| def.recetas.first().map(|r| r.id().to_string()))
        .unwrap_or_else(|| def.material.rsplit(':').next().unwrap_or(&def.material).to_string());
    api::id(&base)
}

/// Creates or updates an item and registers its recipes. Returns its id.
pub fn definir(server: &Server, mut def: ItemDef, origen: Origen) -> Result<String, String> {
    let id = id_de(&def);
    def.id = Some(id.clone());
    if let Some(malo) = def.encantamientos.keys().find(|e| !api::encantamiento_valido(e)) {
        return Err(ts("xi.enchant_unknown", &[("enchant", malo)]));
    }
    if let Some(malo) = def.habilidades.keys().find(|k| !DISPARADORES.contains(&disparador_de(k))) {
        return Err(ts("xi.bad_value", &[("value", malo)]));
    }
    if let Some(p) = &def.progresion {
        if let Some(t) = p.nucleos.keys().find(|t| !(2..=progresion::TIER_MAX).contains(*t)) {
            return Err(ts("xi.bad_value", &[("value", &format!("tier {t}"))]));
        }
        if let Some(r) = p.rarezas.keys().find(|r| !rareza::valida(r)) {
            return Err(ts("xi.bad_value", &[("value", r)]));
        }
    }
    if let Some(d) = def.drops.iter().find(|d| d.mob != "*" && nombres::entidad(&d.mob).is_none()) {
        return Err(ts("xi.unknown_name", &[("value", &d.mob)]));
    }
    // Host calls first, without holding any lock (the host may call back into the plugin).
    for receta in &def.recetas {
        api::registrar_receta(server, &def, receta);
    }
    {
        let mut recetas = bloquear(&RECETAS);
        for receta in &def.recetas {
            recetas.insert(api::id(receta.id()), id.clone());
        }
    }
    bloquear(&DEFINICIONES).insert(id.clone(), def);
    if origen == Origen::Usuario {
        let archivo = {
            let mut archivos = bloquear(&ARCHIVOS);
            archivo_de(&archivos, &id).unwrap_or_else(|| {
                archivos.entry(NUEVOS.into()).or_default().entradas.push(Entrada::Item(id.clone()));
                NUEVOS.into()
            })
        };
        guardar(&archivo);
    }
    Ok(id)
}

/// The file an item was loaded from or saved to.
fn archivo_de(archivos: &BTreeMap<String, ArchivoItems>, id: &str) -> Option<String> {
    archivos
        .iter()
        .find(|(_, a)| a.entradas.iter().any(|e| matches!(e, Entrada::Item(i) if i == id)))
        .map(|(nombre, _)| nombre.clone())
}

pub fn obtener(id: &str) -> Option<ItemDef> {
    bloquear(&DEFINICIONES).get(&api::id(id)).cloned()
}

pub fn ids() -> Vec<String> {
    bloquear(&DEFINICIONES).keys().cloned().collect()
}

/// Whether any item has an ability for `disparador` (lets event handlers skip work).
pub fn hay_habilidad(disparador: &str) -> bool {
    bloquear(&DEFINICIONES).values().any(|d| d.habilidades.keys().any(|k| disparador_de(k) == disparador))
}

/// Whether any item has tier/level progression.
pub fn hay_progresion() -> bool {
    bloquear(&DEFINICIONES).values().any(|d| d.progresion.is_some())
}

pub fn item_de_receta(receta: &str) -> Option<String> {
    bloquear(&RECETAS).get(receta).cloned()
}

/// Items that vanilla crafting recipes make, one per line without namespace.
const VANILLA: &str = include_str!("vanilla_crafteo.txt");

fn vanilla_craftea(material: &str) -> bool {
    let corto = api::corto_material(material);
    VANILLA.lines().any(|l| l == corto)
}

fn es_de_mesa(r: &Receta) -> bool {
    matches!(r, Receta::Shaped { .. } | Receta::Shapeless { .. })
}

/// Result material (the craft token when there is one) -> ids of the items with
/// crafting-table recipes that make it.
fn por_material() -> BTreeMap<String, Vec<String>> {
    let mut mapa: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, def) in bloquear(&DEFINICIONES).iter() {
        if def.recetas.iter().any(es_de_mesa) {
            mapa.entry(api::material_de_receta(def)).or_default().push(id.clone());
        }
    }
    mapa
}

/// Unpatched Pumpkin only says which material was crafted. The XeItems item a crafted
/// plain `material` must be: only when a single item crafts it and vanilla doesn't.
pub fn unico_por_material(material: &str) -> Option<String> {
    let material = api::material(material);
    if vanilla_craftea(&material) {
        return None;
    }
    match por_material().remove(&material)?.as_slice() {
        [id] => Some(id.clone()),
        _ => None,
    }
}

/// Logs the items whose crafted copies can't be told apart on unpatched Pumpkin.
fn avisar_ambiguos() {
    for (material, ids) in por_material() {
        for id in &ids {
            let otros: Vec<String> = ids.iter().filter(|o| *o != id).map(|o| api::corto(o)).collect();
            let motivo = if vanilla_craftea(&material) {
                ts("items.craft.reason_vanilla", &[])
            } else if !otros.is_empty() {
                ts("items.craft.reason_shared", &[("others", &otros.join(", "))])
            } else {
                continue;
            };
            let m = ts(
                "items.craft.ambiguous",
                &[("id", &api::corto(id)), ("material", &api::corto_material(&material)), ("reason", &motivo)],
            );
            warn(&m);
        }
    }
}

/// Items that mobs can drop.
pub fn con_drops() -> Vec<ItemDef> {
    bloquear(&DEFINICIONES).values().filter(|d| !d.drops.is_empty()).cloned().collect()
}

/// Deletes an item. Its recipes stay registered until the server restarts (the API
/// can't remove recipes).
pub fn borrar(id: &str) -> bool {
    let id = api::id(id);
    if bloquear(&DEFINICIONES).remove(&id).is_none() {
        return false;
    }
    bloquear(&RECETAS).retain(|_, item| *item != id);
    let archivo = {
        let mut archivos = bloquear(&ARCHIVOS);
        let archivo = archivo_de(&archivos, &id);
        if let Some(a) = archivo.as_ref().and_then(|a| archivos.get_mut(a)) {
            a.entradas.retain(|e| !matches!(e, Entrada::Item(i) if *i == id));
        }
        archivo
    };
    if let Some(archivo) = archivo {
        guardar(&archivo);
    }
    true
}

/// Re-registers every item and recipe.
pub fn recargar(server: &Server) -> usize {
    let todos: Vec<ItemDef> = bloquear(&DEFINICIONES).values().cloned().collect();
    let n = todos.len();
    for def in todos {
        let _ = definir(server, def, Origen::Archivo);
    }
    n
}

/// An item file as written: `{"items": [...]}` in JSON, `[[items]]` tables in TOML.
#[derive(Serialize)]
struct Salida<'a> {
    items: Vec<ItemSalida<'a>>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ItemSalida<'a> {
    Def(&'a ItemDef),
    Crudo(&'a Value),
}

fn serializar(salida: &Salida, formato: Formato) -> Result<String, String> {
    match formato {
        Formato::Json => serde_json::to_string_pretty(salida).map(|t| t + "\n").map_err(|e| e.to_string()),
        Formato::Toml => toml::to_string_pretty(salida).map_err(|e| e.to_string()),
    }
}

/// Writes one item file with its current items.
fn guardar(archivo: &str) {
    let Some(formato) = formato(archivo) else { return };
    let texto = {
        let archivos = bloquear(&ARCHIVOS);
        let Some(a) = archivos.get(archivo) else { return };
        if a.ilegible {
            error(&ts("items.file_locked", &[("file", &ruta(archivo))]));
            return;
        }
        let defs = bloquear(&DEFINICIONES);
        let items = a
            .entradas
            .iter()
            .filter_map(|e| match e {
                Entrada::Item(id) => defs.get(id).map(ItemSalida::Def),
                Entrada::Crudo(valor) => Some(ItemSalida::Crudo(valor)),
            })
            .collect();
        serializar(&Salida { items }, formato)
    };
    match texto {
        Ok(texto) => {
            datos::escribir(&ruta(archivo), &texto);
        }
        Err(e) => error(&ts("items.save_failed", &[("file", &ruta(archivo)), ("error", &e)])),
    }
}

/// The `items` list of an item file, each item still unparsed.
fn leer_items(texto: &str, formato: Formato) -> Result<Vec<Value>, String> {
    #[derive(Deserialize)]
    struct Contenido {
        #[serde(default)]
        items: Vec<Value>,
    }
    let contenido: Contenido = match formato {
        Formato::Json => serde_json::from_str(texto).map_err(|e| e.to_string())?,
        Formato::Toml => toml::from_str(texto).map_err(|e| e.to_string())?,
    };
    Ok(contenido.items)
}

/// Creates the items folder the first time: moves the old items.json into it or, on a new
/// server, writes the bundled item files.
fn preparar_carpeta() {
    if datos::existe(CARPETA) || !datos::crear_carpeta(CARPETA) {
        return;
    }
    if datos::existe(ANTIGUO) {
        if datos::mover(ANTIGUO, &ruta(NUEVOS)) {
            info(&ts("items.migrated", &[("from", &ANTIGUO), ("to", &ruta(NUEVOS))]));
        }
        return;
    }
    for (archivo, contenido) in PLANTILLAS {
        datos::escribir(&ruta(archivo), contenido);
    }
}

fn cargar_archivo(server: &Server, archivo: &str, formato: Formato) {
    let leido = match datos::leer(&ruta(archivo)) {
        Some(texto) => leer_items(&texto, formato),
        None => Err(ts("items.file_unreadable", &[])),
    };
    let valores = match leido {
        Ok(valores) => valores,
        Err(e) => {
            error(&ts("items.import.invalid", &[("file", &ruta(archivo)), ("error", &e)]));
            bloquear(&ARCHIVOS).insert(archivo.into(), ArchivoItems { ilegible: true, ..Default::default() });
            return;
        }
    };
    bloquear(&ARCHIVOS).insert(archivo.into(), ArchivoItems::default());
    let mut cargados = 0;
    // Item by item, so one broken item doesn't prevent loading the rest.
    for (i, valor) in valores.into_iter().enumerate() {
        let resultado = serde_json::from_value::<ItemDef>(valor.clone()).map_err(|e| e.to_string()).and_then(|def| {
            let anterior = obtener(&id_de(&def));
            definir(server, def, Origen::Archivo).map(|id| (id, anterior))
        });
        let entrada = match resultado {
            Ok((id, anterior)) => {
                cargados += 1;
                if let Some(anterior) = anterior {
                    reemplazado(&id, anterior, archivo);
                }
                Entrada::Item(id)
            }
            Err(e) => {
                error(&ts("items.import.item_error", &[("file", &ruta(archivo)), ("n", &(i + 1)), ("error", &e)]));
                Entrada::Crudo(valor)
            }
        };
        if let Some(a) = bloquear(&ARCHIVOS).get_mut(archivo) {
            a.entradas.push(entrada);
        }
    }
    info(&ts("items.import.loaded", &[("count", &cargados), ("file", &ruta(archivo))]));
}

/// `id` was defined again by `archivo`: the earlier definition stays in its file as it
/// was, but no longer counts as that file's copy of the item.
fn reemplazado(id: &str, anterior: ItemDef, archivo: &str) {
    let mut archivos = bloquear(&ARCHIVOS);
    let Some(antes) = archivo_de(&archivos, id) else { return };
    warn(&ts("items.duplicate", &[("id", &api::corto(id)), ("file", &ruta(&antes)), ("by", &ruta(archivo))]));
    let Ok(valor) = serde_json::to_value(&anterior) else { return };
    let Some(a) = archivos.get_mut(&antes) else { return };
    if let Some(e) = a.entradas.iter_mut().find(|e| matches!(e, Entrada::Item(i) if i == id)) {
        *e = Entrada::Crudo(valor);
    }
}

/// Loads every .json and .toml file in the items folder, in name order (a file can
/// redefine an item of an earlier one).
pub fn cargar(server: &Server) {
    preparar_carpeta();
    for archivo in datos::listar(CARPETA) {
        if let Some(formato) = formato(&archivo) {
            cargar_archivo(server, &archivo, formato);
        }
    }
    if crate::config().crafting.fix_crafted_items {
        avisar_ambiguos();
    }
    avisar_brillo();
}

/// Logs the items with glint turned on, which they can't have for now (see `api::propiedades`).
fn avisar_brillo() {
    let ids: Vec<String> = bloquear(&DEFINICIONES).values().filter(|d| d.brillo).map(|d| api::corto(&id_de(d))).collect();
    if !ids.is_empty() {
        warn(&ts("items.glint_off", &[("items", &ids.join(", "))]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bundled item files load, and every item survives being written as TOML and
    /// read back (what happens to a .toml item file after a change made in game).
    #[test]
    fn plantillas_y_toml() {
        let mut ids = std::collections::BTreeSet::new();
        for (archivo, texto) in PLANTILLAS {
            let valores = leer_items(texto, formato(archivo).unwrap()).unwrap();
            assert!(!valores.is_empty(), "{archivo}");
            let defs: Vec<ItemDef> = valores
                .into_iter()
                .map(|v| serde_json::from_value(v).unwrap_or_else(|e| panic!("{archivo}: {e}")))
                .collect();
            for def in &defs {
                assert!(ids.insert(def.id.clone().unwrap()), "{archivo}: duplicate {:?}", def.id);
            }
            let salida = Salida { items: defs.iter().map(ItemSalida::Def).collect() };
            let toml = serializar(&salida, Formato::Toml).unwrap();
            let json = serializar(&salida, Formato::Json).unwrap();
            let desde_toml = leer_items(&toml, Formato::Toml).unwrap();
            let desde_json = leer_items(&json, Formato::Json).unwrap();
            let normalizar = |valores: Vec<Value>| -> Vec<Value> {
                valores
                    .into_iter()
                    .map(|v| serde_json::to_value(serde_json::from_value::<ItemDef>(v).unwrap()).unwrap())
                    .collect()
            };
            assert_eq!(normalizar(desde_toml), normalizar(desde_json), "{archivo}");
        }
    }

    #[test]
    fn formatos() {
        assert!(matches!(formato("a.json"), Some(Formato::Json)));
        assert!(matches!(formato("a.TOML"), Some(Formato::Toml)));
        assert!(formato("a.json.tmp").is_none());
        assert!(formato("json").is_none());
    }
}
