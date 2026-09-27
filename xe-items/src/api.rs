//! Turning item definitions into ItemStacks and registered recipes.

use pumpkin_plugin_api::data_components::DataComponent;
use pumpkin_plugin_api::recipe::{CookingRecipe, CookingType, RecipeCategory, ShapedRecipe, ShapelessRecipe, WitIngredient};
use pumpkin_plugin_api::text::TextComponent;
use pumpkin_plugin_api::{Enchantment, ItemStack, PersistentDataHolder, Server};
use xe_common::lang::{RESPALDO, servidor};

use crate::modelo::{Ingrediente, ItemDef, Lore, Receta, Texto};
use crate::instancias::Instancia;
use crate::progresion::{self, Estado};

/// `magic_wand` -> `xeitems:magic_wand` (namespace from config.toml).
pub fn id(nombre: &str) -> String {
    if nombre.contains(':') {
        nombre.to_string()
    } else {
        format!("{}:{nombre}", crate::config().namespace)
    }
}

pub fn corto(id: &str) -> String {
    id.strip_prefix(&format!("{}:", crate::config().namespace)).unwrap_or(id).to_string()
}

pub fn material(nombre: &str) -> String {
    if nombre.contains(':') { nombre.to_string() } else { format!("minecraft:{nombre}") }
}

/// `minecraft:diamond` -> `diamond`.
pub fn corto_material(nombre: &str) -> String {
    nombre.strip_prefix("minecraft:").unwrap_or(nombre).to_string()
}

// --- texts in the server language ---

pub fn texto(t: &Texto) -> String {
    match t {
        Texto::Simple(s) => s.clone(),
        Texto::PorIdioma(m) => {
            m.get(servidor()).or_else(|| m.get(RESPALDO)).or_else(|| m.values().next()).cloned().unwrap_or_default()
        }
    }
}

pub fn nombre(def: &ItemDef) -> Option<String> {
    def.nombre.as_ref().map(texto)
}

pub fn lore(def: &ItemDef) -> Vec<String> {
    match &def.lore {
        None => vec![],
        Some(Lore::Lineas(l)) => l.clone(),
        Some(Lore::PorIdioma(m)) => m
            .get(servidor())
            .or_else(|| m.get(RESPALDO))
            .or_else(|| m.values().next())
            .cloned()
            .unwrap_or_default(),
    }
}

// --- enchantments ---

const ENCANTAMIENTOS: [(&str, Enchantment); 43] = [
    ("aqua_affinity", Enchantment::AquaAffinity), ("bane_of_arthropods", Enchantment::BaneOfArthropods),
    ("binding_curse", Enchantment::BindingCurse), ("blast_protection", Enchantment::BlastProtection),
    ("breach", Enchantment::Breach), ("channeling", Enchantment::Channeling), ("density", Enchantment::Density),
    ("depth_strider", Enchantment::DepthStrider), ("efficiency", Enchantment::Efficiency),
    ("feather_falling", Enchantment::FeatherFalling), ("fire_aspect", Enchantment::FireAspect),
    ("fire_protection", Enchantment::FireProtection), ("flame", Enchantment::Flame), ("fortune", Enchantment::Fortune),
    ("frost_walker", Enchantment::FrostWalker), ("impaling", Enchantment::Impaling), ("infinity", Enchantment::Infinity),
    ("knockback", Enchantment::Knockback), ("looting", Enchantment::Looting), ("loyalty", Enchantment::Loyalty),
    ("luck_of_the_sea", Enchantment::LuckOfTheSea), ("lunge", Enchantment::Lunge), ("lure", Enchantment::Lure),
    ("mending", Enchantment::Mending), ("multishot", Enchantment::Multishot), ("piercing", Enchantment::Piercing),
    ("power", Enchantment::Power), ("projectile_protection", Enchantment::ProjectileProtection),
    ("protection", Enchantment::Protection), ("punch", Enchantment::Punch), ("quick_charge", Enchantment::QuickCharge),
    ("respiration", Enchantment::Respiration), ("riptide", Enchantment::Riptide), ("sharpness", Enchantment::Sharpness),
    ("silk_touch", Enchantment::SilkTouch), ("smite", Enchantment::Smite), ("soul_speed", Enchantment::SoulSpeed),
    ("sweeping_edge", Enchantment::SweepingEdge), ("swift_sneak", Enchantment::SwiftSneak), ("thorns", Enchantment::Thorns),
    ("unbreaking", Enchantment::Unbreaking), ("vanishing_curse", Enchantment::VanishingCurse),
    ("wind_burst", Enchantment::WindBurst),
];

