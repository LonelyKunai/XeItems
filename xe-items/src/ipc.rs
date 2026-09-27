//! The XeItems API for other plugins, through Pumpkin's plugin messages (IPC, the
//! recipient is "XeItems"). A request is a JSON object with an "op"; the reply is a JSON
//! object with "ok": true and the op's fields, or an error text. Operations (README.md
//! has the full list):
//!
//! - `version`: API version and item namespace.
//! - `items`: every item (id, name, material, whether it has progression).
//! - `give`: a new copy (rolled rarity, record) for an online player, like a mob drop.
//! - `equip`: puts plain stacks of items on entities of a world (mob gear).
//! - `held`: the item a player holds and its tier, level and rarity.
//! - `grant_xp`: progression XP for the copy a player holds.

use std::sync::OnceLock;

use pumpkin_plugin_api::world::EquipmentSlot;
use pumpkin_plugin_api::{Player, Server};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{api, inventario, progresion, rareza, registro};

pub const VERSION: u32 = 1;

/// Plugin messages don't come with a server handle: this one is kept from on_load.
static SERVIDOR: OnceLock<Server> = OnceLock::new();

pub fn iniciar(server: Server) {
    let _ = SERVIDOR.set(server);
}

fn si() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Peticion {
    Version,
    Items,
    Give {
        player: String,
        id: String,
        #[serde(default)]
        amount: Option<u32>,
        #[serde(default)]
        rarity: Option<String>,
        /// Shown as the copy's origin (`/xi progress`), e.g. "xepve:rotting_brute".
        #[serde(default)]
        source: Option<String>,
        #[serde(default = "si")]
        announce: bool,
    },
    Equip {
        world: String,
        entities: Vec<Equipo>,
    },
    Held {
        player: String,
    },
    GrantXp {
        player: String,
        amount: u32,
    },
}

#[derive(Deserialize)]
struct Equipo {
    entity: u32,
    slot: String,
    item: String,
}

pub fn atender(remitente: &str, mensaje: &[u8]) -> Result<Vec<u8>, String> {
    let peticion: Peticion = serde_json::from_slice(mensaje).map_err(|e| format!("bad request: {e}"))?;
    let mut respuesta = match peticion {
        Peticion::Version => json!({ "api": VERSION, "namespace": crate::config().namespace }),
        Peticion::Items => items(),
        Peticion::Give { player, id, amount, rarity, source, announce } => {
            let origen = source.unwrap_or_else(|| format!("plugin:{remitente}"));
            give(&player, &id, amount.unwrap_or(1), rarity.as_deref(), &origen, announce)?
        }
        Peticion::Equip { world, entities } => equip(&world, &entities)?,
        Peticion::Held { player } => held(&player)?,
        Peticion::GrantXp { player, amount } => grant_xp(&player, amount)?,
    };
    respuesta["ok"] = Value::Bool(true);
    serde_json::to_vec(&respuesta).map_err(|e| e.to_string())
}

fn servidor() -> Result<&'static Server, String> {
    SERVIDOR.get().ok_or_else(|| "XeItems is not loaded yet".to_string())
}

fn jugador(uuid: &str) -> Result<Player, String> {
    let id = pumpkin_plugin_api::uuid::parse(uuid).ok_or_else(|| format!("bad uuid '{uuid}'"))?;
    servidor()?.get_player_by_uuid(id).ok_or_else(|| format!("player {uuid} is not online"))
}

fn items() -> Value {
    let lista: Vec<Value> = registro::ids()
        .into_iter()
        .filter_map(|id| registro::obtener(&id))
        .map(|def| {
            json!({
                "id": registro::id_de(&def),
                "name": api::nombre(&def),
                "material": api::material(&def.material),
                "progression": def.progresion.is_some(),
            })
        })
        .collect();
    json!({ "items": lista })
}

