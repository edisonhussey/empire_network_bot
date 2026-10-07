with open("crates/empire-daemon/src/direct/mod.rs", "r") as f:
    content = f.read()

find_str = """pub async fn run(
    request: DirectConnectRequest,
    store: Store,
    active_transport: Arc<RwLock<Option<mpsc::Sender<InjectionRequest>>>>,
    status: Arc<RwLock<DirectStatus>>,
    licence: LicenceGate,
) {
    let endpoint = request.endpoint.clone();
    status.write().await.endpoint = Some(endpoint.clone());"""

replace_str = """pub async fn run(
    request: DirectConnectRequest,
    store: Store,
    active_transport: Arc<RwLock<Option<mpsc::Sender<InjectionRequest>>>>,
    status: Arc<RwLock<DirectStatus>>,
    licence: LicenceGate,
) {
    let endpoint = request.endpoint.clone();
    let _account_id = request.credentials.player_name.trim().to_ascii_lowercase();
    status.write().await.endpoint = Some(endpoint.clone());"""

content = content.replace(find_str, replace_str)

with open("crates/empire-daemon/src/direct/mod.rs", "w") as f:
    f.write(content)

print("Restored account_id in run")
