//! Print the handshake versions the live web client uses, read the way the service reads them before its final retry
//! (`empire-core::client_release`). Only the public client files are fetched; no game server is contacted.
//!
//!     cargo run -p empire-socket --example client_versions                      # the live client
//!     cargo run -p empire-socket --example client_versions -- data/raw/game_files/client   # a saved copy, offline
use empire_core::client_release::{self, CLIENT_ROOT};
use empire_core::session::{CLIENT_VERSION, SMARTFOX_API_VERSION};

#[tokio::main]
async fn main() {
    let source = std::env::args().nth(1).unwrap_or_else(|| CLIENT_ROOT.to_owned());
    let derived = if source.starts_with("http://") || source.starts_with("https://") {
        empire_socket::client_release::fetch_handshake_versions(&source).await.map_err(|error| error.to_string())
    } else {
        from_folder(&source)
    };
    println!("built in:  client {CLIENT_VERSION} / verChk {SMARTFOX_API_VERSION}");
    match derived {
        Ok(versions) => {
            println!("{source}: {versions}");
            let same = versions.client_version == CLIENT_VERSION && versions.smartfox_api_version == SMARTFOX_API_VERSION;
            println!("{}", if same { "UNCHANGED" } else { "CHANGED: the service would use these on its final retry" });
        }
        Err(error) => {
            println!("FAILED: {error}");
            std::process::exit(1);
        }
    }
}

fn from_folder(folder: &str) -> Result<client_release::HandshakeVersions, String> {
    let read = |name: &str| std::fs::read_to_string(format!("{folder}/{name}")).map_err(|error| format!("{name}: {error}"));
    let index = read("index.html")?;
    let bundle = read(client_release::game_bundle_path(&index).map_err(|error| error.to_string())?)?;
    let dll = read(client_release::ggs_dll_path(&index).map_err(|error| error.to_string())?)?;
    client_release::derive(&bundle, &dll).map_err(|error| error.to_string())
}
