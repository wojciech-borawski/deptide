use serde::{Deserialize, Serialize};

use super::preview::UTF8_BOM;

/// One changed run of a line diff: `old_*` index lines of this machine's file,
/// `new_*` lines of the received file, both 0-based and counted after the BOM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineChunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
}

/// Why `merge_lines` refused a chunk list; `index` is the offending chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkError {
    /// Takes and removes no lines.
    Empty { index: usize },
    /// Starts before the end of the chunk before it, on either side.
    Overlaps { index: usize },
    /// Reaches past the last line of either file.
    OutOfBounds { index: usize },
    /// Would put a line that has no line break in front of another line.
    JoinsLines { index: usize },
}

/// This machine's file with the given chunks of the received file taken in.
///
/// Lines split like the preview's `splitLines`: `\r\n`, `\r` and `\n` each end
/// a line, and text after the last break is a last line. Lines outside the
/// chunks are copied byte for byte from `local`. The output starts with a
/// UTF-8 BOM exactly when `local` does; a BOM on `received` is dropped.
/// `chunks` must be sorted and apart on both sides.
pub fn merge_lines(
    local: &[u8],
    received: &[u8],
    chunks: &[LineChunk],
) -> Result<Vec<u8>, ChunkError> {
    let bom = local.starts_with(UTF8_BOM);
    let local_lines = split_lines(strip_bom(local));
    let received_lines = split_lines(strip_bom(received));
    check_chunks(chunks, local_lines.len(), received_lines.len())?;

    let mut output = Output {
        bytes: Vec::with_capacity(local.len().max(received.len())),
        open_line: false,
    };
    if bom {
        output.bytes.extend_from_slice(UTF8_BOM);
    }

    let mut position = 0;
    for (index, chunk) in chunks.iter().enumerate() {
        output.push(&local_lines[position..chunk.old_start], index)?;
        output.push(
            &received_lines[chunk.new_start..chunk.new_start + chunk.new_count],
            index,
        )?;
        position = chunk.old_start + chunk.old_count;
    }
    output.push(&local_lines[position..], chunks.len().saturating_sub(1))?;

    Ok(output.bytes)
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes)
}

/// Lines with their line breaks; `\r\n` is one break.
fn split_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;

    while index < bytes.len() {
        let end = match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => index + 2,
            b'\r' | b'\n' => index + 1,
            _ => {
                index += 1;
                continue;
            }
        };
        lines.push(&bytes[start..end]);
        start = end;
        index = end;
    }
    if start < bytes.len() {
        lines.push(&bytes[start..]);
    }

    lines
}

fn check_chunks(
    chunks: &[LineChunk],
    local_lines: usize,
    received_lines: usize,
) -> Result<(), ChunkError> {
    let mut old_end = 0;
    let mut new_end = 0;

    for (index, chunk) in chunks.iter().enumerate() {
        if chunk.old_count == 0 && chunk.new_count == 0 {
            return Err(ChunkError::Empty { index });
        }
        let (Some(old_stop), Some(new_stop)) = (
            chunk.old_start.checked_add(chunk.old_count),
            chunk.new_start.checked_add(chunk.new_count),
        ) else {
            return Err(ChunkError::OutOfBounds { index });
        };
        if old_stop > local_lines || new_stop > received_lines {
            return Err(ChunkError::OutOfBounds { index });
        }
        if chunk.old_start < old_end || chunk.new_start < new_end {
            return Err(ChunkError::Overlaps { index });
        }
        old_end = old_stop;
        new_end = new_stop;
    }

    Ok(())
}

struct Output {
    bytes: Vec<u8>,
    /// The last line written has no line break.
    open_line: bool,
}

