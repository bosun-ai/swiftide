//! Binary-safe file download out of any tool executor.
//!
//! Works over plain shell commands, so every executor supports it without daemon changes: the
//! remote file is read in fixed-size chunks (`dd | base64`), decoded and appended to the local
//! file, then verified against a SHA-256 computed inside the executor. Memory use stays within
//! one chunk, and disk use by the caller's limit, checked before the first chunk.

use std::path::Path;

use anyhow::{Context as _, Result, bail};
use base64::Engine as _;
use sha2::{Digest as _, Sha256};
use swiftide_core::{Command, ToolExecutor};
use tokio::io::{AsyncWriteExt as _, BufWriter};

use super::traits::stdout_of;

/// Bytes per chunk; about 5.3 MiB of base64 per command response.
pub(crate) const CHUNK_SIZE: u64 = 4 * 1024 * 1024;

/// Most output a size or checksum command prints.
const SHORT_OUTPUT: usize = 1024;

/// Downloads `remote` from the executor into `local`, returning the number of bytes written.
/// Fails before writing anything when the executor reports more than `max_bytes`.
///
/// The local file is created or truncated, and removed again when the download fails.
pub(crate) async fn download<E>(
    executor: &E,
    remote: &Path,
    local: &Path,
    max_bytes: u64,
    chunk_size: u64,
) -> Result<u64>
where
    E: ToolExecutor + ?Sized,
{
    let result = download_to(executor, remote, local, max_bytes, chunk_size).await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(local).await;
    }
    result.with_context(|| format!("failed to download {}", remote.display()))
}

async fn download_to<E>(
    executor: &E,
    remote: &Path,
    local: &Path,
    max_bytes: u64,
    chunk_size: u64,
) -> Result<u64>
where
    E: ToolExecutor + ?Sized,
{
    let quoted = quote(remote);
    let size = remote_size(executor, &quoted).await?;
    if size > max_bytes {
        bail!("the file has {size} bytes, more than the {max_bytes} allowed");
    }

    let file = tokio::fs::File::create(local)
        .await
        .with_context(|| format!("failed to create {}", local.display()))?;
    let mut writer = BufWriter::new(file);
    let mut hasher = Sha256::new();

    let mut offset = 0;
    for index in 0..size.div_ceil(chunk_size) {
        let expected = chunk_size.min(size - offset);
        let bytes = read_chunk(executor, &quoted, chunk_size, index).await?;
        if bytes.len() as u64 != expected {
            bail!(
                "chunk {index} has {} bytes, expected {expected}; the file changed during download",
                bytes.len()
            );
        }
        hasher.update(&bytes);
        writer.write_all(&bytes).await?;
        offset += expected;
    }
    writer.flush().await?;

    let local_sha = format!("{:x}", hasher.finalize());
    let remote_sha = remote_sha256(executor, &quoted).await?;
    if local_sha != remote_sha {
        bail!("checksum mismatch: downloaded {local_sha}, executor reports {remote_sha}");
    }
    Ok(size)
}

async fn remote_size<E: ToolExecutor + ?Sized>(executor: &E, quoted: &str) -> Result<u64> {
    let stdout = stdout_of(
        executor,
        Command::shell(format!("[ -f {quoted} ] && wc -c < {quoted}")),
        SHORT_OUTPUT,
    )
    .await
    .context("remote file not found or not a regular file")?;
    let text = String::from_utf8_lossy(&stdout);
    text.trim()
        .parse()
        .with_context(|| format!("unexpected size output {text:?}"))
}

async fn read_chunk<E: ToolExecutor + ?Sized>(
    executor: &E,
    quoted: &str,
    chunk_size: u64,
    index: u64,
) -> Result<Vec<u8>> {
    let mut encoded = stdout_of(
        executor,
        Command::shell(format!(
            "dd if={quoted} bs={chunk_size} skip={index} count=1 2>/dev/null | base64"
        )),
        encoded_size(chunk_size),
    )
    .await?;
    // GNU base64 wraps lines; the decoder does not accept whitespace.
    encoded.retain(|byte| !byte.is_ascii_whitespace());
    base64::engine::general_purpose::STANDARD
        .decode(&encoded)
        .with_context(|| format!("chunk {index} is not valid base64"))
}

async fn remote_sha256<E: ToolExecutor + ?Sized>(executor: &E, quoted: &str) -> Result<String> {
    let stdout = stdout_of(
        executor,
        Command::shell(format!(
            "if command -v sha256sum >/dev/null 2>&1; then sha256sum < {quoted}; \
             else shasum -a 256 < {quoted}; fi"
        )),
        SHORT_OUTPUT,
    )
    .await?;
    String::from_utf8_lossy(&stdout)
        .split_whitespace()
        .next()
        .map(str::to_ascii_lowercase)
        .context("executor returned no checksum")
}

/// Most bytes `base64` prints for `chunk_size` bytes: four per three, wrapped in lines of at
/// least 64 characters.
fn encoded_size(chunk_size: u64) -> usize {
    let encoded = usize::try_from(chunk_size.div_ceil(3) * 4).unwrap_or(usize::MAX);
    encoded.saturating_add(encoded / 64 + 1)
}

fn quote(path: &Path) -> String {
    shell_escape::unix::escape(path.to_string_lossy()).into_owned()
}
