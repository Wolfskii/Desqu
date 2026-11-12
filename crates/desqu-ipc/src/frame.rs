//! A frame is a 4-byte little-endian length followed by that many bytes of UTF-8 JSON.
//! Frames above `MAX_FRAME` are refused in both directions.

use std::io::{self, Read, Write};

pub const MAX_FRAME: usize = 64 * 1024;

pub fn write_frame(w: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "frame too large"));
    }
    // Checked above: the length always fits in u32.
    let len = payload.len() as u32;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

/// `Ok(None)` at a clean end of stream, between frames. A stream that ends inside a frame is an error.
pub fn read_frame(r: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut header = [0u8; 4];
    let mut filled = 0;
    while filled < header.len() {
        match r.read(&mut header[filled..])? {
            0 if filled == 0 => return Ok(None),
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "stream ended inside a frame header",
                ))
            }
            n => filled += n,
        }
    }
    let len = u32::from_le_bytes(header) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    Ok(Some(payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn framed(payloads: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        for p in payloads {
            write_frame(&mut out, p).unwrap();
        }
        out
    }

    #[test]
    fn round_trips_several_frames_then_reports_a_clean_end() {
        let bytes = framed(&[b"{\"type\":\"a\"}", b"", b"third"]);
        let mut r = Cursor::new(bytes);
        assert_eq!(read_frame(&mut r).unwrap().unwrap(), b"{\"type\":\"a\"}");
        assert_eq!(read_frame(&mut r).unwrap().unwrap(), b"");
        assert_eq!(read_frame(&mut r).unwrap().unwrap(), b"third");
        assert_eq!(read_frame(&mut r).unwrap(), None);
    }

    #[test]
    fn the_length_prefix_is_little_endian() {
        let bytes = framed(&[b"abc"]);
        assert_eq!(&bytes[..4], &[3, 0, 0, 0]);
        assert_eq!(&bytes[4..], b"abc");
    }

    #[test]
    fn refuses_to_write_an_oversize_frame() {
        let big = vec![0u8; MAX_FRAME + 1];
        let mut out = Vec::new();
        let err = write_frame(&mut out, &big).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(out.is_empty());
    }

    #[test]
    fn refuses_an_oversize_header_without_reading_the_payload() {
        let mut bytes = ((MAX_FRAME as u32) + 1).to_le_bytes().to_vec();
        bytes.extend_from_slice(b"x");
        let err = read_frame(&mut Cursor::new(bytes)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_truncated_header_is_an_error_not_a_clean_end() {
        let err = read_frame(&mut Cursor::new(vec![5u8, 0])).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn a_truncated_payload_is_an_error() {
        let mut bytes = 10u32.to_le_bytes().to_vec();
        bytes.extend_from_slice(b"short");
        let err = read_frame(&mut Cursor::new(bytes)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn accepts_a_frame_of_exactly_the_maximum_size() {
        let payload = vec![b'a'; MAX_FRAME];
        let bytes = framed(&[&payload]);
        let got = read_frame(&mut Cursor::new(bytes)).unwrap().unwrap();
        assert_eq!(got.len(), MAX_FRAME);
    }
}
