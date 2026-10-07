with open("crates/empire-daemon/src/direct/mod.rs", "r") as f:
    content = f.read()

find_str = """async fn run_inner(
    request: DirectConnectRequest,
    store: &Store,
    active_transport: &Arc<RwLock<Option<mpsc::Sender<InjectionRequest>>>>,
    status: &Arc<RwLock<DirectStatus>>,
    licence: &LicenceGate,
) -> anyhow::Result<()> {
    licence.require_bootstrap("game_network").await?;
    let reuse_existing_map ="""

replace_str = """async fn run_inner(
    request: DirectConnectRequest,
    store: &Store,
    active_transport: &Arc<RwLock<Option<mpsc::Sender<InjectionRequest>>>>,
    status: &Arc<RwLock<DirectStatus>>,
    licence: &LicenceGate,
) -> anyhow::Result<()> {
    licence.require_bootstrap("game_network").await?;
    let account_id = request.credentials.player_name.trim().to_ascii_lowercase();
    let reuse_existing_map ="""

content = content.replace(find_str, replace_str)

with open("crates/empire-daemon/src/direct/mod.rs", "w") as f:
    f.write(content)

print("Restored account_id in run_inner")
