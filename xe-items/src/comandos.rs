//! /xeitems (/xi) <subcommand> [args...]. Each subcommand is a literal node (the client
//! completes the names by itself) and needs XeItems:<subcommand>; help needs nothing.
//! The arguments after the subcommand are tab-completed by [`sugerir`].

use std::collections::BTreeMap;
use std::fmt::Display;

use pumpkin_plugin_api::command::{Command, CommandSender};
use pumpkin_plugin_api::text::TextComponent;
use pumpkin_plugin_api::{Context, Player, Server};
use xe_common::comandos::{literal_con_sugerencias, manejador, Funcion};
use xe_common::lang::{de_sender, t, ts, IDIOMAS, RESPALDO};
use xe_common::info;

use crate::modelo::{
    disparador_de, Accion, Atributo, Comida, Coste, Ingrediente, ItemDef, Lore, Progresion, Receta, Texto, DISPARADORES,
};
use crate::progresion::{self, Estado, NIVEL_MAX, TIER_MAX};
use crate::registro::{self, Origen};
use crate::{api, editor, instancias, inventario, nombres, rareza};

pub const PERMISO: &str = "XeItems:command";

pub const SUBCOMANDOS: [(&str, Funcion); 25] = [
    ("help", help), ("create", create), ("edit", edit), ("name", name), ("lore", lore),
    ("enchant", enchant), ("amount", amount), ("material", material), ("set", set),
    ("attribute", attribute), ("ability", ability), ("progression", progression), ("progress", progress),
    ("drop", drop_), ("recipe", recipe), ("give", give), ("list", list), ("info", info_), ("export", export),
    ("import", import), ("clone", clone), ("fromhand", fromhand), ("update", update), ("delete", delete), ("reload", reload),
];

const USOS: [(&str, &str); 25] = [
    ("help", "/xeitems help"),
    ("create", "/xeitems create <id> <material>"),
    ("edit", "/xeitems edit <id>"),
    ("name", "/xeitems name <id> [lang:<en_us|es_es>] <text...>"),
    ("lore", "/xeitems lore <id> add <text...> | remove <line> | clear"),
    ("enchant", "/xeitems enchant <id> <enchantment> <level>  (0 = remove)"),
    ("amount", "/xeitems amount <id> <amount>"),
    ("material", "/xeitems material <id> <material>"),
    ("set", "/xeitems set <id> unbreakable|glint <true|false>\n/xeitems set <id> durability|stack <number|none>\n/xeitems set <id> rarity <common|uncommon|rare|epic|none>\n/xeitems set <id> model <namespace:path|none>\n/xeitems set <id> modeldata <number|none>\n/xeitems set <id> food <nutrition> <saturation> [always] [seconds] | food none\n/xeitems set <id> craftmaterial <material|none>  (craft token: what its recipes give, swapped for the item; use one vanilla can't craft)"),
    ("attribute", "/xeitems attribute <id> <attribute> <amount> [add|multiply_base|multiply_total] [slot] [level:<per level>] [tier:<per tier>]  (0 = remove)\n/xeitems attribute <id> clear"),
    ("ability", "/xeitems ability <id> <trigger>[:name] <action> [args...]\n  triggers: right_click, sneak_right_click, left_click, hit, eat (several per trigger: hit, hit:inferno, ...)\n  actions: effect <effect> [level] [seconds] [self|target] [+1 level every N tiers], command <command...>,\n  sound <sound> [volume] [pitch], particle <particle> [count], heal <amount>, damage <amount>, fire <seconds>,\n  message <text...>, dash <strength>, area <radius> <damage> [fire seconds] [knockback], shield <amount>, lifesteal <fraction>\n/xeitems ability <id> <trigger> cooldown <ticks> | consume <true|false> | remove <number> | clear\n/xeitems ability <id> <trigger> require <tier> [level] | name <text...>\n/xeitems ability <id> <trigger> scale <per level|default> [per tier|default]  (0.05 = +5 % power; default = the item's power_level/power_tier)"),
    ("progression", "/xeitems progression <id> on | off\n/xeitems progression <id> <xp_base|xp_growth|xp_per_tier|power_level|power_tier|cooldown_tier|max_tier|max_level> <number>\n/xeitems progression <id> xp <hit|kill|right_click|sneak_right_click|left_click|eat> <amount>\n/xeitems progression <id> cost <item> <amount per tier> | cost none\n/xeitems progression <id> core <tier> <core item id> | core <tier> none\n/xeitems progression <id> rarity <common|uncommon|rare|epic|legendary> <weight> | rarity default"),
    ("progress", "/xeitems progress <player> [info|tier|level|addxp <value>|rarity <rarity>|reset]  (the copy in their hand)\n/xeitems progress #<creation id> ...  (any copy, e.g. #1a2b3c4d from its lore)"),
    ("drop", "/xeitems drop <id> <mob|*> <chance>  (0.02 = 2 %, 0 = remove)\n/xeitems drop <id> clear"),
    ("recipe", "/xeitems recipe <id> shaped <row1/row2/row3> <K=item,...>\n/xeitems recipe <id> shapeless <item,item,...>\n/xeitems recipe <id> cooking <item> [smelting|blasting|smoking|campfire] [ticks]\n/xeitems recipe <id> remove <recipe>"),
    ("give", "/xeitems give <id> [player] [amount] [rarity]"),
    ("list", "/xeitems list"),
    ("info", "/xeitems info <id>"),
    ("export", "/xeitems export <id>"),
    ("import", "/xeitems import <json>"),
    ("clone", "/xeitems clone <id> <new_id>"),
    ("fromhand", "/xeitems fromhand <id>"),
    ("update", "/xeitems update [id]"),
    ("delete", "/xeitems delete <id>"),
    ("reload", "/xeitems reload"),
];

pub fn permiso_de(sub: &str) -> String {
    format!("XeItems:{sub}")
}

pub fn registrar(context: &Context) {
    let mut comando = Command::new(&["xeitems".into(), "xi".into()], &ts("cmd.xeitems", &[])).execute(manejador(help, None));
    for (sub, funcion) in SUBCOMANDOS {
        // The permission is checked in `ejecutar`.
        comando = comando.then(literal_con_sugerencias(sub, "args", None, funcion, sugerir));
    }
    context.register_command(comando, PERMISO);
}

/// Everything a subcommand needs; messages come out in the sender's language.
struct Ctx<'a> {
    sender: &'a CommandSender,
    server: &'a Server,
    idioma: &'static str,
    /// Raw text after the subcommand (for JSON).
    texto: String,
    p: Vec<String>,
}

type Res = Result<String, String>;

