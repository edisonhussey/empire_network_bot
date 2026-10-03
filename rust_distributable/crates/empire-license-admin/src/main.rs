use std::{
    env, fs,
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;

const PREFIX: &str = "OA1";
const KEY_ID: &str = "oa-main-2026";

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("keygen") => keygen(value(&args, "--private")?, value(&args, "--public")?),
        Some("public") => derive_public(value(&args, "--private")?),
        Some("issue") => issue(&args),
        _ => bail!(
            "usage:\n  empire-license-admin keygen --private PATH --public PATH\n  empire-license-admin public --private PATH\n  empire-license-admin issue --private PATH --license-id ID --subject NAME --days N [--revision N] [--tier NAME] [--features a,b] [--output PATH]"
        ),
    }
}

fn keygen(private_path: &str, public_path: &str) -> anyhow::Result<()> {
    let private = Path::new(private_path);
    if private.exists() {
        bail!("refusing to overwrite an existing private key")
    }
    let mut secret = [0_u8; 32];
    getrandom::fill(&mut secret)
        .map_err(|error| anyhow::anyhow!("operating-system randomness failed: {error}"))?;
    let signing = SigningKey::from_bytes(&secret);
    write_private(private, &URL_SAFE_NO_PAD.encode(signing.to_bytes()))?;
    fs::write(
        public_path,
        format!(
            "{}\n",
            URL_SAFE_NO_PAD.encode(signing.verifying_key().to_bytes())
        ),
    )
    .with_context(|| format!("failed to write public key {public_path}"))?;
    println!(
        "KEYGEN_OK key_id={KEY_ID} private={} public={public_path}",
        private.display()
    );
    Ok(())
}

fn derive_public(private_path: &str) -> anyhow::Result<()> {
    let signing = read_private(private_path)?;
    println!(
        "{}",
        URL_SAFE_NO_PAD.encode(signing.verifying_key().to_bytes())
    );
    Ok(())
}

fn issue(args: &[String]) -> anyhow::Result<()> {
    let signing = read_private(value(args, "--private")?)?;
    let now = now_epoch();
    let days: i64 = value(args, "--days")?
        .parse()
        .context("--days must be an integer")?;
    if !(1..=3660).contains(&days) {
        bail!("--days must be between 1 and 3660")
    }
    let revision: u64 = optional(args, "--revision")
        .unwrap_or("1")
        .parse()
        .context("--revision must be a positive integer")?;
    if revision == 0 {
        bail!("--revision must be positive")
    }
    let features: Vec<String> = optional(args, "--features")
        .unwrap_or("game_network,account_initialize,automation")
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect();
    let claims = json!({
        "schema": 1,
        "key_id": KEY_ID,
        "license_id": value(args, "--license-id")?,
        "subject": value(args, "--subject")?,
        "issued_at": now,
        "not_before": now,
        "expires_at": now.saturating_add(days.saturating_mul(86_400)),
        "revision": revision,
        "tier": optional(args, "--tier").unwrap_or("pro"),
        "features": features,
    });
    let payload = serde_json::to_vec(&claims)?;
    let signature = signing.sign(&payload);
    let token = format!(
        "{PREFIX}.{}.{}",
        URL_SAFE_NO_PAD.encode(payload),
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    );
    if let Some(path) = optional(args, "--output") {
        write_secret(Path::new(path), &token)?;
        println!("ISSUE_OK output={path}");
    } else {
        println!("{token}");
    }
    Ok(())
}

fn read_private(path: &str) -> anyhow::Result<SigningKey> {
    let encoded =
        fs::read_to_string(path).with_context(|| format!("failed to read private key {path}"))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded.trim())
        .context("private key is not valid base64url")?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("private key must be 32 bytes"))?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn write_private(path: &Path, value: &str) -> anyhow::Result<()> {
    write_secret(path, value)
}

fn write_secret(path: &Path, value: &str) -> anyhow::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("failed to create private key {}", path.display()))?;
    file.write_all(format!("{value}\n").as_bytes())
        .with_context(|| format!("failed to write private key {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync private key {}", path.display()))?;
    Ok(())
}

fn value<'a>(args: &'a [String], name: &str) -> anyhow::Result<&'a str> {
    optional(args, name).with_context(|| format!("missing {name}"))
}

fn optional<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].as_str())
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
