use serde::Deserialize;

/// plugins/data/XeItems/config.toml (template: ../config.toml).
#[derive(Deserialize)]
#[serde(default)]
pub struct Config {
    pub language: String,
    pub namespace: String,
    pub op_level: u8,
    pub crafting: Crafting,
    pub items: Items,
    pub examples: Examples,
    pub editor: Editor,
    pub rarity: Rarity,
}

/// Weights for the rarity of each new copy of an item with progression.
#[derive(Deserialize)]
#[serde(default)]
pub struct Rarity {
    pub common: u32,
    pub uncommon: u32,
    pub rare: u32,
    pub epic: u32,
    pub legendary: u32,
}

impl Default for Rarity {
    fn default() -> Self {
        Self { common: 55, uncommon: 25, rare: 12, epic: 6, legendary: 2 }
    }
}

#[derive(Deserialize)]
#[serde(default)]
pub struct Items {
    pub update_on_join: bool,
}

impl Default for Items {
    fn default() -> Self {
        Self { update_on_join: true }
    }
}

#[derive(Deserialize)]
#[serde(default)]
pub struct Crafting {
    pub fix_crafted_items: bool,
    pub fix_timeout: u64,
}

#[derive(Deserialize)]
#[serde(default)]
pub struct Examples {
    pub diamond_dirt: bool,
}

#[derive(Deserialize)]
#[serde(default)]
pub struct Editor {
    pub cancel_words: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: "es_es".into(),
            namespace: "xeitems".into(),
            op_level: 2,
            crafting: Crafting::default(),
            items: Items::default(),
            examples: Examples::default(),
            editor: Editor::default(),
            rarity: Rarity::default(),
        }
    }
}

impl Default for Crafting {
    fn default() -> Self {
        Self { fix_crafted_items: true, fix_timeout: 30 }
    }
}

impl Default for Examples {
    fn default() -> Self {
        Self { diamond_dirt: true }
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self { cancel_words: vec!["cancel".into(), "cancelar".into()] }
    }
}
