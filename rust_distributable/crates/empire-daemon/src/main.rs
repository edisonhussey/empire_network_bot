#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .compact()
        .init();

    tokio::select! {
        result = empire_daemon::serve_from_env() => result,
        signal = tokio::signal::ctrl_c() => {
            signal?;
            Ok(())
        }
    }
}
