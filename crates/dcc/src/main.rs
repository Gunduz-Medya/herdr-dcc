//! The `dcc` command line. Every command prints one JSON document. Errors go to stderr as
//! `{"error":{"kind","message"}}` with a non zero exit code (2 usage, 3 not found, 1 the rest).

use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::process::{Command as Proc, ExitCode};

use clap::{Parser, Subcommand};
use serde_json::{Value, json};

use dcc::agent::launch::{LaunchParams, WorktreeMode};
use dcc::agent::record::{CliSource, RecordOptions};
use dcc::agent::{contract, exec, hooks, launch, record, statusline};
use dcc::config::{inventory, skillshare};
use dcc::herdr::api::{CliHerdr, Herdr};
use dcc::herdr::{compat, core, install, isolation, layout, mods};
use dcc::secret::{KeyringBackend, SERVICE, SecretBackend, set_with};
use dcc::ssh::model::{Auth, Forward, ForwardKind, HostKeyPolicy, Profile};
use dcc::ssh::{askpass, ops as ssh_ops};
use dcc::store::Store;
use dcc::util::now_ms;
use dcc::workspace::{ops, templates};
use dcc::{Error, Result, jobs};

#[derive(Parser)]
#[command(name = "dcc", about = "Developer Command Center companion")]
struct Cli {
    /// Folder for dcc's own data (SQLite file, job folders). Also read from DCC_DATA_DIR.
    #[arg(long, global = true, env = "DCC_DATA_DIR")]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Secrets in the OS keychain. There is no get.
    Secret {
        #[command(subcommand)]
        action: SecretAction,
    },
    /// Save, restore, rehydrate and close workspaces.
    Workspace {
        #[command(subcommand)]
        action: WorkspaceAction,
    },
    /// Detached jobs a caller with a 30 s limit can start and poll.
    Job {
        #[command(subcommand)]
        action: JobAction,
    },
    /// Herdr setup, isolation, layout, mods and core life cycle.
    Herdr {
        #[command(subcommand)]
        action: HerdrAction,
    },
    /// Agents: start them, record their hooks, show their runs.
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
    /// Neovim of a code tab.
    Code {
        #[command(subcommand)]
        action: CodeAction,
    },
    /// Context use and account quota as the agent's status line reported them.
    Usage {
        #[command(subcommand)]
        action: UsageAction,
    },
    /// AI tool configuration, read only, and Skillshare calls.
    Config {
        /// The home the AI tool configuration is read from. Also read from DCC_AI_HOME.
        #[arg(long, env = "DCC_AI_HOME")]
        home: Option<PathBuf>,
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// SSH profiles, port forwards and saved commands.
    Ssh {
        #[command(subcommand)]
        action: SshAction,
    },
    /// Read only git facts and worktrees.
    Git {
        #[command(subcommand)]
        action: GitAction,
    },
    /// Checks of the Herdr side that DCC depends on.
    Diag {
        #[command(subcommand)]
        action: DiagAction,
    },
    #[command(hide = true)]
    Debug {
        #[command(subcommand)]
        action: DebugAction,
    },
}

#[derive(Subcommand)]
enum CodeAction {
    /// Open files in the neovim of this code tab (needs DCC_NVIM_SOCKET, the shell pane of a
    /// code tab sets it and defines `e` for this command).
    Edit {
        #[arg(required = true)]
        files: Vec<String>,
    },
}

#[derive(Subcommand)]
enum UsageAction {
    /// Context per open run and the quota windows, each "unavailable" when there is no source.
    Show,
    /// Stored samples, only from the time DCC was running.
    History {
        /// context or quota.
        #[arg(long)]
        kind: String,
        #[arg(long)]
        window: Option<String>,
        #[arg(long, default_value_t = 24)]
        hours: i64,
        #[arg(long, default_value_t = 200)]
        limit: i64,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Files of the AI tool configuration (path, kind, size). No file is opened.
    Inventory,
    /// Skillshare as it is set up.
    Skillshare {
        #[command(subcommand)]
        action: SkillshareAction,
    },
    Sync {
        #[command(subcommand)]
        action: ConfigSyncAction,
    },
}

#[derive(Subcommand)]
enum SkillshareAction {
    Status,
    List,
}

#[derive(Subcommand)]
enum ConfigSyncAction {
    /// What a sync would do and the hash of that. Writes nothing.
    Preview,
    /// Back up the targets, then sync. Needs the hash of a preview. Run it as a job, it can take long.
    Apply {
        #[arg(long)]
        preview_hash: String,
        /// Approve removals separately.
        #[arg(long)]
        allow_delete: bool,
    },
}

#[derive(Subcommand)]
enum SshAction {
    Group {
        #[command(subcommand)]
        action: SshGroupAction,
    },
    Profile {
        #[command(subcommand)]
        action: SshProfileAction,
    },
    /// Everything the panel shows in one document (profiles, forwards with state, commands).
    Overview,
    /// Open a tab with an ssh session for a profile.
    Connect { profile: String },
    Forward {
        #[command(subcommand)]
        action: SshForwardAction,
    },
    Command {
        #[command(subcommand)]
        action: SshCommandAction,
    },
    /// Runs ssh for a profile in this terminal. Started by connect and forward start.
    #[command(hide = true)]
    Run {
        profile: String,
        #[arg(long)]
        forward: Option<String>,
    },
    /// The program ssh runs to ask for a password. Internal.
    #[command(hide = true)]
    Askpass {
        #[arg(last = true)]
        prompt: Vec<String>,
    },
}

#[derive(Subcommand)]
enum SshGroupAction {
    Add { name: String },
    List,
}

#[derive(Subcommand)]
enum SshProfileAction {
    Add {
        name: String,
        #[arg(long)]
        host: String,
        #[arg(long)]
        user: String,
        #[arg(long, default_value_t = 22)]
        port: u16,
        #[arg(long)]
        group: Option<String>,
        #[arg(long)]
        identity_file: Option<String>,
        /// key, passphrase or password.
        #[arg(long, default_value = "key")]
        auth: String,
        /// strict (default) or accept-new.
        #[arg(long, default_value = "strict")]
        host_key_policy: String,
        /// ssh -F file. Without it ssh reads the user's own configuration.
        #[arg(long)]
        config_file: Option<String>,
    },
    List,
    Show {
        name: String,
    },
    /// Remove a profile, its forwards and its Keychain item. Needs --confirm.
    Delete {
        name: String,
        #[arg(long)]
        confirm: bool,
    },
    /// Store the password or passphrase of a profile through a hidden prompt on a terminal.
    Secret {
        name: String,
    },
    /// Open a tab with that hidden prompt (for the mod, which has no terminal).
    SecretTab {
        name: String,
    },
}

#[derive(Subcommand)]
enum SshForwardAction {
    Add {
        profile: String,
        name: String,
        /// local, remote or dynamic.
        #[arg(long)]
        kind: String,
        #[arg(long, default_value = "127.0.0.1")]
        bind_host: String,
        #[arg(long)]
        bind_port: u16,
        /// host:port, not used by dynamic forwards.
        #[arg(long)]
        target: Option<String>,
    },
    /// Stored forwards with their state from the core.
    List,
    Start {
        profile: String,
        name: String,
    },
    Stop {
        profile: String,
        name: String,
    },
    Delete {
        profile: String,
        name: String,
    },
}

#[derive(Subcommand)]
enum SshCommandAction {
    Add {
        name: String,
        #[arg(long)]
        profile: Option<String>,
        command: String,
    },
    List,
    /// Type a saved command into a terminal.
    Run {
        name: String,
        #[arg(long)]
        terminal: String,
        #[arg(long)]
        force: bool,
    },
    Delete {
        name: String,
    },
}

#[derive(Subcommand)]
enum SecretAction {
    /// Store a secret through a hidden prompt on a terminal. Never from argv or a pipe.
    Set { name: String },
    /// Print whether a secret exists. Never prints the value.
    Exists { name: String },
    /// Delete a secret.
    Delete { name: String },
}

#[derive(Subcommand)]
enum WorkspaceAction {
    /// Save a live space (name or id). --startup TAB=CMD or TAB:2=CMD (repeatable).
    Save {
        space: String,
        #[arg(long)]
        startup: Vec<String>,
    },
    /// List saved definitions.
    List,
    /// Build the space of a saved definition. Refuses when a space of that name is open.
    Restore { name: String },
    /// Type the startup commands into the shells of an open space (after a core restart).
    Rehydrate { name: String },
    /// Close a space and end terminals it left running.
    Close { space: String },
    /// Templates.
    Template {
        #[command(subcommand)]
        action: TemplateAction,
    },
}

#[derive(Subcommand)]
enum TemplateAction {
    List,
    /// Make a definition from a template, save it and build its space.
    Apply {
        template: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        folder: String,
    },
}

#[derive(Subcommand)]
enum JobAction {
    /// Start `dcc <args>` detached. Prints {"id","dir"} at once.
    Start {
        #[arg(last = true, required = true)]
        args: Vec<String>,
    },
    #[command(hide = true)]
    Run {
        dir: PathBuf,
        #[arg(last = true, required = true)]
        args: Vec<String>,
    },
    /// Print {"state": running | done | dead | unknown, ...}.
    Status { id: String },
    /// Remove a job folder.
    Clean { id: String },
    /// Remove finished or dead job folders older than N seconds. A running job is never touched.
    Sweep {
        #[arg(long, default_value_t = 86400)]
        older_than_secs: i64,
    },
}

#[derive(Subcommand)]
enum HerdrAction {
    /// Show the isolation variables and the core socket path length. Fails when isolation is required and missing.
    CheckEnv,
    /// Compare the installed Herdr versions with compat.toml.
    Compat {
        #[arg(long, default_value = "compat.toml")]
        file: PathBuf,
    },
    /// Build DCC's own layout from the shipped one and a recipe.
    EnsureLayout {
        #[arg(long, default_value = "layouts/dcc.recipe.json")]
        recipe: PathBuf,
        #[arg(long)]
        force: bool,
        /// Also switch the running window to it and report the errors it shows.
        #[arg(long)]
        verify: bool,
    },
    /// Copy the DCC mods into the app home and check them.
    EnsureMods {
        #[arg(long, default_value = "mods")]
        src: PathBuf,
        #[arg(long)]
        no_check: bool,
    },
    /// Install DCC into the Herdr app home. Without --apply it only shows what it would do.
    /// With --apply it backs up what it replaces first.
    Install {
        #[arg(long)]
        apply: bool,
        #[arg(long, default_value = "mods")]
        mods: PathBuf,
        #[arg(long, default_value = "assets/tab-icons")]
        icons: PathBuf,
        #[arg(long, default_value = "layouts/dcc.recipe.json")]
        recipe: PathBuf,
        #[arg(long, default_value = "compat.toml")]
        compat: PathBuf,
    },
    /// Take DCC out again, from the backup an install made. Without --confirm it only lists.
    Uninstall {
        /// Folder name of the backup (printed by install).
        #[arg(long)]
        backup: String,
        #[arg(long)]
        confirm: bool,
    },
    /// Write the DCC tab bar (the installed tabs mod plus a button for each DCC panel).
    EnsureTabs {
        #[arg(long, default_value = "assets/tab-icons")]
        icons: PathBuf,
    },
    /// Core life cycle. Verification is by process id.
    Core {
        #[command(subcommand)]
        action: CoreAction,
    },
}

#[derive(Subcommand)]
enum CoreAction {
    /// Pids of the running core. Starts a core if none runs, so use it on purpose.
    Pids,
    /// Start the core of this session. Does nothing when one already runs.
    Start,
    /// Stop the core.
    Stop,
    /// Exit code 1 and the list when any of these processes still run.
    VerifyClosed {
        #[arg(required = true)]
        pids: Vec<u32>,
    },
}

#[derive(Subcommand)]
enum AgentAction {
    /// Read a Claude Code hook event on stdin and record ids and names only. Always exits 0.
    HookLog,
    /// Read the status line JSON on stdin, keep key names and types only, print one word.
    StatuslineLog,
    /// Start a program in a clean environment with DCC_RUN_ID set. The program replaces dcc.
    Exec {
        #[arg(long)]
        run_id: String,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Start a Claude Code agent: own worktree, settings file, labeled terminal, records.
    Launch {
        #[arg(long)]
        repo: PathBuf,
        /// auto (a new worktree and branch for this agent) or off (the repository folder).
        #[arg(long, default_value = "auto")]
        worktree: String,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        permission_mode: Option<String>,
        /// Space for the agent's tab, made when it does not exist.
        #[arg(long, default_value = "agents")]
        space: String,
        /// The agent program. Found on PATH when not given.
        #[arg(long)]
        program: Option<PathBuf>,
        #[arg(last = true)]
        extra: Vec<String>,
    },
    /// Update the records from the core and print the runs (open ones and recently ended ones).
    Snapshot {
        /// Show the result without writing it to the database.
        #[arg(long)]
        no_write: bool,
        /// How many ended runs to include.
        #[arg(long, default_value_t = 8)]
        closed: i64,
    },
}

#[derive(Subcommand)]
enum GitAction {
    /// Branch, worktree kind and change counts of a folder.
    Info { dir: PathBuf },
    /// Whether lazygit and neovim are installed, with the command that installs a missing one.
    /// dcc installs nothing.
    Tools,
    Worktree {
        #[command(subcommand)]
        action: WorktreeAction,
    },
}

#[derive(Subcommand)]
enum WorktreeAction {
    Add {
        repo: PathBuf,
        #[arg(long)]
        branch: String,
        path: PathBuf,
    },
    List {
        repo: PathBuf,
    },
    /// Remove a linked worktree. Needs --confirm. Git refuses one with changes.
    Remove {
        repo: PathBuf,
        path: PathBuf,
        #[arg(long)]
        confirm: bool,
    },
}

#[derive(Subcommand)]
enum DiagAction {
    /// The OSC 7501 contract test: a fixture agent reports its states and the core must show them.
    Contract {
        /// ok, or broken (a fixture whose reports are invalid, the test must fail).
        #[arg(long, default_value = "ok")]
        mode: String,
        #[arg(long)]
        fixture: Option<PathBuf>,
        #[arg(long, default_value_t = 1200)]
        step_ms: u64,
        #[arg(long, default_value_t = 25)]
        timeout_secs: u64,
    },
}

#[derive(Subcommand)]
enum DebugAction {
    /// Sleep, for the job tests.
    Sleep {
        #[arg(long, default_value_t = 1)]
        seconds: u64,
    },
}

fn emit(result: Result<Value>) -> ExitCode {
    match result {
        Ok(v) => {
            println!("{v}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}", e.to_json());
            ExitCode::from(e.exit_code())
        }
    }
}

/// The data folder. Given with `--data-dir` or `DCC_DATA_DIR`, else `~/.config/herdr-dcc` (next to
/// Herdr's own `~/.config/herdr-app`). Development with isolation required has no default, so a
/// test run can never fall back to the real folder.
fn data_dir(dir: &Option<PathBuf>) -> Result<PathBuf> {
    if let Some(d) = dir {
        return Ok(d.clone());
    }
    if isolation::required() {
        return Err(Error::usage(
            "no data folder, pass --data-dir <path> or set DCC_DATA_DIR",
        ));
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".config").join("herdr-dcc"))
        .ok_or_else(|| Error::usage("no data folder, pass --data-dir <path> or set DCC_DATA_DIR"))
}

fn app_home() -> Result<PathBuf> {
    if let Some(p) = isolation::current().app_home {
        return Ok(p);
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".config").join("herdr-app"))
        .ok_or_else(|| Error::usage("HERDR_APP_HOME and HOME are not set"))
}

fn secret(action: SecretAction) -> Result<Value> {
    let backend = KeyringBackend::new(SERVICE);
    match action {
        SecretAction::Set { name } => {
            if !std::io::stdin().is_terminal() {
                return Err(Error::usage(
                    "secret set needs a terminal for its hidden prompt. It reads nothing from argv or a pipe.",
                ));
            }
            set_with(&backend, &name, |prompt| rpassword::prompt_password(prompt))?;
            Ok(json!({ "name": name, "stored": true }))
        }
        SecretAction::Exists { name } => {
            dcc::secret::validate_name(&name)?;
            Ok(json!({ "name": name, "exists": backend.exists(&name)? }))
        }
        SecretAction::Delete { name } => {
            dcc::secret::validate_name(&name)?;
            if backend.delete(&name)? {
                Ok(json!({ "name": name, "deleted": true }))
            } else {
                Err(Error::not_found("no such secret"))
            }
        }
    }
}

/// The home the AI tool configuration is read from. `DCC_AI_HOME` picks another one. When isolation
/// is required (development) it must be set, so a test run never reaches the real home.
fn ai_home(given: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(h) = given {
        return Ok(h);
    }
    if isolation::required() {
        return Err(Error::isolation(
            "DCC_AI_HOME must point at a scratch home when isolation is required",
        ));
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| Error::usage("HOME is not set"))
}

fn config_cmd(dir: &Option<PathBuf>, home: Option<PathBuf>, action: ConfigAction) -> Result<Value> {
    let home = ai_home(home)?;
    match action {
        ConfigAction::Inventory => Ok(inventory::scan(&home)),
        ConfigAction::Skillshare { action } => {
            let r = skillshare::CliRunner::new(home)?;
            match action {
                SkillshareAction::Status => skillshare::status(&r),
                SkillshareAction::List => skillshare::list(&r),
            }
        }
        ConfigAction::Sync { action } => {
            let r = skillshare::CliRunner::new(home)?;
            match action {
                ConfigSyncAction::Preview => skillshare::preview(&r),
                ConfigSyncAction::Apply {
                    preview_hash,
                    allow_delete,
                } => {
                    let store = Store::open(&data_dir(dir)?)?;
                    skillshare::apply(&r, &store, &preview_hash, allow_delete)
                }
            }
        }
    }
}

fn split_target(t: &str) -> Result<(String, u16)> {
    let (host, port) = t
        .rsplit_once(':')
        .ok_or_else(|| Error::usage("--target is host:port"))?;
    let port = port
        .parse::<u16>()
        .map_err(|_| Error::usage("the target port must be 1 to 65535"))?;
    Ok((host.trim_matches(['[', ']']).to_string(), port))
}

fn ssh_cmd(dir: &Option<PathBuf>, action: SshAction) -> Result<Value> {
    let store = Store::open(&data_dir(dir)?)?;
    let backend = KeyringBackend::new(SERVICE);
    match action {
        SshAction::Group { action } => match action {
            SshGroupAction::Add { name } => {
                store.add_ssh_group(&name)?;
                Ok(json!({ "group": name }))
            }
            SshGroupAction::List => Ok(json!({ "groups": store.ssh_groups()? })),
        },
        SshAction::Profile { action } => match action {
            SshProfileAction::Add {
                name,
                host,
                user,
                port,
                group,
                identity_file,
                auth,
                host_key_policy,
                config_file,
            } => {
                let p = Profile {
                    name,
                    group,
                    host,
                    port,
                    user,
                    identity_file,
                    auth: Auth::parse(&auth)?,
                    host_key_policy: HostKeyPolicy::parse(&host_key_policy)?,
                    config_file,
                };
                store.add_ssh_profile(&p)?;
                Ok(
                    json!({ "profile": p.name, "secret_needed": p.auth.needs_secret(), "secret_name": p.secret_name() }),
                )
            }
            SshProfileAction::List => {
                let rows: Vec<Value> = store
                    .ssh_profiles()?
                    .iter()
                    .map(|p| profile_row(p, &backend))
                    .collect::<Result<_>>()?;
                Ok(json!({ "profiles": rows }))
            }
            SshProfileAction::Show { name } => profile_row(&store.ssh_profile(&name)?, &backend),
            SshProfileAction::Delete { name, confirm } => {
                let p = store.ssh_profile(&name)?;
                if !confirm {
                    return Err(Error::usage(
                        "deleting a profile removes its forwards and its Keychain item, pass --confirm",
                    ));
                }
                store.delete_ssh_profile(&name)?;
                let secret_removed = backend.delete(&p.secret_name())?;
                Ok(json!({ "deleted": name, "secret_removed": secret_removed }))
            }
            SshProfileAction::SecretTab { name } => ssh_ops::secret_tab(
                &store,
                &CliHerdr::new()?,
                &name,
                &std::env::current_exe()?,
                &abs_data_dir(dir)?,
            ),
            SshProfileAction::Secret { name } => {
                let p = store.ssh_profile(&name)?;
                if !p.auth.needs_secret() {
                    return Err(Error::usage(
                        "this profile uses a key without passphrase, there is no secret",
                    ));
                }
                if !std::io::stdin().is_terminal() {
                    return Err(Error::usage(
                        "profile secret needs a terminal for its hidden prompt. It reads nothing from argv or a pipe.",
                    ));
                }
                set_with(&backend, &p.secret_name(), |prompt| {
                    rpassword::prompt_password(prompt)
                })?;
                Ok(json!({ "profile": name, "stored": true }))
            }
        },
        SshAction::Overview => {
            let profiles: Vec<Value> = store
                .ssh_profiles()?
                .iter()
                .map(|p| profile_row(p, &backend))
                .collect::<Result<_>>()?;
            let (forwards, warning) =
                match CliHerdr::new().and_then(|h| ssh_ops::forwards_status(&store, &h)) {
                    Ok(v) => (v["forwards"].clone(), Value::Null),
                    Err(e) => (json!([]), json!(e.message)),
                };
            let commands: Vec<Value> = store
                .saved_commands()?
                .iter()
                .map(|c| json!({ "name": c.name, "command": c.command, "profile": c.profile }))
                .collect();
            Ok(
                json!({ "profiles": profiles, "forwards": forwards, "commands": commands, "warning": warning }),
            )
        }
        SshAction::Connect { profile } => {
            let h = CliHerdr::new()?;
            ssh_ops::connect(
                &store,
                &h,
                &backend,
                &profile,
                &std::env::current_exe()?,
                &abs_data_dir(dir)?,
            )
        }
        SshAction::Forward { action } => match action {
            SshForwardAction::Add {
                profile,
                name,
                kind,
                bind_host,
                bind_port,
                target,
            } => {
                let kind = ForwardKind::parse(&kind)?;
                let (target_host, target_port) = match (kind, target) {
                    (ForwardKind::Dynamic, None) => (String::new(), 0),
                    (ForwardKind::Dynamic, Some(_)) => {
                        return Err(Error::usage("a dynamic forward has no --target"));
                    }
                    (_, Some(t)) => split_target(&t)?,
                    (_, None) => return Err(Error::usage("--target host:port is required")),
                };
                let f = Forward {
                    id: 0,
                    profile,
                    name,
                    kind,
                    bind_host,
                    bind_port,
                    target_host,
                    target_port,
                };
                store.add_forward(&f)?;
                Ok(ssh_ops::forward_row(&f))
            }
            SshForwardAction::List => ssh_ops::forwards_status(&store, &CliHerdr::new()?),
            SshForwardAction::Start { profile, name } => ssh_ops::forward_start(
                &store,
                &CliHerdr::new()?,
                &backend,
                &profile,
                &name,
                &std::env::current_exe()?,
                &abs_data_dir(dir)?,
            ),
            SshForwardAction::Stop { profile, name } => {
                ssh_ops::forward_stop(&CliHerdr::new()?, &profile, &name)
            }
            SshForwardAction::Delete { profile, name } => {
                store.delete_forward(&profile, &name)?;
                Ok(json!({ "deleted": format!("{profile}/{name}") }))
            }
        },
        SshAction::Command { action } => match action {
            SshCommandAction::Add {
                name,
                profile,
                command,
            } => ssh_ops::add_command(&store, &name, &command, profile.as_deref()),
            SshCommandAction::List => {
                let rows: Vec<Value> = store
                    .saved_commands()?
                    .iter()
                    .map(|c| json!({ "name": c.name, "command": c.command, "profile": c.profile }))
                    .collect();
                Ok(json!({ "commands": rows }))
            }
            SshCommandAction::Run {
                name,
                terminal,
                force,
            } => ssh_ops::run_command(&store, &CliHerdr::new()?, &name, &terminal, force),
            SshCommandAction::Delete { name } => {
                store.delete_saved_command(&name)?;
                Ok(json!({ "deleted": name }))
            }
        },
        SshAction::Run { .. } | SshAction::Askpass { .. } => Err(Error::usage("internal command")),
    }
}

fn profile_row(p: &Profile, backend: &dyn SecretBackend) -> Result<Value> {
    let secret = if p.auth.needs_secret() {
        json!(backend.exists(&p.secret_name())?)
    } else {
        Value::Null
    };
    Ok(json!({
        "name": p.name, "group": p.group, "host": p.host, "port": p.port, "user": p.user,
        "identity_file": p.identity_file, "auth": p.auth.as_str(),
        "host_key_policy": p.host_key_policy.as_str(), "config_file": p.config_file,
        "secret_stored": secret,
    }))
}

fn abs_data_dir(dir: &Option<PathBuf>) -> Result<PathBuf> {
    let d = data_dir(dir)?;
    Ok(d.canonicalize().unwrap_or(d))
}

fn workspace(dir: &Option<PathBuf>, action: WorkspaceAction) -> Result<Value> {
    if let WorkspaceAction::Template {
        action: TemplateAction::List,
    } = &action
    {
        return Ok(json!({ "templates": templates::names() }));
    }
    let store = Store::open(&data_dir(dir)?)?;
    match action {
        WorkspaceAction::List => {
            let rows: Vec<Value> = store.workspaces()?.into_iter().map(|w| json!({ "name": w.name, "folder": w.folder, "kind": w.kind, "saved_ms": w.saved_ms })).collect();
            Ok(json!({ "workspaces": rows }))
        }
        WorkspaceAction::Save { space, startup } => {
            let def = ops::save_snapshot(&CliHerdr::new()?, &space, &startup)?;
            store.save_workspace(
                &def.name,
                &def.folder,
                &def.kind,
                &serde_json::to_string(&def)?,
            )?;
            Ok(json!({ "saved": def }))
        }
        WorkspaceAction::Restore { name } => {
            ops::restore_by_name(&store, &CliHerdr::new()?, &name, ops::Options::default())
        }
        WorkspaceAction::Rehydrate { name } => {
            ops::rehydrate_by_name(&store, &CliHerdr::new()?, &name)
        }
        WorkspaceAction::Close { space } => ops::close_space(&CliHerdr::new()?, &space),
        WorkspaceAction::Template {
            action:
                TemplateAction::Apply {
                    template,
                    name,
                    folder,
                },
        } => {
            let data = abs_data_dir(dir)?;
            dcc::util::create_private_dir(&data.join("nvim"))?;
            let link = templates::EditorLink {
                socket: templates::editor_socket(&data, &name),
                dcc: std::env::current_exe()?,
            };
            let def = templates::instantiate_linked(&template, &name, &folder, &link)?;
            let herdr = CliHerdr::new()?;
            let report = ops::build(&herdr, &def, ops::Options::default())?;
            store.save_workspace(
                &def.name,
                &def.folder,
                &def.kind,
                &serde_json::to_string(&def)?,
            )?;
            Ok(report)
        }
        WorkspaceAction::Template {
            action: TemplateAction::List,
        } => Ok(json!({ "templates": templates::names() })),
    }
}

fn herdr_status_errors() -> Result<Value> {
    let out = Proc::new("herdr-app")
        .arg("status")
        .output()
        .map_err(|e| Error::external(format!("cannot run herdr-app: {e}")))?;
    let v: Value = serde_json::from_slice(&out.stdout)?;
    Ok(v.get("errors").cloned().unwrap_or(Value::Null))
}

fn herdr_cmd(dir: &Option<PathBuf>, action: HerdrAction) -> Result<Value> {
    match action {
        HerdrAction::CheckEnv => {
            let iso = isolation::current();
            let state = isolation::state_home();
            let (len, ok) = match (&state, &iso.session) {
                (Some(s), Some(session)) => {
                    let len = isolation::holder_socket_len(s, session);
                    (Some(len), len <= isolation::MAX_SOCKET_BYTES)
                }
                _ => (None, true),
            };
            isolation::require()?;
            let report = json!({
                "isolation_required": isolation::required(),
                "app_home": iso.app_home.map(|p| p.to_string_lossy().to_string()),
                "session": iso.session,
                "state_home": state.map(|p| p.to_string_lossy().to_string()),
                "socket_path_estimate_bytes": len,
                "socket_limit_bytes": isolation::MAX_SOCKET_BYTES,
            });
            if ok {
                Ok(report)
            } else {
                Err(Error::isolation(format!(
                    "the core socket path would be about {} bytes, the limit is {}",
                    len.unwrap_or(0),
                    isolation::MAX_SOCKET_BYTES
                )))
            }
        }
        HerdrAction::Compat { file } => {
            let text = std::fs::read_to_string(&file)
                .map_err(|e| Error::not_found(format!("cannot read {}: {e}", file.display())))?;
            let pairs = compat::parse(&text);
            let (app, core) = (
                compat::program_version("herdr-app")?,
                compat::program_version("hc")?,
            );
            let supported = compat::is_supported(&pairs, &app, &core);
            Ok(
                json!({ "herdr_app": app, "herdr_core": core, "supported": supported,
                       "warning": if supported { Value::Null } else { json!("this version pair is not in compat.toml, run the contract test first") } }),
            )
        }
        HerdrAction::EnsureLayout {
            recipe,
            force,
            verify,
        } => {
            let store = Store::open(&data_dir(dir)?)?;
            let version = compat::program_version("herdr-app")?;
            let mut report = layout::ensure(&app_home()?, &recipe, &version, &store, force)?;
            if verify {
                let name = report["layout"].as_str().unwrap_or("dcc").to_string();
                let h = CliHerdr::new()?;
                h.app_do("layout.switch", json!({ "name": name }))?;
                std::thread::sleep(std::time::Duration::from_millis(1500));
                let errors = herdr_status_errors()?;
                report["verified"] = json!(errors.as_array().is_some_and(Vec::is_empty));
                report["status_errors"] = errors;
            }
            Ok(report)
        }
        HerdrAction::EnsureMods { src, no_check } => {
            let exe = std::env::current_exe()?;
            let data = data_dir(dir).ok().map(|d| d.canonicalize().unwrap_or(d));
            let ai_home = std::env::var("DCC_AI_HOME").ok();
            let config = json!({ "dcc": exe.to_string_lossy(), "data_dir": data.map(|d| d.to_string_lossy().to_string()), "ai_home": ai_home });
            mods::ensure(&src, &app_home()?, !no_check, Some(&config))
        }
        HerdrAction::Install {
            apply,
            mods,
            icons,
            recipe,
            compat,
        } => {
            let data = abs_data_dir(dir)?;
            let exe = install::exe_path()?;
            let home = app_home()?;
            let supported = std::fs::read_to_string(&compat).ok().and_then(|text| {
                let pairs = compat::parse(&text);
                Some(compat::is_supported(
                    &pairs,
                    &compat::program_version("herdr-app").ok()?,
                    &compat::program_version("hc").ok()?,
                ))
            });
            let p = install::Params {
                app_home: &home,
                mods_src: &mods,
                icons: &icons,
                recipe: &recipe,
                data_dir: &data,
                exe: &exe,
                herdr_supported: supported,
            };
            if apply {
                install::install(&p, &Store::open(&data)?)
            } else {
                install::plan(&p)
            }
        }
        HerdrAction::Uninstall { backup, confirm } => {
            install::uninstall(&app_home()?, &abs_data_dir(dir)?, &backup, confirm)
        }
        HerdrAction::EnsureTabs { icons } => dcc::herdr::tabbar::ensure(&app_home()?, &icons),
        HerdrAction::Core { action } => match action {
            CoreAction::Pids => Ok(json!({ "pids": core::pids()? })),
            CoreAction::Start => core::start(),
            CoreAction::Stop => core::stop(),
            CoreAction::VerifyClosed { pids } => {
                let alive = core::alive_of(&pids)?;
                if alive.is_empty() {
                    Ok(json!({ "closed": true, "checked": pids }))
                } else {
                    Err(Error::external(format!("still running: {alive:?}")))
                }
            }
        },
    }
}

/// A hook must never fail or slow an agent down. Everything is swallowed, errors are noted by kind only.
fn hook_entry(dir: &Option<PathBuf>, statusline: bool) -> String {
    let mut raw = String::new();
    let _ = std::io::stdin().take(1024 * 1024).read_to_string(&mut raw);
    let Some(dir) = dir else { return raw };
    let run_id = std::env::var("DCC_RUN_ID")
        .ok()
        .filter(|r| hooks::valid_session_id(r));
    let outcome: Result<()> = (|| {
        let run_id = run_id.ok_or_else(|| Error::usage("DCC_RUN_ID is not set"))?;
        if statusline {
            statusline::record(dir, &run_id, &raw)?;
            let store = Store::open_with_timeout(dir, std::time::Duration::from_millis(1000))?;
            store.record_usage(&run_id, &raw, now_ms())?;
            return Ok(());
        }
        let facts = hooks::parse(&raw)?;
        let store = Store::open_with_timeout(dir, std::time::Duration::from_millis(1000))?;
        hooks::record(&store, &run_id, &facts, now_ms())
    })();
    if let Err(e) = outcome {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("hook-errors.log"))
        {
            let _ = writeln!(f, "{} {}", now_ms(), e.kind);
        }
    }
    raw
}

