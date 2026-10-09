use std::{env, error::Error, fs, path::PathBuf};

use wayland_oxide_scanner::{Scanner, Side};

fn main() -> Result<(), Box<dyn Error>> {
    let protocols = ["./protocols/wayland.xml", "./protocols/xdg-shell.xml"];
    for path in protocols {
        println!("cargo:rerun-if-changed={path}");
    }
    let code = Scanner::new().generate(protocols, Side::Client)?;

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR"));
    fs::write(out.join("wayland.rs"), code)?;
    for protocol in ["strings", "enums"] {
        let path = format!("./tests/protocols/{protocol}.xml");
        println!("cargo:rerun-if-changed={path}");
        for (side_name, side) in [("client", Side::Client), ("server", Side::Server)] {
            let code = Scanner::new().generate([&path], side)?;
            fs::write(out.join(format!("{protocol}-{side_name}.rs")), code)?;
        }
    }
    Ok(())
}
