//! Small helpers shared by the modules. No platform specific code outside the cfg blocks.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// FNV-1a 64 bit, as 16 hex digits. Stable across runs and versions (unlike `DefaultHasher`).
pub fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Creates the folder and its parents. On Unix the folder itself becomes 0700.
pub fn create_private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .map_err(|e| Error::io(format!("cannot create {}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|e| Error::io(format!("cannot set 0700 on {}: {e}", path.display())))?;
    }
    Ok(())
}

/// A random (version 4) uuid. The bytes come from the random keys the standard library hashes with
/// (a fresh `RandomState` per call), mixed with the clock and the process id. Good enough to name
/// runs and sessions, not for secrets.
pub fn new_uuid() -> String {
    use std::hash::{BuildHasher, Hasher, RandomState};
    let mut bytes = [0u8; 16];
    for (i, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut h = RandomState::new().build_hasher();
        h.write_i64(now_ms());
        h.write_u32(std::process::id());
        h.write_usize(i);
        chunk.copy_from_slice(&h.finish().to_be_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        hex[0..4].concat(),
        hex[4..6].concat(),
        hex[6..8].concat(),
        hex[8..10].concat(),
        hex[10..16].concat()
    )
}

/// Is a process with this id alive. Unix only for now, Windows is untested and says so.
pub fn process_alive(pid: u32) -> Result<bool> {
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| Error::external(format!("cannot run kill: {e}")))?;
        Ok(status.success())
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, Command::new("cmd"));
        Err(Error::external(
            "process_alive is not implemented on this platform (Windows is untested)",
        ))
    }
}

/// A fresh folder for tests, under the repo's own `target/test-tmp` (never the system temp folder).
pub fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("test-tmp")
        .join(format!("{tag}-{}-{}", std::process::id(), now_ms()));
    fs::create_dir_all(&dir).expect("cannot create test folder");
    dir
}

/// Runs git in `dir` with a fixed identity, for tests. Panics when git fails.
pub fn sh(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(args)
        .output()
        .expect("cannot run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A scratch git repository on `main` with one commit, for tests.
pub fn scratch_repo(tag: &str) -> std::path::PathBuf {
    let d = scratch_dir(tag);
    sh(&d, &["init", "-q", "-b", "main"]);
    fs::write(d.join("README.md"), "x\n").expect("write");
    sh(&d, &["add", "README.md"]);
    sh(&d, &["commit", "-q", "-m", "init"]);
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_is_stable_and_sensitive() {
        assert_eq!(fnv1a_hex(b""), "cbf29ce484222325");
        assert_eq!(fnv1a_hex(b"a"), "af63dc4c8601ec8c");
        assert_ne!(fnv1a_hex(b"ab"), fnv1a_hex(b"ba"));
    }

    #[test]
    fn uuids_look_right_and_differ() {
        let a = new_uuid();
        let b = new_uuid();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(a.as_bytes()[14], b'4');
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }

    #[test]
    fn own_process_is_alive() {
        assert!(process_alive(std::process::id()).unwrap());
    }
}
