//! The two version values the handshake carries, read from the game's own public web client.
//!
//! The server refuses a session whose versions it no longer accepts ([problem 10], [problem 20]). Both values are
//! written into the public client, and the client computes them the same way every time, so they can be read rather
//! than guessed:
//!
//! * **`client_version`** (sent in the sys `login` and in `vck`): `Game.bundle.<hash>.js` carries the package entry
//!   `name:"TranspilationEmpire",version:"A.B.C"`. The client turns that into the wire value in
//!   `CastleVersionInformation` with `A + pad3(B) + pad3(C before any "-")`, so `1.171.9` is `1171009`.
//! * **`smartfox_api_version`** (sent in `verChk`): `dll/ggs.dll.<hash>.js` opens every socket with
//!   `sendXMLMessage(.., "verChk", 0, "<ver v='166' />")`.
//!
//! `index.html` at the client's root names both files (`href="Game.bundle.<hash>.js"`, `href="dll/ggs.dll.<hash>.js"`).
//! This module only parses text; fetching is in `empire-socket::client_release`.
//!
//! [problem 10]: ../../../docs/problems/10.md
//! [problem 20]: ../../../docs/problems/20.md

use std::fmt;

/// Where the live web client is published.
pub const CLIENT_ROOT: &str = "https://empire-html5.goodgamestudios.com/default/";

const GAME_BUNDLE_PREFIX: &str = "Game.bundle.";
const GGS_DLL_PREFIX: &str = "dll/ggs.dll.";
const APP_VERSION_MARKER: &str = "TranspilationEmpire\",version:\"";
const VER_CHK_MARKER: &str = "\"verChk\",0,\"<ver v='";

/// The handshake's version values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeVersions {
    pub client_version: String,
    pub smartfox_api_version: String,
}

impl fmt::Display for HandshakeVersions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "client {} / verChk {}", self.client_version, self.smartfox_api_version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeriveError {
    /// `index.html` does not name the file starting with this prefix.
    AssetNotNamed(&'static str),
    /// `Game.bundle` has no `TranspilationEmpire` version.
    NoAppVersion,
    /// The app version is not `digits.digits.digits[-suffix]`, or a part is longer than three digits.
    BadAppVersion(String),
    /// The dll has no `verChk` `<ver v='N' />`.
    NoSmartfoxVersion,
}

impl fmt::Display for DeriveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AssetNotNamed(prefix) => write!(formatter, "the client's index.html names no {prefix}*.js"),
            Self::NoAppVersion => write!(formatter, "Game.bundle has no TranspilationEmpire version"),
            Self::BadAppVersion(version) => write!(formatter, "TranspilationEmpire version {version:?} is not A.B.C"),
            Self::NoSmartfoxVersion => write!(formatter, "the ggs dll has no verChk version"),
        }
    }
}

impl std::error::Error for DeriveError {}

/// The client's own rule, `1.171.9` -> `1171009`. Stricter than the client: every part must be digits and the minor
/// and patch at most three, because a longer part would make the wire value ambiguous.
pub fn wire_client_version(app_version: &str) -> Result<String, DeriveError> {
    let bad = || DeriveError::BadAppVersion(app_version.to_owned());
    let mut parts = app_version.split('.');
    let (Some(major), Some(minor), Some(patch), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return Err(bad());
    };
    let patch = patch.split('-').next().unwrap_or_default();
    let digits = |part: &str, max: usize| !part.is_empty() && part.len() <= max && part.bytes().all(|b| b.is_ascii_digit());
    if !digits(major, 4) || !digits(minor, 3) || !digits(patch, 3) {
        return Err(bad());
    }
    Ok(format!("{major}{minor:0>3}{patch:0>3}"))
}

/// The path of the `<prefix><hash>.js` file `index.html` loads, relative to the client root.
pub fn asset_path<'a>(index_html: &'a str, prefix: &'static str) -> Result<&'a str, DeriveError> {
    let needle = format!("\"{prefix}");
    let start = index_html.find(&needle).ok_or(DeriveError::AssetNotNamed(prefix))? + 1;
    let rest = &index_html[start..];
    let end = rest.find('"').ok_or(DeriveError::AssetNotNamed(prefix))?;
    let path = &rest[..end];
    let hash = &path[prefix.len()..];
    let valid = hash.ends_with(".js") && hash[..hash.len() - 3].bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.');
    if valid { Ok(path) } else { Err(DeriveError::AssetNotNamed(prefix)) }
}

pub fn game_bundle_path(index_html: &str) -> Result<&str, DeriveError> {
    asset_path(index_html, GAME_BUNDLE_PREFIX)
}

