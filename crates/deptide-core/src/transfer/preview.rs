use std::fs;
use std::io::{self, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::files::BINARY_SNIFF_BYTES;
use super::receive::safe_relative;
use crate::error::{AppError, AppResult};

pub const PREVIEW_LIMIT: u64 = 1024 * 1024;
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FileSide {
    /// UTF-8 text without its byte order mark; `bom` says whether the file had one.
    Text { text: String, bom: bool, size: u64 },
    /// A NUL byte in the first 8 KiB.
    Binary { size: u64 },
    /// Text over `PREVIEW_LIMIT` bytes.
    TooLarge { size: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveFileContents {
    /// The file in the received folder, `None` when it has no such file.
    pub received: Option<FileSide>,
    /// The file in the target project, `None` when it has no such file.
    pub local: Option<FileSide>,
}

pub fn read_file_pair(
    source: &Path,
    target_directory: &Path,
    relative: &str,
) -> AppResult<ReceiveFileContents> {
    let path = safe_relative(relative)
        .ok_or_else(|| AppError::new(format!("Invalid file path: {relative}")))?;

    Ok(ReceiveFileContents {
        received: read_side(&source.join(&path))?,
        local: read_side(&target_directory.join(&path))?,
    })
}

fn read_side(path: &Path) -> AppResult<Option<FileSide>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => metadata,
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let size = metadata.len();

    let mut file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    (&mut file)
        .take(BINARY_SNIFF_BYTES)
        .read_to_end(&mut bytes)?;
    if bytes.contains(&0) {
        return Ok(Some(FileSide::Binary { size }));
    }
    if size > PREVIEW_LIMIT {
        return Ok(Some(FileSide::TooLarge { size }));
    }

    file.take(PREVIEW_LIMIT + 1 - bytes.len() as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > PREVIEW_LIMIT {
        return Ok(Some(FileSide::TooLarge {
            size: bytes.len() as u64,
        }));
    }

    let bom = bytes.starts_with(UTF8_BOM);
    let body = if bom {
        &bytes[UTF8_BOM.len()..]
    } else {
        &bytes[..]
    };
    Ok(Some(FileSide::Text {
        text: String::from_utf8_lossy(body).into_owned(),
        bom,
        size,
    }))
}
