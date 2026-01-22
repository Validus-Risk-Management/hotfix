use crate::store::{MessageStore, Result, StoreError};
use anyhow::Context;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Metadata for a stored message, tracking its position and size in the body file
#[derive(Debug, Clone)]
struct MessageDef {
    offset: u64,
    size: usize,
}

/// File-based message store implementation.
///
/// Uses multiple files for storage:
/// - `.body`: Append-only file containing raw message data
/// - `.header`: Index file mapping sequence numbers to message positions
/// - `.seqnums`: Stores sender and target sequence numbers
/// - `.session`: Stores session creation time
pub struct FileStore {
    base_path: PathBuf,
    body_file: BufWriter<File>,
    header_file: BufWriter<File>,
    seqnums_file: File,
    sender_seq_number: u64,
    target_seq_number: u64,
    creation_time: DateTime<Utc>,
    message_index: HashMap<u64, MessageDef>,
    current_body_offset: u64,
}

impl FileStore {
    pub fn new(directory: impl AsRef<Path>, name: &str) -> anyhow::Result<Self> {
        let base_path = directory.as_ref().join(name);
        std::fs::create_dir_all(directory)?;

        let body_path = base_path.with_extension("body");
        let header_path = base_path.with_extension("header");
        let seqnums_path = base_path.with_extension("seqnums");

        let creation_time = Self::get_or_create_session_time(&base_path)?;
        let (sender_seq_number, target_seq_number) = Self::read_initial_seqnums(&base_path)?;

        // open or create body and header files
        let body_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&body_path)?;
        let current_body_offset = body_file.metadata()?.len();
        let body_file = BufWriter::new(body_file);

        let header_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&header_path)?;
        let header_file = BufWriter::new(header_file);

        let seqnums_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&seqnums_path)?;

        // load existing message index from header file
        let message_index = Self::load_message_index(&header_path)?;

        Ok(Self {
            base_path,
            body_file,
            header_file,
            seqnums_file,
            sender_seq_number,
            target_seq_number,
            creation_time,
            message_index,
            current_body_offset,
        })
    }

    /// Retrieves the session creation time from the session file.
    ///
    /// It initialises the session file if it doesn't exist.
    fn get_or_create_session_time(base_path: &Path) -> anyhow::Result<DateTime<Utc>> {
        let session_path = base_path.with_extension("session");
        let session_time = if session_path.exists() {
            let content = std::fs::read_to_string(&session_path)?;
            content.trim().parse::<DateTime<Utc>>()?
        } else {
            let now = Utc::now();
            std::fs::write(&session_path, now.to_rfc3339())?;
            now
        };

        Ok(session_time)
    }

    /// Retrieves the sequence numbers from the seqnums file.
    ///
    /// It defaults to `(0, 0)` if the file doesn't exist or if it's empty.
    fn read_initial_seqnums(base_path: &Path) -> anyhow::Result<(u64, u64)> {
        let seqnums_path = base_path.with_extension("seqnums");
        let (sender_seq_number, target_seq_number) = if seqnums_path.exists() {
            let content =
                std::fs::read_to_string(&seqnums_path).context("failed to read seqnums file")?;
            if content.trim().is_empty() {
                (0u64, 0u64)
            } else {
                Self::parse_seqnums(&content)?
            }
        } else {
            (0u64, 0u64)
        };

        Ok((sender_seq_number, target_seq_number))
    }

    fn parse_seqnums(content: &str) -> anyhow::Result<(u64, u64)> {
        let parts: Vec<&str> = content.trim().split(':').map(|s| s.trim()).collect();
        if parts.len() != 2 {
            anyhow::bail!("invalid seqnums format");
        }
        let sender = parts[0]
            .parse::<u64>()
            .context("failed to parse sender sequence number")?;
        let target = parts[1]
            .parse::<u64>()
            .context("failed to parse target sequence number")?;
        Ok((sender, target))
    }

    fn load_message_index(header_path: &Path) -> anyhow::Result<HashMap<u64, MessageDef>> {
        let mut index = HashMap::new();

        if !header_path.exists() {
            return Ok(index);
        }

        let file = File::open(header_path).context("failed to open header file for reading")?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line.context("failed to read header line")?;
            let parts: Vec<&str> = line.trim().split(',').collect();
            if parts.len() != 3 {
                continue;
            }

            if let (Ok(seq_num), Ok(offset), Ok(size)) = (
                parts[0].parse::<u64>(),
                parts[1].parse::<u64>(),
                parts[2].parse::<usize>(),
            ) {
                index.insert(seq_num, MessageDef { offset, size });
            }
        }

        Ok(index)
    }

    fn write_seqnums_with(&mut self, sender: u64, target: u64) -> std::io::Result<()> {
        self.seqnums_file.seek(SeekFrom::Start(0))?;
        self.seqnums_file.set_len(0)?;
        write!(self.seqnums_file, "{:020} : {:020}", sender, target)?;
        self.seqnums_file.flush()?;
        Ok(())
    }

    fn write_message(&mut self, sequence_number: u64, message: &[u8]) -> std::io::Result<()> {
        let msg_size = message.len();
        let offset = self.current_body_offset;

        // write the message itself
        self.body_file.write_all(message)?;
        self.body_file.flush()?;

        // write the offset to the header file
        writeln!(
            self.header_file,
            "{},{},{}",
            sequence_number, offset, msg_size
        )?;
        self.header_file.flush()?;

        self.message_index.insert(
            sequence_number,
            MessageDef {
                offset,
                size: msg_size,
            },
        );
        self.current_body_offset += msg_size as u64;

        Ok(())
    }

    fn perform_reset(&mut self) -> std::io::Result<()> {
        self.body_file.flush()?;
        self.header_file.flush()?;

        // remove all files
        let body_path = self.base_path.with_extension("body");
        let header_path = self.base_path.with_extension("header");
        let seqnums_path = self.base_path.with_extension("seqnums");
        let session_path = self.base_path.with_extension("session");

        if body_path.exists() {
            std::fs::remove_file(&body_path)?;
        }
        if header_path.exists() {
            std::fs::remove_file(&header_path)?;
        }
        if seqnums_path.exists() {
            std::fs::remove_file(&seqnums_path)?;
        }
        if session_path.exists() {
            std::fs::remove_file(&session_path)?;
        }

        // reset in-memory state
        self.sender_seq_number = 0;
        self.target_seq_number = 0;
        self.creation_time = Utc::now();
        self.message_index.clear();
        self.current_body_offset = 0;

        // recreate files
        let now = Utc::now();
        std::fs::write(&session_path, now.to_rfc3339())?;

        let body_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&body_path)?;
        self.body_file = BufWriter::new(body_file);

        let header_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&header_path)?;
        self.header_file = BufWriter::new(header_file);

        self.seqnums_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&seqnums_path)?;

        self.creation_time = now;

        Ok(())
    }

    fn read_messages(&self, begin: usize, end: usize) -> std::io::Result<Vec<Vec<u8>>> {
        let mut messages = Vec::with_capacity(end - begin + 1);

        let body_path = self.base_path.with_extension("body");
        let mut body_file = File::open(body_path)?;

        for seq_num in begin..=end {
            if let Some(msg_def) = self.message_index.get(&(seq_num as u64)) {
                body_file.seek(SeekFrom::Start(msg_def.offset))?;

                let mut buffer = vec![0u8; msg_def.size];
                body_file.read_exact(&mut buffer)?;

                messages.push(buffer);
            }
        }

        Ok(messages)
    }
}