/// Vanilla enchantment names (without namespace), for the editor.
pub fn nombres_encantamientos() -> Vec<&'static str> {
    ENCANTAMIENTOS.iter().map(|(n, _)| *n).collect()
}

fn vanilla(id: &str) -> Option<Enchantment> {
    let (espacio, nombre) = id.rsplit_once(':').unwrap_or(("minecraft", id));
    (espacio == "minecraft").then(|| ENCANTAMIENTOS.iter().find(|(n, _)| *n == nombre).map(|(_, e)| *e))?
}

/// Vanilla enchantments or any namespaced custom one (`myplugin:poison`).
pub fn encantamiento_valido(id: &str) -> bool {
    vanilla(id).is_some() || id.rsplit_once(':').is_some_and(|(e, _)| e != "minecraft")
}

// --- data components (Minecraft network encoding, as the host decodes them) ---

pub const RAREZAS: [&str; 4] = ["common", "uncommon", "rare", "epic"];

fn varint(buf: &mut Vec<u8>, valor: i32) {
    let mut v = valor as u32;
    loop {
        if v & !0x7F == 0 {
            buf.push(v as u8);
            return;
        }
        buf.push((v & 0x7F | 0x80) as u8);
        v >>= 7;
    }
}

fn cadena(buf: &mut Vec<u8>, s: &str) {
    varint(buf, s.len() as i32);
    buf.extend_from_slice(s.as_bytes());
}

fn flotante(buf: &mut Vec<u8>, f: f32) {
    buf.extend_from_slice(&f.to_be_bytes());
}

fn componente(item: &ItemStack, c: DataComponent, llenar: impl FnOnce(&mut Vec<u8>)) {
    let mut buf = vec![];
    llenar(&mut buf);
    item.set_component(c, &buf);
}

fn propiedades(item: &ItemStack, def: &ItemDef) {
    if def.irrompible {
        componente(item, DataComponent::Unbreakable, |_| {});
    }
    if let Some(max) = def.durabilidad {
        componente(item, DataComponent::MaxDamage, |b| varint(b, max.clamp(1, i32::MAX as u32) as i32));
        componente(item, DataComponent::Damage, |b| varint(b, 0));
    }
    // Items with progression don't stack: each one has its own tier and level.
    if let Some(max) = def.max_stack.or(def.progresion.as_ref().map(|_| 1)) {
        componente(item, DataComponent::MaxStackSize, |b| varint(b, i32::from(max.clamp(1, 99))));
    }
    if let Some(r) = def.rareza.as_deref().and_then(|r| RAREZAS.iter().position(|x| x.eq_ignore_ascii_case(r))) {
        componente(item, DataComponent::Rarity, |b| varint(b, r as i32));
    }
    // `brillo` isn't put on the item either: Pumpkin saves enchantment_glint_override as
    // an empty tag too, the player's file can't be read back and everything in it is
    // lost. Copies that already have it are rebuilt without it (see `tiene_brillo`).
    if let Some(m) = &def.modelo {
        componente(item, DataComponent::ItemModel, |b| cadena(b, &material(m)));
    }
    if let Some(f) = def.modelo_datos {
        // floats, flags, strings, colors
        componente(item, DataComponent::CustomModelData, |b| {
            varint(b, 1);
            flotante(b, f);
            varint(b, 0);
            varint(b, 0);
            varint(b, 0);
        });
    }
    if let Some(c) = &def.comida {
        componente(item, DataComponent::Food, |b| {
            varint(b, c.nutricion.min(i32::MAX as u32) as i32);
            flotante(b, c.saturacion);
            b.push(u8::from(c.siempre));
        });
        // Makes any material edible: seconds, animation (1 = eat), inline sound, particles, effects.
        componente(item, DataComponent::Consumable, |b| {
            flotante(b, c.segundos.unwrap_or(1.6).max(0.05));
            varint(b, 1);
            varint(b, 0);
            cadena(b, "entity.generic.eat");
            b.push(0);
            b.push(1);
            varint(b, 0);
        });
    }
    // Attributes are not put on the item: Pumpkin saves the attribute_modifiers
    // component as an empty tag and the player's whole file becomes unreadable. They
    // go on the player instead (see `atributos`).
}

pub const RANURAS: [&str; 9] = ["mainhand", "offhand", "hand", "head", "chest", "legs", "feet", "armor", "any"];

// --- ItemStacks ---

/// Custom data key that marks a stack as an XeItems item (value: the item id).
const CLAVE_ID: &str = "id";