impl Ctx<'_> {
    fn t(&self, clave: &str, valores: &[(&str, &dyn Display)]) -> String {
        t(self.idioma, clave, valores)
    }

    fn uso(&self, sub: &str) -> String {
        let uso = USOS.iter().find(|(s, _)| *s == sub).map_or("", |(_, u)| u);
        self.t("xi.usage", &[("usage", &uso)])
    }

    fn entero(&self, valor: &str) -> Result<i64, String> {
        valor.parse().map_err(|_| self.t("xi.number", &[("value", &valor)]))
    }

    fn decimal(&self, valor: &str) -> Result<f64, String> {
        valor.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| self.t("xi.number", &[("value", &valor)]))
    }

    fn booleano(&self, valor: &str) -> Result<bool, String> {
        match valor.to_lowercase().as_str() {
            "true" | "on" | "yes" | "si" | "1" => Ok(true),
            "false" | "off" | "no" | "0" => Ok(false),
            _ => Err(self.t("xi.bad_value", &[("value", &valor)])),
        }
    }

    /// `7` or `VII`.
    fn tier(&self, valor: &str) -> Result<u8, String> {
        (1..=TIER_MAX)
            .find(|n| progresion::romano(*n).eq_ignore_ascii_case(valor) || n.to_string() == valor)
            .ok_or_else(|| self.t("xi.bad_value", &[("value", &valor)]))
    }

    fn nivel(&self, valor: &str) -> Result<u8, String> {
        Ok(self.entero(valor)?.clamp(1, i64::from(NIVEL_MAX)) as u8)
    }

    /// The named player, or the sender.
    fn jugador(&self, nombre: Option<&String>) -> Result<Player, String> {
        match nombre {
            Some(n) => self.server.get_player_by_name(n).ok_or_else(|| self.t("xi.player_not_found", &[("player", n)])),
            None => self.sender.as_player().ok_or_else(|| self.t("xi.console_needs_player", &[])),
        }
    }

    fn definicion(&self, id: &str) -> Result<ItemDef, String> {
        registro::obtener(id).ok_or_else(|| self.t("xi.not_found", &[("id", &api::id(id))]))
    }

    fn guardar(&self, def: ItemDef) -> Res {
        registro::definir(self.server, def, Origen::Usuario).map_err(|e| self.t("xi.error", &[("error", &e)]))
    }

    /// Applies `cambio` to the item and saves it; returns the item id.
    fn modificar(&self, id: &str, cambio: impl FnOnce(&mut ItemDef) -> Result<(), String>) -> Res {
        let mut def = self.definicion(id)?;
        cambio(&mut def)?;
        self.guardar(def)
    }

    /// For a new item: fails if the id is taken.
    fn id_libre(&self, id: &str) -> Res {
        let id = api::id(id);
        if registro::obtener(&id).is_some() {
            return Err(self.t("xi.exists", &[("id", &id)]));
        }
        Ok(id)
    }
}

fn ejecutar(sender: &CommandSender, server: &Server, sub: &str, args: Option<String>, f: fn(&Ctx) -> Res) -> i32 {
    let texto = args.unwrap_or_default();
    let ctx = Ctx {
        sender,
        server,
        idioma: de_sender(sender),
        p: texto.split_whitespace().map(String::from).collect(),
        texto,
    };
    let resultado = if sub != "help" && !sender.has_permission(server, &permiso_de(sub)) {
        Err(ctx.t("common.no_permission",&[("permission", &permiso_de(sub))]))
    } else {
        f(&ctx)
    };
    let (mensaje, codigo) = match resultado {
        Ok(m) => (m, 1),
        Err(m) => (m, 0),
    };
    for linea in mensaje.lines().filter(|l| !l.is_empty()) {
        sender.send_message(TextComponent::text(linea));
    }
    codigo
}

/// `subcomando!(fn_name, body)`, or `subcomando!(fn_name, "sub", body)` when the
/// subcommand isn't a valid function name.
macro_rules! subcomando {
    ($nombre:ident, $sub:literal, $cuerpo:expr) => {
        fn $nombre(sender: &CommandSender, server: &Server, args: Option<String>) -> i32 {
            ejecutar(sender, server, $sub, args, $cuerpo)
        }
    };
    ($nombre:ident, $cuerpo:expr) => {
        fn $nombre(sender: &CommandSender, server: &Server, args: Option<String>) -> i32 {
            ejecutar(sender, server, stringify!($nombre), args, $cuerpo)
        }
    };
}

fn texto_libre(p: &[String]) -> String {
    p.join(" ").replace('&', "§")
}

/// "a,b|c" -> [a, [b, c]]; tags (#...) are kept.
fn ingredientes(lista: &str) -> Vec<Ingrediente> {
    lista
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|parte| {
            let opciones: Vec<String> =
                parte.split('|').map(|o| if o.starts_with('#') { o.to_string() } else { api::material(o) }).collect();
            if opciones.len() == 1 { Ingrediente::Uno(opciones[0].clone()) } else { Ingrediente::Varios(opciones) }
        })
        .collect()
}

pub fn siguiente_receta(def: &ItemDef, item_id: &str) -> String {
    let usados: Vec<&str> = def.recetas.iter().map(Receta::id).collect();
    let base = api::corto(item_id);
    let mut n = usados.len() + 1;
    while usados.contains(&format!("{base}_{n}").as_str()) {
        n += 1;
    }
    format!("{base}_{n}")
}

subcomando!(help, |c| {
    let mut m = c.t("xi.help", &[]);
    for (s, u) in USOS {
        if s != "help" {
            m.push('\n');
            m.push_str(u);
        }
    }
    Ok(m)
});

subcomando!(create, |c| {
    let [id, material] = c.p.as_slice() else { return Err(c.uso("create")) };
    let id = c.id_libre(id)?;
    let material = api::material(material);
    c.guardar(ItemDef { id: Some(id.clone()), material: material.clone(), ..Default::default() })?;
    Ok(c.t("xi.created", &[("id", &id), ("material", &material)]))
});

subcomando!(edit, |c| {
    let [id] = c.p.as_slice() else { return Err(c.uso("edit")) };
    c.definicion(id)?;
    let Some(jugador) = c.sender.as_player() else { return Err(c.t("common.only_players", &[])) };
    editor::abrir(&jugador, &api::id(id));
    Ok(String::new())
});

subcomando!(name, |c| {
    if c.p.len() < 2 {
        return Err(c.uso("name"));
    }
    let idioma = c.p[1].strip_prefix("lang:").map(str::to_lowercase);
    let resto = if idioma.is_some() { &c.p[2..] } else { &c.p[1..] };
    if resto.is_empty() {
        return Err(c.uso("name"));
    }
    if let Some(i) = idioma.as_deref().filter(|i| !IDIOMAS.contains(i)) {
        return Err(c.t("xi.bad_value", &[("value", &i)]));
    }
    let nombre = texto_libre(resto);
    let id = c.modificar(&c.p[0], |d| {
        d.nombre = Some(match idioma {
            None => Texto::Simple(nombre),
            Some(i) => {
                let mut mapa = match d.nombre.take() {
                    Some(Texto::PorIdioma(m)) => m,
                    // The old plain name stays as the fallback language.
                    Some(Texto::Simple(s)) => BTreeMap::from([(RESPALDO.to_string(), s)]),
                    None => BTreeMap::new(),
                };
                mapa.insert(i, nombre);
                Texto::PorIdioma(mapa)
            }
        });
        Ok(())
    })?;
    Ok(c.t("xi.name_set", &[("id", &id)]))
});

subcomando!(lore, |c| match c.p.get(1).map(String::as_str) {
    Some("add") if c.p.len() >= 3 => {
        let linea = texto_libre(&c.p[2..]);
        let id = c.modificar(&c.p[0], |d| {
            let mut lineas = api::lore(d);
            lineas.push(linea);
            d.lore = Some(Lore::Lineas(lineas));
            Ok(())
        })?;
        Ok(c.t("xi.lore_added", &[("id", &id)]))
    }
    Some("remove") if c.p.len() == 3 => {
        let n = c.entero(&c.p[2])?;
        let id = c.modificar(&c.p[0], |d| {
            let mut lineas = api::lore(d);
            if n < 1 || n as usize > lineas.len() {
                return Err(c.t("xi.no_such_line", &[("line", &n), ("count", &lineas.len())]));
            }
            lineas.remove(n as usize - 1);
            d.lore = if lineas.is_empty() { None } else { Some(Lore::Lineas(lineas)) };
            Ok(())
        })?;
        Ok(c.t("xi.lore_removed", &[("id", &id), ("line", &n)]))
    }
    Some("clear") if c.p.len() == 2 => {
        let id = c.modificar(&c.p[0], |d| {
            d.lore = None;
            Ok(())
        })?;
        Ok(c.t("xi.lore_cleared", &[("id", &id)]))
    }
    _ => Err(c.uso("lore")),
});

