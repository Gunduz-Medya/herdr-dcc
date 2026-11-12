//! Profiles, forwards and saved commands in SQLite. No secret and no session content is stored.

use rusqlite::{OptionalExtension, Row, params};

use super::model::{
    Auth, Forward, ForwardKind, HostKeyPolicy, Profile, SavedCommand, validate_name,
};
use crate::error::{Error, Result};
use crate::store::Store;
use crate::util::now_ms;

fn profile_from(r: &Row) -> rusqlite::Result<(Profile, String, String, i64)> {
    Ok((
        Profile {
            name: r.get("name")?,
            group: r.get("grp")?,
            host: r.get("host")?,
            port: u16::try_from(r.get::<_, i64>("port")?).unwrap_or(0),
            user: r.get("user")?,
            identity_file: r.get("identity_file")?,
            auth: Auth::Key,
            host_key_policy: HostKeyPolicy::Strict,
            config_file: r.get("config_file")?,
        },
        r.get("auth")?,
        r.get("host_key_policy")?,
        r.get("created_ms")?,
    ))
}

fn finish(t: (Profile, String, String, i64)) -> Result<Profile> {
    let (mut p, auth, policy, _) = t;
    p.auth = Auth::parse(&auth)?;
    p.host_key_policy = HostKeyPolicy::parse(&policy)?;
    Ok(p)
}

fn forward_from(r: &Row) -> rusqlite::Result<(Forward, String)> {
    Ok((
        Forward {
            id: r.get("id")?,
            profile: r.get("profile")?,
            name: r.get("name")?,
            kind: ForwardKind::Local,
            bind_host: r.get("bind_host")?,
            bind_port: u16::try_from(r.get::<_, i64>("bind_port")?).unwrap_or(0),
            target_host: r.get("target_host")?,
            target_port: u16::try_from(r.get::<_, i64>("target_port")?).unwrap_or(0),
        },
        r.get("kind")?,
    ))
}

impl Store {
    pub fn add_ssh_group(&self, name: &str) -> Result<()> {
        validate_name("group", name)?;
        let n = self.conn().execute(
            "INSERT OR IGNORE INTO ssh_groups (name) VALUES (?1)",
            params![name],
        )?;
        if n == 0 {
            return Err(Error::conflict(format!("group {name} exists")));
        }
        Ok(())
    }

    pub fn ssh_groups(&self) -> Result<Vec<String>> {
        let mut st = self
            .conn()
            .prepare("SELECT name FROM ssh_groups ORDER BY name")?;
        let rows = st.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn add_ssh_profile(&self, p: &Profile) -> Result<()> {
        p.validate()?;
        if let Some(g) = &p.group
            && !self.ssh_groups()?.contains(g)
        {
            return Err(Error::not_found(format!("no group {g}, add it first")));
        }
        let n = self.conn().execute(
            "INSERT OR IGNORE INTO ssh_profiles
             (name, grp, host, port, user, identity_file, auth, host_key_policy, config_file, created_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                p.name,
                p.group,
                p.host,
                p.port,
                p.user,
                p.identity_file,
                p.auth.as_str(),
                p.host_key_policy.as_str(),
                p.config_file,
                now_ms()
            ],
        )?;
        if n == 0 {
            return Err(Error::conflict(format!("profile {} exists", p.name)));
        }
        Ok(())
    }

    pub fn ssh_profile(&self, name: &str) -> Result<Profile> {
        self.conn()
            .query_row(
                "SELECT * FROM ssh_profiles WHERE name = ?1",
                params![name],
                profile_from,
            )
            .optional()?
            .map(finish)
            .transpose()?
            .ok_or_else(|| Error::not_found(format!("no profile {name}")))
    }

    pub fn ssh_profiles(&self) -> Result<Vec<Profile>> {
        let mut st = self
            .conn()
            .prepare("SELECT * FROM ssh_profiles ORDER BY grp IS NULL, grp, name")?;
        let rows = st.query_map([], profile_from)?;
        rows.map(|r| finish(r?)).collect()
    }

    /// Forwards and saved commands of the profile go with it (forwards) or lose the link (commands).
    pub fn delete_ssh_profile(&self, name: &str) -> Result<()> {
        let n = self
            .conn()
            .execute("DELETE FROM ssh_profiles WHERE name = ?1", params![name])?;
        if n == 0 {
            return Err(Error::not_found(format!("no profile {name}")));
        }
        Ok(())
    }

    pub fn add_forward(&self, f: &Forward) -> Result<()> {
        f.validate()?;
        self.ssh_profile(&f.profile)?;
        let n = self.conn().execute(
            "INSERT OR IGNORE INTO port_forwards
             (profile, name, kind, bind_host, bind_port, target_host, target_port)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                f.profile,
                f.name,
                f.kind.as_str(),
                f.bind_host,
                f.bind_port,
                f.target_host,
                f.target_port
            ],
        )?;
        if n == 0 {
            return Err(Error::conflict(format!(
                "profile {} already has a forward called {}",
                f.profile, f.name
            )));
        }
        Ok(())
    }

    pub fn forward(&self, profile: &str, name: &str) -> Result<Forward> {
        let t = self
            .conn()
            .query_row(
                "SELECT * FROM port_forwards WHERE profile = ?1 AND name = ?2",
                params![profile, name],
                forward_from,
            )
            .optional()?
            .ok_or_else(|| Error::not_found(format!("no forward {name} on {profile}")))?;
        let (mut f, kind) = t;
        f.kind = ForwardKind::parse(&kind)?;
        Ok(f)
    }

