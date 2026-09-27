//! Tier (I-XV) and level progression, kept per copy of an item: every copy gets a
//! creation id (`uid` on the stack) and a record in instances.json (see `instancias`)
//! with its tier, level, XP and the rarity rolled when it was made. The copy keeps its
//! progress whoever holds it. Items earn XP when their abilities are used, on hits and
//! on kills; at their top level they ascend to the next tier (back to level 1): by
//! themselves, by paying a cost, or, into a bracket that needs one, with a crafted core
//! (sneak + right click). Abilities unlock at a tier/level and get stronger with tier,
//! level and rarity. Kills can also drop items (see `ItemDef::drops`).

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use pumpkin_plugin_api::events::{EntityDeathEvent, EventData, EventPriority, PlayerDeathEvent};
use pumpkin_plugin_api::scheduler::schedule_repeating_task;
use pumpkin_plugin_api::text::TextComponent;
use pumpkin_plugin_api::uuid::Uuid;
use pumpkin_plugin_api::{Context, Entity, EventHandler, ItemStack, PersistentDataHolder, Player, Server};
use xe_common::lang::{de_jugador, t, ts};

use crate::instancias::{self, Instancia};
use crate::modelo::{disparador_de, Atributo, Habilidad, ItemDef, Progresion};
use crate::{api, inventario, nombres, rareza, registro};

pub const TIER_MAX: u8 = 15;
pub const NIVEL_MAX: u8 = 35;

/// The copy's creation id.
const CLAVE_UID: &str = "uid";
/// What the stack's lore shows: "tier|level|xp|rarity" (redrawn when it changes).
const CLAVE_MUESTRA: &str = "shown";

/// A kill counts for the last player who hit the victim this recently.
const MUERTE_VALIDA: Duration = Duration::from_secs(10);

struct Golpe {
    jugador: Uuid,
    /// The copy they hit with, if it has progression.
    uid: Option<String>,
    /// `minecraft:zombie`
    mob: Option<String>,
    cuando: Instant,
}

/// Victim entity id -> last player hit.
static GOLPES: Mutex<BTreeMap<i32, Golpe>> = Mutex::new(BTreeMap::new());
/// Dropped items waiting for space in the killer's inventory.
static PENDIENTES: Mutex<Vec<(Uuid, ItemStack)>> = Mutex::new(Vec::new());

fn bloquear<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone, Copy)]
pub struct Estado {
    pub tier: u8,
    pub nivel: u8,
    /// XP towards the next level.
    pub xp: u32,
}

impl Default for Estado {
    fn default() -> Self {
        Self { tier: 1, nivel: 1, xp: 0 }
    }
}

impl Estado {
    pub fn nuevo(tier: u8, nivel: u8, xp: u32) -> Self {
        Self { tier: tier.clamp(1, TIER_MAX), nivel: nivel.clamp(1, NIVEL_MAX), xp }
    }

    /// At least tier `tier` and, within that tier, level `nivel`.
    pub fn alcanza(&self, tier: u8, nivel: u8) -> bool {
        (self.tier, self.nivel) >= (tier, nivel)
    }
}

pub fn romano(n: u8) -> &'static str {
    const R: [&str; 16] = ["", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII", "XIII", "XIV", "XV"];
    R.get(usize::from(n)).copied().unwrap_or("?")
}

/// Colour of a tier: grey, green, aqua, purple, gold, then red for XV.
pub fn color(tier: u8) -> &'static str {
    match tier {
        0..=2 => "§7",
        3..=5 => "§a",
        6..=8 => "§b",
        9..=11 => "§d",
        12..=14 => "§6",
        _ => "§c",
    }
}

/// `§bVII`
pub fn tier_texto(tier: u8) -> String {
    format!("{}{}", color(tier), romano(tier))
}

pub fn tier_max(p: &Progresion) -> u8 {
    p.tier_max.clamp(1, TIER_MAX)
}

/// Highest level of each tier for this item.
pub fn nivel_max(p: &Progresion) -> u8 {
    p.nivel_max.clamp(1, NIVEL_MAX)
}

