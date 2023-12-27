//! Utilities

use std::io::{stdin, stdout, ErrorKind, Write};
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

/// Create an empty file and all its parent directories
pub fn create_empty_file(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().unwrap_or(path))?;

    std::fs::write(path, vec![])?;
    Ok(())
}

/// Get the sha256 hash of a file
pub fn sha256sum(path: &Path) -> std::io::Result<String> {
    let output = Command::new("sha256sum").arg(path.as_os_str()).output()?;

    if !output.status.success() {
        return Err(std::io::Error::new(
            ErrorKind::Other,
            "Failed to compute sum!",
        ));
    }

    let hash = String::from_utf8_lossy(&output.stdout)
        .split(' ')
        .next()
        .unwrap_or("")
        .to_string();

    Ok(hash)
}

/// Get the sha256 hash of a string
pub fn sha256sum_str(str: &str) -> std::io::Result<String> {
    let temp = mktemp::Temp::new_file()?;
    std::fs::write(&temp, str)?;

    sha256sum(&temp)
}

/// Request user's input
pub fn request_input(field: &str) -> std::io::Result<String> {
    print!("Please input {}: ", field);
    stdout().flush()?;
    let mut s = String::new();
    stdin().read_line(&mut s)?;

    if s.ends_with('\n') {
        s.pop();
    }

    if s.ends_with('\r') {
        s.pop();
    }

    Ok(s)
}

/// Generate a random string of a given size
///
/// ```
/// use dockerust::utils::rand_str;
///
/// let size = 10;
/// let rand = rand_str(size);
/// assert_eq!(size, rand.len());
/// ```
pub fn rand_str(len: usize) -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .take(len)
        .collect()
}

/// Get the current time since epoch
///
/// ```
/// use dockerust::utils::time;
///
/// let time = time();
/// ```
pub fn time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