subcomando!(enchant, |c| {
    let [id, encantamiento, nivel] = c.p.as_slice() else { return Err(c.uso("enchant")) };
    let encantamiento = api::material(encantamiento);
    let nivel = c.entero(nivel)?;
    if !api::encantamiento_valido(&encantamiento) {
        return Err(c.t("xi.enchant_unknown", &[("enchant", &encantamiento)]));
    }
    let clave = encantamiento.clone();
    let id = c.modificar(id, move |d| {
        if nivel <= 0 {
            d.encantamientos.remove(&clave);
        } else {
            d.encantamientos.insert(clave, nivel.min(255) as u32);
        }
        Ok(())
    })?;
    if nivel <= 0 {
        Ok(c.t("xi.enchant_removed", &[("id", &id), ("enchant", &encantamiento)]))
    } else {
        Ok(c.t("xi.enchant_set", &[("id", &id), ("enchant", &encantamiento), ("level", &nivel)]))
    }
});

subcomando!(amount, |c| {
    let [id, cantidad] = c.p.as_slice() else { return Err(c.uso("amount")) };
    let cantidad = c.entero(cantidad)?.clamp(1, 64) as u8;
    let id = c.modificar(id, |d| {
        d.cantidad = Some(cantidad);
        Ok(())
    })?;
    Ok(c.t("xi.amount_set", &[("id", &id), ("amount", &cantidad)]))
});

subcomando!(material, |c| {
    let [id, material] = c.p.as_slice() else { return Err(c.uso("material")) };
    let material = api::material(material);
    let m = material.clone();
    let id = c.modificar(id, |d| {
        d.material = m;
        Ok(())
    })?;
    Ok(c.t("xi.material_set", &[("id", &id), ("material", &material)]))
});

const PROPIEDADES: [&str; 9] = ["unbreakable", "glint", "durability", "stack", "rarity", "model", "modeldata", "food", "craftmaterial"];

subcomando!(set, |c| {
    if c.p.len() < 3 {
        return Err(c.uso("set"));
    }
    let (propiedad, valor) = (c.p[1].to_lowercase(), c.p[2].as_str());
    let quitar = valor.eq_ignore_ascii_case("none");
    let mut def = c.definicion(&c.p[0])?;
    match propiedad.as_str() {
        "unbreakable" => def.irrompible = c.booleano(valor)?,
        "glint" => def.brillo = c.booleano(valor)?,
        "durability" => def.durabilidad = if quitar { None } else { Some(c.entero(valor)?.clamp(1, i64::from(i32::MAX)) as u32) },
        "stack" => def.max_stack = if quitar { None } else { Some(c.entero(valor)?.clamp(1, 99) as u8) },
        "rarity" => {
            let r = valor.to_lowercase();
            if !quitar && !api::RAREZAS.contains(&r.as_str()) {
                return Err(c.t("xi.bad_value", &[("value", &valor)]));
            }
            def.rareza = (!quitar).then_some(r);
        }
        "model" => def.modelo = (!quitar).then(|| api::material(valor)),
        "modeldata" => def.modelo_datos = if quitar { None } else { Some(c.decimal(valor)? as f32) },
        // Craft token: what the recipes give, swapped for the item after crafting.
        "craftmaterial" => def.material_receta = (!quitar).then(|| api::material(valor)),
        "food" if quitar => def.comida = None,
        "food" => {
            let saturacion = c.p.get(3).ok_or_else(|| c.uso("set"))?;
            let mut comida = Comida {
                nutricion: c.entero(valor)?.clamp(0, 1000) as u32,
                saturacion: c.decimal(saturacion)? as f32,
                siempre: false,
                segundos: None,
            };
            for extra in &c.p[4..] {
                if extra.eq_ignore_ascii_case("always") {
                    comida.siempre = true;
                } else {
                    comida.segundos = Some(c.decimal(extra)?.max(0.05) as f32);
                }
            }
            def.comida = Some(comida);
        }
        _ => return Err(c.uso("set")),
    }
    // Numbers and booleans must be the only argument (food takes more).
    if propiedad != "food" && c.p.len() != 3 {
        return Err(c.uso("set"));
    }
    let id = c.guardar(def)?;
    Ok(c.t("xi.property_set", &[("id", &id), ("property", &propiedad), ("value", &c.p[2..].join(" "))]))
});

subcomando!(attribute, |c| {
    match c.p.as_slice() {
        [id, clear] if clear == "clear" => {
            let id = c.modificar(id, |d| {
                d.atributos.clear();
                Ok(())
            })?;
            Ok(c.t("xi.attributes_cleared", &[("id", &id)]))
        }
        [id, atributo, cantidad, resto @ ..] if resto.len() <= 4 => {
            if nombres::atributo(atributo).is_none() {
                return Err(c.t("xi.unknown_name", &[("value", atributo)]));
            }
            let atributo = atributo.to_lowercase().trim_start_matches("minecraft:").replace("generic.", "");
            let cantidad = c.decimal(cantidad)?;
            let mut operacion = None;
            let mut ranura = None;
            let (mut por_nivel, mut por_tier) = (None, None);
            for r in resto {
                let r = r.to_lowercase();
                if let Some(v) = r.strip_prefix("level:") {
                    por_nivel = Some(c.decimal(v)?).filter(|v| *v != 0.0);
                } else if let Some(v) = r.strip_prefix("tier:") {
                    por_tier = Some(c.decimal(v)?).filter(|v| *v != 0.0);
                } else if ["add", "multiply_base", "multiply_total"].contains(&r.as_str()) {
                    operacion = (r != "add").then_some(r);
                } else if api::RANURAS.contains(&r.as_str()) {
                    ranura = (r != "mainhand").then_some(r);
                } else {
                    return Err(c.t("xi.bad_value", &[("value", &r)]));
                }
            }
            let nombre = atributo.clone();
            let id = c.modificar(id, |d| {
                // One modifier per attribute and slot: replace it (or remove it with 0).
                d.atributos.retain(|a| !(a.atributo == nombre && a.ranura == ranura));
                if cantidad != 0.0 {
                    d.atributos.push(Atributo { atributo: nombre, cantidad, operacion, ranura, por_nivel, por_tier });
                }
                Ok(())
            })?;
            Ok(c.t("xi.attribute_set", &[("id", &id), ("attribute", &atributo), ("amount", &cantidad)]))
        }
        _ => Err(c.uso("attribute")),
    }
});

