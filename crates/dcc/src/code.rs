//! Sending a file to the neovim of a code tab. The shell pane of the tab knows the socket that
//! neovim listens on (`DCC_NVIM_SOCKET`), and `dcc code edit <file>` asks that neovim to open the
//! file, so a file named in the bottom pane opens in the editor pane.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use crate::error::{Error, Result};

pub const ENV_SOCKET: &str = "DCC_NVIM_SOCKET";

/// Absolute paths only. A relative one is read against `cwd`, and an absolute path never starts
/// with a dash, so a file name can not be taken for an option.
pub fn absolute_files(cwd: &Path, files: &[String]) -> Result<Vec<PathBuf>> {
    if files.is_empty() {
        return Err(Error::usage("name at least one file to open"));
    }
    files
        .iter()
        .map(|f| {
            if f.is_empty() || f.contains('\0') {
                return Err(Error::usage("a file name is empty or has a NUL byte"));
            }
            let p = Path::new(f);
            Ok(if p.is_absolute() {
                p.to_path_buf()
            } else {
                cwd.join(p)
            })
        })
        .collect()
}

/// Arguments for `nvim`, which sends the files to the running instance and exits.
pub fn remote_args(socket: &str, files: &[PathBuf]) -> Vec<String> {
    let mut a = vec![
        "--server".to_string(),
        socket.to_string(),
        "--remote".to_string(),
    ];
    a.extend(files.iter().map(|f| f.to_string_lossy().to_string()));
    a
}

pub fn edit(socket: Option<String>, cwd: &Path, files: &[String]) -> Result<Value> {
    let paths = absolute_files(cwd, files)?;
    let socket = socket.filter(|s| !s.is_empty()).ok_or_else(|| {
        Error::not_found(format!(
            "this terminal is not linked to a neovim ({ENV_SOCKET} is not set). Open a code tab from DCC to get one."
        ))
    })?;
    if !Path::new(&socket).exists() {
        return Err(Error::not_found(
            "the neovim of this code tab is not running (its socket is gone). Start it again in the editor pane.",
        ));
    }
    let nvim = crate::agent::adapter::find_program("nvim")
        .ok_or_else(|| Error::not_found("nvim was not found on PATH"))?;
    let out = Command::new(nvim)
        .args(remote_args(&socket, &paths))
        .output()
        .map_err(|e| Error::external(format!("cannot run nvim: {e}")))?;
    if !out.status.success() {
        return Err(Error::external(format!(
            "neovim did not take the files: {}",
            String::from_utf8_lossy(&out.stderr)
                .trim()
                .lines()
                .next()
                .unwrap_or("no message")
        )));
    }
    Ok(json!({ "opened": paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_names_are_read_against_the_current_folder_and_absolute_ones_stay() {
        let f = absolute_files(
            Path::new("/work"),
            &["a.txt".into(), "/etc/x".into(), "-oddname".into()],
        )
        .unwrap();
        assert_eq!(
            f,
            vec![
                PathBuf::from("/work/a.txt"),
                PathBuf::from("/etc/x"),
                PathBuf::from("/work/-oddname")
            ]
        );
        assert!(f.iter().all(|p| p.is_absolute()));
    }

    #[test]
    fn bad_input_is_refused_before_anything_runs() {
        assert_eq!(
            absolute_files(Path::new("/w"), &[]).unwrap_err().kind,
            "usage"
        );
        assert_eq!(
            absolute_files(Path::new("/w"), &["".into()])
                .unwrap_err()
                .kind,
            "usage"
        );
        assert_eq!(
            edit(None, Path::new("/w"), &["a".into()]).unwrap_err().kind,
            "not_found"
        );
        assert_eq!(
            edit(
                Some("/no/such/socket".into()),
                Path::new("/w"),
                &["a".into()]
            )
            .unwrap_err()
            .kind,
            "not_found"
        );
    }

    #[test]
    fn the_remote_call_names_the_server_and_the_files() {
        let a = remote_args("/s/x.sock", &[PathBuf::from("/w/a"), PathBuf::from("/w/b")]);
        assert_eq!(a, vec!["--server", "/s/x.sock", "--remote", "/w/a", "/w/b"]);
    }
}
