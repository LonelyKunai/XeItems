//! Crafting XeItems recipes.
//!
//! Every crafted item with progression becomes a new copy: a creation id and a record
//! with a rolled rarity, and the crafter is told which rarity they got.
//!
//! Patched Pumpkin (D:\Pumpkin-src) hands out the full result stack and gives
//! CraftItemEvent the real recipe id: the result only has to be made a copy once it
//! reaches the inventory.
//!
//! Unpatched Pumpkin gives plain results (type and count only), and CraftItemEvent's
//! "recipe id" is really the result's item key. The plugin can't see the grid, so a plain
//! craft is only fixed when its material can mean a single XeItems item (see
//! `registro::unico_por_material`). The crafter's inventory is then watched; when the
//! plain item shows up (dropped from the cursor or shift-clicked) it is replaced with the
//! full item. Plain items the player already had are left alone.
//!
//! Items with a craft token (`material_receta`) have recipes that give a plain token on
//! both: it's always swapped this way, whatever `fix_crafted_items` says.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use pumpkin_plugin_api::events::{CraftItemEvent, EventData, EventPriority, InventoryClickEvent};
use pumpkin_plugin_api::scheduler::schedule_repeating_task;
use pumpkin_plugin_api::uuid::Uuid;
use pumpkin_plugin_api::{Context, EventHandler, ItemStack, Player, Screen, Server};

use crate::modelo::ItemDef;
use crate::{api, inventario, progresion, registro};

struct Pendiente {
    jugador: Uuid,
    item: String,
    material: String,
    /// Plain stacks of that material per slot before crafting.
    antes: BTreeMap<u32, u8>,
    hasta: Instant,
}

static PENDIENTES: Mutex<Vec<Pendiente>> = Mutex::new(Vec::new());

/// Patched Pumpkin: full results of items with progression that still have to be made
/// copies: (player, item id, until).
static NUEVAS: Mutex<Vec<(Uuid, String, Instant)>> = Mutex::new(Vec::new());

/// Players whose last click was on a crafting result slot (slot 0 of their own inventory
/// or of a crafting table). CraftItemEvent fires for slot 0 of any window, chests too.
static EN_RESULTADO: Mutex<Vec<(u64, u64)>> = Mutex::new(Vec::new());

fn clave(uid: Uuid) -> (u64, u64) {
    (uid.high, uid.low)
}

struct AlClicar;

impl EventHandler<InventoryClickEvent> for AlClicar {
    fn handle(&self, _server: Server, evento: EventData<InventoryClickEvent>) -> EventData<InventoryClickEvent> {
        let k = clave(evento.player.get_id());
        let mut en = EN_RESULTADO.lock().unwrap_or_else(|e| e.into_inner());
        en.retain(|x| *x != k);
        // No window type: the player's own inventory (2x2 grid).
        if evento.slot == 0 && evento.window_type.is_none_or(|w| w == Screen::Crafting) {
            en.push(k);
        }
        evento
    }
}

fn sin_espacio(clave: &str) -> &str {
    clave.rsplit(':').next().unwrap_or(clave)
}

fn es_plano(stack: &ItemStack, material: &str) -> bool {
    sin_espacio(&stack.get_registry_key()) == sin_espacio(material)
        && api::id_de_stack(stack).is_none()
        && stack.get_custom_name().is_none()
        && stack.get_lore().is_empty()
        && stack.get_enchantments().is_empty()
}

fn planos(jugador: &Player, material: &str) -> BTreeMap<u32, u8> {
    let inv = inventario::de(jugador);
    inventario::SLOTS
        .clone()
        .filter_map(|s| inv.get_item(s).filter(|st| es_plano(st, material)).map(|st| (s, st.get_count())))
        .collect()
}

struct AlCraftear;

impl EventHandler<CraftItemEvent> for AlCraftear {
    fn handle(&self, _server: Server, evento: EventData<CraftItemEvent>) -> EventData<CraftItemEvent> {
        if evento.cancelled {
            return evento;
        }
        // Patched Pumpkin: an XeItems recipe id, and the result is already the full item
        // (or a plain craft token).
        if let Some(item) = registro::item_de_receta(&evento.recipe_id) {
            match registro::obtener(&item) {
                Some(def) if def.material_receta.is_some() => esperar(&evento.player, &def),
                Some(def) if def.progresion.is_some() => {
                    let hasta = Instant::now() + Duration::from_secs(crate::config().crafting.fix_timeout);
                    NUEVAS.lock().unwrap_or_else(|e| e.into_inner()).push((evento.player.get_id(), item, hasta));
                }
                _ => {}
            }
            return evento;
        }
        let en_resultado = EN_RESULTADO.lock().unwrap_or_else(|e| e.into_inner()).contains(&clave(evento.player.get_id()));
        if en_resultado {
            // Unpatched Pumpkin: `recipe_id` is the crafted item's key.
            if let Some(def) = registro::unico_por_material(&evento.recipe_id).and_then(|id| registro::obtener(&id)) {
                if def.material_receta.is_some() || crate::config().crafting.fix_crafted_items {
                    esperar(&evento.player, &def);
                }
            }
        }
        evento
    }
}