fn agent_cmd(dir: &Option<PathBuf>, action: AgentAction) -> Result<Value> {
    match action {
        AgentAction::HookLog | AgentAction::StatuslineLog => unreachable!("handled in main"),
        AgentAction::Exec { run_id, command } => {
            let (program, args) = command
                .split_first()
                .ok_or_else(|| Error::usage("nothing to run"))?;
            let code = exec::run(&run_id, program, args)?;
            std::process::exit(code);
        }
        AgentAction::Launch {
            repo,
            worktree,
            branch,
            model,
            permission_mode,
            space,
            program,
            extra,
        } => {
            let worktree = match worktree.as_str() {
                "auto" => WorktreeMode::Auto,
                "off" => WorktreeMode::Off,
                other => {
                    return Err(Error::usage(format!(
                        "--worktree is auto or off, not {other}"
                    )));
                }
            };
            let data_dir = data_dir(dir)?;
            let store = Store::open(&data_dir)?;
            let params = LaunchParams {
                repo: repo.canonicalize().unwrap_or(repo),
                worktree,
                branch,
                model,
                permission_mode,
                extra_args: extra,
                space,
                program,
                dcc_exe: std::env::current_exe()?,
                data_dir: data_dir.canonicalize().unwrap_or(data_dir),
                herdr_session: isolation::current().session,
            };
            launch::launch(&store, &CliHerdr::new()?, &params, now_ms())
        }
        AgentAction::Snapshot { no_write, closed } => {
            isolation::require()?;
            let store = Store::open(&data_dir(dir)?)?;
            let opts = RecordOptions {
                write: !no_write,
                now_ms: now_ms(),
                closed_limit: closed,
                herdr_session: isolation::current().session,
            };
            record::snapshot(&store, &CliSource, &opts)
        }
    }
}