/// Parses the action part of /xeitems ability.
fn accion(c: &Ctx, p: &[String]) -> Result<Accion, String> {
    let malo = || c.uso("ability");
    let tipo = p.first().ok_or_else(malo)?.to_lowercase();
    let arg = |i: usize| p.get(i).map(String::as_str);
    let numero = |i: usize| arg(i).map(|v| c.decimal(v)).transpose();
    let nombre = |valido: bool, v: &str| if valido { Ok(v.to_string()) } else { Err(c.t("xi.unknown_name", &[("value", &v)])) };
    Ok(match tipo.as_str() {
        "effect" if (2..=6).contains(&p.len()) => {
            let objetivo = match arg(4) {
                None | Some("self") => None,
                Some("target") => Some("target".to_string()),
                Some(o) => return Err(c.t("xi.bad_value", &[("value", &o)])),
            };
            Accion::Efecto {
                efecto: nombre(nombres::efecto(&p[1]).is_some(), &p[1].to_lowercase())?,
                nivel: numero(2)?.map(|n| n.clamp(1.0, 255.0) as u8),
                segundos: numero(3)?.map(|n| n.clamp(1.0, 1_000_000.0) as u32),
                objetivo,
                subir_cada: numero(5)?.map(|n| n.clamp(1.0, 15.0) as u8),
            }
        }
        "area" if (3..=5).contains(&p.len()) => Accion::Area {
            radio: numero(1)?.unwrap_or(1.0).clamp(0.5, 32.0),
            cantidad: numero(2)?.map(|n| n.max(0.0) as f32).filter(|n| *n > 0.0),
            fuego: numero(3)?.map(|n| n.clamp(0.0, 1_000_000.0) as u32).filter(|n| *n > 0),
            empuje: numero(4)?.map(|n| n.clamp(-10.0, 10.0)).filter(|n| *n != 0.0),
        },
        "shield" if p.len() == 2 => Accion::Escudo { cantidad: numero(1)?.unwrap_or(0.0).clamp(0.0, 2048.0) as f32 },
        "lifesteal" if p.len() == 2 => Accion::Robovida { porcentaje: numero(1)?.unwrap_or(0.0).clamp(0.0, 10.0) as f32 },
        "sound" if (2..=4).contains(&p.len()) => Accion::Sonido {
            sonido: nombre(nombres::sonido(&p[1]).is_some(), &p[1].to_lowercase())?,
            volumen: numero(2)?.map(|n| n.clamp(0.0, 10.0) as f32),
            tono: numero(3)?.map(|n| n.clamp(0.5, 2.0) as f32),
        },
        "particle" if (2..=3).contains(&p.len()) => Accion::Particula {
            particula: nombre(nombres::particula(&p[1]).is_some(), &p[1].to_lowercase())?,
            cantidad: numero(2)?.map(|n| n.clamp(1.0, 1000.0) as u32),
        },
        "command" if p.len() >= 2 => Accion::Comando { comando: p[1..].join(" ") },
        "message" if p.len() >= 2 => Accion::Mensaje { texto: p[1..].join(" ") },
        "heal" if p.len() == 2 => Accion::Curar { cantidad: numero(1)?.unwrap_or(0.0).max(0.0) as f32 },
        "damage" if p.len() == 2 => Accion::Danio { cantidad: numero(1)?.unwrap_or(0.0).max(0.0) as f32 },
        "fire" if p.len() == 2 => Accion::Fuego { segundos: numero(1)?.unwrap_or(0.0).clamp(0.0, 1_000_000.0) as u32 },
        "dash" if p.len() == 2 => Accion::Impulso { fuerza: numero(1)?.unwrap_or(0.0).clamp(-10.0, 10.0) },
        _ => return Err(malo()),
    })
}

const ACCIONES: [&str; 12] =
    ["effect", "command", "sound", "particle", "heal", "damage", "fire", "message", "dash", "area", "shield", "lifesteal"];

subcomando!(ability, |c| {
    if c.p.len() < 3 {
        return Err(c.uso("ability"));
    }
    // `hit` or `hit:name`.
    let disparador = c.p[1].to_lowercase();
    if !DISPARADORES.contains(&disparador_de(&disparador)) || disparador.ends_with(':') {
        return Err(c.t("xi.bad_value", &[("value", &c.p[1])]));
    }
    let resto = &c.p[2..];
    let d2 = disparador.clone();
    let (clave, valor): (&str, String) = match resto[0].to_lowercase().as_str() {
        "require" if (2..=3).contains(&resto.len()) => {
            let tier = c.tier(&resto[1])?;
            let nivel = match resto.get(2) {
                Some(n) => c.nivel(n)?,
                None => 1,
            };
            c.modificar(&c.p[0], |d| {
                let h = d.habilidades.entry(d2).or_default();
                h.tier = (tier > 1).then_some(tier);
                h.nivel = (nivel > 1).then_some(nivel);
                Ok(())
            })?;
            ("require", format!("Tier {} Lv {nivel}", progresion::romano(tier)))
        }
        "name" if resto.len() >= 2 => {
            let nombre = texto_libre(&resto[1..]);
            let n2 = nombre.clone();
            c.modificar(&c.p[0], |d| {
                d.habilidades.entry(d2).or_default().nombre = Some(Texto::Simple(n2));
                Ok(())
            })?;
            ("name", nombre)
        }
        "clear" if resto.len() == 1 => {
            let id = c.modificar(&c.p[0], |d| {
                d.habilidades.remove(&d2);
                Ok(())
            })?;
            return Ok(c.t("xi.ability_cleared", &[("id", &id), ("trigger", &disparador)]));
        }
        "cooldown" if resto.len() == 2 => {
            let ticks = c.entero(&resto[1])?.clamp(0, i64::from(i32::MAX)) as u32;
            c.modificar(&c.p[0], |d| {
                d.habilidades.entry(d2).or_default().cooldown = (ticks > 0).then_some(ticks);
                Ok(())
            })?;
            ("cooldown", ticks.to_string())
        }
        "scale" if (2..=3).contains(&resto.len()) => {
            // Per level / per tier power for this ability; "default" goes back to the item's.
            let valor = |v: &String| -> Result<Option<f64>, String> {
                if v.eq_ignore_ascii_case("default") { Ok(None) } else { Ok(Some(c.decimal(v)?.clamp(0.0, 10.0))) }
            };
            let nivel = valor(&resto[1])?;
            let tier = resto.get(2).map(valor).transpose()?;
            c.modificar(&c.p[0], |d| {
                let h = d.habilidades.entry(d2).or_default();
                h.poder_nivel = nivel;
                if let Some(t) = tier {
                    h.poder_tier = t;
                }
                Ok(())
            })?;
            let mostrar = |v: Option<f64>| v.map_or_else(|| "default".to_string(), |n| format!("+{}%", n * 100.0));
            let mut texto = format!("{}/lv", mostrar(nivel));
            if let Some(t) = tier {
                texto.push_str(&format!(", {}/tier", mostrar(t)));
            }
            ("scale", texto)
        }
        "consume" if resto.len() == 2 => {
            let si = c.booleano(&resto[1])?;
            c.modificar(&c.p[0], |d| {
                d.habilidades.entry(d2).or_default().consumir = si;
                Ok(())
            })?;
            ("consume", si.to_string())
        }
        "remove" if resto.len() == 2 => {
            let n = c.entero(&resto[1])?;
            c.modificar(&c.p[0], |d| {
                let acciones = d.habilidades.get_mut(&d2).map(|h| &mut h.acciones);
                match acciones {
                    Some(a) if n >= 1 && n as usize <= a.len() => {
                        a.remove(n as usize - 1);
                        Ok(())
                    }
                    a => Err(c.t("xi.no_such_line", &[("line", &n), ("count", &a.map_or(0, |a| a.len()))])),
                }
            })?;
            ("remove", n.to_string())
        }
        _ => {
            let nueva = accion(c, resto)?;
            let tipo = nueva.tipo();
            c.modificar(&c.p[0], |d| {
                d.habilidades.entry(d2).or_default().acciones.push(nueva);
                Ok(())
            })?;
            ("action", tipo.to_string())
        }
    };
    Ok(c.t("xi.ability_set", &[("id", &api::id(&c.p[0])), ("trigger", &disparador), ("what", &clave), ("value", &valor)]))
});

const AJUSTES: [&str; 8] =
    ["xp_base", "xp_growth", "xp_per_tier", "power_level", "power_tier", "cooldown_tier", "max_tier", "max_level"];

