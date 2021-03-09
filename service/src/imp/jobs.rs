use library::{Log, Supervisor};

use anyhow::Result;
use std::collections::HashMap;
use std::hash::Hash;
use std::process::ExitStatus;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

use super::identity;

pub type Jobs = HashMap<JobKey, Job>;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct JobKey(pub identity::Identity, pub JobId);

/// A unique job identifier -- now just a UUID
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobId(uuid::Uuid);

impl JobId {
    pub fn new() -> Self {
        JobId(uuid::Uuid::new_v4())
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for JobId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let uuid = uuid::Uuid::parse_str(s)?;
        Ok(JobId(uuid))
    }
}

enum JobState {
    Created,
    Running {
        stop_signal: oneshot::Sender<()>,
        exit_status_receiver: oneshot::Receiver<ExitStatus>,
    },
    Exited {
        exit_status: i32,
    },
    Stopped,
}

/// Represents a job
///
/// Contains state information plus a `Supervisor` for executing the process and a `Log` to
/// store its output.
pub struct Job {
    state: JobState,
    supervisor: Supervisor,
    log: Arc<Mutex<Log>>,
}

pub enum Status {
    Running,
    Exited { exit_status: i32 },
    Stopped,
}

impl Job {
    pub fn new(executable: &str, arguments: &[String]) -> Result<(JobId, Job)> {
        let job_id = JobId::new();
        let log = Arc::new(Mutex::new(Log::new()?));
        let supervisor = Supervisor::new(executable, arguments)?;
        Ok((
            job_id,
            Job {
                state: JobState::Created,
                supervisor,
                log,
            },
        ))
    }

    pub async fn start(&mut self) -> Result<()> {
        match self.state {
            JobState::Created => {
                let (stop_signal, stop_signal_receiver) = oneshot::channel();
                let (exit_status_sender, exit_status_receiver) = oneshot::channel();
                self.supervisor.monitor(
                    Arc::clone(&self.log),
                    stop_signal_receiver,
                    exit_status_sender,
                    Box::new(library::NoOpController {}),
                )?;
                self.state = JobState::Running {
                    stop_signal,
                    exit_status_receiver,
                };
                Ok(())
            }
            _ => Err(anyhow::anyhow!(
                "cannot start a job that has already been started"
            )),
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        self.update_status();

        let state = std::mem::replace(&mut self.state, JobState::Stopped);
        match state {
            JobState::Running {
                stop_signal,
                exit_status_receiver: _,
            } => {
                // we can't do much here if send fails -- but that probably means that the
                // receiver has been closed so it's safe to assume that the process has
                // already stopped
                let _ = stop_signal.send(());
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn status(&mut self) -> Status {
        self.update_status();

        match &self.state {
            JobState::Created
            | JobState::Running {
                stop_signal: _,
                exit_status_receiver: _,
            } => Status::Running,
            JobState::Stopped => Status::Stopped,
            JobState::Exited { exit_status } => Status::Exited {
                exit_status: *exit_status,
            },
        }
    }

    fn update_status(&mut self) {
        let new_state = match &mut self.state {
            JobState::Running {
                stop_signal: _,
                exit_status_receiver,
            } => match exit_status_receiver.try_recv() {
                Ok(exit_status) => match exit_status.code() {
                    Some(code) => Some(JobState::Exited { exit_status: code }),
                    None => Some(JobState::Stopped),
                },
                Err(oneshot::error::TryRecvError::Closed) => Some(JobState::Stopped),
                Err(_) => None,
            },
            _ => None,
        };
        if let Some(new_state) = new_state {
            self.state = new_state;
        }
    }

    pub async fn fetch_output(&mut self) -> Result<mpsc::Receiver<library::Record>> {
        let (mut reader, rx) = self.log.lock().await.reader().await?;
        tokio::task::spawn(async move {
            // ideally we would at least log errors here -- omitted in the prototype
            let _ = reader.start().await;
        });
        Ok(rx)
    }
}
