//! Item abilities: what happens when a player right/left clicks with an item, hits an
//! entity with it or eats it. Items are recognised by their XeItems id tag. Several
//! abilities can share a trigger (`hit`, `hit:inferno`); items with progression only
//! run the ones their tier/level has unlocked, and make them stronger.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use pumpkin_plugin_api::events::{
    EntityDamageByEntityEvent, EventData, EventPriority, InteractAction, PlayerInteractEvent, PlayerItemConsumeEvent,
};
use pumpkin_plugin_api::player::StatusEffectInstance;
use pumpkin_plugin_api::server::CommandSender;
use pumpkin_plugin_api::text::TextComponent;
use pumpkin_plugin_api::{Context, DamageType, Entity, EventHandler, Player, Server};

use crate::modelo::{disparador_de, Accion};
use crate::{api, inventario, nombres, progresion, registro};

/// Player + item + ability -> when the cooldown ends.
static ESPERAS: Mutex<BTreeMap<String, Instant>> = Mutex::new(BTreeMap::new());

fn bloquear<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// What an action run needs besides the action itself.
struct Uso<'a> {
    objetivo: Option<&'a Entity>,
    /// Damage of the hit (hit trigger), for lifesteal.
    danio: f32,
    /// Progression multiplier (1 without progression).
    poder: f64,
    tier: u8,
}

/// Runs the held item's unlocked abilities for `disparador`. Returns true if any ran.
fn usar(server: &Server, jugador: &Player, disparador: &str, objetivo: Option<&Entity>, danio: f32) -> bool {
    let Some((slot, stack)) = inventario::en_mano(jugador) else { return false };
    let Some(id) = api::id_de_stack(&stack) else { return false };
    let Some(def) = registro::obtener(&id) else { return false };
    let (estado, rareza) = progresion::estado_rareza(&stack);
    let uid = jugador.get_id();
    let ahora = Instant::now();
    let (mut usada, mut consumir) = (false, false);
    let mut espera: Option<u32> = None;
    for (clave_h, h) in def.habilidades.iter().filter(|(k, _)| disparador_de(k) == disparador) {
        if !progresion::desbloqueada(h, estado) {
            continue;
        }
        let clave = format!("{}:{}:{id}:{clave_h}", uid.high, uid.low);
        if bloquear(&ESPERAS).get(&clave).is_some_and(|fin| *fin > ahora) {
            continue;
        }
        let ticks = h.cooldown.map_or(0, |t| progresion::cooldown(&def, estado, t));
        if ticks > 0 {
            bloquear(&ESPERAS).insert(clave, ahora + Duration::from_millis(u64::from(ticks) * 50));
        }
        // The client-side overlay shows the shortest one, so the rest aren't blocked.
        espera = Some(espera.map_or(ticks, |e| e.min(ticks)));
        let uso = Uso { objetivo, danio, poder: progresion::poder(&def, h, estado, &rareza), tier: estado.tier };
        for accion in &h.acciones {
            ejecutar(server, jugador, accion, &uso);
        }
        usada = true;
        consumir |= h.consumir;
    }
    if let Some(ticks) = espera.filter(|t| *t > 0) {
        jugador.set_item_cooldown(&stack.get_registry_key(), ticks.min(i32::MAX as u32) as i32);
    }
    if consumir {
        let resto = stack.get_count().saturating_sub(1);
        if resto == 0 {
            inventario::poner(jugador, slot, None);
        } else {
            stack.set_count(resto);
            inventario::poner(jugador, slot, Some(stack));
        }
    } else if usada && disparador != "hit" {
        // Hits give XP whether an ability ran or not (see AlGolpear).
        if let Some(p) = &def.progresion {
            progresion::ganar(jugador, slot, &stack, &def, progresion::xp_de(p, disparador));
        }
    }
    usada
}

fn al_objetivo(o: Option<&String>) -> bool {
    o.is_some_and(|o| o == "target")
}

fn consola(server: &Server, comando: &str) {
    server.execute_command(comando.trim_start_matches('/'), CommandSender::Console);
}

fn escalar(valor: f32, poder: f64) -> f32 {
    (f64::from(valor) * poder) as f32
}

fn escalar_segundos(segundos: u32, poder: f64) -> u32 {
    (f64::from(segundos) * poder).round().clamp(0.0, 1_000_000.0) as u32
}

