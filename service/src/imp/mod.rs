use proto::worker_server::WorkerServer;
use proto::{
    job_output, start_job_result, status_result, JobId, JobOutput, JobSubmission, StartJobResult,
    StatusResult, StopResult,
};

use futures::Stream;
use library::RecordType;
use std::pin::Pin;
use tokio::sync::mpsc;
use tonic::transport::{
    server::{Router, Unimplemented},
    Certificate, Identity, Server, ServerTlsConfig,
};
use tonic::{Request, Response, Status};

use self::jobs::JobKey;

pub mod handler;
mod identity;
pub mod jobs;

type WorkerResult<T> = Result<Response<T>, Status>;

pub struct Worker {
    handler: handler::Handler,
}

/// This is the implementation of the GRPC service
///
/// RPC arguments are validated/parsed here, but it's the `Handler` that implements the
/// actual operations so it's independent of GRPC.
/// Results returned by the handler are then converted into the appropriate protobuf
/// messages.
#[tonic::async_trait]
impl proto::worker_server::Worker for Worker {
    async fn start_job(&self, request: Request<JobSubmission>) -> WorkerResult<StartJobResult> {
        // ideally this could be done as a middleware, but I haven't found a way to implement it with tonic...
        let username = get_username_or_fail(&request)?;

        let job_submission = request.get_ref();
        let start_job_result = match self
            .handler
            .start_job(
                username,
                &job_submission.executable,
                &job_submission.arguments,
            )
            .await
        {
            Ok(job_id) => StartJobResult {
                result: Some(start_job_result::Result::Id(job_id.to_string())),
            },
            Err(e) => StartJobResult {
                result: Some(start_job_result::Result::Error(format!(
                    "failed to start job: {}",
                    e.to_string()
                ))),
            },
        };
        Ok(Response::new(start_job_result))
    }

    async fn stop_job(&self, request: Request<JobId>) -> WorkerResult<StopResult> {
        // ideally this could be done as a middleware, but I haven't found a way to implement it with tonic...
        let username = get_username_or_fail(&request)?;

        let id = &request.get_ref().id;
        let job_id: jobs::JobId = match id.parse() {
            Ok(id) => id,
            Err(_) => {
                return Ok(Response::new(StopResult {
                    error: format!("invalid id: {}", id),
                }))
            }
        };

        let job_key = JobKey(username, job_id);
        let stop_job_result = match self.handler.stop_job(&job_key).await {
            Ok(()) => StopResult {
                error: "".to_string(),
            },
            Err(e) => StopResult {
                error: e.to_string(),
            },
        };

        Ok(Response::new(stop_job_result))
    }

    async fn query_status(&self, request: Request<JobId>) -> WorkerResult<StatusResult> {
        // ideally this could be done as a middleware, but I haven't found a way to implement it with tonic...
        let username = get_username_or_fail(&request)?;

        let id = &request.get_ref().id;
        let job_id: jobs::JobId = match id.parse() {
            Ok(id) => id,
            Err(_) => {
                return Ok(Response::new(StatusResult {
                    result: Some(status_result::Result::Error(format!("invalid id: {}", id))),
                }))
            }
        };

        let job_key = JobKey(username, job_id);
        let result = match self.handler.query_status(&job_key).await {
            Ok(status) => match status {
                jobs::Status::Running => status_result::Result::Running(status_result::Running {}),
                jobs::Status::Exited { exit_status } => {
                    status_result::Result::Exited(status_result::Exited { exit_status })
                }
                jobs::Status::Stopped => status_result::Result::Stopped(status_result::Stopped {}),
            },
            Err(e) => status_result::Result::Error(e.to_string()),
        };
        Ok(Response::new(StatusResult {
            result: Some(result),
        }))
    }

    type FetchOutputStream = Pin<Box<dyn Stream<Item = Result<JobOutput, Status>> + Send + Sync>>;

    async fn fetch_output(&self, request: Request<JobId>) -> WorkerResult<Self::FetchOutputStream> {
        // ideally this could be done as a middleware, but I haven't found a way to implement it with tonic...
        let username = get_username_or_fail(&request)?;

        // use a bounded channel here to decouple producer/consumer, the capacity should be
        // small enough so that we do not store too much output in memory
        let (tx, rx) = mpsc::channel(10);

        let id = &request.get_ref().id;
        let job_id: jobs::JobId = match id.parse() {
            Ok(id) => id,
            Err(_) => {
                return Ok(Response::new(Box::pin(
                    tokio_stream::wrappers::ReceiverStream::new(rx),
                )))
            }
        };

        let job_key = JobKey(username, job_id);
        match self.handler.fetch_output(&job_key).await {
            Ok(mut record_receiver) => {
                tokio::spawn(async move {
                    // convert log records to JobOutput messages
                    while let Some(record) = record_receiver.recv().await {
                        let job_output = match &record.record_type {
                            RecordType::Stdout => JobOutput {
                                output_type: Some(job_output::OutputType::Stdout(record.data)),
                            },
                            RecordType::Stderr => JobOutput {
                                output_type: Some(job_output::OutputType::Stderr(record.data)),
                            },
                        };
                        if tx.send(Ok(job_output)).await.is_err() {
                            break;
                        }
                    }
                });
            }
            Err(_) => {}
        }

        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        )))
    }
}

fn get_username_or_fail<T>(request: &Request<T>) -> Result<identity::Identity, Status> {
    identity::get_username_from_request(&request)
        .ok_or(Status::unauthenticated("no peer identity found"))
}

type Imp = Router<WorkerServer<Worker>, Unimplemented>;

pub fn new(mut server: Server) -> Imp {
    let worker = Worker {
        handler: handler::Handler::default(),
    };

    let service = server.add_service(WorkerServer::new(worker));

    service
}

pub fn new_tls_server() -> Result<Server, tonic::transport::Error> {
    let cert = include_bytes!("../../../data/pki/server-cert.pem");
    let key = include_bytes!("../../../data/pki/server-key-pkcs8.pem");
    let identity = Identity::from_pem(cert, key);

    let user_ca_cert = include_bytes!("../../../data/pki/user-ca-cert.pem");
    let user_ca_cert = Certificate::from_pem(user_ca_cert);

    let tls_config = ServerTlsConfig::new()
        .identity(identity)
        .client_ca_root(user_ca_cert);
    Server::builder().tls_config(tls_config)
}