/// Id of the core that is the only way into `tier`, if it needs one.
pub fn puerta(p: &Progresion, tier: u8) -> Option<String> {
    p.nucleos.get(&tier).filter(|_| tier <= tier_max(p)).map(|n| api::id(n))
}

fn nombre_de(id: &str) -> String {
    registro::obtener(id).and_then(|d| api::nombre(&d)).unwrap_or_else(|| api::corto(id))
}

// --- copies ---

fn ns() -> &'static str {
    &crate::config().namespace
}

pub fn uid_de(stack: &ItemStack) -> Option<String> {
    PersistentDataHolder::get_string(stack, ns(), CLAVE_UID)
}

/// The copy's creation id and record.
pub fn instancia_de(stack: &ItemStack) -> Option<(String, Instancia)> {
    let uid = uid_de(stack)?;
    let inst = instancias::obtener(&uid)?;
    Some((uid, inst))
}

/// Progress and rarity of a stack (tier I, level 1, common without a record).
pub fn estado_rareza(stack: &ItemStack) -> (Estado, String) {
    instancia_de(stack).map_or_else(|| (Estado::default(), "common".into()), |(_, i)| (i.estado(), i.rareza))
}

/// What a stack's lore depends on: its definition and its copy's tier, level and rarity
/// (not XP: XP changes on every hit, and rewriting the held stack is what leaves ghost
/// copies on the client, see `inventario::poner`).
fn marca(def: &ItemDef, inst: Option<&Instancia>) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    serde_json::to_string(def).unwrap_or_default().hash(&mut h);
    match inst {
        Some(i) => format!("{:x}|{}|{}|{}", h.finish(), i.tier, i.nivel, i.rareza),
        None => format!("{:x}", h.finish()),
    }
}

/// A new copy of an item made for a player (crafted, dropped, given). With progression
/// it gets a record, with `rareza` or a rolled rarity.
pub fn crear(def: &ItemDef, cantidad: Option<u8>, origen: &str, creador: Option<&Player>, rareza: Option<&str>) -> ItemStack {
    let pila = api::stack(def, cantidad);
    if def.progresion.is_none() {
        return pila;
    }
    let r = rareza.unwrap_or_else(|| rareza::tirar(def));
    let uid = instancias::crear(&registro::id_de(def), r, origen, creador.map(Player::get_name));
    PersistentDataHolder::set_string(&pila, ns(), CLAVE_UID, &uid);
    redibujar(&pila).unwrap_or(pila)
}

/// Makes a stack that isn't a copy yet (a crafted result) into a new copy made by
/// `creador`, with a rolled rarity. None if it isn't an item with progression.
pub fn convertir(pila: &ItemStack, origen: &str, creador: &Player) -> Option<ItemStack> {
    let def = registro::obtener(&api::id_de_stack(pila)?)?;
    def.progresion.as_ref()?;
    let uid = instancias::crear(&registro::id_de(&def), rareza::tirar(&def), origen, Some(creador.get_name()));
    PersistentDataHolder::set_string(pila, ns(), CLAVE_UID, &uid);
    redibujar(pila)
}

/// Tells the player which rarity their new copy got (a title for epic and legendary).
pub fn anunciar_nueva(jugador: &Player, pila: &ItemStack) {
    let Some((_, inst)) = instancia_de(pila) else { return };
    let Some(def) = registro::obtener(&inst.item) else { return };
    let i = de_jugador(jugador);
    let m = t(i, "xi.new_copy", &[("item", &nombre_item(&def)), ("rarity", &rareza::etiqueta(&inst.rareza))]);
    jugador.send_system_message(TextComponent::text(&m), false);
    if rareza::bono(&inst.rareza) >= 0.2 {
        titulo(jugador, &rareza::etiqueta(&inst.rareza), &nombre_item(&def));
        sonido(jugador, "ui.toast.challenge_complete", 1.2);
    }
}