/// Damages (and burns / pushes) every living entity near the player but the player.
fn area(jugador: &Player, radio: f64, danio: f32, fuego: Option<u32>, empuje: Option<f64>) {
    let (x, y, z) = jugador.get_position();
    let yo = jugador.get_id();
    let r = radio.clamp(0.5, 32.0);
    for e in jugador.as_entity().get_nearby_entities(r, r, r) {
        let uid = e.get_uuid();
        if uid.high == yo.high && uid.low == yo.low {
            continue;
        }
        let (ex, ey, ez) = e.get_position();
        let (dx, dz) = (ex - x, ez - z);
        let distancia = (dx * dx + (ey - y) * (ey - y) + dz * dz).sqrt();
        if distancia > r {
            continue;
        }
        let Some(vivo) = e.as_living() else { continue };
        if danio > 0.0 {
            vivo.damage(danio, DamageType::PlayerAttack);
        }
        if let Some(s) = fuego.filter(|s| *s > 0) {
            e.set_fire_ticks(s.saturating_mul(20).min(i32::MAX as u32) as i32);
        }
        if let Some(f) = empuje.filter(|f| *f != 0.0) {
            let plano = (dx * dx + dz * dz).sqrt().max(0.01);
            e.set_velocity((dx / plano * f, 0.35 + f.abs() * 0.1, dz / plano * f));
        }
    }
}

fn ejecutar(server: &Server, jugador: &Player, accion: &Accion, uso: &Uso) {
    let (x, y, z) = jugador.get_position();
    let mundo = jugador.get_world();
    let objetivo = uso.objetivo;
    match accion {
        Accion::Efecto { efecto, nivel, segundos, objetivo: o, subir_cada } => {
            let Some(tipo) = nombres::efecto(efecto) else { return };
            let segundos = Some(escalar_segundos(segundos.unwrap_or(10), uso.poder));
            let ticks = segundos.unwrap_or(10).saturating_mul(20);
            let extra = subir_cada.filter(|n| *n > 0).map_or(0, |n| (uso.tier - 1) / n);
            let amplificador = (nivel.unwrap_or(1).max(1) - 1).saturating_add(extra);
            let instancia = StatusEffectInstance {
                effect_type: tipo,
                duration: ticks,
                amplifier: amplificador,
                ambient: false,
                show_particles: true,
                show_icon: true,
            };
            if !al_objetivo(o.as_ref()) {
                jugador.add_effect(instancia);
            } else if let Some(e) = objetivo {
                match server.get_player_by_uuid(e.get_uuid()) {
                    Some(p) => p.add_effect(instancia),
                    // Mobs have no effect API: use the vanilla command.
                    None => consola(
                        server,
                        &format!(
                            "effect give {} {} {} {amplificador}",
                            pumpkin_plugin_api::uuid::to_string(e.get_uuid()),
                            api::material(efecto),
                            segundos.unwrap_or(10)
                        ),
                    ),
                }
            }
        }
        Accion::Comando { comando } => {
            let objetivo_uuid = objetivo.map(|e| pumpkin_plugin_api::uuid::to_string(e.get_uuid())).unwrap_or_default();
            let texto = comando
                .replace("{player}", &jugador.get_name())
                .replace("{uuid}", &pumpkin_plugin_api::uuid::to_string(jugador.get_id()))
                .replace("{target}", &objetivo_uuid)
                .replace("{x}", &format!("{x:.2}"))
                .replace("{y}", &format!("{y:.2}"))
                .replace("{z}", &format!("{z:.2}"));
            consola(server, &texto);
        }
        Accion::Sonido { sonido, volumen, tono } => {
            if nombres::sonido(sonido).is_some() {
                nombres::reproducir(sonido, "@a", (x, y, z), volumen.unwrap_or(1.0), tono.unwrap_or(1.0));
            }
        }
        Accion::Particula { particula, cantidad } => {
            if let Some(p) = nombres::particula(particula) {
                let n = cantidad.unwrap_or(20).min(i32::MAX as u32) as i32;
                mundo.spawn_particle(p, (x, y + 1.0, z), (0.4, 0.6, 0.4), 0.05, n);
            }
        }
        Accion::Curar { cantidad } => jugador.heal(escalar(*cantidad, uso.poder)),
        Accion::Danio { cantidad } => {
            if let Some(l) = objetivo.and_then(Entity::as_living) {
                l.damage(escalar(*cantidad, uso.poder), DamageType::PlayerAttack);
            }
        }
        Accion::Fuego { segundos } => {
            if let Some(e) = objetivo {
                e.set_fire_ticks(escalar_segundos(*segundos, uso.poder).saturating_mul(20).min(i32::MAX as u32) as i32);
            }
        }
        Accion::Area { radio, cantidad, fuego, empuje } => area(
            jugador,
            *radio,
            escalar(cantidad.unwrap_or(0.0), uso.poder),
            fuego.map(|s| escalar_segundos(s, uso.poder)),
            *empuje,
        ),
        Accion::Escudo { cantidad } => {
            if let Some(vivo) = jugador.as_entity().as_living() {
                let nuevo = escalar(*cantidad, uso.poder);
                if nuevo > vivo.get_absorption() {
                    vivo.set_absorption(nuevo);
                }
            }
        }
        Accion::Robovida { porcentaje } => {
            let curar = uso.danio * escalar(*porcentaje, uso.poder);
            if curar > 0.0 {
                jugador.heal(curar);
            }
        }
        Accion::Mensaje { texto } => {
            jugador.send_system_message(TextComponent::text(&texto.replace('&', "§")), false);
        }
        Accion::Impulso { fuerza } => {
            let (yaw, pitch) = (f64::from(jugador.get_yaw()).to_radians(), f64::from(jugador.get_pitch()).to_radians());
            let direccion = (-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos());
            jugador.set_velocity((direccion.0 * fuerza, direccion.1 * fuerza + 0.1, direccion.2 * fuerza));
        }
    }
}

