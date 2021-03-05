mod imp;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = "[::1]:10000".parse().unwrap();
    imp::new()
        .await
        .serve(addr)
        .await
        .map_err(|e| anyhow::anyhow!("failed to start service: {}", e))
}