/// A new stack. For items with progression it isn't a copy yet (no creation id): use
/// `progresion::crear` to make one for a player.
pub fn stack(def: &ItemDef, cantidad: Option<u8>) -> ItemStack {
    stack_de(def, cantidad, None)
}

/// A stack showing a copy's record (creation id and record), or tier I with no rarity.
fn stack_de(def: &ItemDef, cantidad: Option<u8>, copia: Option<(&str, &Instancia)>) -> ItemStack {
    let item = ItemStack::new(&material(&def.material), cantidad.or(def.cantidad).unwrap_or(1).max(1));
    if let Some(n) = nombre(def) {
        item.set_custom_name(Some(TextComponent::text(&n)));
    }
    let estado = copia.map_or_else(Estado::default, |(_, i)| i.estado());
    let rareza = copia.map(|(_, i)| i.rareza.as_str());
    let mut lineas = lore(def);
    lineas.extend(progresion::lore(def, estado, rareza));
    lineas.extend(crate::atributos::lore(def, estado, rareza.unwrap_or("common")));
    if let Some((uid, _)) = copia {
        // Short creation id, e.g. #1a2b3c4d (commands accept it).
        lineas.push(format!("§8#{}", uid.get(..8).unwrap_or(uid)));
    }
    if !lineas.is_empty() {
        item.set_lore(lineas.iter().map(|l| TextComponent::text(l)).collect());
    }
    for (encantamiento, nivel) in &def.encantamientos {
        match vanilla(encantamiento) {
            Some(e) => item.add_enchantment(e, *nivel),
            None => item.add_custom_enchantment(encantamiento, *nivel),
        }
    }
    propiedades(&item, def);
    if let Some(i) = &def.id {
        PersistentDataHolder::set_string(&item, &crate::config().namespace, CLAVE_ID, &id(i));
    }
    item
}

/// Whether the stack has the enchantment glint component, which makes Pumpkin write an
/// unreadable file for whoever or whatever holds it (see `propiedades`).
pub fn tiene_brillo(item: &ItemStack) -> bool {
    item.get_components().into_iter().any(|c| c.component == DataComponent::EnchantmentGlintOverride)
}

/// The XeItems id a stack was made from, if any.
pub fn id_de_stack(item: &ItemStack) -> Option<String> {
    PersistentDataHolder::get_string(item, &crate::config().namespace, CLAVE_ID)
}

/// A fresh stack of `viejo` from the current definition, keeping its count, damage and
/// the enchantments the player added (ones the definition doesn't set), showing the
/// copy's record when given (use `progresion::redibujar`, which finds it).
pub fn reconstruir(viejo: &ItemStack, copia: Option<(&str, &Instancia)>) -> Option<ItemStack> {
    let def = crate::registro::obtener(&id_de_stack(viejo)?)?;
    let nuevo = stack_de(&def, Some(viejo.get_count()), copia);
    // Custom durability, or vanilla wear while the material is the same (items with
    // progression are rebuilt on every hit and must not get repaired).
    if def.durabilidad.is_some() || material(&def.material) == material(&viejo.get_registry_key()) {
        if let Some(d) = viejo.get_components().into_iter().find(|c| c.component == DataComponent::Damage) {
            nuevo.set_component(DataComponent::Damage, &d.value);
        }
    }
    for e in viejo.get_enchantments() {
        let definido = ENCANTAMIENTOS
            .iter()
            .find(|(_, x)| *x == e.enchantment)
            .is_some_and(|(n, _)| def.encantamientos.contains_key(&format!("minecraft:{n}")));
        if !definido {
            nuevo.add_enchantment(e.enchantment, e.level);
        }
    }
    for e in viejo.get_custom_enchantments() {
        if !def.encantamientos.contains_key(&e.enchantment_id) {
            nuevo.add_custom_enchantment(&e.enchantment_id, e.level);
        }
    }
    Some(nuevo)
}

/// Definition built from a real stack (for /xeitems fromhand).
pub fn desde_stack(item: &ItemStack, id_item: &str) -> ItemDef {
    let mut def = ItemDef { id: Some(id(id_item)), material: material(&item.get_registry_key()), ..Default::default() };
    def.nombre = item.get_custom_name().map(|n| Texto::Simple(n.get_text()));
    let lore: Vec<String> = item.get_lore().iter().map(TextComponent::get_text).collect();
    if !lore.is_empty() {
        def.lore = Some(Lore::Lineas(lore));
    }
    for e in item.get_enchantments() {
        if let Some((n, _)) = ENCANTAMIENTOS.iter().find(|(_, x)| *x == e.enchantment) {
            def.encantamientos.insert(format!("minecraft:{n}"), e.level);
        }
    }
    for e in item.get_custom_enchantments() {
        def.encantamientos.insert(e.enchantment_id, e.level);
    }
    def
}

