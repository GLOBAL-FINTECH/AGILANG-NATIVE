use once_cell::sync::Lazy;
use std::io::{self, Read, Write};
use std::sync::Mutex;

static STDOUT_MUTEX: Lazy<Mutex<io::Stdout>> = Lazy::new(|| Mutex::new(io::stdout()));

pub fn read_framed_message<R: Read>(reader: &mut R) -> Result<String, Box<dyn std::error::Error>> {
    let mut headers = String::new();
    let mut buf = [0u8; 1];
    loop {
        reader.read_exact(&mut buf)?;
        headers.push(buf[0] as char);
        if headers.ends_with("\r\n\r\n") {
            break;
        }
    }

    let mut content_length = 0;
    for line in headers.lines() {
        if line.to_lowercase().starts_with("content-length:") {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() == 2 {
                content_length = parts[1].trim().parse::<usize>()?;
            }
        }
    }

    if content_length == 0 {
        return Err("Content-Length header is missing or 0".into());
    }

    let mut payload = vec![0u8; content_length];
    reader.read_exact(&mut payload)?;
    Ok(String::from_utf8(payload)?)
}

pub fn write_framed_message(content: &str) -> io::Result<()> {
    let mut stdout = STDOUT_MUTEX.lock().unwrap();
    write!(
        stdout,
        "Content-Length: {}\r\n\r\n{}",
        content.len(),
        content
    )?;
    stdout.flush()
}
