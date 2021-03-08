use proto::worker_client::WorkerClient;
use proto::{
    job_output, start_job_result, status_result, JobId, JobOutput, JobSubmission, StopResult,
};
use service::imp;

use futures::FutureExt;
use std::time::Duration;
use tokio::sync::oneshot;
use tonic::{
    transport::{Certificate, ClientTlsConfig, Endpoint, Identity},
    Request,
};

#[tokio::test]
async fn test_job_submission() -> () {
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

    // check failing submission
    let res = client
        .start_job(Request::new(JobSubmission {
            executable: "/bin/nonexistent".to_owned(),
            arguments: vec![],
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        res.result.unwrap(),
        start_job_result::Result::Error(
            "failed to start job: No such file or directory (os error 2)".to_string()
        )
    );

    // successful submission
    let res = client
        .start_job(Request::new(JobSubmission {
            executable: "/bin/echo".to_owned(),
            arguments: ["test".to_string()].to_vec(),
        }))
        .await
        .unwrap()
        .into_inner();
    let job_id = match res.result.unwrap() {
        start_job_result::Result::Id(id) => id,
        _ => panic!("expected an id"),
    };

    // check status
    let res = client
        .query_status(Request::new(JobId { id: job_id.clone() }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        res.result.unwrap(),
        status_result::Result::Exited(status_result::Exited { exit_status: 0 })
    );

    // check output
    let mut stream = client
        .fetch_output(Request::new(JobId { id: job_id.clone() }))
        .await
        .unwrap()
        .into_inner();
    while let Some(output) = stream.message().await.unwrap() {
        assert_eq!(
            output,
            JobOutput {
                output_type: Some(job_output::OutputType::Stdout(b"test\n".to_vec()))
            }
        )
    }

    let res = client
        .stop_job(Request::new(JobId { id: job_id.clone() }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        res,
        StopResult {
            error: "".to_string()
        }
    );

    let res = client
        .stop_job(Request::new(JobId { id: job_id.clone() }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        res,
        StopResult {
            error: "no such job id".to_string()
        }
    );

    drop(tx);
    service_handle.await.unwrap();
}

#[tokio::test]
async fn test_authorization() -> () {
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
    let user1_cert = include_bytes!("../../data/pki/user1-cert.pem");
    let user1_key = include_bytes!("../../data/pki/user1-key-pkcs8.pem");

    let user1_channel = tryhard::retry_fn(|| async {
        let server_ca_cert = Certificate::from_pem(&server_ca_cert);
        let identity = Identity::from_pem(&user1_cert, &user1_key);

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
    .unwrap();

    let mut user1_client = WorkerClient::new(user1_channel);

    // successful submission
    let res = user1_client
        .start_job(Request::new(JobSubmission {
            executable: "/bin/echo".to_owned(),
            arguments: ["test".to_string()].to_vec(),
        }))
        .await
        .unwrap()
        .into_inner();
    let job_id = match res.result.unwrap() {
        start_job_result::Result::Id(id) => id,
        _ => panic!("expected an id"),
    };

    // check status
    let res = user1_client
        .query_status(Request::new(JobId { id: job_id.clone() }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        res.result.unwrap(),
        status_result::Result::Exited(status_result::Exited { exit_status: 0 })
    );

    // check with another user
    let user2_cert = include_bytes!("../../data/pki/user2-cert.pem");
    let user2_key = include_bytes!("../../data/pki/user2-key-pkcs8.pem");

    let user2_channel = tryhard::retry_fn(|| async {
        let server_ca_cert = Certificate::from_pem(&server_ca_cert);
        let identity = Identity::from_pem(&user2_cert, &user2_key);

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
    .unwrap();

    let mut user2_client = WorkerClient::new(user2_channel);

    // check status
    let res = user2_client
        .query_status(Request::new(JobId { id: job_id.clone() }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        res.result.unwrap(),
        status_result::Result::Error("no such job id".to_owned())
    );

    drop(tx);
    service_handle.await.unwrap();
}
