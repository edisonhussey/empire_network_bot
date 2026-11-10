//! Fetch the game's public web client and read the handshake's versions from it (`empire-core::client_release`).
//!
//! Only public, unauthenticated pages are fetched (`index.html`, `Game.bundle.*.js`, `dll/ggs.dll.*.js`); nothing
//! is sent to a game server. This is a one-shot HTTP/1.1 `GET` over the TLS the game socket already uses, so the
//! service needs no HTTP client crate.

use std::{io, time::Duration};

use empire_core::client_release::{self, HandshakeVersions};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::Wire;

const TIMEOUT: Duration = Duration::from_secs(60);
/// `Game.bundle` is about 16 MB; refuse anything far beyond that.
const MAX_BODY: usize = 64 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;

/// The live client's versions, read from `root` (normally [`client_release::CLIENT_ROOT`]).
pub async fn fetch_handshake_versions(root: &str) -> io::Result<HandshakeVersions> {
    let root = if root.ends_with('/') { root.to_owned() } else { format!("{root}/") };
    let index = get_text(&format!("{root}index.html")).await?;
    let bundle_path = client_release::game_bundle_path(&index).map_err(io::Error::other)?;
    let dll_path = client_release::ggs_dll_path(&index).map_err(io::Error::other)?;
    let bundle = get_text(&format!("{root}{bundle_path}")).await?;
    let dll = get_text(&format!("{root}{dll_path}")).await?;
    client_release::derive(&bundle, &dll).map_err(io::Error::other)
}

/// The body of an `https://` (or `http://`) URL as text, following up to three redirects.
pub async fn get_text(url: &str) -> io::Result<String> {
    let mut url = url.to_owned();
    for _ in 0..=MAX_REDIRECTS {
        let response = tokio::time::timeout(TIMEOUT, get_once(&url))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, format!("no answer from {url} in {TIMEOUT:?}")))??;
        match response.status {
            200 => return Ok(String::from_utf8_lossy(&response.body).into_owned()),
            301 | 302 | 303 | 307 | 308 => {
                let location = response
                    .location
                    .ok_or_else(|| io::Error::other(format!("{url}: redirect without a Location")))?;
                url = resolve(&url, &location);
            }
            status => return Err(io::Error::other(format!("{url}: HTTP {status}"))),
        }
    }
    Err(io::Error::other(format!("{url}: more than {MAX_REDIRECTS} redirects")))
}

struct Response {
    status: u16,
    location: Option<String>,
    body: Vec<u8>,
}

struct Target<'a> {
    tls: bool,
    host: &'a str,
    port: u16,
    path: &'a str,
}

fn split_url(url: &str) -> io::Result<Target<'_>> {
    let (tls, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        (false, rest)
    } else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("not an http(s) URL: {url}")));
    };
    let (authority, path) = rest.find('/').map_or((rest, "/"), |slash| (&rest[..slash], &rest[slash..]));
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, url.to_owned()))?),
        None => (authority, if tls { 443 } else { 80 }),
    };
    Ok(Target { tls, host, port, path })
}

fn resolve(base: &str, location: &str) -> String {
    if location.starts_with("http://") || location.starts_with("https://") {
        return location.to_owned();
    }
    let scheme_end = base.find("://").map_or(0, |at| at + 3);
    let origin_end = base[scheme_end..].find('/').map_or(base.len(), |at| scheme_end + at);
    if location.starts_with('/') {
        format!("{}{location}", &base[..origin_end])
    } else {
        let dir_end = base.rfind('/').filter(|at| *at >= origin_end).map_or(base.len(), |at| at + 1);
        format!("{}{}{location}", &base[..dir_end], if dir_end == base.len() && !base.ends_with('/') { "/" } else { "" })
    }
}

