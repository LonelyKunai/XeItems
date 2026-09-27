//! Visual item editor: /xeitems edit <id>
//!   - main view (9x3): buttons for name, lore, enchantments, amount, material and recipes
//!   - enchantments view (9x6): one book per vanilla enchantment
//!   - recipe view (3x3): the player places real ingredients; closing it creates the
//!     recipe and returns the items
//! Name and lore are typed in chat. open_gui() takes the Gui, so every change opens a
//! new one; the recipe grid is read through an inventory handle taken before opening.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use pumpkin_plugin_api::common::ClickType;
use pumpkin_plugin_api::events::{EventData, EventPriority, InventoryClickEvent, InventoryCloseEvent, PlayerChatEvent};
use pumpkin_plugin_api::gui::Gui;
use pumpkin_plugin_api::scheduler::{schedule_delayed_task, schedule_repeating_task};
use pumpkin_plugin_api::text::TextComponent;
use pumpkin_plugin_api::uuid::Uuid;
use pumpkin_plugin_api::{Context, EventHandler, Inventory, ItemStack, Player, Screen, Server};
use xe_common::lang::{de_jugador, decir_jugador, t};

use crate::modelo::{ItemDef, Lore, Receta, Texto};
use crate::registro::{self, Origen};
use crate::{api, comandos, inventario};

const PRINCIPAL: Screen = Screen::Generic9x3;
const ENCANTAMIENTOS: Screen = Screen::Generic9x6;
const RECETA: Screen = Screen::Generic3x3;

const SLOT_VISTA: i16 = 4;
const SLOT_NOMBRE: i16 = 10;
const SLOT_LORE: i16 = 11;
const SLOT_ENCANTAMIENTOS: i16 = 12;
const SLOT_CANTIDAD: i16 = 13;
const SLOT_MATERIAL: i16 = 14;
const SLOT_SHAPED: i16 = 15;
const SLOT_SHAPELESS: i16 = 16;
const SLOT_IRROMPIBLE: i16 = 19;
const SLOT_BRILLO: i16 = 20;
const SLOT_COCCION: i16 = 21;
const SLOT_RECETAS: i16 = 22;
const SLOT_EXPORTAR: i16 = 23;
const SLOT_RAREZA: i16 = 24;
const SLOT_DETALLES: i16 = 25;
const SLOT_VOLVER: i16 = 49;

/// Closes that arrive this soon after the plugin opened a window belong to the previous one.
const CIERRE_IGNORADO: Duration = Duration::from_millis(500);

struct Estado {
    item: String,
    pantalla: Screen,
    abierto: Instant,
    /// Recipe view: (type, cooking station, grid handle).
    receta: Option<(&'static str, &'static str, Inventory)>,
}