#[async_trait::async_trait]
impl MessageStore for FileStore {
    async fn add(&mut self, sequence_number: u64, message: &[u8]) -> Result<()> {
        self.write_message(sequence_number, message)
            .map_err(|err| StoreError::PersistMessage {
                sequence_number,
                source: err.into(),
            })
    }

    async fn get_slice(&self, begin: usize, end: usize) -> Result<Vec<Vec<u8>>> {
        self.read_messages(begin, end)
            .map_err(|e| StoreError::RetrieveMessages {
                begin,
                end,
                source: e.into(),
            })
    }

    fn next_sender_seq_number(&self) -> u64 {
        self.sender_seq_number + 1
    }

    fn next_target_seq_number(&self) -> u64 {
        self.target_seq_number + 1
    }

    async fn increment_sender_seq_number(&mut self) -> Result<()> {
        let new_value = self.sender_seq_number + 1;
        self.write_seqnums_with(new_value, self.target_seq_number)
            .map_err(|e| StoreError::UpdateSequenceNumber(e.into()))?;
        self.sender_seq_number = new_value;
        Ok(())
    }

    async fn increment_target_seq_number(&mut self) -> Result<()> {
        let new_value = self.target_seq_number + 1;
        self.write_seqnums_with(self.sender_seq_number, new_value)
            .map_err(|e| StoreError::UpdateSequenceNumber(e.into()))?;
        self.target_seq_number = new_value;
        Ok(())
    }

    async fn set_target_seq_number(&mut self, seq_number: u64) -> Result<()> {
        self.write_seqnums_with(self.sender_seq_number, seq_number)
            .map_err(|e| StoreError::UpdateSequenceNumber(e.into()))?;
        self.target_seq_number = seq_number;
        Ok(())
    }

    async fn reset(&mut self) -> Result<()> {
        self.perform_reset()
            .map_err(|e| StoreError::Reset(e.into()))
    }

    fn creation_time(&self) -> DateTime<Utc> {
        self.creation_time
    }
}