    pub fn forwards(&self) -> Result<Vec<Forward>> {
        let mut st = self
            .conn()
            .prepare("SELECT * FROM port_forwards ORDER BY profile, name")?;
        let rows = st.query_map([], forward_from)?;
        rows.map(|r| {
            let (mut f, kind) = r?;
            f.kind = ForwardKind::parse(&kind)?;
            Ok(f)
        })
        .collect()
    }

    pub fn delete_forward(&self, profile: &str, name: &str) -> Result<()> {
        let n = self.conn().execute(
            "DELETE FROM port_forwards WHERE profile = ?1 AND name = ?2",
            params![profile, name],
        )?;
        if n == 0 {
            return Err(Error::not_found(format!("no forward {name} on {profile}")));
        }
        Ok(())
    }

    pub fn add_saved_command(&self, c: &SavedCommand) -> Result<()> {
        validate_name("command", &c.name)?;
        if let Some(p) = &c.profile {
            self.ssh_profile(p)?;
        }
        let n = self.conn().execute(
            "INSERT OR IGNORE INTO saved_commands (name, command, profile, created_ms) VALUES (?1,?2,?3,?4)",
            params![c.name, c.command, c.profile, now_ms()],
        )?;
        if n == 0 {
            return Err(Error::conflict(format!("command {} exists", c.name)));
        }
        Ok(())
    }

    pub fn saved_command(&self, name: &str) -> Result<SavedCommand> {
        self.conn()
            .query_row(
                "SELECT name, command, profile FROM saved_commands WHERE name = ?1",
                params![name],
                |r| {
                    Ok(SavedCommand {
                        name: r.get(0)?,
                        command: r.get(1)?,
                        profile: r.get(2)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| Error::not_found(format!("no command {name}")))
    }

    pub fn saved_commands(&self) -> Result<Vec<SavedCommand>> {
        let mut st = self
            .conn()
            .prepare("SELECT name, command, profile FROM saved_commands ORDER BY name")?;
        let rows = st.query_map([], |r| {
            Ok(SavedCommand {
                name: r.get(0)?,
                command: r.get(1)?,
                profile: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn delete_saved_command(&self, name: &str) -> Result<()> {
        let n = self
            .conn()
            .execute("DELETE FROM saved_commands WHERE name = ?1", params![name])?;
        if n == 0 {
            return Err(Error::not_found(format!("no command {name}")));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::tests::profile;
    use super::*;

    fn forward(profile: &str, name: &str) -> Forward {
        Forward {
            id: 0,
            profile: profile.into(),
            name: name.into(),
            kind: ForwardKind::Local,
            bind_host: "127.0.0.1".into(),
            bind_port: 8080,
            target_host: "10.0.0.1".into(),
            target_port: 80,
        }
    }

    #[test]
    fn profiles_round_trip_and_stay_unique() {
        let s = Store::open_in_memory().unwrap();
        s.add_ssh_group("work").unwrap();
        let mut p = profile();
        p.group = Some("work".into());
        p.auth = Auth::Password;
        s.add_ssh_profile(&p).unwrap();
        assert_eq!(s.ssh_profile("prod").unwrap(), p);
        assert_eq!(s.add_ssh_profile(&p).unwrap_err().kind, "conflict");
        assert_eq!(s.ssh_profiles().unwrap().len(), 1);
        assert_eq!(s.add_ssh_group("work").unwrap_err().kind, "conflict");
        let mut q = profile();
        q.name = "other".into();
        q.group = Some("missing".into());
        assert_eq!(s.add_ssh_profile(&q).unwrap_err().kind, "not_found");
        assert_eq!(s.ssh_profile("nope").unwrap_err().kind, "not_found");
    }

    #[test]
    fn invalid_profiles_are_not_stored() {
        let s = Store::open_in_memory().unwrap();
        let mut p = profile();
        p.host = "-oProxyCommand=x".into();
        assert_eq!(s.add_ssh_profile(&p).unwrap_err().kind, "usage");
        assert!(s.ssh_profiles().unwrap().is_empty());
    }

    #[test]
    fn forwards_follow_their_profile_and_commands_keep_living() {
        let s = Store::open_in_memory().unwrap();
        s.add_ssh_profile(&profile()).unwrap();
        s.add_forward(&forward("prod", "web")).unwrap();
        assert_eq!(
            s.add_forward(&forward("prod", "web")).unwrap_err().kind,
            "conflict"
        );
        assert_eq!(
            s.add_forward(&forward("ghost", "web")).unwrap_err().kind,
            "not_found"
        );
        assert_eq!(s.forward("prod", "web").unwrap().bind_port, 8080);
        s.add_saved_command(&SavedCommand {
            name: "uptime".into(),
            command: "uptime".into(),
            profile: Some("prod".into()),
        })
        .unwrap();
        s.delete_ssh_profile("prod").unwrap();
        assert!(s.forwards().unwrap().is_empty());
        let c = s.saved_command("uptime").unwrap();
        assert_eq!(c.profile, None);
        s.delete_saved_command("uptime").unwrap();
        assert_eq!(
            s.delete_saved_command("uptime").unwrap_err().kind,
            "not_found"
        );
    }
}
