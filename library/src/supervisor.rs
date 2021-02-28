use std::ffi::OsStr;
use std::io;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::log::LogWriter;
use crate::record::RecordType;

/// [`Supervisor`] represents a running process plus some tasks reading its output and writing
/// it to a [`Log`].
pub struct Supervisor<L: LogWriter> {
    command: Command,
    log: Arc<Mutex<L>>,
}

// Arbitrary upper limit for reading at most this number of bytes from stderr and stdout.
// This (plus log record header) is effectively an upper limit of log record size and
// the log reader code assumes that its chunk size for reading the log file is larger than
// the largest record.
pub const READ_CHUNK_SIZE: usize = 1024;

impl<L: LogWriter> Supervisor<L> {
    pub fn new<S, I, A>(executable: S, args: I, log: Arc<Mutex<L>>) -> io::Result<Self>
    where
        S: AsRef<OsStr>,
        I: IntoIterator<Item = A>,
        A: AsRef<OsStr>,
    {
        let mut command = Command::new(executable);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        Ok(Supervisor { command, log })
    }

    /// Start the subprocess and wait for it to exit.
    ///
    /// Process stdout and stderr is written into the log.
    ///
    /// Returns the exit status of the process.
    pub async fn monitor(&mut self) -> io::Result<ExitStatus> {
        let mut child = self.command.spawn()?;

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

        let (exit_status, _, _) = tokio::join!(child.wait(), stdout_future, stderr_future);

        exit_status
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
        let mut s = Supervisor::new(
            "/bin/sh",
            &["-c", "echo stdout; echo stderr 1>&2; exit 1"],
            Arc::clone(&log),
        )
        .unwrap();
        let exit_status = s.monitor().await.unwrap();
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
}
