//! The item file format (same as the Python version): `{"items": [...]}` in a .json file
//! or `[[items]]` tables in a .toml file.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ItemDef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub material: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nombre: Option<Texto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lore: Option<Lore>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cantidad: Option<u8>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub encantamientos: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recetas: Vec<Receta>,
    /// Craft token: what the recipes give instead of `material` (e.g. "nautilus_shell"),
    /// swapped for the real item once crafted. Lets an item whose material vanilla or
    /// another item also crafts be recognised on unpatched Pumpkin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_receta: Option<String>,
    #[serde(default, skip_serializing_if = "no")]
    pub irrompible: bool,
    /// Maximum durability (the item takes damage like a tool).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durabilidad: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_stack: Option<u8>,
    /// common, uncommon, rare or epic (colour of the name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rareza: Option<String>,
    /// Enchantment glint without enchantments. Not put on stacks for now: Pumpkin can't
    /// save it (see `api::propiedades`).
    #[serde(default, skip_serializing_if = "no")]
    pub brillo: bool,
    /// Resource pack model (`item_model`, e.g. "xeitems:magic_wand").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modelo: Option<String>,
    /// `custom_model_data` float, for resource packs that select on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modelo_datos: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comida: Option<Comida>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub atributos: Vec<Atributo>,
    /// Trigger (right_click, sneak_right_click, left_click, hit, eat) -> what happens.
    /// Several abilities can share a trigger: `hit`, `hit:inferno`, ...
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub habilidades: BTreeMap<String, Habilidad>,
    /// Tier (I-XV) and level (1-35) progression, kept per copy of the item (each copy
    /// has a creation id, its record is in instances.json) with a rarity rolled when the
    /// copy is made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progresion: Option<Progresion>,
    /// Mobs that drop the item when a player kills them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drops: Vec<Drop>,
}

/// How an item levels up. Levels need XP; at level 35 the item ascends to the next tier
/// (back to level 1). Abilities get stronger with both.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Progresion {
    /// XP from level 1 to 2 (tier I).
    pub xp_base: u32,
    /// Each level needs this times the previous one (1.07 = +7 %).
    pub xp_crecimiento: f64,
    /// Each tier multiplies every level's XP by this.
    pub xp_por_tier: f64,
    /// XP earned per trigger (hit, kill, right_click, sneak_right_click, left_click, eat).
    pub xp: BTreeMap<String, u32>,
    /// Extra ability power per level above 1 (0.02 = +2 %).
    pub poder_nivel: f64,
    /// Extra ability power per tier above I.
    pub poder_tier: f64,
    /// Cooldown reduction per tier above I (0.03 = -3 %).
    pub cooldown_tier: f64,
    /// Highest tier this item can reach (1-15).
    pub tier_max: u8,
    /// None: ascends by itself at level 35. Some: sneak + right click at level 35 pays
    /// `cantidad` x current tier of `item` from the inventory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coste_ascenso: Option<Coste>,
    /// Highest level of each tier for this item (1-35).
    pub nivel_max: u8,
    /// Tier -> id of the XeItems item (a crafted core) that is the only way into it, e.g.
    /// 6 and 11: I-V, VI-X and XI-XV level up by XP, and the next bracket needs a core.
    /// At the top level of the tier before, XP stops until the player sneaks and right
    /// clicks with the item while carrying the core.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub nucleos: BTreeMap<u8, String>,
    /// Rarity -> weight when a copy is crafted, dropped or given. Empty: config.toml.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub rarezas: BTreeMap<String, u32>,
}

/// A mob that can drop the item when a player kills it.
#[derive(Clone, Serialize, Deserialize)]
pub struct Drop {
    /// `minecraft:blaze`, or `*` for any mob.
    pub mob: String,
    /// 0.02 = 2 %.
    pub probabilidad: f64,
}

