use anyhow::{Context, Result};
use std::io::{Read, Write};

pub const MAX_MESSAGE_SIZE: usize = 1024 * 1024; // 1MB native messaging limit

pub fn read_native_message<R: Read>(reader: &mut R) -> Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e).context("Failed to read message length prefix"),
    }

    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_MESSAGE_SIZE {
        anyhow::bail!("Native messaging payload size {} exceeds limit of 1MB", len);
    }

    let mut payload = vec![0u8; len];
    reader
        .read_exact(&mut payload)
        .context("Failed to read native message payload")?;

    Ok(Some(payload))
}

pub fn write_native_message<W: Write>(writer: &mut W, payload: &[u8]) -> Result<()> {
    let len = payload.len() as u32;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(payload)?;
    writer.flush()?;
    Ok(())
}
