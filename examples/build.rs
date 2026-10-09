use std::{env, error::Error, fs, path::PathBuf};

use wayland_oxide_scanner::{Scanner, Side};

fn main() -> Result<(), Box<dyn Error>> {
    let code = Scanner::new().generate(
        ["./protocols/wayland.xml", "./protocols/xdg-shell.xml"],
        Side::Client,
    )?;

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR"));
    fs::write(out.join("wayland.rs"), code)?;
    for protocol in ["strings", "enums"] {
        for (side_name, side) in [("client", Side::Client), ("server", Side::Server)] {
            let code =
                Scanner::new().generate([format!("./tests/protocols/{protocol}.xml")], side)?;
            fs::write(out.join(format!("{protocol}-{side_name}.rs")), code)?;
        }
    }
    Ok(())
}