fn give(uuid: &str, id: &str, total: u32, rareza_: Option<&str>, origen: &str, avisar: bool) -> Result<Value, String> {
    let def = registro::obtener(id).ok_or_else(|| format!("unknown item '{id}'"))?;
    let rareza_ = rareza_.map(str::to_lowercase);
    if let Some(r) = rareza_.as_deref().filter(|r| !rareza::valida(r)) {
        return Err(format!("unknown rarity '{r}'"));
    }
    let j = jugador(uuid)?;
    let total = total.clamp(1, 64 * 36);
    // Copies with progression are one per stack; the rest go in full stacks.
    let por_stack = if def.progresion.is_some() { 1 } else { u32::from(api::stack(&def, Some(1)).get_max_count().max(1)) };
    let (mut dados, mut rarezas) = (0, vec![]);
    while dados < total {
        let n = (total - dados).min(por_stack) as u8;
        let r = progresion::dar_drop(&j, &def, n, origen, rareza_.as_deref(), avisar);
        if !r.is_empty() {
            rarezas.push(r);
        }
        dados += u32::from(n);
    }
    Ok(json!({ "id": registro::id_de(&def), "given": dados, "rarities": rarezas }))
}

fn ranura(nombre: &str) -> Option<EquipmentSlot> {
    Some(match nombre.to_lowercase().as_str() {
        "mainhand" | "main_hand" | "hand" => EquipmentSlot::MainHand,
        "offhand" | "off_hand" => EquipmentSlot::OffHand,
        "head" | "helmet" => EquipmentSlot::Head,
        "chest" | "chestplate" => EquipmentSlot::Chest,
        "legs" | "leggings" => EquipmentSlot::Legs,
        "feet" | "boots" => EquipmentSlot::Feet,
        "body" => EquipmentSlot::Body,
        _ => return None,
    })
}

/// Plain stacks (no copy record: mob gear isn't a copy anyone owns). One pass over the
/// world's entities for the whole batch.
fn equip(mundo: &str, lista: &[Equipo]) -> Result<Value, String> {
    let world = servidor()?.get_world_by_name(mundo).ok_or_else(|| format!("unknown world '{mundo}'"))?;
    let mut puestos = 0;
    let mut fallos = vec![];
    for e in world.get_entities() {
        let id = e.get_id();
        let mut pedidos = lista.iter().filter(|q| q.entity == id).peekable();
        if pedidos.peek().is_none() {
            continue;
        }
        let Some(vivo) = e.as_living() else { continue };
        for q in pedidos {
            match (ranura(&q.slot), registro::obtener(&q.item)) {
                (Some(r), Some(def)) => {
                    vivo.set_equipment(r, Some(api::stack(&def, Some(1))));
                    puestos += 1;
                }
                (None, _) => fallos.push(format!("bad slot '{}'", q.slot)),
                (_, None) => fallos.push(format!("unknown item '{}'", q.item)),
            }
        }
    }
    Ok(json!({ "equipped": puestos, "errors": fallos }))
}

fn held(uuid: &str) -> Result<Value, String> {
    let j = jugador(uuid)?;
    let Some((_, stack)) = inventario::en_mano(&j) else { return Ok(json!({ "id": null })) };
    let Some(id) = api::id_de_stack(&stack) else { return Ok(json!({ "id": null })) };
    let progresion_ = registro::obtener(&id).is_some_and(|d| d.progresion.is_some());
    let (estado, rareza_) = progresion::estado_rareza(&stack);
    Ok(json!({
        "id": id,
        "progression": progresion_,
        "tier": estado.tier,
        "level": estado.nivel,
        "rarity": rareza_,
    }))
}

fn grant_xp(uuid: &str, cantidad: u32) -> Result<Value, String> {
    let j = jugador(uuid)?;
    let copia = inventario::en_mano(&j).and_then(|(slot, stack)| {
        let def = registro::obtener(&api::id_de_stack(&stack)?)?;
        def.progresion.as_ref()?;
        progresion::instancia_de(&stack)?;
        Some((slot, stack, def))
    });
    let Some((slot, stack, def)) = copia else { return Ok(json!({ "granted": false })) };
    progresion::ganar(&j, slot, &stack, &def, cantidad);
    Ok(json!({ "granted": true, "id": registro::id_de(&def) }))
}