/// `viejo` rebuilt from its definition and its copy's record. A copy with progression
/// but no record (made before creation ids, or by other means) gets one here.
pub fn redibujar(viejo: &ItemStack) -> Option<ItemStack> {
    let id = api::id_de_stack(viejo)?;
    let def = registro::obtener(&id)?;
    if def.progresion.is_none() {
        let nuevo = api::reconstruir(viejo, None)?;
        PersistentDataHolder::set_string(&nuevo, ns(), CLAVE_MUESTRA, &marca(&def, None));
        return Some(nuevo);
    }
    let uid = uid_de(viejo)
        .filter(|u| instancias::obtener(u).is_some())
        .unwrap_or_else(|| instancias::crear(&id, rareza::tirar(&def), "found", None));
    let inst = instancias::obtener(&uid)?;
    let nuevo = api::reconstruir(viejo, Some((&uid, &inst)))?;
    PersistentDataHolder::set_string(&nuevo, ns(), CLAVE_UID, &uid);
    PersistentDataHolder::set_string(&nuevo, ns(), CLAVE_MUESTRA, &marca(&def, Some(&inst)));
    Some(nuevo)
}

/// Whether the stack already shows its definition and (with progression) its copy's
/// record as they are now. Not an XeItems item: nothing to update.
pub fn al_dia(stack: &ItemStack) -> bool {
    let Some(def) = api::id_de_stack(stack).and_then(|id| registro::obtener(&id)) else { return true };
    // Made when glint was still put on items: rebuilt without it, or it can't be saved.
    if api::tiene_brillo(stack) {
        return false;
    }
    let esperado = if def.progresion.is_some() {
        let Some((_, i)) = instancia_de(stack) else { return false };
        marca(&def, Some(&i))
    } else {
        marca(&def, None)
    };
    PersistentDataHolder::get_string(stack, ns(), CLAVE_MUESTRA) == Some(esperado)
}

// --- formulas ---

/// XP needed to leave the current level (at the top level: to ascend). None when the
/// copy can't go further on XP alone: highest tier, or top level with an ascension cost
/// or a core to use.
pub fn necesita(p: &Progresion, e: Estado) -> Option<u32> {
    if e.nivel >= nivel_max(p) && (p.coste_ascenso.is_some() || e.tier >= tier_max(p) || puerta(p, e.tier + 1).is_some()) {
        return None;
    }
    let xp = f64::from(p.xp_base.max(1))
        * p.xp_crecimiento.max(1.0).powi(i32::from(e.nivel) - 1)
        * p.xp_por_tier.max(1.0).powi(i32::from(e.tier) - 1);
    Some(xp.round().clamp(1.0, f64::from(i32::MAX)) as u32)
}

pub enum Cambio {
    Nivel(u8),
    Tier(u8),
}

/// Adds XP, going up as many levels (and tiers) as it pays for.
pub fn sumar(p: &Progresion, mut e: Estado, cantidad: u32) -> (Estado, Vec<Cambio>) {
    let mut cambios = vec![];
    e.xp = e.xp.saturating_add(cantidad);
    loop {
        let Some(n) = necesita(p, e) else {
            e.xp = 0;
            break;
        };
        if e.xp < n {
            break;
        }
        e.xp -= n;
        if e.nivel >= nivel_max(p) {
            e.tier += 1;
            e.nivel = 1;
            cambios.push(Cambio::Tier(e.tier));
        } else {
            e.nivel += 1;
            cambios.push(Cambio::Nivel(e.nivel));
        }
    }
    (e, cambios)
}

/// Multiplier for an ability's damage, healing, shields and durations: its own
/// `poder_nivel`/`poder_tier` when set, otherwise the item's; times the rarity bonus.
pub fn poder(def: &ItemDef, h: &Habilidad, e: Estado, rareza_: &str) -> f64 {
    def.progresion.as_ref().map_or(1.0, |p| {
        let (nivel, tier) = (h.poder_nivel.unwrap_or(p.poder_nivel), h.poder_tier.unwrap_or(p.poder_tier));
        (1.0 + nivel * f64::from(e.nivel - 1) + tier * f64::from(e.tier - 1)) * (1.0 + rareza::bono(rareza_))
    })
}