subcomando!(progression, |c| {
    if c.p.len() < 2 {
        return Err(c.uso("progression"));
    }
    let ajuste = c.p[1].to_lowercase();
    let resto = &c.p[2..];
    let mut def = c.definicion(&c.p[0])?;
    let valor = match (ajuste.as_str(), resto) {
        ("on", []) => {
            def.progresion.get_or_insert_with(Progresion::default);
            "on".to_string()
        }
        ("off", []) => {
            def.progresion = None;
            let id = c.guardar(def)?;
            return Ok(c.t("xi.progression_off", &[("id", &id)]));
        }
        (a, [v]) if AJUSTES.contains(&a) => {
            let p = def.progresion.get_or_insert_with(Progresion::default);
            let n = c.decimal(v)?;
            match a {
                "xp_base" => p.xp_base = n.clamp(1.0, 1_000_000.0) as u32,
                "xp_growth" => p.xp_crecimiento = n.clamp(1.0, 10.0),
                "xp_per_tier" => p.xp_por_tier = n.clamp(1.0, 10.0),
                "power_level" => p.poder_nivel = n.clamp(0.0, 10.0),
                "power_tier" => p.poder_tier = n.clamp(0.0, 10.0),
                "cooldown_tier" => p.cooldown_tier = n.clamp(0.0, 0.2),
                "max_level" => p.nivel_max = n.clamp(1.0, f64::from(NIVEL_MAX)) as u8,
                _ => p.tier_max = c.tier(v)?,
            }
            v.clone()
        }
        ("core", [tier, nucleo]) => {
            let tier = c.tier(tier)?;
            if tier < 2 {
                return Err(c.t("xi.bad_value", &[("value", &tier)]));
            }
            let p = def.progresion.get_or_insert_with(Progresion::default);
            if nucleo.eq_ignore_ascii_case("none") {
                p.nucleos.remove(&tier);
            } else {
                c.definicion(nucleo)?;
                p.nucleos.insert(tier, api::id(nucleo));
            }
            format!("Tier {} -> {nucleo}", progresion::romano(tier))
        }
        ("rarity", [d]) if d.eq_ignore_ascii_case("default") => {
            def.progresion.get_or_insert_with(Progresion::default).rarezas.clear();
            "default".to_string()
        }
        ("rarity", [r, peso]) => {
            let r = r.to_lowercase();
            if !rareza::valida(&r) {
                return Err(c.t("xi.bad_value", &[("value", &r)]));
            }
            let peso = c.entero(peso)?.clamp(0, 1_000_000) as u32;
            let p = def.progresion.get_or_insert_with(Progresion::default);
            // Starting from config.toml's weights, so the other rarities keep theirs.
            if p.rarezas.is_empty() {
                let w = &crate::config().rarity;
                p.rarezas = [("common", w.common), ("uncommon", w.uncommon), ("rare", w.rare), ("epic", w.epic), ("legendary", w.legendary)]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect();
            }
            p.rarezas.insert(r.clone(), peso);
            format!("{r} {peso}")
        }
        ("xp", [disparador, cantidad]) => {
            let disparador = disparador.to_lowercase();
            if disparador != "kill" && !DISPARADORES.contains(&disparador.as_str()) {
                return Err(c.t("xi.bad_value", &[("value", &disparador)]));
            }
            let cantidad = c.entero(cantidad)?.clamp(0, 1_000_000) as u32;
            def.progresion.get_or_insert_with(Progresion::default).xp.insert(disparador.clone(), cantidad);
            format!("{disparador} {cantidad}")
        }
        ("cost", [none]) if none.eq_ignore_ascii_case("none") => {
            def.progresion.get_or_insert_with(Progresion::default).coste_ascenso = None;
            "none".to_string()
        }
        ("cost", [item, cantidad]) => {
            let item = api::material(item);
            let cantidad = c.entero(cantidad)?.clamp(1, 64 * 36) as u32;
            def.progresion.get_or_insert_with(Progresion::default).coste_ascenso = Some(Coste { item: item.clone(), cantidad });
            format!("{cantidad}x {item} / tier")
        }
        _ => return Err(c.uso("progression")),
    };
    let id = c.guardar(def)?;
    Ok(c.t("xi.progression_set", &[("id", &id), ("setting", &ajuste), ("value", &valor)]))
});

subcomando!(progress, |c| {
    let Some(objetivo) = c.p.first() else { return Err(c.uso("progress")) };
    // `#1a2b3c4d`: a copy by (the start of) its creation id; otherwise a player's held copy.
    let (uid, quien) = match objetivo.strip_prefix('#') {
        Some(prefijo) => match instancias::buscar(prefijo).as_slice() {
            [uid] => (uid.clone(), None),
            [] => return Err(c.t("xi.prog.no_copy", &[("id", &prefijo)])),
            _ => return Err(c.t("xi.prog.ambiguous", &[("id", &prefijo)])),
        },
        None => {
            let jugador = c.jugador(Some(objetivo))?;
            let pila = inventario::en_mano(&jugador).map(|(_, s)| s);
            let uid = pila.as_ref().and_then(progresion::uid_de).filter(|u| instancias::obtener(u).is_some());
            let uid = uid.ok_or_else(|| c.t("xi.prog.not_held", &[("player", &jugador.get_name())]))?;
            (uid, Some(jugador))
        }
    };
    let describir = |uid: &str| -> String {
        let Some(i) = instancias::obtener(uid) else { return String::new() };
        let p = registro::obtener(&i.item).and_then(|d| d.progresion).unwrap_or_default();
        let e = i.estado();
        let necesita = progresion::necesita(&p, e).map_or("MAX".to_string(), |n| n.to_string());
        c.t(
            "xi.prog.state",
            &[
                ("uid", &uid.get(..8).unwrap_or(uid)),
                ("id", &api::corto(&i.item)),
                ("rarity", &rareza::etiqueta(&i.rareza)),
                ("tier", &progresion::tier_texto(e.tier)),
                ("level", &e.nivel),
                ("xp", &e.xp),
                ("need", &necesita),
                ("origin", &i.origen),
                ("creator", &i.creador.clone().unwrap_or_else(|| "-".into())),
            ],
        )
    };
    let resto: Vec<String> = c.p[1..].iter().map(|s| s.to_lowercase()).collect();
    let actual = instancias::obtener(&uid).map(|i| i.estado()).unwrap_or_default();
    let nuevo = match resto.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] | ["info"] => return Ok(describir(&uid)),
        ["reset"] => Estado::default(),
        ["tier", v] => Estado::nuevo(c.tier(v)?, actual.nivel, 0),
        ["level", v] => Estado::nuevo(actual.tier, c.nivel(v)?, 0),
        ["addxp", v] => {
            let xp = c.entero(v)?.clamp(0, i64::from(i32::MAX)) as u32;
            let def = instancias::obtener(&uid).and_then(|i| registro::obtener(&i.item));
            let p = def.and_then(|d| d.progresion).unwrap_or_default();
            progresion::sumar(&p, actual, xp).0
        }
        ["rarity", r] => {
            if !rareza::valida(r) {
                return Err(c.t("xi.bad_value", &[("value", r)]));
            }
            instancias::fijar_rareza(&uid, r);
            if let Some(j) = &quien {
                inventario::actualizar(j, None);
            }
            return Ok(describir(&uid));
        }
        _ => return Err(c.uso("progress")),
    };
    progresion::cambiar(&uid, quien.as_ref(), |_, _| nuevo).ok_or_else(|| c.uso("progress"))?;
    Ok(describir(&uid))
});