// --- events ---

struct AlInteractuar;

impl EventHandler<PlayerInteractEvent> for AlInteractuar {
    fn handle(&self, server: Server, mut evento: EventData<PlayerInteractEvent>) -> EventData<PlayerInteractEvent> {
        let derecho = matches!(evento.action, InteractAction::RightClickAir | InteractAction::RightClickBlock);
        let agachado = evento.player.as_entity().is_sneaking();
        if derecho && agachado && progresion::ascender(&evento.player) {
            evento.cancelled = true;
            return evento;
        }
        let usada = if !derecho {
            usar(&server, &evento.player, "left_click", None, 0.0)
        } else {
            // Sneaking runs the sneak abilities; if none ran, the normal ones.
            (agachado && registro::hay_habilidad("sneak_right_click") && usar(&server, &evento.player, "sneak_right_click", None, 0.0))
                || usar(&server, &evento.player, "right_click", None, 0.0)
        };
        // An item with a right-click ability is not placed/used as well.
        if usada && derecho {
            evento.cancelled = true;
        }
        evento
    }
}

struct AlGolpear;

impl EventHandler<EntityDamageByEntityEvent> for AlGolpear {
    fn handle(&self, server: Server, evento: EventData<EntityDamageByEntityEvent>) -> EventData<EntityDamageByEntityEvent> {
        if evento.cancelled || !(registro::hay_habilidad("hit") || registro::hay_progresion()) {
            return evento;
        }
        // The event only gives entity ids: the attacker is the player looking at the victim.
        for jugador in server.get_all_players() {
            let Some(victima) = jugador.get_target_entity(6.0) else { continue };
            if victima.get_id() as i32 == evento.entity_id {
                usar(&server, &jugador, "hit", Some(&victima), evento.damage);
                progresion::al_golpear(&jugador, &victima);
                break;
            }
        }
        evento
    }
}

struct AlComer;

impl EventHandler<PlayerItemConsumeEvent> for AlComer {
    fn handle(&self, server: Server, evento: EventData<PlayerItemConsumeEvent>) -> EventData<PlayerItemConsumeEvent> {
        if !evento.cancelled {
            usar(&server, &evento.player, "eat", None, 0.0);
        }
        evento
    }
}

fn limpiar(_server: Server) {
    let ahora = Instant::now();
    bloquear(&ESPERAS).retain(|_, fin| *fin > ahora);
}

pub fn registrar(context: &Context) -> pumpkin_plugin_api::Result<()> {
    context.register_event_handler(AlInteractuar, EventPriority::Normal, true)?;
    context.register_event_handler(AlGolpear, EventPriority::Normal, true)?;
    context.register_event_handler(AlComer, EventPriority::Normal, true)?;
    pumpkin_plugin_api::scheduler::schedule_repeating_task(1200, 1200, limpiar);
    Ok(())
}