/// Watches `jugador`'s inventory for the plain result of crafting `def` (its craft token
/// or its material) to swap it for the item.
fn esperar(jugador: &Player, def: &ItemDef) {
    let material = api::material_de_receta(def);
    let antes = planos(jugador, &material);
    PENDIENTES.lock().unwrap_or_else(|e| e.into_inner()).push(Pendiente {
        jugador: jugador.get_id(),
        item: registro::id_de(def),
        material,
        antes,
        hasta: Instant::now() + Duration::from_secs(crate::config().crafting.fix_timeout),
    });
}

/// Returns true when the crafted item was found and replaced.
fn arreglar(jugador: &Player, p: &Pendiente) -> bool {
    let Some(def) = registro::obtener(&p.item) else { return true };
    let inv = inventario::de(jugador);
    for (slot, cantidad) in planos(jugador, &p.material) {
        let previa = p.antes.get(&slot).copied().unwrap_or(0);
        if cantidad <= previa {
            continue;
        }
        let hueco = if previa == 0 { Some(slot) } else { inventario::hueco_libre(&inv) };
        let Some(hueco) = hueco else { continue };
        let nuevo = progresion::crear(&def, Some(cantidad - previa), "craft", Some(jugador), None);
        progresion::anunciar_nueva(jugador, &nuevo);
        if previa > 0 {
            if let Some(viejo) = inv.get_item(slot) {
                viejo.set_count(previa);
                inventario::poner(jugador, slot, Some(viejo));
            }
        }
        inventario::poner(jugador, hueco, Some(nuevo));
        return true;
    }
    false
}

/// Patched Pumpkin: makes the crafted result (a stack of `item` that isn't a copy yet)
/// a copy. Returns true when done.
fn hacer_copia(jugador: &Player, item: &str) -> bool {
    let inv = inventario::de(jugador);
    for slot in inventario::SLOTS {
        let Some(pila) = inv.get_item(slot) else { continue };
        if api::id_de_stack(&pila).as_deref() != Some(item) || progresion::uid_de(&pila).is_some() {
            continue;
        }
        if let Some(copia) = progresion::convertir(&pila, "craft", jugador) {
            progresion::anunciar_nueva(jugador, &copia);
            inventario::poner(jugador, slot, Some(copia));
        }
        return true;
    }
    false
}

fn tarea(server: Server) {
    let nuevas = std::mem::take(&mut *NUEVAS.lock().unwrap_or_else(|e| e.into_inner()));
    if !nuevas.is_empty() {
        let ahora = Instant::now();
        let seguir: Vec<(Uuid, String, Instant)> = nuevas
            .into_iter()
            .filter(|(j, item, hasta)| match server.get_player_by_uuid(*j) {
                Some(jugador) => ahora <= *hasta && !hacer_copia(&jugador, item),
                None => false,
            })
            .collect();
        NUEVAS.lock().unwrap_or_else(|e| e.into_inner()).extend(seguir);
    }
    let pendientes = std::mem::take(&mut *PENDIENTES.lock().unwrap_or_else(|e| e.into_inner()));
    if pendientes.is_empty() {
        return;
    }
    let ahora = Instant::now();
    let seguir: Vec<Pendiente> = pendientes
        .into_iter()
        .filter(|p| match server.get_player_by_uuid(p.jugador) {
            Some(jugador) => ahora <= p.hasta && !arreglar(&jugador, p),
            None => false,
        })
        .collect();
    PENDIENTES.lock().unwrap_or_else(|e| e.into_inner()).extend(seguir);
}

pub fn registrar(context: &Context) -> pumpkin_plugin_api::Result<()> {
    // Always listened to: crafted items with progression become copies and craft tokens
    // are swapped either way.
    context.register_event_handler(AlCraftear, EventPriority::Normal, true)?;
    context.register_event_handler(AlClicar, EventPriority::Normal, true)?;
    schedule_repeating_task(2, 2, tarea);
    Ok(())
}