subcomando!(drop_, "drop", |c| {
    let id = match c.p.as_slice() {
        [id, clear] if clear == "clear" => c.modificar(id, |d| {
            d.drops.clear();
            Ok(())
        })?,
        [id, mob, chance] => {
            let mob = if mob == "*" { mob.clone() } else { api::material(mob) };
            if mob != "*" && nombres::entidad(&mob).is_none() {
                return Err(c.t("xi.unknown_name", &[("value", &mob)]));
            }
            let p = c.decimal(chance)?.clamp(0.0, 1.0);
            let m = mob.clone();
            c.modificar(id, move |d| {
                d.drops.retain(|x| x.mob != m);
                if p > 0.0 {
                    d.drops.push(crate::modelo::Drop { mob: m, probabilidad: p });
                }
                Ok(())
            })?
        }
        _ => return Err(c.uso("drop")),
    };
    let def = c.definicion(&id)?;
    let lista: Vec<String> = def.drops.iter().map(|d| format!("{} {}%", api::corto_material(&d.mob), d.probabilidad * 100.0)).collect();
    Ok(c.t("xi.drops_set", &[("id", &id), ("drops", &if lista.is_empty() { "-".into() } else { lista.join(", ") })]))
});

subcomando!(recipe, |c| {
    const ESTACIONES: [&str; 4] = ["smelting", "blasting", "smoking", "campfire"];
    let p = &c.p;
    if p.len() < 3 {
        return Err(c.uso("recipe"));
    }
    let id = api::id(&p[0]);
    let def = c.definicion(&id)?;
    if p[1] == "remove" && p.len() == 3 {
        let receta = api::id(&p[2]);
        let id = c.modificar(&id, |d| {
            let antes = d.recetas.len();
            d.recetas.retain(|r| api::id(r.id()) != receta);
            if d.recetas.len() == antes {
                return Err(c.t("xi.recipe_not_found", &[("recipe", &receta)]));
            }
            Ok(())
        })?;
        return Ok(c.t("xi.recipe_removed", &[("id", &id), ("recipe", &receta)]));
    }
    let receta_id = siguiente_receta(&def, &id);
    let receta = match p[1].as_str() {
        "shaped" if p.len() == 4 => {
            let mut claves = BTreeMap::new();
            for par in p[3].split(',').filter(|s| !s.is_empty()) {
                match par.split_once('=') {
                    Some((letra, valor)) if letra.chars().count() == 1 && !valor.is_empty() => {
                        if let Some(i) = ingredientes(valor).into_iter().next() {
                            claves.insert(letra.to_string(), i);
                        }
                    }
                    _ => return Err(c.t("xi.recipe_bad_key", &[("key", &par)])),
                }
            }
            Receta::Shaped {
                id: receta_id.clone(),
                patron: p[2].replace('_', " ").split('/').map(String::from).collect(),
                claves,
                categoria: None,
                grupo: None,
            }
        }
        "shapeless" if p.len() == 3 => Receta::Shapeless {
            id: receta_id.clone(),
            ingredientes: ingredientes(&p[2]),
            categoria: None,
            grupo: None,
        },
        "cooking" if p.len() <= 5 && p.get(3).is_none_or(|e| ESTACIONES.contains(&e.as_str())) => {
            let tiempo = match p.get(4) {
                Some(t) => Some(c.entero(t)?.max(1) as u32),
                None => None,
            };
            Receta::Cooking {
                id: receta_id.clone(),
                entrada: ingredientes(&p[2]).into_iter().next().ok_or_else(|| c.uso("recipe"))?,
                estacion: Some(p.get(3).map_or("SMELTING".into(), |e| e.to_uppercase())),
                experiencia: None,
                tiempo,
                categoria: None,
                grupo: None,
            }
        }
        _ => return Err(c.uso("recipe")),
    };
    let id = c.modificar(&id, |d| {
        d.recetas.push(receta);
        Ok(())
    })?;
    Ok(c.t("xi.recipe_added", &[("id", &id), ("recipe", &api::id(&receta_id))]))
});

subcomando!(give, |c| {
    if c.p.is_empty() || c.p.len() > 4 {
        return Err(c.uso("give"));
    }
    let id = api::id(&c.p[0]);
    let def = c.definicion(&id)?;
    let jugador = c.jugador(c.p.get(1))?;
    let total = match c.p.get(2) {
        Some(n) => c.entero(n)?.clamp(1, 64 * 36) as u32,
        None => u32::from(def.cantidad.unwrap_or(1)),
    };
    let rareza_ = match c.p.get(3).map(|r| r.to_lowercase()) {
        Some(r) if rareza::valida(&r) => Some(r),
        Some(r) => return Err(c.t("xi.bad_value", &[("value", &r)])),
        None => None,
    };
    // Split into stacks of the item's own maximum size.
    let por_stack = u32::from(api::stack(&def, Some(1)).get_max_count().max(1));
    let mut dados = 0;
    while dados < total {
        let n = (total - dados).min(por_stack) as u8;
        // Each copy of an item with progression is a new copy (creation id, rarity).
        let hay_sitio = inventario::hueco_libre(&inventario::de(&jugador)).is_some();
        let stack = if hay_sitio { progresion::crear(&def, Some(n), "give", Some(&jugador), rareza_.as_deref()) } else { api::stack(&def, Some(n)) };
        if hay_sitio && def.progresion.is_some() {
            progresion::anunciar_nueva(&jugador, &stack);
        }
        if !hay_sitio || inventario::dar(&jugador, stack).is_err() {
            if dados == 0 {
                return Err(c.t("xi.inventory_full", &[("player", &jugador.get_name())]));
            }
            return Ok(c.t("xi.given_partial", &[("id", &id), ("player", &jugador.get_name()), ("count", &dados), ("total", &total)]));
        }
        dados += u32::from(n);
    }
    Ok(c.t("xi.given", &[("id", &id), ("player", &jugador.get_name()), ("count", &total)]))
});

subcomando!(list, |c| {
    let ids = registro::ids();
    if ids.is_empty() {
        return Ok(c.t("xi.list_empty", &[]));
    }
    Ok(c.t("xi.list", &[("count", &ids.len()), ("items", &ids.join(", "))]))
});

fn describir_accion(a: &Accion) -> String {
    match a {
        Accion::Efecto { efecto, nivel, segundos, objetivo, subir_cada } => format!(
            "effect {efecto} {} {}s{}{}",
            nivel.unwrap_or(1),
            segundos.unwrap_or(10),
            if objetivo.is_some() { " (target)" } else { "" },
            subir_cada.map_or(String::new(), |n| format!(" +1 every {n} tiers"))
        ),
        Accion::Area { radio, cantidad, fuego, empuje } => format!(
            "area {radio} damage {}{}{}",
            cantidad.unwrap_or(0.0),
            fuego.map_or(String::new(), |s| format!(" fire {s}s")),
            empuje.map_or(String::new(), |f| format!(" knockback {f}"))
        ),
        Accion::Escudo { cantidad } => format!("shield {cantidad}"),
        Accion::Robovida { porcentaje } => format!("lifesteal {}%", porcentaje * 100.0),
        Accion::Comando { comando } => format!("command /{}", comando.trim_start_matches('/')),
        Accion::Sonido { sonido, .. } => format!("sound {sonido}"),
        Accion::Particula { particula, cantidad } => format!("particle {particula} x{}", cantidad.unwrap_or(20)),
        Accion::Curar { cantidad } => format!("heal {cantidad}"),
        Accion::Danio { cantidad } => format!("damage {cantidad}"),
        Accion::Fuego { segundos } => format!("fire {segundos}s"),
        Accion::Mensaje { texto } => format!("message {texto}"),
        Accion::Impulso { fuerza } => format!("dash {fuerza}"),
    }
}