// --- recipes ---

const MADERAS: [&str; 11] = ["oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry", "pale_oak", "crimson", "warped"];
const ARBOLES: [&str; 9] = ["oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry", "pale_oak"];
const COLORES: [&str; 16] = [
    "white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray", "light_gray", "cyan", "purple", "blue",
    "brown", "green", "red", "black",
];

fn lista(prefijos: &[&str], sufijo: &str) -> Vec<String> {
    prefijos.iter().map(|p| format!("minecraft:{p}{sufijo}")).collect()
}

/// Tags that are sent as item lists: recipes with tag ingredients give no result on Pumpkin.
fn tag_conocido(tag: &str) -> Option<Vec<String>> {
    let con_bambu: Vec<&str> = MADERAS.iter().copied().chain(["bamboo"]).collect();
    let troncos = |maderas: &[&str]| -> Vec<String> {
        maderas
            .iter()
            .map(|m| match *m {
                "crimson" | "warped" => format!("minecraft:{m}_stem"),
                _ => format!("minecraft:{m}_log"),
            })
            .collect()
    };
    Some(match tag {
        "minecraft:planks" => lista(&con_bambu, "_planks"),
        "minecraft:logs" => troncos(&MADERAS),
        "minecraft:logs_that_burn" => troncos(&ARBOLES),
        "minecraft:wooden_slabs" => lista(&con_bambu, "_slab"),
        "minecraft:wooden_stairs" => lista(&con_bambu, "_stairs"),
        "minecraft:wooden_fences" => lista(&con_bambu, "_fence"),
        "minecraft:wooden_buttons" => lista(&con_bambu, "_button"),
        "minecraft:wooden_pressure_plates" => lista(&con_bambu, "_pressure_plate"),
        "minecraft:wooden_doors" => lista(&con_bambu, "_door"),
        "minecraft:wooden_trapdoors" => lista(&con_bambu, "_trapdoor"),
        "minecraft:leaves" => lista(&ARBOLES, "_leaves").into_iter().chain(lista(&["azalea", "flowering_azalea"], "_leaves")).collect(),
        "minecraft:wool" => lista(&COLORES, "_wool"),
        "minecraft:wool_carpets" => lista(&COLORES, "_carpet"),
        "minecraft:terracotta" => lista(&COLORES, "_terracotta").into_iter().chain(["minecraft:terracotta".into()]).collect(),
        "minecraft:coals" => lista(&["coal", "charcoal"], ""),
        "minecraft:stone_crafting_materials" | "minecraft:stone_tool_materials" => {
            lista(&["cobblestone", "blackstone", "cobbled_deepslate"], "")
        }
        "minecraft:sand" => lista(&["sand", "red_sand", "suspicious_sand"], ""),
        "minecraft:soul_fire_base_blocks" => lista(&["soul_sand", "soul_soil"], ""),
        _ => return None,
    })
}

fn ingrediente(i: &Ingrediente) -> WitIngredient {
    match i {
        Ingrediente::Varios(lista) => WitIngredient::OneOf(lista.iter().map(|s| material(s)).collect()),
        Ingrediente::Uno(s) => match s.strip_prefix('#') {
            Some(tag) => {
                let tag = material(tag);
                tag_conocido(&tag).map_or(WitIngredient::Tag(tag), WitIngredient::OneOf)
            }
            None => WitIngredient::Item(material(s)),
        },
    }
}

fn nombre_ingrediente(i: &Ingrediente) -> String {
    match i {
        Ingrediente::Uno(s) => corto_material(s.trim_start_matches('#')),
        Ingrediente::Varios(l) => l.iter().map(|s| corto_material(s)).collect::<Vec<_>>().join("/"),
    }
}

/// "4x netherite_ingot, 4x magma_cream, 1x iron_sword": what a recipe takes.
pub fn resumen_receta(receta: &Receta) -> String {
    let mut cuenta: Vec<(String, u32)> = vec![];
    let mut sumar = |nombre: String, n: u32| match cuenta.iter_mut().find(|(k, _)| *k == nombre) {
        Some((_, c)) => *c += n,
        None => cuenta.push((nombre, n)),
    };
    match receta {
        Receta::Shaped { patron, claves, .. } => {
            for (letra, ing) in claves {
                let n = patron.iter().map(|f| f.matches(letra.as_str()).count() as u32).sum();
                if n > 0 {
                    sumar(nombre_ingrediente(ing), n);
                }
            }
        }
        Receta::Shapeless { ingredientes, .. } => ingredientes.iter().for_each(|i| sumar(nombre_ingrediente(i), 1)),
        Receta::Cooking { entrada, .. } => sumar(nombre_ingrediente(entrada), 1),
    }
    cuenta.sort_by(|a, b| b.1.cmp(&a.1));
    cuenta.iter().map(|(k, n)| format!("{n}x {k}")).collect::<Vec<_>>().join(", ")
}