pub fn ggs_dll_path(index_html: &str) -> Result<&str, DeriveError> {
    asset_path(index_html, GGS_DLL_PREFIX)
}

/// `A.B.C` from `name:"TranspilationEmpire",version:"A.B.C"` in `Game.bundle`.
pub fn app_version(game_bundle: &str) -> Result<&str, DeriveError> {
    let start = game_bundle.find(APP_VERSION_MARKER).ok_or(DeriveError::NoAppVersion)? + APP_VERSION_MARKER.len();
    let rest = &game_bundle[start..];
    let end = rest.find('"').filter(|end| *end <= 32).ok_or(DeriveError::NoAppVersion)?;
    Ok(&rest[..end])
}

/// `N` from `"verChk",0,"<ver v='N' />"` in the ggs dll.
pub fn smartfox_api_version(ggs_dll: &str) -> Result<&str, DeriveError> {
    let start = ggs_dll.find(VER_CHK_MARKER).ok_or(DeriveError::NoSmartfoxVersion)? + VER_CHK_MARKER.len();
    let rest = &ggs_dll[start..];
    let end = rest.find('\'').ok_or(DeriveError::NoSmartfoxVersion)?;
    let version = &rest[..end];
    if version.is_empty() || version.len() > 6 || !version.bytes().all(|b| b.is_ascii_digit()) {
        return Err(DeriveError::NoSmartfoxVersion);
    }
    Ok(version)
}

/// Both values from the two client files.
pub fn derive(game_bundle: &str, ggs_dll: &str) -> Result<HandshakeVersions, DeriveError> {
    Ok(HandshakeVersions {
        client_version: wire_client_version(app_version(game_bundle)?)?,
        smartfox_api_version: smartfox_api_version(ggs_dll)?.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cut from the files of the live client saved on 2026-10-14 (`data/raw/game_files/client/`).
    const INDEX: &str = r#"<link id="dll" rel="preload" href="dll/ggs.dll.0b914fe686cb8e274f41.js" as="script">
    <link id="Loader" rel="preload" href="CastleLoader.bundle.a8f55a09b7e8c07abc9e.js" as="script">
    <link id="Game" rel="preload" href="Game.bundle.4230c06b48520e83ead4.js" as="script">"#;
    const BUNDLE: &str = r#"e.exports={name:"TranspilationEmpire",version:"1.170.22",description:"Transpiled version of the Flash game",main:"index.js"}"#;
    const DLL: &str = r#"this.socket.onopen=function(){C.debug("Connected"),n.sendXMLMessage(f,"verChk",0,"<ver v='166' />")}"#;

    #[test]
    fn the_wire_value_follows_the_clients_padding_rule() {
        // The two values the live server accepted: problem 10 (1.170.22) and problem 20 (1.171.9).
        assert_eq!(wire_client_version("1.170.22").unwrap(), "1170022");
        assert_eq!(wire_client_version("1.171.9").unwrap(), "1171009");
        assert_eq!(wire_client_version("1.169.11").unwrap(), "1169011");
        assert_eq!(wire_client_version("1.172.0-hotfix").unwrap(), "1172000");
        for bad in ["", "1.171", "1.171.9.1", "1.1000.1", "1.x.1", "1..1", "1.171.1234"] {
            assert!(wire_client_version(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn both_values_come_out_of_the_saved_client() {
        assert_eq!(game_bundle_path(INDEX).unwrap(), "Game.bundle.4230c06b48520e83ead4.js");
        assert_eq!(ggs_dll_path(INDEX).unwrap(), "dll/ggs.dll.0b914fe686cb8e274f41.js");
        assert_eq!(
            derive(BUNDLE, DLL).unwrap(),
            HandshakeVersions { client_version: "1170022".to_owned(), smartfox_api_version: "166".to_owned() }
        );
    }

    #[test]
    fn a_page_that_changed_shape_derives_nothing() {
        assert_eq!(game_bundle_path("<html></html>"), Err(DeriveError::AssetNotNamed(GAME_BUNDLE_PREFIX)));
        assert_eq!(asset_path(r#"href="Game.bundle.../../x.js""#, GAME_BUNDLE_PREFIX), Err(DeriveError::AssetNotNamed(GAME_BUNDLE_PREFIX)));
        assert_eq!(derive("no version here", DLL), Err(DeriveError::NoAppVersion));
        assert_eq!(derive(BUNDLE, "verChk without a version"), Err(DeriveError::NoSmartfoxVersion));
        assert_eq!(smartfox_api_version(r#""verChk",0,"<ver v='abc' />""#), Err(DeriveError::NoSmartfoxVersion));
    }
}
