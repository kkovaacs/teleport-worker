use proto::worker_client::WorkerClient;
use proto::{start_job_result, status_result, JobId, JobSubmission, StopResult};
use service::imp;

use futures::FutureExt;
use tokio::sync::oneshot;
use tonic::Request;

#[tokio::test]
async fn test_job_submission() -> () {
    let (tx, rx) = oneshot::channel::<()>();

    let addr = "127.0.0.1:23485".parse().unwrap();
    let service_handle = tokio::spawn(async move {
        imp::new()
            .serve_with_shutdown(addr, rx.map(drop))
            .await
            .unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut client = WorkerClient::connect("http://127.0.0.1:23485")
        .await
        .unwrap();

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

    // check status after waiting some time for the process to exit
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
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
        dbg!(output);
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
