//! Generates name -> variant tables for the WIT enums the API has no lookup for
//! (sounds, particles, effects, attributes). Names are the WIT ones with '-' turned
//! into '_' (entity_player_levelup); a wrong variant name fails the build.

use std::fmt::Write;
use std::path::Path;

const ENUMS: [(&str, &str, &str); 5] = [
    ("sounds.wit", "sound", "SONIDOS"),
    ("particles.wit", "particle", "PARTICULAS"),
    ("status-effect.wit", "status-effect-type", "EFECTOS"),
    ("attributes.wit", "attribute", "ATRIBUTOS"),
    ("entity-types.wit", "entity-type", "ENTIDADES"),
];

fn camel(kebab: &str) -> String {
    kebab
        .split('-')
        .map(|parte| {
            let mut c = parte.chars();
            c.next().map_or_else(String::new, |p| p.to_ascii_uppercase().to_string() + c.as_str())
        })
        .collect()
}

fn variantes(wit: &str, nombre: &str) -> Vec<String> {
    let inicio = wit.find(&format!("enum {nombre} {{")).unwrap_or_else(|| panic!("enum {nombre} not found"));
    let cuerpo = &wit[inicio..];
    let cuerpo = &cuerpo[cuerpo.find('{').unwrap() + 1..cuerpo.find('}').unwrap()];
    cuerpo
        .lines()
        .map(|l| l.split("//").next().unwrap_or("").trim().trim_end_matches(',').trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.trim_start_matches('%').to_string())
        .collect()
}

fn main() {
    let wit = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vendor/pumpkin-plugin-api/wit");
    let mut salida = String::new();
    for (archivo, enumeracion, tabla) in ENUMS {
        let ruta = wit.join(archivo);
        println!("cargo:rerun-if-changed={}", ruta.display());
        let texto = std::fs::read_to_string(&ruta).unwrap();
        let tipo = camel(enumeracion);
        let lista = variantes(&texto, enumeracion);
        writeln!(salida, "pub const {tabla}: [(&str, {tipo}); {}] = [", lista.len()).unwrap();
        for v in &lista {
            writeln!(salida, "    (\"{}\", {tipo}::{}),", v.replace('-', "_"), camel(v)).unwrap();
        }
        salida.push_str("];\n");
    }
    let destino = Path::new(&std::env::var("OUT_DIR").unwrap()).join("nombres.rs");
    std::fs::write(destino, salida).unwrap();
}
