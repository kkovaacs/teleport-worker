use anyhow::Result;
use tokio::sync::{mpsc, Mutex};

use crate::imp::identity::Identity;
use crate::imp::jobs::{Job, JobId, JobKey, Jobs, Status};
use library::Record;

/// Implements the actual operations and stores service state (jobs)
///
/// This is independent of the GRPC interface so it could be tested and re-used for other APIs.
#[derive(Default)]
pub struct Handler {
    jobs: Mutex<Jobs>,
}

impl Handler {
    pub async fn start_job(
        &self,
        owner: Identity,
        executable: &str,
        arguments: &[String],
    ) -> Result<JobId> {
        let (job_id, mut job) = Job::new(executable, arguments)?;
        job.start().await?;
        self.jobs.lock().await.insert(JobKey(owner, job_id), job);
        Ok(job_id)
    }

    pub async fn stop_job(&self, job_key: &JobKey) -> Result<()> {
        let job = self.jobs.lock().await.remove(job_key);
        match job {
            Some(mut job) => job.stop(),
            None => Err(anyhow::anyhow!("no such job id")),
        }
    }

    pub async fn query_status(&self, job_key: &JobKey) -> Result<Status> {
        let mut jobs = self.jobs.lock().await;
        let job = jobs.get_mut(job_key);
        match job {
            Some(job) => Ok(job.status()),
            None => Err(anyhow::anyhow!("no such job id")),
        }
    }

    pub async fn fetch_output(&self, job_key: &JobKey) -> Result<mpsc::Receiver<Record>> {
        let mut jobs = self.jobs.lock().await;
        let job = jobs.get_mut(job_key);
        match job {
            Some(job) => {
                let record_receiver = job.fetch_output().await?;
                Ok(record_receiver)
            }
            None => Err(anyhow::anyhow!("no such job id")),
        }
    }
}
