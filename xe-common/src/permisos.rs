//! Permission nodes: every command and subcommand has its own node, and `<Plugin>:admin`
//! grants all the nodes of its plugin.

use pumpkin_plugin_api::Context;
use pumpkin_plugin_api::permission::{Permission, PermissionChild, PermissionDefault, PermissionLevel};

use crate::lang::ts;

pub fn nivel_op(nivel: u8) -> PermissionLevel {
    match nivel {
        0 => PermissionLevel::Zero,
        1 => PermissionLevel::One,
        2 => PermissionLevel::Two,
        3 => PermissionLevel::Three,
        _ => PermissionLevel::Four,
    }
}

/// Description of the node for `comando` (`/pve spawn`).
pub fn descripcion(comando: &str) -> String {
    ts("perm.command", &[("command", &comando)])
}

/// Registers `nodos` (node, description) with `defecto`, and `<plugin>:admin`, granted to
/// operators of `nivel_admin`, which grants every one of them.
pub fn registrar(
    context: &Context,
    plugin: &str,
    nodos: &[(String, String)],
    defecto: PermissionDefault,
    nivel_admin: PermissionLevel,
) -> pumpkin_plugin_api::Result<()> {
    for (node, description) in nodos {
        context.register_permission(&Permission {
            node: node.clone(),
            description: description.clone(),
            default: defecto.clone(),
            children: vec![],
        })?;
    }
    context.register_permission(&Permission {
        node: format!("{plugin}:admin"),
        description: ts("perm.admin", &[("plugin", &plugin)]),
        default: PermissionDefault::Op(nivel_admin),
        children: nodos.iter().map(|(node, _)| PermissionChild { node: node.clone(), value: true }).collect(),
    })
}
