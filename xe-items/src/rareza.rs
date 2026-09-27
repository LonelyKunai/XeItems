//! Rarity of each copy of an item with progression, rolled when the copy is made
//! (crafted, dropped by a mob, given). Rarer copies have stronger abilities and stats.

use crate::modelo::ItemDef;

/// Name, colour, bonus to ability power and attributes, stars.
pub const RAREZAS: [(&str, &str, f64, usize); 5] = [
    ("common", "§f", 0.0, 1),
    ("uncommon", "§e", 0.05, 2),
    ("rare", "§b", 0.10, 3),
    ("epic", "§d", 0.20, 4),
    ("legendary", "§6", 0.35, 5),
];

pub fn nombres() -> impl Iterator<Item = &'static str> {
    RAREZAS.iter().map(|r| r.0)
}

fn buscar(nombre: &str) -> &'static (&'static str, &'static str, f64, usize) {
    RAREZAS.iter().find(|r| r.0.eq_ignore_ascii_case(nombre)).unwrap_or(&RAREZAS[0])
}

pub fn valida(nombre: &str) -> bool {
    RAREZAS.iter().any(|r| r.0.eq_ignore_ascii_case(nombre))
}

/// Extra ability power and attributes (0.35 = +35 %).
pub fn bono(nombre: &str) -> f64 {
    buscar(nombre).2
}

/// `§6★★★★★ Legendary` in the server language.
pub fn etiqueta(nombre: &str) -> String {
    let (n, c, _, estrellas) = buscar(nombre);
    let texto = xe_common::lang::ts(&format!("items.rarity.{n}"), &[]);
    format!("{c}{}§8{} {c}{texto}", "★".repeat(*estrellas), "☆".repeat(5 - estrellas))
}

/// A random number in 0..n, from the host's random uuids.
pub fn azar(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    let u = pumpkin_plugin_api::uuid::generate();
    (u.high ^ u.low.rotate_left(17)) % n
}

/// 0.0..1.0
pub fn azar_real() -> f64 {
    azar(1_000_000) as f64 / 1_000_000.0
}

/// Rolls a rarity with the item's weights (or config.toml's).
pub fn tirar(def: &ItemDef) -> &'static str {
    let propios = def.progresion.as_ref().map(|p| &p.rarezas).filter(|r| !r.is_empty());
    let c = &crate::config().rarity;
    let pesos: Vec<(&'static str, u64)> = RAREZAS
        .iter()
        .map(|(n, ..)| {
            let peso = match propios {
                Some(p) => p.get(*n).copied().unwrap_or(0),
                None => match *n {
                    "common" => c.common,
                    "uncommon" => c.uncommon,
                    "rare" => c.rare,
                    "epic" => c.epic,
                    _ => c.legendary,
                },
            };
            (*n, u64::from(peso))
        })
        .collect();
    let total: u64 = pesos.iter().map(|(_, p)| p).sum();
    let mut tirada = azar(total);
    for (n, p) in pesos {
        if tirada < p {
            return n;
        }
        tirada -= p;
    }
    RAREZAS[0].0
}
