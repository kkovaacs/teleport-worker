use proto::{
    job_output::OutputType, start_job_result, status_result, worker_client::WorkerClient, JobId,
    JobOutput, JobSubmission, StartJobResult, StatusResult,
};

use anyhow::Result;
use tokio::io::AsyncWriteExt;
use tonic::{transport::Channel, Request};

pub(crate) async fn start(
    channel: &mut Channel,
    executable: &str,
    arguments: &[String],
) -> Result<String> {
    let mut client = WorkerClient::new(channel);
    let request = Request::new(JobSubmission {
        executable: executable.to_string(),
        arguments: arguments.to_vec(),
    });
    let response = client.start_job(request).await?.into_inner();
    match response {
        StartJobResult {
            result: Some(start_job_result::Result::Id(job_id)),
        } => Ok(job_id),
        StartJobResult {
            result: Some(start_job_result::Result::Error(error)),
        } => Err(anyhow::anyhow!("Job submission failed: {}", error)),
        _ => Err(anyhow::anyhow!(
            "Job submission failed: invalid response received"
        )),
    }
}

pub(crate) async fn stop(channel: &mut Channel, job_id: String) -> Result<String> {
    let mut client = WorkerClient::new(channel);
    let request = Request::new(JobId { id: job_id });
    let response = client.stop_job(request).await?.into_inner();
    if response.error.is_empty() {
        Ok("".to_owned())
    } else {
        Err(anyhow::anyhow!("Stopping job failed: {}", response.error))
    }
}

pub(crate) async fn query_status(channel: &mut Channel, job_id: String) -> Result<String> {
    let mut client = WorkerClient::new(channel);
    let request = Request::new(JobId { id: job_id });
    let response = client.query_status(request).await?.into_inner();
    match response {
        StatusResult {
            result: Some(status_result::Result::Running(_)),
        } => Ok("Running".to_owned()),
        StatusResult {
            result: Some(status_result::Result::Exited(status_result::Exited { exit_status })),
        } => Ok(format!("Exited with status {}", exit_status)),
        StatusResult {
            result: Some(status_result::Result::Stopped(_)),
        } => Ok("Stopped".to_owned()),
        StatusResult {
            result: Some(status_result::Result::Error(error)),
        } => Err(anyhow::anyhow!("Querying status failed: {}", error)),
        _ => Err(anyhow::anyhow!(
            "Querying status failed: invalid response received"
        )),
    }
}

pub(crate) async fn fetch_output<
    O: tokio::io::AsyncWrite + Unpin,
    E: tokio::io::AsyncWrite + Unpin,
>(
    channel: &mut Channel,
    job_id: String,
    mut stdout: O,
    mut stderr: E,
) -> Result<String> {
    let mut client = WorkerClient::new(channel);
    let request = Request::new(JobId { id: job_id });
    let mut stream = client.fetch_output(request).await?.into_inner();
    while let Some(JobOutput {
        output_type: Some(output),
    }) = stream.message().await?
    {
        match output {
            OutputType::Stdout(data) => stdout.write_all(&data).await?,
            OutputType::Stderr(data) => stderr.write_all(&data).await?,
        };
    }
    Ok("".to_owned())
}