impl Default for Progresion {
    fn default() -> Self {
        Self {
            xp_base: 30,
            xp_crecimiento: 1.08,
            xp_por_tier: 1.25,
            xp: [("hit", 2), ("kill", 10), ("right_click", 1), ("sneak_right_click", 2)]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            poder_nivel: 0.02,
            poder_tier: 0.15,
            cooldown_tier: 0.03,
            tier_max: 15,
            coste_ascenso: None,
            nivel_max: 35,
            nucleos: BTreeMap::new(),
            rarezas: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Coste {
    pub item: String,
    pub cantidad: u32,
}

fn no(b: &bool) -> bool {
    !*b
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Comida {
    pub nutricion: u32,
    pub saturacion: f32,
    /// Can be eaten with a full hunger bar.
    #[serde(default, skip_serializing_if = "no")]
    pub siempre: bool,
    /// Seconds it takes to eat (default 1.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segundos: Option<f32>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Atributo {
    /// attack_damage, movement_speed, max_health, ...
    pub atributo: String,
    pub cantidad: f64,
    /// add (default), multiply_base or multiply_total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operacion: Option<String>,
    /// mainhand (default), offhand, hand, head, chest, legs, feet, armor, any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ranura: Option<String>,
    /// Added per level above 1 (items with progression).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub por_nivel: Option<f64>,
    /// Added per tier above I (items with progression).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub por_tier: Option<f64>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Habilidad {
    /// Shown in the item's lore with its unlock requirement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nombre: Option<Texto>,
    /// Tier needed to use it (1 = I).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<u8>,
    /// Level needed within that tier (or any higher tier).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nivel: Option<u8>,
    /// Ticks before it can be used again (20 = 1 second).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown: Option<u32>,
    /// Uses up one item each time.
    #[serde(default, skip_serializing_if = "no")]
    pub consumir: bool,
    /// Extra power per level above 1 for this ability only (replaces the item's
    /// `poder_nivel`; 0.05 = +5 %).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poder_nivel: Option<f64>,
    /// Extra power per tier above I for this ability only (replaces `poder_tier`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poder_tier: Option<f64>,
    #[serde(default)]
    pub acciones: Vec<Accion>,
}

/// One thing an ability does. `objetivo` is "self" (default) or "target" (the entity
/// that was hit, only for the hit trigger).
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "lowercase")]
pub enum Accion {
    Efecto {
        efecto: String,
        /// 1 = level I.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nivel: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        segundos: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        objetivo: Option<String>,
        /// +1 effect level every this many tiers (5: I-V base, VI-X +1, XI-XV +2).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subir_cada: Option<u8>,
    },
    /// Run as the console; {player}, {x}, {y}, {z} are replaced.
    Comando { comando: String },
    Sonido {
        sonido: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        volumen: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tono: Option<f32>,
    },
    Particula {
        particula: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cantidad: Option<u32>,
    },
    Curar { cantidad: f32 },
    /// Extra damage to the target (hit trigger).
    Danio { cantidad: f32 },
    /// Sets the target on fire (hit trigger).
    Fuego { segundos: u32 },
    Mensaje { texto: String },
    /// Launches the player where they are looking.
    Impulso { fuerza: f64 },
    /// Hits every living entity within `radio` blocks of the player.
    Area {
        radio: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cantidad: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fuego: Option<u32>,
        /// Knockback away from the player.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        empuje: Option<f64>,
    },
    /// Absorption (golden hearts), in health points.
    Escudo { cantidad: f32 },
    /// Heals a share of the damage dealt (hit trigger); 0.2 = 20 %.
    Robovida { porcentaje: f32 },
}

impl Accion {
    pub fn tipo(&self) -> &'static str {
        match self {
            Self::Efecto { .. } => "effect",
            Self::Comando { .. } => "command",
            Self::Sonido { .. } => "sound",
            Self::Particula { .. } => "particle",
            Self::Curar { .. } => "heal",
            Self::Danio { .. } => "damage",
            Self::Fuego { .. } => "fire",
            Self::Mensaje { .. } => "message",
            Self::Impulso { .. } => "dash",
            Self::Area { .. } => "area",
            Self::Escudo { .. } => "shield",
            Self::Robovida { .. } => "lifesteal",
        }
    }
}

pub const DISPARADORES: [&str; 5] = ["right_click", "sneak_right_click", "left_click", "hit", "eat"];

/// `hit:inferno` -> `hit`.
pub fn disparador_de(clave: &str) -> &str {
    clave.split(':').next().unwrap_or(clave)
}

/// Plain text, or one text per language (`{"en_us": "...", "es_es": "..."}`).
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Texto {
    Simple(String),
    PorIdioma(BTreeMap<String, String>),
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Lore {
    Lineas(Vec<String>),
    PorIdioma(BTreeMap<String, Vec<String>>),
}

/// `"minecraft:dirt"`, `"#minecraft:planks"` (tag) or `["a", "b"]` (any of them).
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Ingrediente {
    Uno(String),
    Varios(Vec<String>),
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "lowercase")]
pub enum Receta {
    Shaped {
        id: String,
        patron: Vec<String>,
        claves: BTreeMap<String, Ingrediente>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        categoria: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        grupo: Option<String>,
    },
    Shapeless {
        id: String,
        ingredientes: Vec<Ingrediente>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        categoria: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        grupo: Option<String>,
    },
    Cooking {
        id: String,
        entrada: Ingrediente,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        estacion: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        experiencia: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tiempo: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        categoria: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        grupo: Option<String>,
    },
}

impl Receta {
    pub fn id(&self) -> &str {
        match self {
            Self::Shaped { id, .. } | Self::Shapeless { id, .. } | Self::Cooking { id, .. } => id,
        }
    }

    pub fn tipo(&self) -> &'static str {
        match self {
            Self::Shaped { .. } => "shaped",
            Self::Shapeless { .. } => "shapeless",
            Self::Cooking { .. } => "cooking",
        }
    }
}