fn git_cmd(action: GitAction) -> Result<Value> {
    match action {
        GitAction::Info { dir } => Ok(dcc::git::info(&dir.to_string_lossy())
            .map(|g| g.to_json())
            .unwrap_or_else(|| json!({ "ok": false }))),
        GitAction::Tools => {
            let rows: Vec<Value> = dcc::git::tools()
                .iter()
                .map(|t| json!({ "name": t.name, "path": t.path.as_ref().map(|p| p.to_string_lossy().to_string()), "install": t.install }))
                .collect();
            Ok(json!({ "tools": rows }))
        }
        GitAction::Worktree { action } => match action {
            WorktreeAction::Add { repo, branch, path } => {
                dcc::git::worktree_add(&repo.to_string_lossy(), &branch, &path)?;
                Ok(json!({ "added": path.to_string_lossy(), "branch": branch }))
            }
            WorktreeAction::List { repo } => Ok(
                json!({ "worktrees": dcc::git::worktree_list(&repo.to_string_lossy())?.into_iter().map(|(p, b)| json!({ "path": p, "branch": b })).collect::<Vec<_>>() }),
            ),
            WorktreeAction::Remove {
                repo,
                path,
                confirm,
            } => {
                dcc::git::worktree_remove(
                    &repo.to_string_lossy(),
                    &path.to_string_lossy(),
                    confirm,
                )?;
                Ok(json!({ "removed": path.to_string_lossy() }))
            }
        },
    }
}