impl Output {
    fn push(&mut self, lines: &[&[u8]], chunk: usize) -> Result<(), ChunkError> {
        for line in lines {
            if self.open_line {
                return Err(ChunkError::JoinsLines { index: chunk });
            }
            self.bytes.extend_from_slice(line);
            self.open_line = !line.ends_with(b"\r") && !line.ends_with(b"\n");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(old_start: usize, old_count: usize, new_start: usize, new_count: usize) -> LineChunk {
        LineChunk {
            old_start,
            old_count,
            new_start,
            new_count,
        }
    }

    fn merged(local: &str, received: &str, chunks: &[LineChunk]) -> String {
        String::from_utf8(merge_lines(local.as_bytes(), received.as_bytes(), chunks).unwrap())
            .unwrap()
    }

    const LOCAL: &str = "a\nb\nc\nd\ne\n";
    const RECEIVED: &str = "a\nB\nc\nd\nE\nf\n";

    fn both() -> [LineChunk; 2] {
        [chunk(1, 1, 1, 1), chunk(4, 1, 4, 2)]
    }

    #[test]
    fn taking_every_chunk_gives_the_received_file() {
        assert_eq!(merged(LOCAL, RECEIVED, &both()), RECEIVED);
    }

    #[test]
    fn taking_no_chunk_gives_this_machines_file() {
        assert_eq!(merged(LOCAL, RECEIVED, &[]), LOCAL);
    }

    #[test]
    fn one_chunk_changes_only_its_own_lines() {
        assert_eq!(merged(LOCAL, RECEIVED, &both()[..1]), "a\nB\nc\nd\ne\n");
        assert_eq!(merged(LOCAL, RECEIVED, &both()[1..]), "a\nb\nc\nd\nE\nf\n");
    }

    #[test]
    fn crlf_is_one_line_break() {
        let local = "one\r\ntwo\r\nthree\r\n";
        let received = "one\r\nTWO\r\nthree\r\n";

        assert_eq!(merged(local, received, &[chunk(1, 1, 1, 1)]), received);
    }

    #[test]
    fn a_lone_cr_ends_a_line() {
        let local = "p\rq\rr\r";
        let received = "p\rQ\rr\r";

        assert_eq!(merged(local, received, &[chunk(1, 1, 1, 1)]), received);
        assert_eq!(
            merge_lines(local.as_bytes(), received.as_bytes(), &[chunk(3, 0, 3, 1)]),
            Err(ChunkError::OutOfBounds { index: 0 }),
            "three lines on each side"
        );
    }

    #[test]
    fn line_endings_travel_with_their_lines() {
        let local = "a\r\nb\nc\rd";
        let received = "a\nb\nc\rd\n";
        let chunks = [chunk(0, 1, 0, 1), chunk(3, 1, 3, 1)];

        assert_eq!(merged(local, received, &chunks), received);
        assert_eq!(merged(local, received, &chunks[..1]), "a\nb\nc\rd");
        assert_eq!(merged(local, received, &chunks[1..]), "a\r\nb\nc\rd\n");
        assert_eq!(merged(local, received, &[]), local);
    }

    #[test]
    fn a_missing_final_line_break_is_kept_or_taken() {
        let local = "a\nb";
        let received = "a\nb\nc";
        let chunks = [chunk(1, 1, 1, 2)];

        assert_eq!(merged(local, received, &chunks), received);
        assert_eq!(merged(local, received, &[]), local);
        assert_eq!(merged("x\n", "x", &[chunk(0, 1, 0, 1)]), "x");
    }

    #[test]
    fn empty_files_have_no_lines() {
        assert_eq!(merged("", "a\nb", &[chunk(0, 0, 0, 2)]), "a\nb");
        assert_eq!(merged("", "a\nb", &[]), "");
        assert_eq!(merged("a\n", "", &[chunk(0, 1, 0, 0)]), "");
        assert_eq!(merged("", "", &[]), "");
        assert_eq!(
            merge_lines(b"", b"a\n", &[chunk(0, 0, 0, 2)]),
            Err(ChunkError::OutOfBounds { index: 0 }),
            "a final break adds no empty line"
        );
    }

    #[test]
    fn the_bom_of_this_machine_decides_the_output() {
        let bom = "\u{FEFF}";

        assert_eq!(
            merged(&format!("{bom}x\ny\n"), "x\nz\n", &[chunk(1, 1, 1, 1)]),
            format!("{bom}x\nz\n")
        );
        assert_eq!(
            merged(&format!("{bom}x\ny\n"), "x\nz\n", &[]),
            format!("{bom}x\ny\n")
        );
        assert_eq!(
            merged("x\ny\n", &format!("{bom}x\nz\n"), &[chunk(1, 1, 1, 1)]),
            "x\nz\n"
        );
        assert_eq!(
            merged("x\ny\n", &format!("{bom}z\ny\n"), &[chunk(0, 1, 0, 1)]),
            "z\ny\n",
            "the received BOM is not part of its first line"
        );
    }

    #[test]
    fn bytes_that_are_not_utf8_are_copied_unchanged() {
        let local = b"caf\xE9\r\nold\r\n";
        let received = b"caf\xE9\r\nnew\r\n";

        assert_eq!(
            merge_lines(local, received, &[chunk(1, 1, 1, 1)]).unwrap(),
            received.to_vec()
        );
    }

    #[test]
    fn chunks_out_of_order_or_overlapping_are_refused() {
        let reversed = [both()[1], both()[0]];
        let old_overlap = [chunk(1, 2, 1, 1), chunk(2, 1, 3, 1)];
        let new_overlap = [chunk(1, 1, 1, 2), chunk(3, 1, 2, 1)];

        for chunks in [&reversed[..], &old_overlap, &new_overlap] {
            assert_eq!(
                merge_lines(LOCAL.as_bytes(), RECEIVED.as_bytes(), chunks),
                Err(ChunkError::Overlaps { index: 1 }),
                "{chunks:?}"
            );
        }
        assert!(
            merge_lines(
                LOCAL.as_bytes(),
                RECEIVED.as_bytes(),
                &[chunk(1, 1, 1, 1), chunk(2, 1, 2, 1)]
            )
            .is_ok(),
            "touching chunks do not overlap"
        );
    }

    #[test]
    fn chunks_past_either_file_are_refused() {
        let cases = [
            chunk(5, 1, 0, 0),
            chunk(4, 2, 4, 1),
            chunk(0, 0, 6, 1),
            chunk(0, 0, 5, 2),
            chunk(usize::MAX, 2, 0, 1),
            chunk(0, 1, usize::MAX, 2),
        ];

        for case in cases {
            assert_eq!(
                merge_lines(LOCAL.as_bytes(), RECEIVED.as_bytes(), &[case]),
                Err(ChunkError::OutOfBounds { index: 0 }),
                "{case:?}"
            );
        }
        assert_eq!(
            merged(LOCAL, RECEIVED, &[chunk(5, 0, 5, 1)]),
            "a\nb\nc\nd\ne\nf\n",
            "a chunk may start right after the last line"
        );
    }

    #[test]
    fn an_empty_chunk_is_refused() {
        assert_eq!(
            merge_lines(LOCAL.as_bytes(), RECEIVED.as_bytes(), &[chunk(2, 0, 2, 0)]),
            Err(ChunkError::Empty { index: 0 })
        );
    }

    #[test]
    fn a_line_without_a_break_is_never_joined_to_the_next() {
        // A whitespace-blind diff pairs local "a" with received "a\n".
        assert_eq!(
            merge_lines(b"a", b"a\nb\n", &[chunk(1, 0, 1, 1)]),
            Err(ChunkError::JoinsLines { index: 0 })
        );
        assert_eq!(
            merge_lines(b"x\ny\n", b"z", &[chunk(0, 1, 0, 1)]),
            Err(ChunkError::JoinsLines { index: 0 })
        );
        assert_eq!(merged("x\ny", "z", &[chunk(1, 1, 0, 1)]), "x\nz");
    }
}
