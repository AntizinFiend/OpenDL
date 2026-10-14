use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info};

pub struct Storage {
    path: PathBuf,
    file: Arc<File>,
}

impl Storage {
    pub fn create_or_open(path: impl AsRef<Path>, total_size: Option<u64>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .with_context(|| format!("Failed to open destination file: {:?}", path))?;

        if let Some(size) = total_size {
            let current_len = file.metadata()?.len();
            if current_len < size {
                debug!(path = ?path, size = size, "Preallocating file to total size");
                file.set_len(size)
                    .with_context(|| format!("Failed to preallocate file to size {}", size))?;
            }
        }

        info!(path = ?path, total_size = ?total_size, "Storage initialized");

        Ok(Self {
            path,
            file: Arc::new(file),
        })
    }

    pub fn write_at(&self, offset: u64, buf: &[u8]) -> Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileExt;
            let mut written = 0;
            while written < buf.len() {
                let n = self.file.seek_write(&buf[written..], offset + written as u64)?;
                if n == 0 {
                    anyhow::bail!("Write returned 0 bytes");
                }
                written += n;
            }
            Ok(())
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            self.file.write_all_at(buf, offset)?;
            Ok(())
        }

        #[cfg(not(any(windows, unix)))]
        {
            use std::io::{Seek, SeekFrom, Write};
            // Fallback for non-windows/unix: acquire lock and seek+write
            let mut file_clone = self.file.try_clone()?;
            file_clone.seek(SeekFrom::Start(offset))?;
            file_clone.write_all(buf)?;
            Ok(())
        }
    }

    pub fn sync(&self) -> Result<()> {
        self.file.sync_data()?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
