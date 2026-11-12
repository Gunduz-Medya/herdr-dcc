//! A saved command is stored in clear text in SQLite and sent to terminals, so it must not carry
//! a secret. The check looks for the usual shapes. It names the kind it found, never the text.

fn is_reference(v: &str) -> bool {
    let v = v.trim_matches(['"', '\'']);
    v.starts_with('$') || v.starts_with("<") || v.starts_with("${") || v.is_empty()
}

fn secret_key(key: &str) -> bool {
    let k = key.trim_start_matches('-').to_ascii_lowercase();
    [
        "password",
        "passwd",
        "passphrase",
        "secret",
        "token",
        "apikey",
        "api_key",
        "api-key",
        "pwd",
    ]
    .iter()
    .any(|w| k.contains(w))
}

fn alnum_run(s: &str, min: usize) -> bool {
    s.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        .count()
        >= min
}

/// Returns what kind of secret the command looks like to carry, `None` when it looks clean.
pub fn find_secret(command: &str) -> Option<&'static str> {
    if command.contains("-----BEGIN") {
        return Some("private key");
    }
    for (prefix, min, kind) in [
        ("AKIA", 16, "cloud access key"),
        ("ghp_", 20, "access token"),
        ("gho_", 20, "access token"),
        ("github_pat_", 20, "access token"),
        ("glpat-", 16, "access token"),
        ("xoxb-", 10, "access token"),
        ("xoxp-", 10, "access token"),
        ("sk-", 20, "api key"),
    ] {
        let mut rest = command;
        while let Some(i) = rest.find(prefix) {
            let after = &rest[i + prefix.len()..];
            let boundary = i == 0 || !rest[..i].ends_with(|c: char| c.is_ascii_alphanumeric());
            if boundary && alnum_run(after, min) {
                return Some(kind);
            }
            rest = after;
        }
    }
    let lower = command.to_ascii_lowercase();
    if let Some(i) = lower.find("bearer ")
        && alnum_run(&command[i + 7..], 16)
    {
        return Some("bearer token");
    }
    // user:password@host inside a url
    for part in command.split_whitespace() {
        if let Some(i) = part.find("://") {
            let rest = &part[i + 3..];
            if let Some(at) = rest.find('@') {
                let cred = &rest[..at];
                if let Some((_, pass)) = cred.split_once(':')
                    && !is_reference(pass)
                {
                    return Some("password in a url");
                }
            }
        }
    }
    let words: Vec<&str> = command.split_whitespace().collect();
    for (i, w) in words.iter().enumerate() {
        if let Some((k, v)) = w.split_once('=')
            && secret_key(k)
            && !is_reference(v)
        {
            return Some("secret assignment");
        }
        // `--password value` and `sshpass -p value`
        if w.starts_with("--")
            && secret_key(w)
            && !w.contains('=')
            && words
                .get(i + 1)
                .is_some_and(|n| !n.starts_with('-') && !is_reference(n))
        {
            return Some("secret option");
        }
        if *w == "sshpass" {
            return Some("sshpass");
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_usual_shapes() {
        for (cmd, kind) in [
            ("export DB_PASSWORD=hunter2 && run", "secret assignment"),
            ("mysql --password hunter2 -u root", "secret option"),
            (
                "curl -H 'Authorization: Bearer abcdefghijklmnopqrstuvwxyz'",
                "bearer token",
            ),
            (
                "git clone https://me:hunter2@example.com/r.git",
                "password in a url",
            ),
            ("aws s3 ls --key AKIAABCDEFGHIJKLMNOP", "cloud access key"),
            ("echo ghp_abcdefghijklmnopqrstuvwxyz0123", "access token"),
            ("sshpass -p x ssh host", "sshpass"),
            (
                "cat <<EOF\n-----BEGIN OPENSSH PRIVATE KEY-----",
                "private key",
            ),
            ("API_TOKEN=abc123 ./deploy", "secret assignment"),
        ] {
            assert_eq!(find_secret(cmd), Some(kind), "{cmd}");
        }
    }

    #[test]
    fn leaves_ordinary_commands_and_references_alone() {
        for cmd in [
            "uptime",
            "tail -f /var/log/syslog",
            "docker compose up -d",
            "export DB_PASSWORD=$DB_PASSWORD && run",
            "mysql --password \"$MYSQL_PWD\"",
            "ls /tmp/task-sk-notes",
            "git log --oneline",
            "echo password",
            "cat passwords.txt",
            "curl https://example.com/path",
        ] {
            assert_eq!(find_secret(cmd), None, "{cmd}");
        }
    }
}
