//! SSH commands through the real `dcc` binary. No Herdr and no Keychain: key profiles need no
//! secret, and the password path is tested with the askpass server of the library and the real
//! helper binary.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

use dcc::ssh::askpass::AskpassServer;
use dcc::ssh::model::Auth;
use dcc::util::scratch_dir;
use serde_json::Value;

fn dcc(data: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_dcc"));
    c.arg("--data-dir")
        .arg(data)
        .env_remove("DCC_REQUIRE_ISOLATION")
        .env_remove("DCC_SSH_PROGRAM");
    c
}

fn ok(o: &Output) -> Value {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}

fn err(o: &Output) -> Value {
    assert!(!o.status.success());
    serde_json::from_slice(&o.stderr).unwrap()
}

fn add_profile(data: &Path, name: &str) -> Output {
    dcc(data)
        .args([
            "ssh",
            "profile",
            "add",
            name,
            "--host",
            "example.test",
            "--user",
            "deploy",
        ])
        .output()
        .unwrap()
}

#[test]
fn profiles_forwards_and_commands_through_the_cli() {
    let data = scratch_dir("ssh-cli");
    ok(&dcc(&data)
        .args(["ssh", "group", "add", "work"])
        .output()
        .unwrap());
    ok(&add_profile(&data, "prod"));
    assert_eq!(
        err(&add_profile(&data, "prod"))["error"]["kind"],
        "conflict"
    );
    let bad = dcc(&data)
        .args([
            "ssh",
            "profile",
            "add",
            "evil",
            "--host=-oProxyCommand=x",
            "--user",
            "u",
        ])
        .output()
        .unwrap();
    assert_eq!(err(&bad)["error"]["kind"], "usage");

    let list = ok(&dcc(&data)
        .args(["ssh", "profile", "list"])
        .output()
        .unwrap());
    assert_eq!(list["profiles"].as_array().unwrap().len(), 1);
    assert_eq!(list["profiles"][0]["host_key_policy"], "strict");

    let f = dcc(&data)
        .args([
            "ssh",
            "forward",
            "add",
            "prod",
            "web",
            "--kind",
            "local",
            "--bind-port",
            "8080",
            "--target",
            "10.0.0.1:80",
        ])
        .output()
        .unwrap();
    assert_eq!(ok(&f)["bind"], "127.0.0.1:8080");
    let public = dcc(&data)
        .args([
            "ssh",
            "forward",
            "add",
            "prod",
            "open",
            "--kind",
            "dynamic",
            "--bind-host",
            "0.0.0.0",
            "--bind-port",
            "1080",
        ])
        .output()
        .unwrap();
    assert_eq!(err(&public)["error"]["kind"], "usage");

    let c = dcc(&data)
        .args([
            "ssh",
            "command",
            "add",
            "uptime",
            "--profile",
            "prod",
            "uptime",
        ])
        .output()
        .unwrap();
    ok(&c);
    let leak = dcc(&data)
        .args([
            "ssh",
            "command",
            "add",
            "leak",
            "export API_TOKEN=abc123 && run",
        ])
        .output()
        .unwrap();
    let e = err(&leak);
    assert_eq!(e["error"]["kind"], "usage");
    assert!(!e.to_string().contains("abc123"));

    // deleting needs --confirm and takes the forwards with it
    let no = dcc(&data)
        .args(["ssh", "profile", "delete", "prod"])
        .output()
        .unwrap();
    assert_eq!(err(&no)["error"]["kind"], "usage");
    ok(&dcc(&data)
        .args(["ssh", "profile", "delete", "prod", "--confirm"])
        .output()
        .unwrap());
    let cmds = ok(&dcc(&data)
        .args(["ssh", "command", "list"])
        .output()
        .unwrap());
    assert_eq!(cmds["commands"][0]["profile"], Value::Null);
}

#[test]
fn secret_for_a_profile_needs_a_terminal() {
    let data = scratch_dir("ssh-secret-tty");
    let p = dcc(&data)
        .args([
            "ssh", "profile", "add", "pw", "--host", "h.test", "--user", "u", "--auth", "password",
        ])
        .output()
        .unwrap();
    assert_eq!(ok(&p)["secret_name"], "ssh.pw");
    let o = dcc(&data)
        .args(["ssh", "profile", "secret", "pw"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(err(&o)["error"]["kind"], "usage");
}

#[test]
fn the_runner_starts_ssh_with_the_profile_arguments_and_returns_its_exit_code() {
    let data = scratch_dir("ssh-run");
    ok(&add_profile(&data, "prod"));
    let fake = data.join("fake-ssh.sh");
    let out = data.join("argv.txt");
    fs::write(
        &fake,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do echo \"$a\" >> '{}'; done\necho \"askpass=${{SSH_ASKPASS:-none}}\" >> '{}'\nexit 7\n",
            out.display(),
            out.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let o = dcc(&data)
        .args(["ssh", "run", "prod"])
        .env("DCC_SSH_PROGRAM", &fake)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(7));
    let seen = fs::read_to_string(&out).unwrap();
    assert!(seen.lines().any(|l| l == "deploy@example.test"));
    assert!(seen.lines().any(|l| l == "StrictHostKeyChecking=yes"));
    assert!(
        seen.contains("askpass=none"),
        "a key profile must not set up askpass"
    );
}

#[test]
fn the_askpass_helper_prints_the_secret_only_for_the_right_prompt() {
    let server = AskpassServer::start(
        Path::new(env!("CARGO_BIN_EXE_dcc")),
        Auth::Password,
        "pw-for-test-123".into(),
    )
    .unwrap();
    let env = server.env();
    let wrapper = env
        .iter()
        .find(|(k, _)| k == "SSH_ASKPASS")
        .unwrap()
        .1
        .clone();
    let run = |prompt: &str| {
        let mut c = Command::new(&wrapper);
        c.arg(prompt).env_clear();
        for (k, v) in &env {
            c.env(k, v);
        }
        c.output().unwrap()
    };
    let good = run("deploy@example.test's password: ");
    assert!(good.status.success());
    assert_eq!(
        String::from_utf8_lossy(&good.stdout).trim(),
        "pw-for-test-123"
    );

    let host = run("Are you sure you want to continue connecting (yes/no/[fingerprint])? ");
    assert!(!host.status.success());
    assert!(host.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&host.stderr).contains("pw-for-test-123"));

    // without the nonce nothing comes out
    let mut c = Command::new(&wrapper);
    c.arg("u@h's password: ").env_clear();
    c.env(
        "DCC_ASKPASS_SOCKET",
        env.iter()
            .find(|(k, _)| k == "DCC_ASKPASS_SOCKET")
            .unwrap()
            .1
            .clone(),
    );
    c.env("DCC_ASKPASS_NONCE", "guess");
    let o = c.output().unwrap();
    assert!(!o.status.success() && o.stdout.is_empty());
}