fn diag_cmd(dir: &Option<PathBuf>, action: DiagAction) -> Result<Value> {
    let DiagAction::Contract {
        mode,
        fixture,
        step_ms,
        timeout_secs,
    } = action;
    let herdr = CliHerdr::new()?;
    let fixture = match fixture {
        Some(f) => f,
        None => std::env::current_exe()?.with_file_name(if cfg!(windows) {
            "dcc-fixture-agent.exe"
        } else {
            "dcc-fixture-agent"
        }),
    };
    if !fixture.exists() {
        return Err(Error::not_found(format!(
            "fixture agent not found at {}, run cargo build first",
            fixture.display()
        )));
    }
    let probe = contract::CliProbe { herdr: &herdr };
    let report = contract::run(
        &probe,
        &fixture,
        &mode,
        step_ms,
        std::time::Duration::from_secs(timeout_secs),
        std::time::Duration::from_millis(150),
    )?;
    let (app, core) = (
        compat::program_version("herdr-app")?,
        compat::program_version("hc")?,
    );
    let store = Store::open(&data_dir(dir)?)?;
    let id = contract::record(&store, &report, &app, &core)?;
    let doc = json!({ "mode": mode, "recorded_run": id, "herdr_app": app, "herdr_core": core, "result": report.to_json() });
    if report.passed {
        Ok(doc)
    } else {
        Err(Error::new("contract", doc.to_string()))
    }
}