pub fn cooldown(def: &ItemDef, e: Estado, ticks: u32) -> u32 {
    let factor = def.progresion.as_ref().map_or(1.0, |p| (1.0 - p.cooldown_tier * f64::from(e.tier - 1)).max(0.25));
    (f64::from(ticks) * factor).round() as u32
}

pub fn desbloqueada(h: &Habilidad, e: Estado) -> bool {
    e.alcanza(h.tier.unwrap_or(1), h.nivel.unwrap_or(1))
}

/// Attribute amount at a tier and level (before the rarity bonus).
pub fn atributo(a: &Atributo, e: Estado) -> f64 {
    a.cantidad + a.por_nivel.unwrap_or(0.0) * f64::from(e.nivel - 1) + a.por_tier.unwrap_or(0.0) * f64::from(e.tier - 1)
}

pub fn xp_de(p: &Progresion, disparador: &str) -> u32 {
    p.xp.get(disparador).copied().unwrap_or(0)
}

// --- lore ---

/// Ability name in the server language: its `nombre`, or made from its key.
pub fn nombre_habilidad(clave: &str, h: &Habilidad) -> String {
    if let Some(n) = &h.nombre {
        return api::texto(n);
    }
    match clave.split_once(':') {
        Some((_, n)) => n
            .split('_')
            .map(|p| {
                let mut c = p.chars();
                c.next().map_or_else(String::new, |p| p.to_uppercase().collect::<String>() + c.as_str())
            })
            .collect::<Vec<_>>()
            .join(" "),
        None => ts(&format!("items.trigger.{clave}"), &[]),
    }
}

/// Lines added after the item's own lore (server language). `rareza_` is None for a
/// stack that isn't a copy yet (the editor's preview).
pub fn lore(def: &ItemDef, e: Estado, rareza_: Option<&str>) -> Vec<String> {
    let Some(p) = &def.progresion else { return vec![] };
    let mut l = vec![String::new()];
    if let Some(r) = rareza_ {
        l.push(rareza::etiqueta(r));
    }
    l.push(ts("items.prog.header", &[("tier", &tier_texto(e.tier)), ("level", &e.nivel), ("max", &nivel_max(p))]));
    match necesita(p, e) {
        // XP itself is shown in the action bar: the lore only changes on level-ups.
        Some(n) => l.push(ts("items.prog.next", &[("need", &n)])),
        None if e.tier >= tier_max(p) => l.push(ts("items.prog.mastered", &[])),
        None => match puerta(p, e.tier + 1) {
            Some(nucleo) => l.push(ts("items.prog.core_hint", &[("core", &nombre_de(&nucleo)), ("tier", &tier_texto(e.tier + 1))])),
            None => {
                let c = p.coste_ascenso.as_ref().map(|c| (c.cantidad.saturating_mul(u32::from(e.tier)), api::corto_material(&c.item)));
                let (cantidad, item) = c.unwrap_or_default();
                l.push(ts("items.prog.ascend_hint", &[("amount", &cantidad), ("item", &item)]));
            }
        },
    }
    if !def.habilidades.is_empty() {
        l.push(ts("items.prog.abilities", &[]));
        for (clave, h) in &def.habilidades {
            let nombre = nombre_habilidad(clave, h);
            let disparador = ts(&format!("items.trigger.{}", disparador_de(clave)), &[]);
            if desbloqueada(h, e) {
                l.push(ts("items.prog.unlocked", &[("name", &nombre), ("trigger", &disparador)]));
            } else {
                let (tier, nivel) = (h.tier.unwrap_or(1), h.nivel.unwrap_or(1));
                l.push(ts("items.prog.locked", &[("name", &nombre), ("tier", &romano(tier)), ("level", &nivel)]));
            }
        }
    }
    l
}

// --- changing a held copy ---

fn nombre_item(def: &ItemDef) -> String {
    api::nombre(def).unwrap_or_else(|| api::corto(&registro::id_de(def)))
}

