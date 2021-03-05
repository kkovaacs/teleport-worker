use std::io;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex};

use crate::log::LogWriter;
use crate::record::RecordType;

/// [`Supervisor`] represents a running process plus some tasks reading its output and writing
/// it to a [`Log`].
pub struct Supervisor<L: LogWriter> {
    executable: String,
    arguments: Vec<String>,
    log: Arc<Mutex<L>>,
}

// Arbitrary upper limit for reading at most this number of bytes from stderr and stdout.
// This (plus log record header) is effectively an upper limit of log record size and
// the log reader code assumes that its chunk size for reading the log file is larger than
// the largest record.
pub const READ_CHUNK_SIZE: usize = 1024;

impl<L: LogWriter + Send + 'static> Supervisor<L> {
    pub fn new(executable: &str, args: &[String], log: Arc<Mutex<L>>) -> io::Result<Self> {
        let arguments = args.to_vec();
        Ok(Supervisor {
            executable: executable.to_owned(),
            arguments,
            log,
        })
    }

    /// Start the subprocess and wait for it to exit.
    ///
    /// Process stdout and stderr is written into the log.
    ///
    /// Returns the exit status of the process.
    pub fn monitor(
        &self,
        stop_signal_receiver: oneshot::Receiver<()>,
        exit_status_sender: oneshot::Sender<ExitStatus>,
    ) -> io::Result<()> {
        let mut command = Command::new(self.executable.clone());
        command
            .args(self.arguments.clone())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn()?;

        async fn read_output<A: AsyncRead + Unpin, L: LogWriter>(
            log: Arc<Mutex<L>>,
            record_type: RecordType,
            output: Option<A>,
        ) -> io::Result<()> {
            if let Some(mut output) = output {
                loop {
                    let mut buffer = [0u8; READ_CHUNK_SIZE];
                    match output.read(&mut buffer).await {
                        Ok(0) => {
                            // EOF
                            break;
                        }
                        Ok(n) => log.lock().await.write_record(record_type, &buffer[..n])?,
                        Err(e) => return Err(e),
                    }
                }
            }
            Ok(())
        }

        let stdout_future = read_output(
            Arc::clone(&self.log),
            RecordType::Stdout,
            child.stdout.take(),
        );
        let stderr_future = read_output(
            Arc::clone(&self.log),
            RecordType::Stderr,
            child.stderr.take(),
        );

        async fn wait_or_kill_child(
            mut child: Child,
            stop_signal: oneshot::Receiver<()>,
        ) -> io::Result<ExitStatus> {
            tokio::select! {
                exit_status = child.wait() => {exit_status},
                _ = stop_signal => { child.start_kill().unwrap(); child.wait().await }
            }
        }

        let waiter = wait_or_kill_child(child, stop_signal_receiver);

        tokio::task::spawn(async move {
            let (exit_status, _, _) = tokio::join!(waiter, stdout_future, stderr_future);

            let _ = exit_status_sender.send(exit_status.unwrap());
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::Log;
    use crate::record::Record;

    #[tokio::test]
    async fn test_supervisor_output() {
        let log = Arc::new(Mutex::new(Log::new().unwrap()));
        let s = Supervisor::new(
            "/bin/sh",
            &[
                "-c".to_string(),
                "echo stdout; echo stderr 1>&2; exit 1".to_string(),
            ],
            Arc::clone(&log),
        )
        .unwrap();

        let (_stop_signal, stop_signal_receiver) = oneshot::channel();
        let (exit_status_sender, exit_status_receiver) = oneshot::channel();
        s.monitor(stop_signal_receiver, exit_status_sender).unwrap();

        let exit_status = exit_status_receiver.await.unwrap();
        assert_eq!(exit_status.code(), Some(1));
        drop(s);

        let (mut reader, mut rx) = log.lock().await.reader().await.unwrap();
        // drop log so that readers will stop
        drop(log);

        tokio::task::spawn(async move {
            reader.start().await.unwrap();
        });

        assert_eq!(
            rx.recv().await.unwrap(),
            Record {
                record_type: RecordType::Stdout,
                data: b"stdout\n".to_vec()
            }
        );
        assert_eq!(
            rx.recv().await.unwrap(),
            Record {
                record_type: RecordType::Stderr,
                data: b"stderr\n".to_vec()
            }
        );
    }

    #[tokio::test]
    async fn test_stop_signal() {
        let log = Arc::new(Mutex::new(Log::new().unwrap()));
        let s = Supervisor::new("/bin/sleep", &["100".to_string()], Arc::clone(&log)).unwrap();

        let (stop_signal, stop_signal_receiver) = oneshot::channel();
        let (exit_status_sender, exit_status_receiver) = oneshot::channel();
        s.monitor(stop_signal_receiver, exit_status_sender).unwrap();
        stop_signal.send(());
        let exit_status = exit_status_receiver.await.unwrap();
        assert!(!exit_status.success());
    }

    #[tokio::test]
    async fn test_invalid_executable() {
        let log = Arc::new(Mutex::new(Log::new().unwrap()));
        let s = Supervisor::new("/bin/nonexistent", &[], Arc::clone(&log)).unwrap();
        let (_stop_signal, stop_signal_receiver) = oneshot::channel();
        let (exit_status_sender, exit_status_receiver) = oneshot::channel();
        assert!(s.monitor(stop_signal_receiver, exit_status_sender).is_err());
    }
}
