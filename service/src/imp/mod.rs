use proto::{JobSubmission, StartJobResult, start_job_result, JobId, Result as ApiResult, StatusResult, JobOutput};

use futures::Stream;
use std::pin::Pin;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use uuid::Uuid;

type WorkerResult<T> = Result<Response<T>, Status>;

#[derive(Default)]
pub struct Worker;

#[tonic::async_trait]
impl proto::worker_server::Worker for Worker {
    async fn start_job(&self, _request: Request<JobSubmission>) -> WorkerResult<StartJobResult> {
        let job_id = Uuid::new_v4().to_string();
        Ok(Response::new(StartJobResult { result: Some(start_job_result::Result::Id(job_id)) }))
    }

    async fn stop_job(&self, _request: Request<JobId>) -> WorkerResult<ApiResult> {
        unimplemented!();
    }

    async fn query_status(&self, _request: Request<JobId>) -> WorkerResult<StatusResult> {
        unimplemented!();
    }

    type FetchOutputStream = Pin<Box<dyn Stream<Item = Result<JobOutput, Status>> + Send + Sync>>;

    async fn fetch_output(&self, _request: Request<JobId>) -> WorkerResult<Self::FetchOutputStream> {
        unimplemented!();
    }
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:10000".parse().unwrap();
    let server = Worker::default();

    Server::builder()
        .add_service(proto::worker_server::WorkerServer::new(server))
        .serve(addr)
        .await?;

    Ok(())
}