/// Saves the copy's new progress and redraws `stack` (in `slot`) if its lore changed.
fn guardar(jugador: &Player, slot: u32, stack: &ItemStack, uid: &str, e: Estado) {
    instancias::fijar_estado(uid, e);
    if !al_dia(stack) {
        if let Some(nuevo) = redibujar(stack) {
            inventario::poner(jugador, slot, Some(nuevo));
        }
    }
}

/// Only the player hears it.
fn sonido(jugador: &Player, nombre: &str, tono: f32) {
    nombres::reproducir(nombre, &jugador.get_name(), jugador.get_position(), 1.0, tono);
}

fn particulas(jugador: &Player, nombre: &str, n: i32) {
    if let Some(p) = nombres::particula(nombre) {
        let (x, y, z) = jugador.get_position();
        jugador.get_world().spawn_particle(p, (x, y + 1.0, z), (0.5, 0.8, 0.5), 0.1, n);
    }
}

fn titulo(jugador: &Player, arriba: &str, abajo: &str) {
    jugador.send_title_animation(10, 40, 10);
    jugador.show_subtitle(TextComponent::text(abajo));
    jugador.show_title(TextComponent::text(arriba));
}

/// Titles, sounds and unlock messages for going from `antes` to `despues`.
fn anunciar(jugador: &Player, def: &ItemDef, antes: Estado, despues: Estado, cambios: &[Cambio]) {
    let i = de_jugador(jugador);
    let item = nombre_item(def);
    if let Some(Cambio::Tier(tier)) = cambios.iter().rev().find(|c| matches!(c, Cambio::Tier(_))) {
        titulo(
            jugador,
            &t(i, "xi.prog.tier_title", &[("tier", &tier_texto(*tier))]),
            &t(i, "xi.prog.tier_sub", &[("item", &item)]),
        );
        sonido(jugador, "ui.toast.challenge_complete", 1.0);
        particulas(jugador, "totem_of_undying", 60);
    } else if let Some(Cambio::Nivel(nivel)) = cambios.last() {
        titulo(jugador, &t(i, "xi.prog.level_title", &[]), &t(i, "xi.prog.level_sub", &[("item", &item), ("level", nivel)]));
        sonido(jugador, "entity.player.levelup", 1.2);
        particulas(jugador, "happy_villager", 20);
    }
    for (clave, h) in &def.habilidades {
        if !desbloqueada(h, antes) && desbloqueada(h, despues) {
            let m = t(i, "xi.prog.unlocked", &[("name", &nombre_habilidad(clave, h))]);
            jugador.send_system_message(TextComponent::text(&m), false);
        }
    }
    let Some(p) = &def.progresion else { return };
    let tope = nivel_max(p);
    let llego = !antes.alcanza(despues.tier, tope) && despues.nivel >= tope && despues.tier < tier_max(p);
    if !llego {
        return;
    }
    let m = match (puerta(p, despues.tier + 1), &p.coste_ascenso) {
        (Some(nucleo), _) => {
            let receta = registro::obtener(&nucleo).and_then(|d| d.recetas.first().map(api::resumen_receta)).unwrap_or_default();
            t(
                i,
                "xi.prog.ready_core",
                &[("item", &item), ("tier", &tier_texto(despues.tier + 1)), ("core", &nombre_de(&nucleo)), ("materials", &receta)],
            )
        }
        (None, Some(c)) => t(
            i,
            "xi.prog.ready",
            &[("item", &item), ("amount", &c.cantidad.saturating_mul(u32::from(despues.tier))), ("cost", &api::corto_material(&c.item))],
        ),
        (None, None) => return,
    };
    jugador.send_system_message(TextComponent::text(&m), false);
}

