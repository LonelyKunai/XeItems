//! XeItems: custom items and recipes, made in game with /xeitems or a visual editor.

mod api;
mod atributos;
mod comandos;
mod config;
mod crafteo;
mod editor;
mod habilidades;
mod inventario;
mod ipc;
mod modelo;
mod nombres;
mod instancias;
mod progresion;
mod rareza;
mod registro;

use std::sync::OnceLock;

use pumpkin_plugin_api::permission::PermissionDefault;
use pumpkin_plugin_api::{Context, Plugin, PluginMetadata, Server, permissions, register_plugin};
use xe_common::lang::{establecer_idioma, ts};
use xe_common::permisos::{self, nivel_op};
use xe_common::{datos, info};

use config::Config;
use modelo::{Ingrediente, ItemDef, Lore, Receta, Texto};
use registro::Origen;

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn config() -> &'static Config {
    CONFIG.get_or_init(Config::default)
}

struct XeItems;

/// XeItems:command, XeItems:<subcommand> and XeItems:admin (all of them), for operators.
fn permisos(context: &Context) -> pumpkin_plugin_api::Result<()> {
    let mut nodos = vec![(comandos::PERMISO.to_string(), ts("perm.xeitems", &[]))];
    for (sub, _) in comandos::SUBCOMANDOS.iter().filter(|(s, _)| *s != "help") {
        nodos.push((comandos::permiso_de(sub), permisos::descripcion(&format!("/xeitems {sub}"))));
    }
    let nivel = nivel_op(config().op_level);
    permisos::registrar(context, "XeItems", &nodos, PermissionDefault::Op(nivel), nivel)
}

/// Example item: 9 dirt -> Diamond Dirt. Not written to an item file unless edited.
fn diamond_dirt(server: &Server) {
    let def = ItemDef {
        id: Some("diamond_dirt".into()),
        material: "minecraft:diamond_block".into(),
        nombre: Some(Texto::Simple(ts("items.diamond_dirt.name", &[]))),
        lore: Some(Lore::Lineas(vec![ts("items.diamond_dirt.lore1", &[]), ts("items.diamond_dirt.lore2", &[])])),
        recetas: vec![Receta::Shaped {
            id: "diamond_dirt".into(),
            patron: vec!["DDD".into(), "DDD".into(), "DDD".into()],
            claves: [("D".to_string(), Ingrediente::Uno("minecraft:dirt".into()))].into(),
            categoria: None,
            grupo: None,
        }],
        ..Default::default()
    };
    let _ = registro::definir(server, def, Origen::Codigo);
}

impl Plugin for XeItems {
    fn new() -> Self {
        XeItems
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "XeItems".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["Arclinker".into()],
            description: ts("items.description", &[]),
            dependencies: vec![],
            // Data folder: config.toml and the items folder.
            permissions: vec![permissions::FS_WRITE_DATA.into()],
        }
    }

    fn on_load(&self, context: Context) -> pumpkin_plugin_api::Result<()> {
        datos::iniciar(context.get_data_folder());
        let cfg = CONFIG.get_or_init(|| xe_common::config::cargar(include_str!("../config.toml")));
        establecer_idioma(&cfg.language);
        info(&ts("items.started", &[]));

        let server = context.get_server();
        if cfg.examples.diamond_dirt {
            diamond_dirt(&server);
        }
        instancias::cargar();
        registro::cargar(&server);
        crafteo::registrar(&context)?;
        editor::registrar(&context)?;
        habilidades::registrar(&context)?;
        progresion::registrar(&context)?;
        atributos::registrar(&context)?;
        inventario::registrar(&context)?;
        permisos(&context)?;
        comandos::registrar(&context);
        ipc::iniciar(context.get_server());
        Ok(())
    }

    fn on_unload(&self, _context: Context) -> pumpkin_plugin_api::Result<()> {
        instancias::guardar_todo();
        info(&ts("items.stopped", &[]));
        Ok(())
    }

    /// The API for other plugins (see `ipc`).
    fn handle_ipc_message(&self, sender: String, message: Vec<u8>) -> pumpkin_plugin_api::Result<Vec<u8>, String> {
        ipc::atender(&sender, &message)
    }
}

register_plugin!(XeItems);