static ESTADOS: Mutex<BTreeMap<String, Estado>> = Mutex::new(BTreeMap::new());
/// Players typing a name/lore in chat: player -> (item, "nombre" | "lore").
static CHAT: Mutex<BTreeMap<String, (String, &'static str)>> = Mutex::new(BTreeMap::new());
/// Ingredients waiting for inventory space.
static DEVOLVER: Mutex<Vec<(Uuid, ItemStack)>> = Mutex::new(Vec::new());

fn bloquear<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn clave(jugador: &Player) -> String {
    let uid = jugador.get_id();
    format!("{}:{}", uid.high, uid.low)
}

fn boton(material: &str, nombre: TextComponent, lore: &[String]) -> ItemStack {
    let item = ItemStack::new(material, 1);
    item.set_custom_name(Some(nombre));
    if !lore.is_empty() {
        item.set_lore(lore.iter().map(|l| TextComponent::text(l)).collect());
    }
    item
}

fn modificar(jugador: &Player, server: &Server, item: &str, cambio: impl FnOnce(&mut ItemDef)) -> bool {
    let Some(mut def) = registro::obtener(item) else {
        decir_jugador(jugador, "xi.not_found", &[("id", &item)]);
        return false;
    };
    cambio(&mut def);
    match registro::definir(server, def, Origen::Usuario) {
        Ok(_) => true,
        Err(e) => {
            decir_jugador(jugador, "xi.error", &[("error", &e)]);
            false
        }
    }
}

fn mostrar(jugador: &Player, gui: Gui, estado: Estado) {
    bloquear(&ESTADOS).insert(clave(jugador), estado);
    jugador.open_gui(gui);
}

// --- main view ---

pub fn abrir(jugador: &Player, item: &str) {
    let i = de_jugador(jugador);
    let tr = |k: &str| t(i, k, &[]);
    let Some(def) = registro::obtener(item) else { return };
    let gui = Gui::new(PRINCIPAL, TextComponent::text(&t(i, "gui.title", &[("id", &api::corto(item))])));
    gui.set_allow_grab_items(false);
    gui.set_allow_put_items(false);
    for slot in 0..27 {
        gui.set_item(slot, boton("minecraft:gray_stained_glass_pane", TextComponent::text(" "), &[]));
    }
    let vista = api::stack(&def, None);
    vista.add_lore(TextComponent::text(&tr("gui.preview_hint")));
    gui.set_item(SLOT_VISTA as u32, vista);

    let actual = |v: &dyn std::fmt::Display| t(i, "gui.current", &[("value", v)]);
    let ninguno = tr("gui.none");
    let nombre = api::nombre(&def).unwrap_or_else(|| ninguno.clone());
    let mut lore = api::lore(&def);
    if lore.is_empty() {
        lore.push(ninguno.clone());
    }
    lore.push(tr("gui.lore_help"));
    let encs: Vec<String> = if def.encantamientos.is_empty() {
        vec![ninguno.clone()]
    } else {
        def.encantamientos.iter().map(|(e, n)| format!("{e} {n}")).collect()
    };
    let recetas: Vec<String> = if def.recetas.is_empty() {
        vec![ninguno.clone()]
    } else {
        def.recetas.iter().map(|r| format!("{} ({})", r.id(), r.tipo())).collect()
    };
    let si_no = |b: bool| actual(&tr(if b { "gui.yes" } else { "gui.no" }));
    let mut detalles = comandos::detalles(i, &def);
    if detalles.is_empty() {
        detalles.push(ninguno.clone());
    }
    detalles.push(tr("gui.details_help"));
    let botones: [(i16, &str, &str, Vec<String>); 14] = [
        (SLOT_IRROMPIBLE, "minecraft:anvil", "gui.unbreakable", vec![si_no(def.irrompible), tr("gui.toggle_help")]),
        (SLOT_BRILLO, "minecraft:experience_bottle", "gui.glint", vec![si_no(def.brillo), tr("gui.toggle_help")]),
        (SLOT_RAREZA, "minecraft:amethyst_shard", "gui.rarity", vec![actual(&def.rareza.clone().unwrap_or(ninguno.clone())), tr("gui.rarity_help")]),
        (SLOT_DETALLES, "minecraft:spyglass", "gui.details", detalles),
        (SLOT_NOMBRE, "minecraft:name_tag", "gui.name", vec![actual(&nombre), tr("gui.click_chat")]),
        (SLOT_LORE, "minecraft:writable_book", "gui.lore", lore),
        (SLOT_ENCANTAMIENTOS, "minecraft:enchanted_book", "gui.enchants", encs),
        (SLOT_CANTIDAD, "minecraft:hopper", "gui.amount", vec![actual(&def.cantidad.unwrap_or(1)), tr("gui.amount_help")]),
        (SLOT_MATERIAL, "", "gui.material", vec![actual(&def.material), tr("gui.material_help")]),
        (SLOT_SHAPED, "minecraft:crafting_table", "gui.shaped", vec![tr("gui.recipe_help")]),
        (SLOT_SHAPELESS, "minecraft:chest", "gui.shapeless", vec![tr("gui.recipe_help")]),
        (SLOT_COCCION, "minecraft:furnace", "gui.cooking", vec![tr("gui.recipe_help"), tr("gui.cooking_help")]),
        (SLOT_RECETAS, "minecraft:knowledge_book", "gui.recipes", recetas),
        (SLOT_EXPORTAR, "minecraft:paper", "gui.export", vec![tr("gui.export_help")]),
    ];
    for (slot, material, titulo, lineas) in botones {
        let material = if material.is_empty() { api::material(&def.material) } else { material.to_string() };
        gui.set_item(slot as u32, boton(&material, TextComponent::text(&tr(titulo)), &lineas));
    }
    mostrar(jugador, gui, Estado { item: item.to_string(), pantalla: PRINCIPAL, abierto: Instant::now(), receta: None });
}

fn pedir_chat(jugador: &Player, item: &str, campo: &'static str) {
    bloquear(&CHAT).insert(clave(jugador), (item.to_string(), campo));
    decir_jugador(jugador, if campo == "nombre" { "gui.type_name" } else { "gui.type_lore" }, &[]);
}

fn exportar(jugador: &Player, item: &str) {
    let json = registro::obtener(item).and_then(|d| serde_json::to_string(&d).ok()).unwrap_or_default();
    let mensaje = TextComponent::text(&t(de_jugador(jugador), "gui.export_click", &[("id", &item)]))
        .click_copy_to_clipboard(&json)
        .hover_show_text(TextComponent::text(&json));
    jugador.send_system_message(mensaje, false);
}

fn es_shift(c: ClickType) -> bool {
    matches!(c, ClickType::ShiftLeft | ClickType::ShiftRight)
}

fn clic_principal(jugador: &Player, server: &Server, item: &str, slot: i16, clic: ClickType, cursor: Option<&ItemStack>) {
    let derecho = matches!(clic, ClickType::Right | ClickType::ShiftRight);
    let cambio = match slot {
        SLOT_VISTA => {
            if let Some(def) = registro::obtener(item) {
                if inventario::dar(jugador, crate::progresion::crear(&def, None, "give", Some(jugador), None)).is_err() {
                    decir_jugador(jugador, "xi.inventory_full", &[("player", &jugador.get_name())]);
                }
            }
            false
        }
        SLOT_NOMBRE => {
            pedir_chat(jugador, item, "nombre");
            false
        }
        SLOT_LORE if derecho => modificar(jugador, server, item, |d| d.lore = None),
        SLOT_LORE => {
            pedir_chat(jugador, item, "lore");
            false
        }
        SLOT_ENCANTAMIENTOS => {
            abrir_encantamientos(jugador, item);
            false
        }
        SLOT_CANTIDAD => {
            let paso: i16 = if es_shift(clic) { 8 } else { 1 } * if derecho { -1 } else { 1 };
            modificar(jugador, server, item, |d| {
                d.cantidad = Some((i16::from(d.cantidad.unwrap_or(1)) + paso).clamp(1, 64) as u8);
            })
        }
        SLOT_MATERIAL => match cursor {
            Some(c) => {
                let material = api::material(&c.get_registry_key());
                modificar(jugador, server, item, |d| d.material = material)
            }
            None => {
                decir_jugador(jugador, "gui.hold_item", &[]);
                false
            }
        },
        SLOT_SHAPED => {
            abrir_receta(jugador, item, "shaped", "SMELTING");
            false
        }
        SLOT_SHAPELESS => {
            abrir_receta(jugador, item, "shapeless", "SMELTING");
            false
        }
        SLOT_COCCION => {
            let estacion = if es_shift(clic) { "SMOKING" } else if derecho { "BLASTING" } else { "SMELTING" };
            abrir_receta(jugador, item, "cooking", estacion);
            false
        }
        SLOT_EXPORTAR => {
            exportar(jugador, item);
            false
        }
        SLOT_IRROMPIBLE => modificar(jugador, server, item, |d| d.irrompible = !d.irrompible),
        SLOT_BRILLO => modificar(jugador, server, item, |d| d.brillo = !d.brillo),
        SLOT_RAREZA => modificar(jugador, server, item, |d| {
            d.rareza = if derecho {
                None
            } else {
                let i = d.rareza.as_deref().and_then(|r| api::RAREZAS.iter().position(|x| *x == r)).map_or(0, |i| (i + 1) % 4);
                Some(api::RAREZAS[i].to_string())
            };
        }),
        SLOT_DETALLES => {
            if let Some(def) = registro::obtener(item) {
                for linea in comandos::resumen(de_jugador(jugador), &def) {
                    jugador.send_system_message(TextComponent::text(&linea), false);
                }
            }
            false
        }
        _ => false,
    };
    if cambio {
        abrir(jugador, item);
    }
}

// --- enchantments view ---

fn abrir_encantamientos(jugador: &Player, item: &str) {
    let i = de_jugador(jugador);
    let Some(def) = registro::obtener(item) else { return };
    let gui = Gui::new(ENCANTAMIENTOS, TextComponent::text(&t(i, "gui.enchants_title", &[("id", &api::corto(item))])));
    gui.set_allow_grab_items(false);
    gui.set_allow_put_items(false);
    for (slot, nombre) in api::nombres_encantamientos().into_iter().enumerate() {
        let nivel = def.encantamientos.get(&format!("minecraft:{nombre}")).copied().unwrap_or(0);
        let titulo = TextComponent::translate(&format!("enchantment.minecraft.{nombre}"), vec![]);
        let material = if nivel > 0 { "minecraft:enchanted_book" } else { "minecraft:book" };
        gui.set_item(slot as u32, boton(material, titulo, &[t(i, "gui.level", &[("level", &nivel)]), t(i, "gui.ench_help", &[])]));
    }
    gui.set_item(SLOT_VOLVER as u32, boton("minecraft:arrow", TextComponent::text(&t(i, "gui.back", &[])), &[]));
    mostrar(jugador, gui, Estado { item: item.to_string(), pantalla: ENCANTAMIENTOS, abierto: Instant::now(), receta: None });
}

fn clic_encantamientos(jugador: &Player, server: &Server, item: &str, slot: i16, clic: ClickType) {
    if slot == SLOT_VOLVER {
        abrir(jugador, item);
        return;
    }
    let Some(nombre) = api::nombres_encantamientos().get(slot as usize).copied() else { return };
    let clave_enc = format!("minecraft:{nombre}");
    let cambiado = modificar(jugador, server, item, |d| {
        let nivel = i64::from(d.encantamientos.get(&clave_enc).copied().unwrap_or(0));
        let nuevo = if es_shift(clic) {
            0
        } else if clic == ClickType::Right {
            nivel - 1
        } else {
            (nivel + 1).min(255)
        };
        if nuevo <= 0 {
            d.encantamientos.remove(&clave_enc);
        } else {
            d.encantamientos.insert(clave_enc, nuevo as u32);
        }
    });
    if cambiado {
        abrir_encantamientos(jugador, item);
    }
}

// --- recipe view ---

fn abrir_receta(jugador: &Player, item: &str, tipo: &'static str, estacion: &'static str) {
    let i = de_jugador(jugador);
    let titulo = t(i, "gui.recipe_title", &[("type", &t(i, &format!("gui.{tipo}"), &[])), ("id", &api::corto(item))]);
    let gui = Gui::new(RECETA, TextComponent::text(&titulo));
    gui.set_allow_grab_items(true);
    gui.set_allow_put_items(true);
    let inv = gui.get_inventory();
    mostrar(
        jugador,
        gui,
        Estado { item: item.to_string(), pantalla: RECETA, abierto: Instant::now(), receta: Some((tipo, estacion, inv)) },
    );
}

fn terminar_receta(jugador: &Player, server: &Server, item: &str, tipo: &str, estacion: &str, inv: &Inventory) {
    let stacks: Vec<Option<ItemStack>> = (0..9).map(|s| inv.get_item(s)).collect();
    inv.clear();
    let claves: Vec<Option<String>> =
        stacks.iter().map(|s| s.as_ref().map(|st| api::material(&st.get_registry_key()))).collect();
    let mut sobran = false;
    for stack in stacks.into_iter().flatten() {
        if let Err(stack) = inventario::dar(jugador, stack) {
            bloquear(&DEVOLVER).push((jugador.get_id(), stack));
            sobran = true;
        }
    }
    if sobran {
        decir_jugador(jugador, "gui.returned_later", &[]);
    }
    let usados: Vec<String> = claves.iter().flatten().cloned().collect();
    if usados.is_empty() {
        decir_jugador(jugador, "gui.recipe_empty", &[]);
        return;
    }
    let Some(def) = registro::obtener(item) else { return };
    let id = comandos::siguiente_receta(&def, item);
    use crate::modelo::Ingrediente::Uno;
    let receta = match tipo {
        "shaped" => {
            let mut letras: Vec<String> = vec![];
            for c in &usados {
                if !letras.contains(c) {
                    letras.push(c.clone());
                }
            }
            let letra = |c: &String| char::from(b'A' + letras.iter().position(|x| x == c).unwrap_or(0) as u8);
            let patron = (0..3)
                .map(|f| claves[f * 3..f * 3 + 3].iter().map(|c| c.as_ref().map_or(' ', letra)).collect())
                .collect();
            let claves = letras.iter().map(|c| (letra(c).to_string(), Uno(c.clone()))).collect();
            Receta::Shaped { id, patron, claves, categoria: None, grupo: None }
        }
        "shapeless" => Receta::Shapeless { id, ingredientes: usados.into_iter().map(Uno).collect(), categoria: None, grupo: None },
        _ => Receta::Cooking {
            id,
            entrada: Uno(usados[0].clone()),
            estacion: Some(estacion.to_string()),
            experiencia: None,
            tiempo: None,
            categoria: None,
            grupo: None,
        },
    };
    modificar(jugador, server, item, |d| d.recetas.push(receta));
}

// --- events ---

fn reabrir_luego(jugador: &Player, item: String) {
    let uid = jugador.get_id();
    schedule_delayed_task(1, move |server| {
        if let Some(j) = server.get_player_by_uuid(uid) {
            if registro::obtener(&item).is_some() {
                abrir(&j, &item);
            }
        }
    });
}

struct AlClic;

impl EventHandler<InventoryClickEvent> for AlClic {
    fn handle(&self, server: Server, mut evento: EventData<InventoryClickEvent>) -> EventData<InventoryClickEvent> {
        let datos = {
            let estados = bloquear(&ESTADOS);
            estados.get(&clave(&evento.player)).map(|e| (e.item.clone(), e.pantalla))
        };
        let Some((item, pantalla)) = datos else { return evento };
        if pantalla == RECETA || evento.window_type.is_some_and(|w| w != pantalla) {
            return evento;
        }
        let tamano = if pantalla == PRINCIPAL { 27 } else { 54 };
        if (0..tamano).contains(&evento.raw_slot) {
            evento.cancelled = true;
            if pantalla == PRINCIPAL {
                clic_principal(&evento.player, &server, &item, evento.raw_slot, evento.click_type, evento.cursor.as_ref());
            } else {
                clic_encantamientos(&evento.player, &server, &item, evento.raw_slot, evento.click_type);
            }
        } else if es_shift(evento.click_type) || evento.click_type == ClickType::DoubleClick {
            // Don't let items move from the player's inventory into the editor.
            evento.cancelled = true;
        }
        evento
    }
}

struct AlCerrar;

impl EventHandler<InventoryCloseEvent> for AlCerrar {
    fn handle(&self, server: Server, evento: EventData<InventoryCloseEvent>) -> EventData<InventoryCloseEvent> {
        let k = clave(&evento.player);
        let estado = {
            let mut estados = bloquear(&ESTADOS);
            let valido = estados.get(&k).is_some_and(|e| {
                evento.window_type.is_none_or(|w| w == e.pantalla) && e.abierto.elapsed() >= CIERRE_IGNORADO
            });
            if valido { estados.remove(&k) } else { None }
        };
        if let Some(Estado { item, receta: Some((tipo, estacion, inv)), .. }) = estado {
            terminar_receta(&evento.player, &server, &item, tipo, estacion, &inv);
            reabrir_luego(&evento.player, item);
        }
        evento
    }
}

struct AlChat;

impl EventHandler<PlayerChatEvent> for AlChat {
    fn handle(&self, server: Server, mut evento: EventData<PlayerChatEvent>) -> EventData<PlayerChatEvent> {
        let Some((item, campo)) = bloquear(&CHAT).remove(&clave(&evento.player)) else { return evento };
        evento.cancelled = true;
        let mensaje = evento.message.trim().replace('&', "§");
        let cancelar = crate::config().editor.cancel_words.iter().any(|w| w.eq_ignore_ascii_case(&mensaje));
        if mensaje.is_empty() || cancelar {
            decir_jugador(&evento.player, "gui.cancelled", &[]);
        } else if campo == "nombre" {
            modificar(&evento.player, &server, &item, |d| d.nombre = Some(Texto::Simple(mensaje)));
        } else {
            modificar(&evento.player, &server, &item, |d| {
                let mut lineas = api::lore(d);
                lineas.push(mensaje);
                d.lore = Some(Lore::Lineas(lineas));
            });
        }
        reabrir_luego(&evento.player, item);
        evento
    }
}

fn devolver_pendientes(server: Server) {
    let pendientes = std::mem::take(&mut *bloquear(&DEVOLVER));
    let mut quedan = vec![];
    for (uid, stack) in pendientes {
        match server.get_player_by_uuid(uid) {
            Some(j) => {
                if let Err(stack) = inventario::dar(&j, stack) {
                    quedan.push((uid, stack));
                }
            }
            None => quedan.push((uid, stack)),
        }
    }
    bloquear(&DEVOLVER).extend(quedan);
}

pub fn registrar(context: &Context) -> pumpkin_plugin_api::Result<()> {
    context.register_event_handler(AlClic, EventPriority::Normal, true)?;
    context.register_event_handler(AlCerrar, EventPriority::Normal, true)?;
    context.register_event_handler(AlChat, EventPriority::Normal, true)?;
    schedule_repeating_task(20, 20, devolver_pendientes);
    Ok(())
}