/// Multi-line summary of an item (for /xeitems info and the editor).
pub fn resumen(idioma: &str, def: &ItemDef) -> Vec<String> {
    let tr = |k: &str| t(idioma, k, &[]);
    let mut l = vec![match &def.material_receta {
        Some(token) => format!("§6{}§r ({}, crafted as {})", registro::id_de(def), def.material, token),
        None => format!("§6{}§r ({})", registro::id_de(def), def.material),
    }];
    if let Some(n) = api::nombre(def) {
        l.push(format!("§7{}:§r {n}", tr("xi.info.name")));
    }
    for linea in api::lore(def) {
        l.push(format!("§7  |§r {linea}"));
    }
    if !def.encantamientos.is_empty() {
        let e: Vec<String> = def.encantamientos.iter().map(|(e, n)| format!("{} {n}", api::corto(e))).collect();
        l.push(format!("§7{}:§r {}", tr("xi.info.enchants"), e.join(", ")));
    }
    let mut props = vec![];
    if def.irrompible {
        props.push("unbreakable".to_string());
    }
    if def.brillo {
        props.push("glint".to_string());
    }
    if let Some(d) = def.durabilidad {
        props.push(format!("durability {d}"));
    }
    if let Some(s) = def.max_stack {
        props.push(format!("stack {s}"));
    }
    if let Some(r) = &def.rareza {
        props.push(format!("rarity {r}"));
    }
    if let Some(m) = &def.modelo {
        props.push(format!("model {m}"));
    }
    if let Some(m) = def.modelo_datos {
        props.push(format!("modeldata {m}"));
    }
    if let Some(f) = &def.comida {
        props.push(format!("food {} {}{}", f.nutricion, f.saturacion, if f.siempre { " always" } else { "" }));
    }
    if let Some(n) = def.cantidad.filter(|n| *n > 1) {
        props.push(format!("amount {n}"));
    }
    if !props.is_empty() {
        l.push(format!("§7{}:§r {}", tr("xi.info.properties"), props.join(", ")));
    }
    l.extend(detalles(idioma, def));
    for r in &def.recetas {
        l.push(format!("§7{}:§r {} ({})", tr("xi.info.recipe"), r.id(), r.tipo()));
    }
    l
}

/// Attribute and ability lines of [`resumen`].
pub fn detalles(idioma: &str, def: &ItemDef) -> Vec<String> {
    let tr = |k: &str| t(idioma, k, &[]);
    let mut l = vec![];
    if let Some(p) = &def.progresion {
        let xp: Vec<String> = p.xp.iter().map(|(k, v)| format!("{k} {v}")).collect();
        l.push(format!(
            "§7{}:§r max Tier {} Lv {}, xp {} x{} (x{}/tier), power +{}%/lv +{}%/tier, cooldown -{}%/tier",
            tr("xi.info.progression"),
            progresion::romano(progresion::tier_max(p)),
            progresion::nivel_max(p),
            p.xp_base,
            p.xp_crecimiento,
            p.xp_por_tier,
            p.poder_nivel * 100.0,
            p.poder_tier * 100.0,
            p.cooldown_tier * 100.0
        ));
        l.push(format!("§7  XP:§r {}", xp.join(", ")));
        if let Some(c) = &p.coste_ascenso {
            l.push(format!("§7  {}:§r {}x {} / tier", tr("xi.info.ascend_cost"), c.cantidad, c.item));
        }
        for (t, nucleo) in &p.nucleos {
            l.push(format!("§7  {} {}:§r {}", tr("xi.info.core"), progresion::romano(*t), api::corto(nucleo)));
        }
        if !p.rarezas.is_empty() {
            let r: Vec<String> = p.rarezas.iter().map(|(k, v)| format!("{k} {v}")).collect();
            l.push(format!("§7  {}:§r {}", tr("xi.info.rarity_weights"), r.join(", ")));
        }
    }
    if !def.drops.is_empty() {
        let d: Vec<String> = def.drops.iter().map(|d| format!("{} {}%", api::corto_material(&d.mob), d.probabilidad * 100.0)).collect();
        l.push(format!("§7{}:§r {}", tr("xi.info.drops"), d.join(", ")));
    }
    for a in &def.atributos {
        let op = a.operacion.as_deref().unwrap_or("add");
        let mut escala = String::new();
        if let Some(n) = a.por_nivel {
            escala.push_str(&format!(", +{n}/lv"));
        }
        if let Some(n) = a.por_tier {
            escala.push_str(&format!(", +{n}/tier"));
        }
        l.push(format!(
            "§7{}:§r {} {} ({op}, {}{escala})",
            tr("xi.info.attribute"),
            a.atributo,
            a.cantidad,
            a.ranura.as_deref().unwrap_or("mainhand")
        ));
    }
    for (disparador, h) in &def.habilidades {
        let mut extra = vec![];
        if let Some(n) = &h.nombre {
            extra.push(api::texto(n));
        }
        if h.tier.is_some() || h.nivel.is_some() {
            let (tier, nivel) = (h.tier.unwrap_or(1), h.nivel.unwrap_or(1));
            extra.push(format!("{} Tier {} Lv {nivel}", tr("xi.info.requires"), progresion::romano(tier)));
        }
        if let Some(cd) = h.cooldown {
            extra.push(format!("cooldown {cd}"));
        }
        if h.consumir {
            extra.push("consume".into());
        }
        if let Some(n) = h.poder_nivel {
            extra.push(format!("power +{}%/lv", n * 100.0));
        }
        if let Some(n) = h.poder_tier {
            extra.push(format!("power +{}%/tier", n * 100.0));
        }
        l.push(format!("§7{} {disparador}:§r {}", tr("xi.info.ability"), extra.join(", ")));
        for (i, a) in h.acciones.iter().enumerate() {
            l.push(format!("§7  {}.§r {}", i + 1, describir_accion(a)));
        }
    }
    l
}

subcomando!(info_, "info", |c| {
    let [id] = c.p.as_slice() else { return Err(c.uso("info")) };
    Ok(resumen(c.idioma, &c.definicion(id)?).join("\n"))
});

subcomando!(export, |c| {
    let [id] = c.p.as_slice() else { return Err(c.uso("export")) };
    let def = c.definicion(id)?;
    let json = serde_json::to_string(&def).unwrap_or_default();
    info(&format!("[XeItems] export {}: {json}", api::id(id)));
    // Clickable: copies the JSON.
    c.sender.send_message(
        TextComponent::text(&t(c.idioma, "gui.export_click", &[("id", &api::id(id))]))
            .click_copy_to_clipboard(&json)
            .hover_show_text(TextComponent::text(&json)),
    );
    Ok(String::new())
});

subcomando!(import, |c| {
    let json = c.texto.trim();
    if json.is_empty() {
        return Err(c.uso("import"));
    }
    let def: ItemDef = serde_json::from_str(json).map_err(|e| c.t("xi.error", &[("error", &e)]))?;
    let id = c.id_libre(&registro::id_de(&def))?;
    c.guardar(def)?;
    Ok(c.t("xi.imported", &[("id", &id)]))
});

subcomando!(clone, |c| {
    let [origen, nuevo] = c.p.as_slice() else { return Err(c.uso("clone")) };
    let mut def = c.definicion(origen)?;
    let nuevo = c.id_libre(nuevo)?;
    def.id = Some(nuevo.clone());
    // The copy would make the same recipes; it gets none.
    def.recetas.clear();
    c.guardar(def)?;
    Ok(c.t("xi.cloned", &[("id", &api::id(origen)), ("new", &nuevo)]))
});