/// Removes empty rows and columns around the pattern, like vanilla Minecraft.
pub fn recortar(patron: &[String]) -> Vec<String> {
    let ancho = patron.iter().map(|f| f.chars().count()).max().unwrap_or(0);
    let filas: Vec<Vec<char>> = patron
        .iter()
        .map(|f| {
            let mut c: Vec<char> = f.chars().collect();
            c.resize(ancho, ' ');
            c
        })
        .collect();
    let usadas: Vec<usize> = (0..filas.len()).filter(|&i| filas[i].iter().any(|c| *c != ' ')).collect();
    let columnas: Vec<usize> = (0..ancho).filter(|&j| filas.iter().any(|f| f[j] != ' ')).collect();
    let (Some(&f0), Some(&f1), Some(&c0), Some(&c1)) = (usadas.first(), usadas.last(), columnas.first(), columnas.last()) else {
        return patron.to_vec();
    };
    filas[f0..=f1].iter().map(|f| f[c0..=c1].iter().collect()).collect()
}

fn categoria(c: Option<&String>) -> Option<RecipeCategory> {
    Some(match c?.to_uppercase().as_str() {
        "BUILDING" => RecipeCategory::Building,
        "REDSTONE" => RecipeCategory::Redstone,
        "EQUIPMENT" => RecipeCategory::Equipment,
        "FOOD" => RecipeCategory::Food,
        "BLOCKS" => RecipeCategory::Blocks,
        _ => RecipeCategory::Misc,
    })
}

fn estacion(e: Option<&String>) -> CookingType {
    match e.map(|s| s.to_uppercase()).as_deref() {
        Some("BLASTING") => CookingType::Blasting,
        Some("SMOKING") => CookingType::Smoking,
        Some("CAMPFIRE") => CookingType::Campfire,
        _ => CookingType::Smelting,
    }
}

/// What an item's recipes put in the crafting result: its craft token when it has one
/// (see `ItemDef::material_receta`), otherwise its material.
pub fn material_de_receta(def: &ItemDef) -> String {
    material(def.material_receta.as_deref().unwrap_or(&def.material))
}

/// Registers one recipe. Registering takes ownership of the result stack, so every
/// recipe gets its own new stack. With a craft token the result is a plain token,
/// swapped for the item after the craft (see `crafteo`).
pub fn registrar_receta(server: &Server, def: &ItemDef, receta: &Receta) {
    let manager = server.get_recipe_manager();
    let salida = match &def.material_receta {
        Some(_) => ItemStack::new(&material_de_receta(def), def.cantidad.unwrap_or(1).max(1)),
        None => stack(def, None),
    };
    let clave = id(receta.id());
    match receta {
        Receta::Shaped { patron, claves, categoria: c, grupo, .. } => manager.register_shaped(
            &clave,
            ShapedRecipe {
                pattern: recortar(patron),
                key: claves.iter().map(|(letra, i)| (letra.clone(), ingrediente(i))).collect(),
                output: salida,
                group: grupo.clone(),
                category: Some(categoria(c.as_ref()).unwrap_or(RecipeCategory::Misc)),
                show_notification: None,
            },
        ),
        Receta::Shapeless { ingredientes, categoria: c, grupo, .. } => manager.register_shapeless(
            &clave,
            ShapelessRecipe {
                ingredients: ingredientes.iter().map(ingrediente).collect(),
                output: salida,
                group: grupo.clone(),
                category: Some(categoria(c.as_ref()).unwrap_or(RecipeCategory::Misc)),
            },
        ),
        Receta::Cooking { entrada, estacion: e, experiencia, tiempo, categoria: c, grupo, .. } => manager.register_cooking(
            &clave,
            estacion(e.as_ref()),
            CookingRecipe {
                ingredient: ingrediente(entrada),
                output: salida,
                experience: experiencia.unwrap_or(0.1),
                cooking_time: tiempo.unwrap_or(200),
                group: grupo.clone(),
                category: Some(categoria(c.as_ref()).unwrap_or(RecipeCategory::Misc)),
            },
        ),
    }
}
