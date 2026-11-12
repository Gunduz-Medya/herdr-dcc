//! Profiles, forwards and the ssh command lines built from them. Every value is checked when it
//! is stored and again when the command line is built, and the arguments go to `ssh` as separate
//! argv items, never through a shell.

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    /// Key without passphrase.
    Key,
    /// Key with a passphrase kept in the Keychain.
    Passphrase,
    /// Account password kept in the Keychain.
    Password,
}

impl Auth {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "key" => Ok(Self::Key),
            "passphrase" => Ok(Self::Passphrase),
            "password" => Ok(Self::Password),
            other => Err(Error::usage(format!(
                "auth is key, passphrase or password, not {other}"
            ))),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Passphrase => "passphrase",
            Self::Password => "password",
        }
    }
    pub fn needs_secret(self) -> bool {
        self != Self::Key
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyPolicy {
    /// Unknown host keys are refused when ssh would ask (the default).
    Strict,
    /// A first seen host key is stored, a changed one is still refused.
    AcceptNew,
}

impl HostKeyPolicy {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "strict" => Ok(Self::Strict),
            "accept-new" => Ok(Self::AcceptNew),
            other => Err(Error::usage(format!(
                "host key policy is strict or accept-new, not {other}"
            ))),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::AcceptNew => "accept-new",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub identity_file: Option<String>,
    pub auth: Auth,
    pub host_key_policy: HostKeyPolicy,
    /// `ssh -F`. Empty means the user's own configuration.
    pub config_file: Option<String>,
}

impl Profile {
    /// Name of the Keychain item that holds the password or passphrase.
    pub fn secret_name(&self) -> String {
        format!("ssh.{}", self.name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardKind {
    Local,
    Remote,
    Dynamic,
}

impl ForwardKind {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "local" => Ok(Self::Local),
            "remote" => Ok(Self::Remote),
            "dynamic" => Ok(Self::Dynamic),
            other => Err(Error::usage(format!(
                "forward kind is local, remote or dynamic, not {other}"
            ))),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
            Self::Dynamic => "dynamic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forward {
    pub id: i64,
    pub profile: String,
    pub name: String,
    pub kind: ForwardKind,
    pub bind_host: String,
    pub bind_port: u16,
    /// Empty for a dynamic forward.
    pub target_host: String,
    pub target_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedCommand {
    pub name: String,
    pub command: String,
    pub profile: Option<String>,
}

fn is_name(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}

pub fn validate_name(what: &str, s: &str) -> Result<()> {
    if is_name(s, 48) {
        Ok(())
    } else {
        Err(Error::usage(format!(
            "{what} name must be 1 to 48 characters of a-z, 0-9, dash, underscore or dot"
        )))
    }
}

fn host_chars(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 253
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'))
}

pub fn validate_host(s: &str) -> Result<()> {
    if host_chars(s) {
        Ok(())
    } else {
        Err(Error::usage(
            "host may hold letters, digits, dot, dash, underscore and colon, and must not start with a dash",
        ))
    }
}

pub fn validate_user(s: &str) -> Result<()> {
    let ok = !s.is_empty()
        && s.len() <= 64
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if ok {
        Ok(())
    } else {
        Err(Error::usage(
            "user may hold letters, digits, dot, dash and underscore, and must not start with a dash",
        ))
    }
}

pub fn validate_path(what: &str, s: &str) -> Result<()> {
    let ok = s.starts_with('/') && s.len() <= 1024 && !s.chars().any(|c| c.is_control());
    if ok {
        Ok(())
    } else {
        Err(Error::usage(format!(
            "{what} must be an absolute path without control characters"
        )))
    }
}

pub fn validate_port(p: u16) -> Result<()> {
    if p == 0 {
        Err(Error::usage("port must be 1 to 65535"))
    } else {
        Ok(())
    }
}

impl Profile {
    pub fn validate(&self) -> Result<()> {
        validate_name("profile", &self.name)?;
        if let Some(g) = &self.group {
            validate_name("group", g)?;
        }
        validate_host(&self.host)?;
        validate_user(&self.user)?;
        validate_port(self.port)?;
        if let Some(p) = &self.identity_file {
            validate_path("identity file", p)?;
        }
        if let Some(p) = &self.config_file {
            validate_path("config file", p)?;
        }
        if self.auth == Auth::Passphrase && self.identity_file.is_none() {
            return Err(Error::usage("auth passphrase needs --identity-file"));
        }
        Ok(())
    }

    /// Options shared by a session and a forward.
    fn base_args(&self) -> Vec<String> {
        let mut a: Vec<String> = Vec::new();
        if let Some(c) = &self.config_file {
            a.extend(["-F".into(), c.clone()]);
        }
        a.extend(["-p".into(), self.port.to_string()]);
        if let Some(i) = &self.identity_file {
            a.extend([
                "-i".into(),
                i.clone(),
                "-o".into(),
                "IdentitiesOnly=yes".into(),
            ]);
        }
        a.extend([
            "-o".into(),
            format!(
                "StrictHostKeyChecking={}",
                match self.host_key_policy {
                    HostKeyPolicy::Strict => "yes",
                    HostKeyPolicy::AcceptNew => "accept-new",
                }
            ),
        ]);
        match self.auth {
            Auth::Key => {}
            Auth::Passphrase => {
                a.extend([
                    "-o".into(),
                    "PreferredAuthentications=publickey".into(),
                    "-o".into(),
                    "NumberOfPasswordPrompts=1".into(),
                ]);
            }
            Auth::Password => {
                a.extend([
                    "-o".into(),
                    "PreferredAuthentications=password,keyboard-interactive".into(),
                    "-o".into(),
                    "PubkeyAuthentication=no".into(),
                    "-o".into(),
                    "NumberOfPasswordPrompts=1".into(),
                ]);
            }
        }
        a
    }

    /// Arguments for an interactive session (everything after the program name).
    pub fn session_args(&self) -> Result<Vec<String>> {
        self.validate()?;
        let mut a = self.base_args();
        a.push(format!("{}@{}", self.user, self.host));
        Ok(a)
    }

    /// Arguments for a forward that runs without a remote command.
    pub fn forward_args(&self, f: &Forward) -> Result<Vec<String>> {
        self.validate()?;
        f.validate()?;
        let mut a = self.base_args();
        a.extend([
            "-N".into(),
            "-o".into(),
            "ExitOnForwardFailure=yes".into(),
            "-o".into(),
            "ServerAliveInterval=30".into(),
        ]);
        a.extend(f.spec_args());
        a.push(format!("{}@{}", self.user, self.host));
        Ok(a)
    }
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

impl Forward {
    pub fn validate(&self) -> Result<()> {
        validate_name("forward", &self.name)?;
        validate_host(&self.bind_host)?;
        validate_port(self.bind_port)?;
        // a forward that listens on every interface would open the remote network to the lan
        if !is_loopback(&self.bind_host) {
            return Err(Error::usage(
                "the bind address must be 127.0.0.1, localhost or ::1",
            ));
        }
        if self.kind != ForwardKind::Dynamic {
            validate_host(&self.target_host)?;
            validate_port(self.target_port)?;
        }
        Ok(())
    }

    /// `-L`, `-R` or `-D` with its value.
    pub fn spec_args(&self) -> Vec<String> {
        let bind = if self.bind_host.contains(':') {
            format!("[{}]:{}", self.bind_host, self.bind_port)
        } else {
            format!("{}:{}", self.bind_host, self.bind_port)
        };
        let target = if self.target_host.contains(':') {
            format!("[{}]:{}", self.target_host, self.target_port)
        } else {
            format!("{}:{}", self.target_host, self.target_port)
        };
        match self.kind {
            ForwardKind::Local => vec!["-L".into(), format!("{bind}:{target}")],
            ForwardKind::Remote => vec!["-R".into(), format!("{bind}:{target}")],
            ForwardKind::Dynamic => vec!["-D".into(), bind],
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn profile() -> Profile {
        Profile {
            name: "prod".into(),
            group: None,
            host: "example.test".into(),
            port: 22,
            user: "deploy".into(),
            identity_file: None,
            auth: Auth::Key,
            host_key_policy: HostKeyPolicy::Strict,
            config_file: None,
        }
    }

    fn forward(kind: ForwardKind) -> Forward {
        Forward {
            id: 1,
            profile: "prod".into(),
            name: "db".into(),
            kind,
            bind_host: "127.0.0.1".into(),
            bind_port: 5433,
            target_host: "10.0.0.5".into(),
            target_port: 5432,
        }
    }

    #[test]
    fn a_session_is_plain_argv_with_user_and_host_last() {
        let a = profile().session_args().unwrap();
        assert_eq!(a.last().unwrap(), "deploy@example.test");
        assert!(a.windows(2).any(|w| w == ["-p", "22"]));
        assert!(a.contains(&"StrictHostKeyChecking=yes".to_string()));
    }

    #[test]
    fn values_that_could_be_read_as_options_are_refused() {
        let mut p = profile();
        p.host = "-oProxyCommand=x".into();
        assert_eq!(p.session_args().unwrap_err().kind, "usage");
        let mut p = profile();
        p.user = "-l".into();
        assert!(p.validate().is_err());
        let mut p = profile();
        p.host = "a b".into();
        assert!(p.validate().is_err());
        let mut p = profile();
        p.identity_file = Some("relative/key".into());
        assert!(p.validate().is_err());
        let mut p = profile();
        p.auth = Auth::Passphrase;
        assert!(
            p.validate().is_err(),
            "a passphrase profile needs a key file"
        );
        let mut p = profile();
        p.name = "Has Space".into();
        assert!(p.validate().is_err());
    }

    #[test]
    fn password_and_passphrase_profiles_limit_the_methods_and_prompts() {
        let mut p = profile();
        p.auth = Auth::Password;
        let a = p.session_args().unwrap();
        assert!(a.contains(&"PubkeyAuthentication=no".to_string()));
        assert!(a.contains(&"NumberOfPasswordPrompts=1".to_string()));
        p.auth = Auth::Passphrase;
        p.identity_file = Some("/k/id".into());
        let a = p.session_args().unwrap();
        assert!(a.windows(2).any(|w| w == ["-i", "/k/id"]));
        assert!(a.contains(&"IdentitiesOnly=yes".to_string()));
    }

    #[test]
    fn forward_specs() {
        let p = profile();
        let l = p.forward_args(&forward(ForwardKind::Local)).unwrap();
        assert!(
            l.windows(2)
                .any(|w| w == ["-L", "127.0.0.1:5433:10.0.0.5:5432"])
        );
        assert!(l.contains(&"-N".to_string()));
        assert!(l.contains(&"ExitOnForwardFailure=yes".to_string()));
        let r = p.forward_args(&forward(ForwardKind::Remote)).unwrap();
        assert!(
            r.windows(2)
                .any(|w| w == ["-R", "127.0.0.1:5433:10.0.0.5:5432"])
        );
        let mut d = forward(ForwardKind::Dynamic);
        d.target_host.clear();
        d.target_port = 0;
        let d = p.forward_args(&d).unwrap();
        assert!(d.windows(2).any(|w| w == ["-D", "127.0.0.1:5433"]));
        assert_eq!(d.last().unwrap(), "deploy@example.test");
    }

    #[test]
    fn a_forward_may_only_listen_on_loopback() {
        let mut f = forward(ForwardKind::Local);
        f.bind_host = "0.0.0.0".into();
        assert_eq!(f.validate().unwrap_err().kind, "usage");
        f.bind_host = "::1".into();
        assert!(f.validate().is_ok());
        assert!(f.spec_args()[1].starts_with("[::1]:5433:"));
    }

    #[test]
    fn parsers_and_secret_names() {
        assert_eq!(Auth::parse("password").unwrap(), Auth::Password);
        assert!(Auth::parse("agent").is_err());
        assert!(HostKeyPolicy::parse("no").is_err());
        assert!(ForwardKind::parse("x").is_err());
        assert_eq!(profile().secret_name(), "ssh.prod");
        assert!(crate::secret::validate_name(&profile().secret_name()).is_ok());
    }
}