/// Gives `cantidad` XP to the copy `stack` (read from `slot`) and tells the player.
pub fn ganar(jugador: &Player, slot: u32, stack: &ItemStack, def: &ItemDef, cantidad: u32) {
    let Some(p) = &def.progresion else { return };
    // A copy without a record gets one within half a second (see refrescar).
    let Some((uid, inst)) = instancia_de(stack) else { return };
    if cantidad == 0 {
        return;
    }
    let antes = inst.estado();
    let (despues, cambios) = sumar(p, antes, cantidad);
    guardar(jugador, slot, stack, &uid, despues);
    let i = de_jugador(jugador);
    let valores: [(&str, &dyn std::fmt::Display); 5] = [
        ("gain", &cantidad),
        ("tier", &tier_texto(despues.tier)),
        ("level", &despues.nivel),
        ("xp", &despues.xp),
        ("need", &necesita(p, despues).unwrap_or(0)),
    ];
    let clave = if necesita(p, despues).is_some() { "xi.prog.bar" } else { "xi.prog.bar_max" };
    jugador.show_actionbar(TextComponent::text(&t(i, clave, &valores)));
    anunciar(jugador, def, antes, despues, &cambios);
}

/// Changes a copy's progress (commands). `quien` is an online player holding it, who
/// then sees it redrawn and gets the level-up messages. Returns the old and new progress.
pub fn cambiar(uid: &str, quien: Option<&Player>, cambio: impl FnOnce(&Progresion, Estado) -> Estado) -> Option<(Estado, Estado)> {
    let inst = instancias::obtener(uid)?;
    let def = registro::obtener(&inst.item)?;
    let p = def.progresion.as_ref()?;
    let antes = inst.estado();
    let mut despues = cambio(p, antes);
    despues.tier = despues.tier.min(tier_max(p));
    despues.nivel = despues.nivel.min(nivel_max(p));
    instancias::fijar_estado(uid, despues);
    if let Some(j) = quien {
        inventario::actualizar(j, Some(&inst.item));
        let cambios: Vec<Cambio> = if despues.tier > antes.tier {
            vec![Cambio::Tier(despues.tier)]
        } else if despues.alcanza(antes.tier, antes.nivel + 1) {
            vec![Cambio::Nivel(despues.nivel)]
        } else {
            vec![]
        };
        anunciar(j, &def, antes, despues, &cambios);
    }
    Some((antes, despues))
}

// --- ascension: a cost or a core ---

/// Plain (non-XeItems) stacks of `material` in the main inventory.
fn contar(jugador: &Player, material: &str) -> Vec<(u32, ItemStack)> {
    let inv = inventario::de(jugador);
    inventario::SLOTS
        .clone()
        .filter_map(|s| inv.get_item(s).map(|st| (s, st)))
        .filter(|(_, st)| api::material(&st.get_registry_key()) == material && api::id_de_stack(st).is_none())
        .collect()
}

/// Takes `total` from `stacks` (already checked to have enough).
fn quitar(jugador: &Player, stacks: Vec<(u32, ItemStack)>, total: u32) {
    let mut falta = total;
    for (slot, s) in stacks {
        if falta == 0 {
            break;
        }
        let n = u32::from(s.get_count());
        if n <= falta {
            inventario::poner(jugador, slot, None);
            falta -= n;
        } else {
            s.set_count((n - falta) as u8);
            inventario::poner(jugador, slot, Some(s));
            falta = 0;
        }
    }
}

fn pagar(jugador: &Player, material: &str, total: u32) -> bool {
    let stacks = contar(jugador, material);
    if stacks.iter().map(|(_, s)| u32::from(s.get_count())).sum::<u32>() < total {
        return false;
    }
    quitar(jugador, stacks, total);
    true
}

/// Takes one `nucleo` (an XeItems item) from the main inventory.
fn usar_nucleo(jugador: &Player, nucleo: &str) -> bool {
    let inv = inventario::de(jugador);
    let Some(par) = inventario::SLOTS
        .clone()
        .filter_map(|s| inv.get_item(s).map(|st| (s, st)))
        .find(|(_, st)| api::id_de_stack(st).as_deref() == Some(nucleo))
    else {
        return false;
    };
    quitar(jugador, vec![par], 1);
    true
}

