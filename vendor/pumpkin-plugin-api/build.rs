use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../pumpkin-plugin-wit/v0.1");
    println!("cargo:rerun-if-changed=wit");

    let wit_source = Path::new("../pumpkin-plugin-wit/v0.1");
    let wit_dest = Path::new("wit");

    if wit_source.exists() && wit_source.is_dir() {
        if !wit_dest.exists() {
            let _ = fs::create_dir_all(wit_dest);
        }

        if let Ok(entries) = fs::read_dir(wit_source) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("wit") {
                    if let Some(file_name) = path.file_name() {
                        let dest_path = wit_dest.join(file_name);
                        let needs_copy = match (fs::read(&path), fs::read(&dest_path)) {
                            (Ok(src), Ok(dest)) => src != dest,
                            (Ok(_), Err(_)) => true,
                            _ => false,
                        };
                        if needs_copy {
                            let _ = fs::copy(&path, &dest_path);
                        }
                    }
                }
            }
        }
    }
}