fn main() -> ExitCode {
    // Exit code 2 from a hook blocks the tool call in Claude Code, so a hook command that fails to
    // parse its own arguments (an old settings file after a dcc update) must still exit 0.
    let Cli {
        data_dir: dir,
        command,
    } = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let args: Vec<String> = std::env::args().collect();
            if args
                .iter()
                .any(|a| a == "hook-log" || a == "statusline-log")
            {
                if args.iter().any(|a| a == "statusline-log") {
                    println!("dcc");
                }
                return ExitCode::SUCCESS;
            }
            e.exit();
        }
    };
    let result = match command {
        Command::Job {
            action: JobAction::Run { dir: job_dir, args },
        } => {
            return match data_dir(&dir) {
                Ok(d) => ExitCode::from(jobs::run(&job_dir, &d, &args).clamp(0, 255) as u8),
                Err(e) => emit(Err(e)),
            };
        }
        Command::Debug {
            action: DebugAction::Sleep { seconds },
        } => {
            std::thread::sleep(std::time::Duration::from_secs(seconds));
            Ok(json!({ "slept": seconds }))
        }
        Command::Ssh {
            action: SshAction::Askpass { prompt },
        } => {
            return match askpass::answer(&prompt.join(" ")) {
                Ok(secret) => {
                    println!("{secret}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("dcc askpass: {}", e.message);
                    ExitCode::from(1)
                }
            };
        }
        Command::Ssh {
            action: SshAction::Run { profile, forward },
        } => {
            let outcome = data_dir(&dir).and_then(|d| {
                let store = Store::open(&d)?;
                ssh_ops::run_session(
                    &store,
                    &KeyringBackend::new(SERVICE),
                    &profile,
                    forward.as_deref(),
                    &std::env::current_exe()?,
                )
            });
            return match outcome {
                Ok(code) => ExitCode::from(code.clamp(0, 255) as u8),
                Err(e) => {
                    eprintln!("{}", e.to_json());
                    ExitCode::from(e.exit_code())
                }
            };
        }
        Command::Ssh { action } => ssh_cmd(&dir, action),
        Command::Code {
            action: CodeAction::Edit { files },
        } => std::env::current_dir()
            .map_err(Error::from)
            .and_then(|cwd| {
                dcc::code::edit(std::env::var(dcc::code::ENV_SOCKET).ok(), &cwd, &files)
            }),
        Command::Usage { action } => data_dir(&dir).and_then(|d| {
            let store = Store::open(&d)?;
            match action {
                UsageAction::Show => dcc::usage::show(&store, now_ms()),
                UsageAction::History {
                    kind,
                    window,
                    hours,
                    limit,
                } => {
                    if kind != "context" && kind != "quota" {
                        return Err(Error::usage("--kind is context or quota"));
                    }
                    let hours = hours.clamp(1, 24 * 30);
                    dcc::usage::history(
                        &store,
                        &kind,
                        window.as_deref(),
                        now_ms() - hours * 3600 * 1000,
                        limit.clamp(1, 2000),
                    )
                }
            }
        }),
        Command::Config { home, action } => config_cmd(&dir, home, action),
        Command::Secret { action } => secret(action),
        Command::Workspace { action } => workspace(&dir, action),
        Command::Job { action } => data_dir(&dir).and_then(|d| match action {
            JobAction::Start { args } => jobs::start(&d, &args),
            JobAction::Status { id } => jobs::status(&d, &id),
            JobAction::Clean { id } => jobs::clean(&d, &id),
            JobAction::Sweep { older_than_secs } => jobs::sweep(&d, older_than_secs),
            JobAction::Run { .. } => Err(Error::usage("job run is internal")),
        }),
        Command::Herdr { action } => herdr_cmd(&dir, action),
        Command::Agent {
            action: AgentAction::HookLog,
        } => {
            let _ = hook_entry(&dir, false);
            return ExitCode::SUCCESS;
        }
        Command::Agent {
            action: AgentAction::StatuslineLog,
        } => {
            let raw = hook_entry(&dir, true);
            println!("{}", statusline::display_line(&raw));
            return ExitCode::SUCCESS;
        }
        Command::Agent { action } => agent_cmd(&dir, action),
        Command::Git { action } => git_cmd(action),
        Command::Diag { action } => diag_cmd(&dir, action),
    };
    emit(result)
}
