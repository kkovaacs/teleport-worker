use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::imp::jobs::{Job, JobId, Jobs};
use library::{Log, Supervisor};

pub struct Handler {
    jobs: Mutex<Jobs>,
}

impl Handler {
    async fn start_job(&mut self, executable: &str, arguments: &[String]) -> Result<JobId> {
        let (job_id, job) = Job::new(executable, arguments)?;
        self.jobs.lock().await.insert(job_id, job);
        Ok(job_id)
    }
}