/// Sneak + right click with a copy at its top level that ascends with a cost or a core:
/// pays it and moves to the next tier. Returns true if it ascended (the click does
/// nothing else).
pub fn ascender(jugador: &Player) -> bool {
    let Some((slot, stack)) = inventario::en_mano(jugador) else { return false };
    let Some((uid, inst)) = instancia_de(&stack) else { return false };
    let Some(def) = registro::obtener(&inst.item) else { return false };
    let Some(p) = &def.progresion else { return false };
    let antes = inst.estado();
    if antes.nivel < nivel_max(p) || antes.tier >= tier_max(p) {
        return false;
    }
    let i = de_jugador(jugador);
    let siguiente = tier_texto(antes.tier + 1);
    if let Some(nucleo) = puerta(p, antes.tier + 1) {
        if !usar_nucleo(jugador, &nucleo) {
            let m = t(i, "xi.prog.need_core", &[("tier", &siguiente), ("core", &nombre_de(&nucleo))]);
            jugador.show_actionbar(TextComponent::text(&m));
            return false;
        }
    } else if let Some(c) = &p.coste_ascenso {
        let total = c.cantidad.saturating_mul(u32::from(antes.tier));
        if !pagar(jugador, &api::material(&c.item), total) {
            let m = t(i, "xi.prog.need_cost", &[("tier", &siguiente), ("amount", &total), ("item", &api::corto_material(&c.item))]);
            jugador.show_actionbar(TextComponent::text(&m));
            return false;
        }
    } else {
        return false;
    }
    let despues = Estado { tier: antes.tier + 1, nivel: 1, xp: 0 };
    guardar(jugador, slot, &stack, &uid, despues);
    anunciar(jugador, &def, antes, despues, &[Cambio::Tier(despues.tier)]);
    true
}

// --- hits, kills and drops ---

/// Remembers the hit (for kill XP and drops) and gives hit XP to the held copy.
pub fn al_golpear(jugador: &Player, victima: &Entity) {
    let mano = inventario::en_mano(jugador);
    let copia = mano.as_ref().and_then(|(slot, s)| {
        let def = registro::obtener(&api::id_de_stack(s)?)?;
        def.progresion.as_ref()?;
        Some((*slot, s, def))
    });
    let golpe = Golpe {
        jugador: jugador.get_id(),
        uid: copia.as_ref().and_then(|(_, s, _)| uid_de(s)),
        mob: nombres::nombre_entidad(victima.get_type()),
        cuando: Instant::now(),
    };
    bloquear(&GOLPES).insert(victima.get_id() as i32, golpe);
    if let Some((slot, s, def)) = copia {
        let xp = def.progresion.as_ref().map_or(0, |p| xp_de(p, "hit"));
        ganar(jugador, slot, s, &def, xp);
    }
}

fn al_morir(server: &Server, victima: i32) {
    let Some(golpe) = bloquear(&GOLPES).remove(&victima) else { return };
    if golpe.cuando.elapsed() > MUERTE_VALIDA {
        return;
    }
    let Some(jugador) = server.get_player_by_uuid(golpe.jugador) else { return };
    // Kill XP for the copy they killed with, if they still hold it.
    if let Some((slot, stack)) = inventario::en_mano(&jugador) {
        if golpe.uid.is_some() && uid_de(&stack) == golpe.uid {
            if let Some(def) = api::id_de_stack(&stack).and_then(|id| registro::obtener(&id)) {
                let xp = def.progresion.as_ref().map_or(0, |p| xp_de(p, "kill"));
                ganar(&jugador, slot, &stack, &def, xp);
            }
        }
    }
    if let Some(mob) = golpe.mob {
        soltar(&jugador, &mob);
    }
}

/// Rolls the drops of every item for a mob killed by `jugador`. Drops go straight to
/// their inventory (the API can't spawn item stacks in the world).
fn soltar(jugador: &Player, mob: &str) {
    for def in registro::con_drops() {
        for d in def.drops.iter().filter(|d| d.mob == "*" || api::material(&d.mob) == mob) {
            if rareza::azar_real() >= d.probabilidad {
                continue;
            }
            dar_drop(jugador, &def, 1, &format!("drop:{mob}"), None, true);
        }
    }
}

