use library::{Log, Supervisor};

use anyhow::Result;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type Jobs = HashMap<JobId, Job>;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobId(uuid::Uuid);

impl JobId {
    pub fn new() -> Self {
        JobId(uuid::Uuid::new_v4())
    }
}

pub struct Job {
    supervisor: Arc<Supervisor<Log>>,
    log: Arc<Mutex<Log>>,
}

impl Job {
    pub fn new(executable: &str, arguments: &[String]) -> Result<(JobId, Job)> {
        let job_id = JobId::new();
        let log = Arc::new(Mutex::new(Log::new()?));
        let supervisor = Arc::new(Supervisor::new(executable, arguments, Arc::clone(&log))?);
        Ok((job_id, Job { supervisor, log }))
    }

    pub async fn start(&self) -> Result<()> {
        let supervisor = Arc::clone(&self.supervisor);
        tokio::task::spawn(async move { supervisor.monitor().await });
        Ok(())
    }
}
