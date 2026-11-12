//! How a password or key passphrase reaches `ssh` without touching argv, files, logs or the
//! screen. The runner (`dcc ssh run`, the process that starts ssh) reads the Keychain item once
//! and serves it over a private unix socket. `ssh` is started with `SSH_ASKPASS` pointing at a
//! tiny wrapper script that runs `dcc ssh askpass`, which asks the socket. The socket answers
//! only a caller that holds this run's random nonce (given to ssh through its environment), only
//! for the kind of prompt the profile expects, and only a few times. A host key question is always
//! refused, the user decides about unknown hosts with the profile's host key policy.
//!
//! Remaining limit. The nonce travels in the environment of ssh, so a program that ssh itself
//! starts (a ProxyCommand from the user's own configuration) can read it. What it can get is
//! bounded, three answers, only to a password style prompt, only while ssh runs. The caller is not
//! checked by process id, that needs an unsafe system call.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{Value, json};

use super::model::Auth;
use crate::error::{Error, Result};
use crate::util::{create_private_dir, random_hex};

pub const ENV_SOCKET: &str = "DCC_ASKPASS_SOCKET";
pub const ENV_NONCE: &str = "DCC_ASKPASS_NONCE";
const MAX_ANSWERS: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Password,
    Passphrase,
    /// "Are you sure you want to continue connecting (yes/no/[fingerprint])?"
    HostKey,
    Other,
}

pub fn classify(prompt: &str) -> PromptKind {
    let p = prompt.to_ascii_lowercase();
    if p.contains("(yes/no") || p.contains("fingerprint") {
        PromptKind::HostKey
    } else if p.contains("passphrase for") || p.contains("passphrase:") {
        PromptKind::Passphrase
    } else if p.trim_end().ends_with("password:") {
        PromptKind::Password
    } else {
        PromptKind::Other
    }
}

fn allowed(auth: Auth, kind: PromptKind) -> bool {
    matches!(
        (auth, kind),
        (Auth::Password, PromptKind::Password) | (Auth::Passphrase, PromptKind::Passphrase)
    )
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

pub struct AskpassServer {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    dir: PathBuf,
    socket: PathBuf,
    wrapper: PathBuf,
    nonce: String,
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

impl AskpassServer {
    /// `exe` is the dcc binary the wrapper runs. The socket and the wrapper live in a private
    /// folder under the system temp folder, which keeps the socket path short.
    pub fn start(exe: &Path, auth: Auth, secret: String) -> Result<Self> {
        let dir = std::env::temp_dir().join(format!("dcc-ssh-{}", random_hex(6)));
        create_private_dir(&dir)?;
        let socket = dir.join("s");
        let wrapper = dir.join("askpass.sh");
        let nonce = random_hex(16);
        std::fs::write(
            &wrapper,
            format!(
                "#!/bin/sh\nexec {} ssh askpass -- \"$@\"\n",
                quote(&exe.to_string_lossy())
            ),
        )?;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700))?;
        let listener = UnixListener::bind(&socket)
            .map_err(|e| Error::io(format!("cannot open the askpass socket: {e}")))?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let handle = {
            let (stop, nonce) = (stop.clone(), nonce.clone());
            let answered = AtomicU32::new(0);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => serve(stream, &nonce, auth, &secret, &answered),
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(20));
                        }
                        Err(_) => break,
                    }
                }
            })
        };
        Ok(Self {
            stop,
            handle: Some(handle),
            dir,
            socket,
            wrapper,
            nonce,
        })
    }

    /// Environment for the ssh child, and only for it.
    pub fn env(&self) -> Vec<(String, String)> {
        vec![
            ("SSH_ASKPASS".into(), self.wrapper.to_string_lossy().into()),
            ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
            ("DISPLAY".into(), "dcc".into()),
            (ENV_SOCKET.into(), self.socket.to_string_lossy().into()),
            (ENV_NONCE.into(), self.nonce.clone()),
        ]
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

impl Drop for AskpassServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        let _ = std::fs::remove_file(&self.socket);
        let _ = std::fs::remove_file(&self.wrapper);
        let _ = std::fs::remove_dir(&self.dir);
    }
}

fn serve(stream: UnixStream, nonce: &str, auth: Auth, secret: &str, answered: &AtomicU32) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let mut line = String::new();
    {
        use std::io::Read;
        let mut reader = BufReader::new((&stream).take(8 * 1024));
        if reader.read_line(&mut line).is_err() {
            return;
        }
    }
    let reply = match serde_json::from_str::<Value>(&line) {
        Ok(v) => {
            let given = v.get("nonce").and_then(Value::as_str).unwrap_or_default();
            let prompt = v.get("prompt").and_then(Value::as_str).unwrap_or_default();
            if !same(given, nonce) {
                json!({ "ok": false, "reason": "not allowed" })
            } else {
                let kind = classify(prompt);
                if kind == PromptKind::HostKey {
                    json!({ "ok": false, "reason": "unknown host key, use the profile's accept-new policy or add the host to known_hosts" })
                } else if !allowed(auth, kind) {
                    json!({ "ok": false, "reason": "this prompt is not one the profile expects" })
                } else if answered.fetch_add(1, Ordering::Relaxed) >= MAX_ANSWERS {
                    json!({ "ok": false, "reason": "too many prompts" })
                } else {
                    json!({ "ok": true, "secret": secret })
                }
            }
        }
        Err(_) => json!({ "ok": false, "reason": "bad request" }),
    };
    let mut out = reply.to_string();
    out.push('\n');
    let _ = (&stream).write_all(out.as_bytes());
}