subcomando!(fromhand, |c| {
    let [id] = c.p.as_slice() else { return Err(c.uso("fromhand")) };
    let Some(jugador) = c.sender.as_player() else { return Err(c.t("common.only_players", &[])) };
    let id = c.id_libre(id)?;
    let slot = u32::from(jugador.get_inventory().get_selected_slot());
    let Some(stack) = inventario::de(&jugador).get_item(slot) else { return Err(c.t("xi.hand_empty", &[])) };
    let def = api::desde_stack(&stack, &id);
    let material = def.material.clone();
    c.guardar(def)?;
    Ok(c.t("xi.created", &[("id", &id), ("material", &material)]))
});

subcomando!(update, |c| {
    let solo = match c.p.as_slice() {
        [] => None,
        [id] => {
            c.definicion(id)?;
            Some(api::id(id))
        }
        _ => return Err(c.uso("update")),
    };
    let jugadores = c.server.get_all_players();
    let stacks: u32 = jugadores.iter().map(|j| inventario::actualizar(j, solo.as_deref())).sum();
    Ok(c.t("xi.updated", &[("count", &stacks), ("players", &jugadores.len())]))
});

subcomando!(delete, |c| {
    let [id] = c.p.as_slice() else { return Err(c.uso("delete")) };
    if !registro::borrar(id) {
        return Err(c.t("xi.not_found", &[("id", &api::id(id))]));
    }
    Ok(c.t("xi.deleted", &[("id", &api::id(id))]))
});

subcomando!(reload, |c| Ok(c.t("xi.reloaded", &[("count", &registro::recargar(c.server))])));

// --- tab completion ---

fn cadenas<'a>(lista: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    lista.into_iter().map(String::from).collect()
}

fn ids_cortos() -> Vec<String> {
    registro::ids().iter().map(|i| api::corto(i)).collect()
}

fn romanos() -> Vec<String> {
    (1..=TIER_MAX).map(|n| progresion::romano(n).to_string()).collect()
}

fn sugerir(server: &Server, sub: &str, previos: &[&str], _actual: &str) -> Vec<String> {
    let jugadores = || server.get_all_players().iter().map(|p| p.get_name()).collect::<Vec<String>>();
    let n = previos.len();
    let arg = |i: usize| previos.get(i).map(|s| s.to_lowercase()).unwrap_or_default();
    match (sub, n) {
        ("help" | "list" | "reload" | "import", _) => vec![],
        ("create" | "fromhand", _) => vec![],
        ("clone", 1) => vec![],
        ("progress", 0) => jugadores().into_iter().chain(["#".to_string()]).collect(),
        ("progress", 1) => cadenas(["info", "tier", "level", "addxp", "rarity", "reset"]),
        ("progress", 2) => match arg(1).as_str() {
            "tier" => romanos(),
            "rarity" => cadenas(rareza::nombres()),
            _ => vec![],
        },
        ("progress", _) => vec![],
        (_, 0) => ids_cortos(),
        ("enchant", 1) => cadenas(api::nombres_encantamientos()),
        ("lore", 1) => cadenas(["add", "remove", "clear"]),
        ("lore", 2) if arg(1) == "remove" => {
            let lineas = registro::obtener(previos[0]).map_or(0, |d| api::lore(&d).len());
            (1..=lineas).map(|i| i.to_string()).collect()
        }
        ("name", 1) => IDIOMAS.iter().map(|i| format!("lang:{i}")).collect(),
        ("give", 1) => jugadores(),
        ("give", 3) => cadenas(rareza::nombres()),
        ("progression", 1) => cadenas(["on", "off", "xp", "cost", "core", "rarity"].into_iter().chain(AJUSTES)),
        ("progression", 2) => match arg(1).as_str() {
            "xp" => cadenas(DISPARADORES.into_iter().chain(["kill"])),
            "cost" => cadenas(["none", "diamond", "emerald", "netherite_ingot", "nether_star"]),
            "max_tier" | "core" => romanos(),
            "rarity" => cadenas(rareza::nombres().chain(["default"])),
            "max_level" => cadenas(["20", "25", "35"]),
            _ => vec![],
        },
        ("progression", 3) if arg(1) == "core" => ids_cortos().into_iter().chain(["none".to_string()]).collect(),
        ("drop", 1) => cadenas(nombres::claves(&nombres::ENTIDADES).chain(["*", "clear"])),
        ("drop", 2) => cadenas(["0.01", "0.05", "0"]),
        ("set", 1) => cadenas(PROPIEDADES),
        ("set", 2) => match arg(1).as_str() {
            "unbreakable" | "glint" => cadenas(["true", "false"]),
            "rarity" => cadenas(api::RAREZAS.into_iter().chain(["none"])),
            "durability" | "stack" | "model" | "modeldata" | "food" => cadenas(["none"]),
            "craftmaterial" => cadenas(["minecraft:nautilus_shell", "minecraft:heavy_core", "minecraft:phantom_membrane", "none"]),
            _ => vec![],
        },
        ("set", 4 | 5) if arg(1) == "food" => cadenas(["always"]),
        ("attribute", 1) => cadenas(nombres::claves(&nombres::ATRIBUTOS).chain(["clear"])),
        ("attribute", 3..=6) => {
            cadenas(["add", "multiply_base", "multiply_total", "level:", "tier:"].into_iter().chain(api::RANURAS))
        }
        ("ability", 1) => {
            // The item's own keys (hit:inferno, ...) first.
            let mut claves: Vec<String> = registro::obtener(previos[0]).map(|d| d.habilidades.into_keys().collect()).unwrap_or_default();
            let faltan: Vec<String> = DISPARADORES.iter().map(|d| d.to_string()).filter(|d| !claves.contains(d)).collect();
            claves.extend(faltan);
            claves
        }
        ("ability", 2) => cadenas(ACCIONES.into_iter().chain(["cooldown", "consume", "remove", "clear", "require", "name", "scale"])),
        ("ability", 3) => match arg(2).as_str() {
            "require" => romanos(),
            "scale" => cadenas(["0.05", "default"]),
            "effect" => cadenas(nombres::claves(&nombres::EFECTOS)),
            "sound" => cadenas(nombres::claves(&nombres::SONIDOS)),
            "particle" => cadenas(nombres::claves(&nombres::PARTICULAS)),
            "consume" => cadenas(["true", "false"]),
            "remove" => {
                let (id, disparador) = (previos[0], arg(1));
                let n = registro::obtener(id).and_then(|d| d.habilidades.get(&disparador).map(|h| h.acciones.len())).unwrap_or(0);
                (1..=n).map(|i| i.to_string()).collect()
            }
            _ => vec![],
        },
        ("ability", 4) if arg(2) == "scale" => cadenas(["0.2", "default"]),
        ("ability", 6) if arg(2) == "effect" => cadenas(["self", "target"]),
        ("ability", 7) if arg(2) == "effect" => cadenas(["3", "5"]),
        ("recipe", 1) => cadenas(["shaped", "shapeless", "cooking", "remove"]),
        ("recipe", 2) if arg(1) == "remove" => {
            registro::obtener(previos[0]).map(|d| d.recetas.iter().map(|r| api::corto(r.id())).collect()).unwrap_or_default()
        }
        ("recipe", 3) if arg(1) == "cooking" => cadenas(["smelting", "blasting", "smoking", "campfire"]),
        _ => vec![],
    }
}
