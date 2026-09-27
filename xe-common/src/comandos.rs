//! Small helpers to build commands whose handlers are plain functions.
//!
//! A handler receives the sender, the server and the value of its text argument
//! (`None` when the command was run without it). Argument nodes get their handler
//! before being attached with `then()`, which takes ownership of the node.

use pumpkin_plugin_api::Server;
use pumpkin_plugin_api::command::{
    Arg, ArgumentType, Command, CommandError, CommandNode, CommandSender, ConsumedArgs, StringType,
};
use pumpkin_plugin_api::commands::{
    CommandHandler, CommandSuggestion, CommandSuggestionHandler, CommandSuggestions, SuggestionRequest,
};

use crate::lang::decir;

pub type Funcion = fn(&CommandSender, &Server, Option<String>) -> i32;

pub struct Manejador {
    funcion: Funcion,
    argumento: Option<&'static str>,
    /// Checked when it runs: the server only checks the node the command was registered
    /// with, not those of its subcommands.
    permiso: Option<String>,
}

impl CommandHandler for Manejador {
    fn handle(&self, sender: CommandSender, server: Server, args: ConsumedArgs) -> Result<i32, CommandError> {
        if let Some(permiso) = &self.permiso {
            if !sender.has_permission(&server, permiso) {
                decir(&sender, "common.no_permission", &[("permission", permiso)]);
                return Ok(0);
            }
        }
        let valor = self.argumento.and_then(|nombre| match args.get_value(nombre) {
            Arg::Simple(s) | Arg::Msg(s) => Some(s),
            _ => None,
        });
        Ok((self.funcion)(&sender, &server, valor))
    }
}

pub fn manejador(funcion: Funcion, argumento: Option<&'static str>) -> Manejador {
    Manejador { funcion, argumento, permiso: None }
}

impl Manejador {
    /// Only runs for senders with `permiso` (if any).
    pub fn con_permiso(mut self, permiso: Option<&str>) -> Self {
        self.permiso = permiso.map(String::from);
        self
    }
}

fn nombres(lista: &[&str]) -> Vec<String> {
    lista.iter().map(|n| (*n).to_string()).collect()
}

/// `/name` with no arguments.
pub fn simple(lista: &[&str], descripcion: &str, funcion: Funcion) -> Command {
    Command::new(&nombres(lista), descripcion).execute(manejador(funcion, None))
}

/// `/name <arg>`; with `opcional` the command also runs as plain `/name` (value `None`).
pub fn con_argumento(
    lista: &[&str],
    descripcion: &str,
    argumento: &'static str,
    tipo: StringType,
    opcional: bool,
    funcion: Funcion,
) -> Command {
    let nodo = CommandNode::argument(argumento, &ArgumentType::String(tipo))
        .execute(manejador(funcion, Some(argumento)));
    let comando = Command::new(&nombres(lista), descripcion);
    let comando = if opcional { comando.execute(manejador(funcion, None)) } else { comando };
    comando.then(nodo)
}

/// A literal subcommand node (`/cmd <literal> [text...]`) for command trees, that only
/// runs for senders with `permiso` (if any).
pub fn literal_con_texto(literal: &str, argumento: &'static str, permiso: Option<&str>, funcion: Funcion) -> CommandNode {
    let texto = CommandNode::argument(argumento, &ArgumentType::String(StringType::Greedy))
        .execute(manejador(funcion, Some(argumento)).con_permiso(permiso));
    CommandNode::literal(literal).execute(manejador(funcion, None).con_permiso(permiso)).then(texto)
}

/// Tab completion for a greedy text argument: gets the subcommand, the finished words
/// before the cursor and the word being typed; returns candidates for that word.
pub type Sugeridor = fn(&Server, &str, &[&str], &str) -> Vec<String>;

struct Sugerencias {
    sub: &'static str,
    sugeridor: Sugeridor,
}

impl CommandSuggestionHandler for Sugerencias {
    fn suggest(&self, _sender: CommandSender, server: Server, peticion: SuggestionRequest) -> CommandSuggestions {
        let inicio = (peticion.start as usize).min(peticion.input.len());
        let texto = peticion.input.get(inicio..).unwrap_or("");
        let corte = texto.rfind(' ').map_or(0, |i| i + 1);
        let previos: Vec<&str> = texto[..corte].split_whitespace().collect();
        let actual = &texto[corte..];
        let minuscula = actual.to_lowercase();
        let values = (self.sugeridor)(&server, self.sub, &previos, actual)
            .into_iter()
            .filter(|v| v.to_lowercase().starts_with(&minuscula))
            .take(200)
            .map(|value| CommandSuggestion { value, tooltip: None })
            .collect();
        CommandSuggestions { start: (inicio + corte) as u32, length: actual.len() as u32, values }
    }
}

/// [`literal_con_texto`] with tab completion.
pub fn literal_con_sugerencias(
    literal: &'static str,
    argumento: &'static str,
    permiso: Option<&str>,
    funcion: Funcion,
    sugeridor: Sugeridor,
) -> CommandNode {
    let texto = CommandNode::argument(argumento, &ArgumentType::String(StringType::Greedy))
        .execute(manejador(funcion, Some(argumento)).con_permiso(permiso))
        .suggest(Sugerencias { sub: literal, sugeridor });
    CommandNode::literal(literal).execute(manejador(funcion, None).con_permiso(permiso)).then(texto)
}
