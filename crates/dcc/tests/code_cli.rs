//! `dcc code edit` against a real, headless neovim. Skipped when neovim is not installed.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use dcc::util::new_uuid;

fn have_nvim() -> bool {
    Command::new("nvim").arg("--version").output().is_ok()
}

struct Server {
    child: Child,
    socket: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn start_server() -> Server {
    // a short path, unix sockets are limited to about 100 bytes
    let socket = std::env::temp_dir().join(format!("dccnv-{}.sock", &new_uuid()[..8]));
    let child = Command::new("nvim")
        .args(["--headless", "-u", "NONE", "-i", "NONE", "--listen"])
        .arg(&socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !socket.exists() && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(socket.exists(), "neovim did not open its socket");
    Server { child, socket }
}

fn current_file(socket: &Path) -> String {
    let o = Command::new("nvim")
        .arg("--server")
        .arg(socket)
        .args(["--remote-expr", "expand('%:p')"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

fn edit(socket: Option<&Path>, cwd: &Path, files: &[&str]) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_dcc"));
    c.args(["code", "edit"])
        .args(files)
        .current_dir(cwd)
        .env_remove("DCC_NVIM_SOCKET");
    if let Some(s) = socket {
        c.env("DCC_NVIM_SOCKET", s);
    }
    c.output().unwrap()
}

#[test]
fn a_file_named_in_the_shell_opens_in_the_neovim_of_the_tab() {
    if !have_nvim() {
        eprintln!("nvim is not installed, skipping");
        return;
    }
    let server = start_server();
    let work = std::env::temp_dir().join(format!("dccwk-{}", &new_uuid()[..8]));
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join("notes.txt"), "hello\n").unwrap();
    // the work folder may sit behind a link (/var is /private/var on macOS), neovim reports the real path
    let real = work.canonicalize().unwrap();

    let o = edit(Some(&server.socket), &work, &["notes.txt"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(
        current_file(&server.socket),
        real.join("notes.txt").to_string_lossy()
    );

    // a second file, by absolute path, replaces the shown one, and a name that looks like an option is a file after `--`
    std::fs::write(work.join("-weird.md"), "x\n").unwrap();
    let o = edit(Some(&server.socket), &work, &["--", "-weird.md"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(
        current_file(&server.socket),
        real.join("-weird.md").to_string_lossy()
    );
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn without_a_neovim_the_command_says_so_and_changes_nothing() {
    let work = std::env::temp_dir();
    let o = edit(None, &work, &["x.txt"]);
    assert!(!o.status.success());
    let v: serde_json::Value = serde_json::from_slice(&o.stderr).unwrap();
    assert_eq!(v["error"]["kind"], "not_found");
    assert!(v["error"]["message"].as_str().unwrap().contains("code tab"));
    let gone = work.join("dcc-no-such.sock");
    let o = edit(Some(&gone), &work, &["x.txt"]);
    let v: serde_json::Value = serde_json::from_slice(&o.stderr).unwrap();
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not running")
    );
}
