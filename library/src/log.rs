use std::fs::File;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tempfile::{NamedTempFile, TempPath};
use tokio::sync::mpsc;

use crate::log_reader::Reader;
use crate::record::{Record, RecordType};

pub trait LogWriter {
    fn write_record(&mut self, record_type: RecordType, data: &[u8]) -> io::Result<()>;
    fn stop(&self);
}

/// [`Log`] represents a file on the filesystem containing structured records of process output.
///
/// Logs are stored in a named temporary file which is deleted when [`Log`] is dropped.
///
/// On-disk format is simple: the file contains a sequence of records, where each record consists
/// of a header plus the binary data it contains. The header is:
///  * record type (1 byte): stdout or stderr
///  * data length (8 bytes): u64 with length of data
pub struct Log {
    file: File,
    path: TempPath,
    running: Arc<AtomicBool>,
}

// The log reader uses a channel to return log records to consumers. To have an upper bound on
// the number of records already produced by the reader but not yet consumed we use a bounded
// channel with an arbitrary capacity.
// This provides some way to decouple the async task reading the log file and the task actually
// processing the records while making sure that flow control still works.
pub(crate) const READER_CHANNEL_BUFFER_CAPACITY: usize = 10;

impl Log {
    pub fn new() -> io::Result<Self> {
        let tempfile = NamedTempFile::new()?;
        let (file, path) = tempfile.into_parts();
        let running = Arc::new(AtomicBool::new(true));
        Ok(Self {
            file,
            path,
            running,
        })
    }

    /// Return a reader that can be used to read records from the log file.
    pub async fn reader(&self) -> io::Result<(Reader, mpsc::Receiver<Record>)> {
        let (tx, rx) = mpsc::channel(READER_CHANNEL_BUFFER_CAPACITY);
        let reader = Reader::new(&self.path, tx, Arc::clone(&self.running)).await?;
        Ok((reader, rx))
    }
}

impl LogWriter for Log {
    fn write_record(&mut self, record_type: RecordType, data: &[u8]) -> io::Result<()> {
        let mut header: Vec<u8> = Vec::with_capacity(crate::record::HEADER_LENGTH);
        header.extend_from_slice(&(record_type as u8).to_be_bytes());
        header.extend_from_slice(&(data.len() as u64).to_be_bytes());
        self.file.write_all(&header)?;
        self.file.write_all(data)
    }

    /// Signal readers that no more records will be written
    fn stop(&self) {
        self.running.store(false, Ordering::Release);
    }
}

impl Drop for Log {
    fn drop(&mut self) {
        self.stop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_record() {
        let mut log = Log::new().unwrap();
        let data = b"deadbeef";
        log.write_record(RecordType::Stdout, data).unwrap();
        let file_contents = std::fs::read(&log.path).unwrap();
        let mut expected_contents = Vec::new();
        expected_contents.extend_from_slice(&(RecordType::Stdout as u8).to_be_bytes());
        expected_contents.extend_from_slice(&data.len().to_be_bytes());
        expected_contents.extend_from_slice(data);
        assert_eq!(file_contents, expected_contents)
    }

    #[tokio::test]
    async fn test_log_write_and_read() {
        let mut log = Log::new().unwrap();
        log.write_record(RecordType::Stdout, b"hello stdout!")
            .unwrap();
        log.write_record(RecordType::Stderr, b"hello stderr!")
            .unwrap();

        let (mut reader, mut rx) = log.reader().await.unwrap();

        tokio::task::spawn(async move {
            reader.start().await.unwrap();
        });

        assert_eq!(
            rx.recv().await.unwrap(),
            Record {
                record_type: RecordType::Stdout,
                data: b"hello stdout!".to_vec()
            }
        );
        assert_eq!(
            rx.recv().await.unwrap(),
            Record {
                record_type: RecordType::Stderr,
                data: b"hello stderr!".to_vec()
            }
        );
        drop(log);
        assert_eq!(rx.recv().await, None);
    }
}