async fn get_once(url: &str) -> io::Result<Response> {
    let target = split_url(url)?;
    let tcp = TcpStream::connect((target.host, target.port)).await?;
    let mut wire = if target.tls {
        let connector = native_tls::TlsConnector::new().map_err(io::Error::other)?;
        let secured = tokio_native_tls::TlsConnector::from(connector)
            .connect(target.host, tcp)
            .await
            .map_err(io::Error::other)?;
        Wire::Tls(Box::new(secured))
    } else {
        Wire::Plain(tcp)
    };
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: Mozilla/5.0\r\nAccept: */*\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n",
        target.path, target.host
    );
    wire.write_all(request.as_bytes()).await?;
    let mut raw = Vec::new();
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let read = match wire.read(&mut chunk).await {
            Ok(read) => read,
            // Some servers drop TLS without close_notify after `Connection: close`; the length checks below catch a short body.
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof && !raw.is_empty() => 0,
            Err(error) => return Err(error),
        };
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);
        if raw.len() > MAX_BODY {
            return Err(io::Error::other(format!("{url}: body over {MAX_BODY} bytes")));
        }
    }
    parse_response(&raw)
}

fn parse_response(raw: &[u8]) -> io::Result<Response> {
    let bad = |what: &str| io::Error::new(io::ErrorKind::InvalidData, format!("bad HTTP response: {what}"));
    let head_end = raw.windows(4).position(|window| window == b"\r\n\r\n").ok_or_else(|| bad("no header end"))?;
    let head = std::str::from_utf8(&raw[..head_end]).map_err(|_| bad("header is not text"))?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| bad("no status"))?;
    let mut location = None;
    let mut content_length = None;
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "location" => location = Some(value.to_owned()),
            "content-length" => content_length = value.parse::<usize>().ok(),
            "transfer-encoding" => chunked = value.to_ascii_lowercase().contains("chunked"),
            "content-encoding" if !value.eq_ignore_ascii_case("identity") => {
                return Err(bad(&format!("unasked-for content-encoding {value}")));
            }
            _ => {}
        }
    }
    let rest = &raw[head_end + 4..];
    let body = if chunked {
        dechunk(rest).ok_or_else(|| bad("broken chunked body"))?
    } else if let Some(length) = content_length {
        if rest.len() < length {
            return Err(bad(&format!("body cut short ({} of {length} bytes)", rest.len())));
        }
        rest[..length].to_vec()
    } else {
        rest.to_vec()
    };
    Ok(Response { status, location, body })
}

fn dechunk(mut rest: &[u8]) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let line_end = rest.windows(2).position(|window| window == b"\r\n")?;
        let size_text = std::str::from_utf8(&rest[..line_end]).ok()?;
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        rest = &rest[line_end + 2..];
        if size == 0 {
            return Some(body);
        }
        body.extend_from_slice(rest.get(..size)?);
        rest = rest.get(size..)?.strip_prefix(b"\r\n")?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_chunked_and_redirect_responses() {
        let plain = parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello").unwrap();
        assert_eq!((plain.status, plain.body.as_slice()), (200, &b"hello"[..]));
        let chunked = parse_response(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6;x=1\r\n world\r\n0\r\n\r\n").unwrap();
        assert_eq!(chunked.body, b"hello world");
        let moved = parse_response(b"HTTP/1.1 301 Moved\r\nLocation: /default/\r\nContent-Length: 0\r\n\r\n").unwrap();
        assert_eq!((moved.status, moved.location.as_deref()), (301, Some("/default/")));
        assert!(parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nhello").is_err(), "a short body is an error");
        assert!(parse_response(b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\n\r\nxx").is_err());
    }

    #[test]
    fn urls_split_and_redirects_resolve() {
        let target = split_url("https://empire-html5.goodgamestudios.com/default/index.html").unwrap();
        assert_eq!((target.tls, target.host, target.port, target.path), (true, "empire-html5.goodgamestudios.com", 443, "/default/index.html"));
        assert_eq!(split_url("http://localhost:8080").unwrap().path, "/");
        assert_eq!(resolve("https://a.com/default/index.html", "/x/y.js"), "https://a.com/x/y.js");
        assert_eq!(resolve("https://a.com/default/index.html", "y.js"), "https://a.com/default/y.js");
        assert_eq!(resolve("https://a.com/default/", "https://b.com/z"), "https://b.com/z");
    }

    /// Serves a saved client over a local socket and reads both versions through the same code the service uses.
    #[tokio::test]
    async fn derives_both_versions_from_a_served_client() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let root = format!("http://{}/default/", listener.local_addr().unwrap());
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = vec![0_u8; 4096];
                let read = socket.read(&mut request).await.unwrap();
                let path = String::from_utf8_lossy(&request[..read]).split(' ').nth(1).unwrap_or_default().to_owned();
                let body = match path.as_str() {
                    "/default/index.html" => r#"<link id="dll" href="dll/ggs.dll.0b914fe6.js"><link id="Game" href="Game.bundle.4230c06b.js">"#,
                    "/default/Game.bundle.4230c06b.js" => r#"{name:"TranspilationEmpire",version:"1.172.4",description:""}"#,
                    "/default/dll/ggs.dll.0b914fe6.js" => r#"n.sendXMLMessage(f,"verChk",0,"<ver v='166' />")"#,
                    _ => "",
                };
                let status = if body.is_empty() { "404 Not Found" } else { "200 OK" };
                let response = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n{body}", body.len());
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let versions = fetch_handshake_versions(&root).await.unwrap();
        assert_eq!(versions.client_version, "1172004");
        assert_eq!(versions.smartfox_api_version, "166");
    }
}