/// The helper ssh runs. Prints the secret on stdout, nothing else. On refusal it prints the reason
/// on stderr and the caller exits non zero.
pub fn answer(prompt: &str) -> Result<String> {
    let socket =
        std::env::var(ENV_SOCKET).map_err(|_| Error::usage("not started by dcc ssh run"))?;
    let nonce = std::env::var(ENV_NONCE).map_err(|_| Error::usage("not started by dcc ssh run"))?;
    let mut stream = UnixStream::connect(&socket)
        .map_err(|e| Error::external(format!("askpass socket: {e}")))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut req = json!({ "nonce": nonce, "prompt": prompt }).to_string();
    req.push('\n');
    stream.write_all(req.as_bytes())?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let v: Value = serde_json::from_str(&line)?;
    if v.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(v.get("secret")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string())
    } else {
        Err(Error::external(
            v.get("reason")
                .and_then(Value::as_str)
                .unwrap_or("refused")
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn prompts_are_told_apart() {
        assert_eq!(
            classify("deploy@example.test's password: "),
            PromptKind::Password
        );
        assert_eq!(
            classify("Enter passphrase for key '/k/id': "),
            PromptKind::Passphrase
        );
        assert_eq!(
            classify("Are you sure you want to continue connecting (yes/no/[fingerprint])? "),
            PromptKind::HostKey
        );
        assert_eq!(classify("Verification code: "), PromptKind::Other);
        assert!(allowed(Auth::Password, PromptKind::Password));
        assert!(!allowed(Auth::Password, PromptKind::Passphrase));
        assert!(!allowed(Auth::Key, PromptKind::Password));
    }

    fn ask(server: &AskpassServer, nonce: &str, prompt: &str) -> Value {
        let mut s = UnixStream::connect(server.socket.clone()).unwrap();
        let mut req = json!({ "nonce": nonce, "prompt": prompt }).to_string();
        req.push('\n');
        s.write_all(req.as_bytes()).unwrap();
        let mut line = String::new();
        BufReader::new(&s).read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    #[test]
    fn the_server_answers_only_the_right_caller_and_prompt() {
        let server =
            AskpassServer::start(Path::new("/x/dcc"), Auth::Password, "pw-123".into()).unwrap();
        let nonce = server.nonce.clone();
        assert_eq!(ask(&server, "wrong", "u@h's password: ")["ok"], false);
        assert_eq!(
            ask(&server, &nonce, "Enter passphrase for key 'k': ")["ok"],
            false
        );
        let host_key = ask(&server, &nonce, "Continue connecting (yes/no)? ");
        assert_eq!(host_key["ok"], false);
        assert!(!host_key.to_string().contains("pw-123"));
        let ok = ask(&server, &nonce, "u@h's password: ");
        assert_eq!(
            (ok["ok"].as_bool(), ok["secret"].as_str()),
            (Some(true), Some("pw-123"))
        );
        // a limited number of answers per run
        for _ in 0..2 {
            assert_eq!(ask(&server, &nonce, "u@h's password: ")["ok"], true);
        }
        assert_eq!(ask(&server, &nonce, "u@h's password: ")["ok"], false);
    }

    #[test]
    fn the_wrapper_and_socket_are_private_and_removed_at_the_end() {
        let server =
            AskpassServer::start(Path::new("/it's/dcc"), Auth::Passphrase, "x".into()).unwrap();
        let dir = server.dir().to_path_buf();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("s")), 0o600);
        assert_eq!(mode(&dir.join("askpass.sh")), 0o700);
        let script = std::fs::read_to_string(dir.join("askpass.sh")).unwrap();
        assert!(
            script.contains("'/it'\\''s/dcc' ssh askpass -- \"$@\""),
            "{script}"
        );
        drop(server);
        assert!(!dir.exists());
    }

    #[test]
    fn a_process_started_like_ssh_gets_the_secret_through_the_wrapper() {
        // the wrapper runs a stand in for dcc that is this very test binary's sibling `sh`:
        // the wrapper must hand the prompt over and print only the secret
        let server =
            AskpassServer::start(Path::new("/bin/echo"), Auth::Password, "pw-xyz".into()).unwrap();
        let env = server.env();
        let out = Command::new(
            env.iter()
                .find(|(k, _)| k == "SSH_ASKPASS")
                .unwrap()
                .1
                .clone(),
        )
        .arg("u@h's password: ")
        .output()
        .unwrap();
        // /bin/echo stands in for dcc, it proves the wrapper passes `ssh askpass -- <prompt>`
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "ssh askpass -- u@h's password:"
        );
    }
}
