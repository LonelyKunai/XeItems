//! The player's main inventory: 36 slots (hotbar 0-8 + storage 9-35), through the
//! inventory handle (Player::get/set_inventory_item use an undocumented numbering).
//!
//! Every write goes through [`poner`]: Pumpkin's plugin host tells the client about a
//! changed slot using the inventory index as the window slot, but in the player's window
//! slots 0-8 are the crafting grid, its result and the armour (the hotbar is 36-44).
//! The server's own sync then sends the hotbar slot correctly, and the client is left
//! with a ghost copy in the crafting grid or an armour slot: it looks like the item was
//! duplicated (and in creative mode it can really be taken). Armour ghosts are
//! overwritten right away; crafting-grid ghosts can't be reached from a plugin (they go
//! away on the player's next inventory click), so hotbar slots 0-4 are written as
//! rarely as possible.

use pumpkin_plugin_api::events::{EventData, EventPriority, PlayerJoinEvent};
use pumpkin_plugin_api::scheduler::schedule_delayed_task;
use pumpkin_plugin_api::{Context, EventHandler, Inventory, ItemStack, Player, Server};

use crate::{api, progresion};

pub const SLOTS: std::ops::Range<u32> = 0..36;

/// Free slots are filled in this order: storage, then hotbar slots whose ghost can be
/// fixed (5-8), then the rest of the hotbar.
const ORDEN_LIBRE: [std::ops::Range<u32>; 3] = [9..36, 5..9, 0..5];

pub fn de(jugador: &Player) -> Inventory {
    jugador.get_inventory().as_inventory()
}

/// The held item (main hand) and its hotbar slot.
pub fn en_mano(jugador: &Player) -> Option<(u32, ItemStack)> {
    let slot = u32::from(jugador.get_inventory().get_selected_slot());
    de(jugador).get_item(slot).map(|s| (slot, s))
}

pub fn hueco_libre(inv: &Inventory) -> Option<u32> {
    ORDEN_LIBRE.iter().flat_map(|r| r.clone()).find(|&s| inv.get_item(s).is_none())
}

/// Sets a main-inventory slot (0-35) and fixes the client's ghost copy when it can.
pub fn poner(jugador: &Player, slot: u32, stack: Option<ItemStack>) {
    de(jugador).set_item(slot, stack);
    // The host's packet went to window slot `slot`: for hotbar 5-8 that's an armour
    // slot, so send the real armour piece there again.
    let p = jugador.get_inventory();
    match slot {
        5 => p.set_helmet(p.get_helmet()),
        6 => p.set_chestplate(p.get_chestplate()),
        7 => p.set_leggings(p.get_leggings()),
        8 => p.set_boots(p.get_boots()),
        _ => {}
    }
}

/// Puts `stack` in the first free slot; gives it back if the inventory is full.
pub fn dar(jugador: &Player, stack: ItemStack) -> Result<(), ItemStack> {
    match hueco_libre(&de(jugador)) {
        Some(slot) => {
            poner(jugador, slot, Some(stack));
            Ok(())
        }
        None => Err(stack),
    }
}

/// The rebuilt `viejo` if it is an XeItems item (only `solo`, when given) whose stack is
/// out of date, showing its copy's own progress and rarity.
fn nuevo(viejo: &ItemStack, solo: Option<&str>) -> Option<ItemStack> {
    if solo.is_some_and(|id| api::id_de_stack(viejo).as_deref() != Some(id)) {
        return None;
    }
    if progresion::al_dia(viejo) {
        return None;
    }
    progresion::redibujar(viejo)
}

/// Rebuilds the player's XeItems items (inventory, armour and off hand) that are out of
/// date with their definitions or records. Returns how many stacks were updated.
pub fn actualizar(jugador: &Player, solo: Option<&str>) -> u32 {
    let mut n = 0;
    let inv = de(jugador);
    for slot in SLOTS {
        if let Some(s) = inv.get_item(slot).and_then(|v| nuevo(&v, solo)) {
            poner(jugador, slot, Some(s));
            n += 1;
        }
    }
    let p = jugador.get_inventory();
    // Armour and off hand.
    for ranura in 0..5 {
        let viejo = match ranura {
            0 => p.get_helmet(),
            1 => p.get_chestplate(),
            2 => p.get_leggings(),
            3 => p.get_boots(),
            _ => p.get_off_hand(),
        };
        let Some(s) = viejo.and_then(|v| nuevo(&v, solo)) else { continue };
        match ranura {
            0 => p.set_helmet(Some(s)),
            1 => p.set_chestplate(Some(s)),
            2 => p.set_leggings(Some(s)),
            3 => p.set_boots(Some(s)),
            _ => p.set_off_hand(Some(s)),
        }
        n += 1;
    }
    n
}

struct AlEntrar;

impl EventHandler<PlayerJoinEvent> for AlEntrar {
    fn handle(&self, _server: Server, evento: EventData<PlayerJoinEvent>) -> EventData<PlayerJoinEvent> {
        // A second later, once the inventory has surely been loaded.
        let uid = evento.player.get_id();
        schedule_delayed_task(20, move |server| {
            if let Some(j) = server.get_player_by_uuid(uid) {
                actualizar(&j, None);
            }
        });
        evento
    }
}

pub fn registrar(context: &Context) -> pumpkin_plugin_api::Result<()> {
    if crate::config().items.update_on_join {
        context.register_event_handler(AlEntrar, EventPriority::Normal, true)?;
    }
    Ok(())
}
