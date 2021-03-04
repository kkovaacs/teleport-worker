mod imp;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    imp::run().await
}