/// A dropped stack for `jugador` (a kill here, or another plugin's loot through the
/// API): a new copy with `rareza_` or a rolled rarity, put in their inventory or kept
/// until there is space. Returns the copy's rarity (empty without progression).
pub fn dar_drop(jugador: &Player, def: &ItemDef, cantidad: u8, origen: &str, rareza_: Option<&str>, avisar: bool) -> String {
    let pila = crear(def, Some(cantidad), origen, Some(jugador), rareza_);
    let r = instancia_de(&pila).map(|(_, i)| i.rareza).unwrap_or_default();
    if avisar {
        let m = t(de_jugador(jugador), "xi.drop", &[("item", &nombre_item(def)), ("rarity", &rareza_texto(def, &r))]);
        jugador.send_system_message(TextComponent::text(&m), false);
        sonido(jugador, "entity.item.pickup", 0.8);
        if rareza::bono(&r) >= 0.2 {
            titulo(jugador, &rareza::etiqueta(&r), &nombre_item(def));
        }
    }
    if let Err(pila) = inventario::dar(jugador, pila) {
        decir_lleno(jugador);
        bloquear(&PENDIENTES).push((jugador.get_id(), pila));
    }
    r
}

fn decir_lleno(jugador: &Player) {
    let m = t(de_jugador(jugador), "gui.returned_later", &[]);
    jugador.send_system_message(TextComponent::text(&m), false);
}

/// `§6★★★★★ Legendary` for copies with progression, empty otherwise.
pub fn rareza_texto(def: &ItemDef, r: &str) -> String {
    if def.progresion.is_some() { rareza::etiqueta(r) } else { String::new() }
}

fn entregar(server: Server) {
    let pendientes = std::mem::take(&mut *bloquear(&PENDIENTES));
    let mut quedan = vec![];
    for (uid, pila) in pendientes {
        match server.get_player_by_uuid(uid) {
            Some(j) => {
                if let Err(pila) = inventario::dar(&j, pila) {
                    quedan.push((uid, pila));
                }
            }
            None => quedan.push((uid, pila)),
        }
    }
    bloquear(&PENDIENTES).extend(quedan);
}

struct AlMorir;

impl EventHandler<EntityDeathEvent> for AlMorir {
    fn handle(&self, server: Server, evento: EventData<EntityDeathEvent>) -> EventData<EntityDeathEvent> {
        al_morir(&server, evento.entity_id);
        evento
    }
}

struct AlMorirJugador;

impl EventHandler<PlayerDeathEvent> for AlMorirJugador {
    fn handle(&self, server: Server, evento: EventData<PlayerDeathEvent>) -> EventData<PlayerDeathEvent> {
        if !evento.cancelled {
            al_morir(&server, evento.player.as_entity().get_id() as i32);
        }
        evento
    }
}

fn limpiar(_server: Server) {
    bloquear(&GOLPES).retain(|_, g| g.cuando.elapsed() <= MUERTE_VALIDA);
}

/// Redraws the held copy when it has no record yet or its lore is outdated (changed by
/// a command, a duplicate of another copy, ...).
fn refrescar(server: Server) {
    if !registro::hay_progresion() {
        return;
    }
    for jugador in server.get_all_players() {
        let Some((slot, stack)) = inventario::en_mano(&jugador) else { continue };
        let Some(id) = api::id_de_stack(&stack) else { continue };
        if registro::obtener(&id).is_none_or(|d| d.progresion.is_none()) || al_dia(&stack) {
            continue;
        }
        if let Some(nuevo) = redibujar(&stack) {
            inventario::poner(&jugador, slot, Some(nuevo));
        }
    }
}

pub fn registrar(context: &Context) -> pumpkin_plugin_api::Result<()> {
    context.register_event_handler(AlMorir, EventPriority::Normal, true)?;
    context.register_event_handler(AlMorirJugador, EventPriority::Normal, true)?;
    schedule_repeating_task(200, 200, limpiar);
    schedule_repeating_task(10, 10, refrescar);
    schedule_repeating_task(20, 20, entregar);
    schedule_repeating_task(600, 600, |_| instancias::guardar_todo());
    Ok(())
}
