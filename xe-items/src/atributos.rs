//! Item attributes, applied to the player instead of the item. Pumpkin saves an item's
//! `attribute_modifiers` component as an empty (End) tag, which corrupts the whole
//! player file: the inventory is lost on the next load. So items never carry that
//! component; every half second the attributes of the items in hand and armour are put
//! on the player as modifiers, and the ones no longer held are removed.
//!
//! The amounts are added on top of the item's vanilla stats (an iron sword keeps its 6
//! attack damage) and grow with tier and level like before.

use std::collections::BTreeMap;
use std::sync::Mutex;

use pumpkin_plugin_api::events::{EventData, EventPriority, PlayerLeaveEvent};
use pumpkin_plugin_api::scheduler::schedule_repeating_task;
use pumpkin_plugin_api::{Attribute, AttributeModifier, Context, EventHandler, ItemStack, ModifierOperation, Player, Server};
use xe_common::lang::ts;

use crate::modelo::{Atributo, ItemDef};
use crate::progresion::{self, Estado};
use crate::{api, inventario, nombres, rareza, registro};

/// Modifier id -> what it is, per player uuid: the modifiers this plugin put on them.
type Aplicados = BTreeMap<String, (Attribute, f64, ModifierOperation)>;
static APLICADOS: Mutex<BTreeMap<String, Aplicados>> = Mutex::new(BTreeMap::new());

fn bloquear<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Where an item is, and the `ranura` values that apply there.
const LUGARES: [(&str, &[&str]); 6] = [
    ("mainhand", &["mainhand", "hand", "any"]),
    ("offhand", &["offhand", "hand", "any"]),
    ("head", &["head", "armor", "any"]),
    ("chest", &["chest", "armor", "any"]),
    ("legs", &["legs", "armor", "any"]),
    ("feet", &["feet", "armor", "any"]),
];

fn operacion(a: &Atributo) -> ModifierOperation {
    match a.operacion.as_deref() {
        Some("multiply_base") => ModifierOperation::MultiplyBase,
        Some("multiply_total") => ModifierOperation::MultiplyTotal,
        _ => ModifierOperation::Add,
    }
}

/// The amount for a copy at a tier and level, with its rarity bonus.
pub fn cantidad(def: &ItemDef, a: &Atributo, e: Estado, rareza_: &str) -> f64 {
    if def.progresion.is_some() { progresion::atributo(a, e) * (1.0 + rareza::bono(rareza_)) } else { a.cantidad }
}

/// The modifiers the player should have for what they hold and wear.
fn deseados(jugador: &Player) -> Aplicados {
    let p = jugador.get_inventory();
    let pilas: [Option<ItemStack>; 6] = [
        inventario::en_mano(jugador).map(|(_, s)| s),
        p.get_off_hand(),
        p.get_helmet(),
        p.get_chestplate(),
        p.get_leggings(),
        p.get_boots(),
    ];
    let ns = &crate::config().namespace;
    let mut out = Aplicados::new();
    for ((lugar, ranuras), pila) in LUGARES.iter().zip(pilas) {
        let Some(pila) = pila else { continue };
        let Some(id) = api::id_de_stack(&pila) else { continue };
        let Some(def) = registro::obtener(&id) else { continue };
        if def.atributos.is_empty() {
            continue;
        }
        let (e, r) = progresion::estado_rareza(&pila);
        for (i, a) in def.atributos.iter().enumerate() {
            if !ranuras.contains(&a.ranura.as_deref().unwrap_or("mainhand")) {
                continue;
            }
            let Some(atributo) = nombres::atributo(&a.atributo) else { continue };
            let clave = format!("{ns}:{}_{i}_{lugar}", api::corto(&id).replace(':', "_"));
            out.insert(clave, (atributo, cantidad(&def, a, e, &r), operacion(a)));
        }
    }
    out
}

fn aplicar(jugador: &Player, deseado: Aplicados) {
    let uuid = pumpkin_plugin_api::uuid::to_string(jugador.get_id());
    let actual = bloquear(&APLICADOS).get(&uuid).cloned().unwrap_or_default();
    if actual == deseado {
        return;
    }
    let Some(vivo) = jugador.as_entity().as_living() else { return };
    for (clave, (atributo, ..)) in &actual {
        if deseado.get(clave) != actual.get(clave) {
            vivo.remove_attribute_modifier(*atributo, clave);
        }
    }
    for (clave, (atributo, cantidad, op)) in &deseado {
        if actual.get(clave) != deseado.get(clave) {
            vivo.add_attribute_modifier(*atributo, &AttributeModifier::new(clave.clone(), *cantidad, *op));
        }
    }
    let mut aplicados = bloquear(&APLICADOS);
    if deseado.is_empty() {
        aplicados.remove(&uuid);
    } else {
        aplicados.insert(uuid, deseado);
    }
}

fn tarea(server: Server) {
    for jugador in server.get_all_players() {
        let deseado = deseados(&jugador);
        aplicar(&jugador, deseado);
    }
}

struct AlSalir;

impl EventHandler<PlayerLeaveEvent> for AlSalir {
    fn handle(&self, _server: Server, evento: EventData<PlayerLeaveEvent>) -> EventData<PlayerLeaveEvent> {
        // Take them off so they're never saved with the player.
        aplicar(&evento.player, Aplicados::new());
        evento
    }
}

pub fn registrar(context: &Context) -> pumpkin_plugin_api::Result<()> {
    context.register_event_handler(AlSalir, EventPriority::Normal, true)?;
    schedule_repeating_task(10, 10, tarea);
    Ok(())
}

// --- lore ---

/// `attack_damage` -> "Attack Damage" (or the translation, when there is one).
fn nombre(atributo: &str) -> String {
    let clave = format!("items.attr.{atributo}");
    let t = ts(&clave, &[]);
    if t != clave {
        return t;
    }
    atributo
        .split('_')
        .map(|p| {
            let mut c = p.chars();
            c.next().map_or_else(String::new, |p| p.to_uppercase().collect::<String>() + c.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn numero(v: f64) -> String {
    let s = format!("{:.2}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Lore lines with the item's stats (server language), since the vanilla tooltip no
/// longer shows them.
pub fn lore(def: &ItemDef, e: Estado, rareza_: &str) -> Vec<String> {
    if def.atributos.is_empty() {
        return vec![];
    }
    let mut l = vec![String::new(), ts("items.attr.header", &[])];
    for a in &def.atributos {
        let v = cantidad(def, a, e, rareza_);
        let valor = match a.operacion.as_deref() {
            Some("multiply_base" | "multiply_total") => format!("{}{}%", if v >= 0.0 { "+" } else { "" }, numero(v * 100.0)),
            _ => format!("{}{}", if v >= 0.0 { "+" } else { "" }, numero(v)),
        };
        let donde = match a.ranura.as_deref().unwrap_or("mainhand") {
            "mainhand" => String::new(),
            r => format!(" §8({})", ts(&format!("items.slot.{r}"), &[])),
        };
        l.push(format!("§9{valor} {}{donde}", nombre(&a.atributo)));
    }
    l
}
