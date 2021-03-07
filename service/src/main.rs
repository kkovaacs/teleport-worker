mod imp;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let server = imp::new_tls_server()?;
    let addr = "[::1]:10000".parse().unwrap();
    imp::new(server)
        .serve(addr)
        .await
        .map_err(|e| anyhow::anyhow!("failed to start service: {}", e))
}
