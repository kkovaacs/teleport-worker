use std::convert::TryInto;
use std::io::{self, Cursor, Read, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use byteorder::{BigEndian, ReadBytesExt};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::mpsc::Sender;

use crate::record::{Record, RecordType};

/// A reader for reading structured log records from a file.
///
/// Sends a stream of log records through a channel and provides a way of waiting for new records
/// to be written into the file.
pub struct Reader {
    file: File,
    pos: u64,
    records: Sender<Record>,
    running: Arc<AtomicBool>,
}

// Arbitrary upper limit for reading at most this number of bytes from the file
// This has to be _at least_ as big as the maximum record size, which depends on the
// size of the chunks we're using for reading stdout and stderr.
const READ_CHUNK_SIZE: u64 = 1024 * 1024;

impl Reader {
    /// Constructs a new reader
    ///
    /// Opens the file at `path`.
    /// `records` is a `Sender` that will be used to return the log records.
    /// `running` is an atomic boolean that can be used to stop tailing of the log file.
    pub async fn new<P: AsRef<Path>>(
        path: P,
        records: Sender<Record>,
        running: Arc<AtomicBool>,
    ) -> io::Result<Self> {
        let file = File::open(path).await?;
        let pos = 0;
        Ok(Self {
            file,
            pos,
            records,
            running,
        })
    }

    /// Start reading records from the log file.
    ///
    /// This reads the whole file and starts waiting for new data to be appended to the log file.
    /// Returns if the reader is signalled through the `running` atomic boolean that the log
    /// writing the file has been destroyed and the whole file has been read.
    pub async fn start(&mut self) -> io::Result<()> {
        let mut buffer = Vec::new();
        loop {
            self.read(&mut buffer).await?;
            let eof_reached = buffer.len() < READ_CHUNK_SIZE as usize;
            self.read_records(&buffer).await?;

            // There's no simple API for waiting until a file gets appended, so here we sleep
            // for some time and re-try reading.
            // This value is arbitrary, and could be adjusted if throughput is too low with
            // 100 milliseconds.
            sleep(Duration::from_millis(100));

            if eof_reached && !self.running.load(Ordering::Acquire) {
                break;
            }
        }
        Ok(())
    }

    async fn read_records(&mut self, data: &[u8]) -> io::Result<()> {
        let mut data = Cursor::new(data);
        loop {
            match Self::read_record(&mut data).await {
                Ok(record) => {
                    self.pos += record.len() as u64;
                    if self.records.send(record).await.is_err() {
                        return Err(io::Error::new(io::ErrorKind::Other, "Receiver closed"));
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    async fn read_record<R: Read>(source: &mut R) -> io::Result<Record> {
        let record_type: RecordType = source.read_u8()?.try_into().unwrap();
        let len = source.read_u64::<BigEndian>()?;
        let mut data = vec![0; len as usize];
        source.read_exact(&mut data)?;
        Ok(Record { record_type, data })
    }

    async fn read(&mut self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.clear();
        self.file.seek(SeekFrom::Start(self.pos)).await?;
        (&mut self.file)
            .take(READ_CHUNK_SIZE)
            .read_to_end(buffer)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use tokio::sync::mpsc;
    use tokio_stream::{wrappers::ReceiverStream, StreamExt};

    #[test]
    fn test_chunk_sizes() {
        let record_header_size = Record {
            record_type: RecordType::Stdout,
            data: vec![],
        }
        .len();
        assert!(READ_CHUNK_SIZE as usize > crate::supervisor::READ_CHUNK_SIZE + record_header_size);
    }

    fn record() -> Vec<u8> {
        let data = b"deadbeef";
        let mut record = Vec::new();
        record.extend_from_slice(&(RecordType::Stdout as u8).to_be_bytes());
        record.extend_from_slice(&data.len().to_be_bytes());
        record.extend_from_slice(data);
        record
    }

    #[tokio::test]
    async fn test_read_record() {
        let mut input = Cursor::new(record());
        let record = Reader::read_record(&mut input).await.unwrap();
        assert_eq!(
            record,
            Record {
                record_type: RecordType::Stdout,
                data: b"deadbeef".to_vec()
            }
        );
    }

    #[tokio::test]
    async fn test_read_records() {
        let tempfile = NamedTempFile::new().unwrap();
        let (mut file, path) = tempfile.into_parts();

        let record_len = record().len();
        let records_data: Vec<u8> = record().into_iter().cycle().take(record_len * 4).collect();

        // write two complete records plus some partial data
        file.write_all(&records_data[..record_len * 2 + 3]).unwrap();
        file.sync_all().unwrap();

        let (tx, rx) = mpsc::channel(crate::log::READER_CHANNEL_BUFFER_CAPACITY);
        let running = Arc::new(AtomicBool::new(true));
        let mut reader = Reader::new(path, tx, Arc::clone(&running)).await.unwrap();

        tokio::task::spawn(async move {
            let mut buffer = Vec::new();
            reader.read(&mut buffer).await.unwrap();
            reader.read_records(&buffer).await.unwrap();

            file.write_all(&records_data[record_len * 2 + 3..]).unwrap();
            file.sync_all().unwrap();

            reader.read(&mut buffer).await.unwrap();
            reader.read_records(&buffer).await.unwrap();
        });

        let mut record_stream = ReceiverStream::new(rx);
        while let Some(record) = record_stream.next().await {
            assert_eq!(
                record,
                Record {
                    record_type: RecordType::Stdout,
                    data: b"deadbeef".to_vec()
                }
            );
        }
    }
}
