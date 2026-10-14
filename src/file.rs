use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct OutputWriter {
    overwrite: bool,
    committed_paths: Mutex<HashSet<PathBuf>>,
}

impl OutputWriter {
    pub fn new(overwrite: bool) -> Self {
        Self {
            overwrite,
            committed_paths: Mutex::new(HashSet::new()),
        }
    }

    pub fn write(
        &self,
        input_path: &Path,
        output_path: &Path,
        data: &[u8],
        finalize: impl FnOnce(&Path) -> Result<()>,
    ) -> Result<()> {
        self.check_destination(input_path, output_path)?;
        let parent = output_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut temporary = tempfile::Builder::new()
            .prefix(".decrypt_ncm-")
            .tempfile_in(parent)
            .with_context(|| format!("failed to create temporary file in {}", parent.display()))?;
        temporary
            .write_all(data)
            .context("failed to write temporary audio file")?;
        finalize(temporary.path())?;
        temporary
            .as_file()
            .sync_all()
            .context("failed to sync temporary audio file")?;

        // Serialize commits within a batch, including explicitly requested overwrites.
        // The filesystem operation below also protects default writes across processes.
        let mut committed_paths = self
            .committed_paths
            .lock()
            .map_err(|_| anyhow::anyhow!("output commit lock poisoned"))?;
        self.check_destination(input_path, output_path)?;
        let destination = fs::canonicalize(parent)?.join(
            output_path
                .file_name()
                .context("output path has no file name")?,
        );
        // Retain both spellings when an existing name differs in case on the filesystem.
        let existing_destination = if output_path.try_exists()? {
            Some(fs::canonicalize(output_path)?)
        } else {
            None
        };
        if committed_paths.contains(&destination)
            || existing_destination
                .as_ref()
                .is_some_and(|path| committed_paths.contains(path))
        {
            bail!(
                "multiple inputs map to the same output: {}",
                output_path.display()
            );
        }

        // Close the handle before moving the file, including on Windows.
        let temporary = temporary.into_temp_path();
        let result = if self.overwrite {
            temporary.persist(output_path)
        } else {
            temporary.persist_noclobber(output_path)
        };
        result
            .map_err(|err| err.error)
            .with_context(|| format!("failed to commit output: {}", output_path.display()))?;
        committed_paths.insert(destination);
        if let Some(existing_destination) = existing_destination {
            committed_paths.insert(existing_destination);
        }
        Ok(())
    }

    fn check_destination(&self, input_path: &Path, output_path: &Path) -> Result<()> {
        let metadata = match fs::symlink_metadata(output_path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("failed to inspect output: {}", output_path.display())
                });
            }
        };
        if same_file::is_same_file(input_path, output_path).with_context(|| {
            format!(
                "failed to compare input and output: {}",
                output_path.display()
            )
        })? {
            bail!("output refers to the input file: {}", output_path.display());
        }
        if !self.overwrite {
            bail!(
                "output already exists (use --overwrite to replace it): {}",
                output_path.display()
            );
        }
        if !metadata.is_file() {
            bail!("output is not a regular file: {}", output_path.display());
        }
        Ok(())
    }
}

pub fn is_ncm_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("ncm"))
        .unwrap_or(false)
}

pub fn read_file(file_path: &Path) -> Result<Vec<u8>> {
    let file = File::open(file_path)
        .with_context(|| format!("failed to open file: {}", file_path.display()))?;
    let mut buf = BufReader::new(file);
    let mut data = Vec::new();
    buf.read_to_end(&mut data)
        .with_context(|| format!("failed to read file: {}", file_path.display()))?;
    Ok(data)
}
