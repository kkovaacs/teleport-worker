use std::path::PathBuf;

use structopt::StructOpt;
use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Identity};

mod commands;

#[derive(StructOpt)]
#[structopt(about = "Worker service client")]
struct Options {
    #[structopt(long = "url", default_value = "https://[::1]:10000")]
    url: String,
    #[structopt(long = "cert", parse(from_os_str))]
    cert: PathBuf,
    #[structopt(long = "key", parse(from_os_str))]
    key: PathBuf,
    #[structopt(long = "ca-certificate", parse(from_os_str))]
    server_ca: PathBuf,
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(StructOpt)]
enum Command {
    Start {
        executable: String,
        arguments: Vec<String>,
    },
    Stop {
        id: String,
    },
    QueryStatus {
        id: String,
    },
    FetchOutput {
        id: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let options = Options::from_args();

    let server_ca_cert = tokio::fs::read(options.server_ca).await?;
    let server_ca_cert = Certificate::from_pem(server_ca_cert);

    let client_cert = tokio::fs::read(options.cert).await?;
    let client_key = tokio::fs::read(options.key).await?;
    let identity = Identity::from_pem(client_cert, client_key);

    let tls = ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(server_ca_cert)
        .identity(identity);

    let mut channel = Endpoint::from_shared(options.url.clone())?
        .tls_config(tls)?
        .connect()
        .await?;

    let result = match &options.cmd {
        Command::Start {
            executable,
            arguments,
        } => commands::start(&mut channel, executable, arguments).await,
        Command::Stop { id } => commands::stop(&mut channel, id.clone()).await,
        Command::QueryStatus { id } => commands::query_status(&mut channel, id.clone()).await,
        Command::FetchOutput { id } => {
            let stdout = tokio::io::stdout();
            let stderr = tokio::io::stderr();
            commands::fetch_output(&mut channel, id.clone(), stdout, stderr).await
        }
    };

    match result {
        Ok(output) => {
            if !output.is_empty() {
                println!("{}", output)
            }
        }
        Err(error) => eprintln!("{}", error),
    }

    Ok(())
}
