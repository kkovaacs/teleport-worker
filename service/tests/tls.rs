use futures::FutureExt;
use proto::{worker_client::WorkerClient, JobId};
use service::imp;
use std::time::Duration;
use tokio::sync::oneshot;
use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Identity};
use tonic::Request;

#[tokio::test]
async fn test_tls_auth_succeeds() -> () {
    let (tx, rx) = oneshot::channel::<()>();

    // this is an ugly hack here with ports -- I've found no API in tonic so that we
    // could bind to port zero and query the actual port later...
    let addr = "127.0.0.1:12342".parse().unwrap();
    let service_handle = tokio::spawn(async move {
        imp::new(imp::new_tls_server().unwrap())
            .serve_with_shutdown(addr, rx.map(drop))
            .await
            .unwrap();
    });

    let server_ca_cert = include_bytes!("../../data/pki/server-ca-cert.pem");
    let client_cert = include_bytes!("../../data/pki/user1-cert.pem");
    let client_key = include_bytes!("../../data/pki/user1-key-pkcs8.pem");

    let channel = tryhard::retry_fn(|| async {
        let server_ca_cert = Certificate::from_pem(&server_ca_cert);
        let identity = Identity::from_pem(&client_cert, &client_key);

        let tls = ClientTlsConfig::new()
            .domain_name("localhost")
            .ca_certificate(server_ca_cert)
            .identity(identity);

        let endpoint = Endpoint::from_static("https://127.0.0.1:12342")
            .tls_config(tls)
            .unwrap();

        endpoint.connect().await
    })
    .retries(10)
    .fixed_backoff(Duration::from_millis(100))
    .await
    .unwrap();

    let mut client = WorkerClient::new(channel);
    client
        .stop_job(Request::new(JobId {
            id: "bogus".to_owned(),
        }))
        .await
        .unwrap();

    drop(tx);
    service_handle.await.unwrap();
}

#[tokio::test]
async fn test_tls_auth_fails() -> () {
    let (tx, rx) = oneshot::channel::<()>();

    // this is an ugly hack here with ports -- I've found no API in tonic so that we
    // could bind to port zero and query the actual port later...
    let addr = "127.0.0.1:12343".parse().unwrap();
    let service_handle = tokio::spawn(async move {
        imp::new(imp::new_tls_server().unwrap())
            .serve_with_shutdown(addr, rx.map(drop))
            .await
            .unwrap();
    });

    let server_ca_cert = include_bytes!("../../data/pki/server-ca-cert.pem");

    // wrong identity (improper CA)
    let client_cert = include_bytes!("../../data/pki/server-cert.pem");
    let client_key = include_bytes!("../../data/pki/server-key-pkcs8.pem");

    let channel = tryhard::retry_fn(|| async {
        let server_ca_cert = Certificate::from_pem(&server_ca_cert);
        let identity = Identity::from_pem(&client_cert, &client_key);

        let tls = ClientTlsConfig::new()
            .domain_name("localhost")
            .ca_certificate(server_ca_cert)
            .identity(identity);

        let endpoint = Endpoint::from_static("https://127.0.0.1:12343")
            .tls_config(tls)
            .unwrap();

        endpoint.connect().await
    })
    .retries(10)
    .fixed_backoff(Duration::from_millis(100))
    .await
    // for some reason connect() succeeds even if the Identity we're using with TLS is incorrect
    .unwrap();

    let mut client = WorkerClient::new(channel);
    // expect request to fail after a "successful" connect()
    client
        .stop_job(Request::new(JobId {
            id: "bogus".to_owned(),
        }))
        .await
        .unwrap_err();

    drop(tx);
    service_handle.await.unwrap();
}
