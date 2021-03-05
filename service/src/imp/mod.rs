use proto::worker_server::WorkerServer;
use proto::{
    start_job_result, JobId, JobOutput, JobSubmission, Result as ApiResult, StartJobResult,
    StatusResult,
};

use futures::Stream;
use std::pin::Pin;
use tonic::transport::server::{Router, Unimplemented};
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use uuid::Uuid;

mod handler;
mod jobs;

type WorkerResult<T> = Result<Response<T>, Status>;

#[derive(Default)]
pub struct Worker;

#[tonic::async_trait]
impl proto::worker_server::Worker for Worker {
    async fn start_job(&self, _request: Request<JobSubmission>) -> WorkerResult<StartJobResult> {
        let job_id = Uuid::new_v4().to_string();
        let start_job_result = StartJobResult {
            result: Some(start_job_result::Result::Id(job_id)),
        };
        Ok(Response::new(start_job_result))
    }

    async fn stop_job(&self, _request: Request<JobId>) -> WorkerResult<ApiResult> {
        unimplemented!();
    }

    async fn query_status(&self, _request: Request<JobId>) -> WorkerResult<StatusResult> {
        unimplemented!();
    }

    type FetchOutputStream = Pin<Box<dyn Stream<Item = Result<JobOutput, Status>> + Send + Sync>>;

    async fn fetch_output(
        &self,
        _request: Request<JobId>,
    ) -> WorkerResult<Self::FetchOutputStream> {
        unimplemented!();
    }
}

type Imp = Router<WorkerServer<Worker>, Unimplemented>;

pub async fn new() -> Imp {
    let server = Worker::default();

    let service = Server::builder().add_service(WorkerServer::new(server));

    service
}
