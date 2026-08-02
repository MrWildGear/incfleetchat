use std::fs::OpenOptions;
use std::io::Read;
use std::path::Path;

/// Read an EVE chatlog as text. Live logs are typically UTF-16LE with BOM;
/// fixtures and some exports may be UTF-8.
///
/// On Windows, open with full share mode so we can read while EVE holds the file.
pub fn read_chatlog(path: &Path) -> std::io::Result<String> {
    let bytes = read_bytes_shared(path)?;
    decode_chatlog_bytes(&bytes)
}

fn read_bytes_shared(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
        opts.share_mode(0x0000_0007);
    }
    let mut file = opts.open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub fn decode_chatlog_bytes(bytes: &[u8]) -> std::io::Result<String> {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        // UTF-16LE BOM
        let u16s: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return Ok(String::from_utf16_lossy(&u16s));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let u16s: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return Ok(String::from_utf16_lossy(&u16s));
    }
    // UTF-8 BOM or plain UTF-8
    let slice = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    };
    match std::str::from_utf8(slice) {
        Ok(s) => Ok(s.to_string()),
        Err(_) => {
            // Heuristic: even-length buffer with many NULs → UTF-16LE without BOM
            if bytes.len() >= 4 && bytes.len() % 2 == 0 {
                let nul_even = bytes.iter().step_by(2).filter(|&&b| b == 0).count();
                if nul_even > bytes.len() / 8 {
                    let u16s: Vec<u16> = bytes
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    return Ok(String::from_utf16_lossy(&u16s));
                }
            }
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "chatlog is not valid UTF-8 or UTF-16",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf8_fixture() {
        let text = include_str!("../tests/fixtures/fleet_sample.txt");
        let decoded = decode_chatlog_bytes(text.as_bytes()).unwrap();
        assert!(decoded.contains("Listener:"));
        assert!(decoded.contains("Hamilton Norris > 1"));
    }

    #[test]
    fn decodes_utf16le_bom() {
        let mut bytes = vec![0xFF, 0xFE];
        for u in "Listener:        Test Pilot\n[ 2026.08.01 12:00:00 ] Pilot > 1\n".encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let decoded = decode_chatlog_bytes(&bytes).unwrap();
        assert!(decoded.contains("Test Pilot"));
        assert!(decoded.contains("> 1"));
    }

    #[test]
    fn read_chatlog_opens_with_share_so_locked_files_work() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"Listener:        Test Pilot\n[ 2026.08.01 12:00:00 ] Pilot > 1\n")
            .unwrap();
        f.flush().unwrap();
        // Keep the write handle open while reading (simulates EVE holding the log).
        let text = read_chatlog(f.path()).expect("should read while another handle is open");
        assert!(text.contains("Test Pilot"));
    }

    #[test]
    fn decodes_utf16_with_bom_on_each_message_line() {
        let mut bytes = vec![0xFF, 0xFE];
        let body = "\u{feff}[ 2026.08.02 18:33:11 ] Hamilton Norris > 1\r\n\
\u{feff}[ 2026.08.02 18:34:13 ] Hamilton Norris > 1\r\n";
        for u in body.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let decoded = decode_chatlog_bytes(&bytes).unwrap();
        let sites = crate::parse::parse_site_candidates(&decoded);
        assert_eq!(sites.len(), 2);
    }
}
