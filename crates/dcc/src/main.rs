//! The `dcc` command line. Every command prints one JSON document. Errors go to stderr as
//! `{"error":{"kind","message"}}` with a non zero exit code (2 usage, 3 not found, 1 the rest).

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::{Command as Proc, ExitCode};

use clap::{Parser, Subcommand};
use serde_json::{Value, json};

use dcc::herdr::api::{CliHerdr, Herdr};
use dcc::herdr::{compat, core, isolation, layout, mods};
use dcc::secret::{KeyringBackend, SERVICE, SecretBackend, set_with};
use dcc::store::Store;
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
    #[command(hide = true)]
    Debug {
        #[command(subcommand)]
        action: DebugAction,
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

fn data_dir(dir: &Option<PathBuf>) -> Result<PathBuf> {
    dir.clone()
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
            let def = templates::instantiate(&template, &name, &folder)?;
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
            let config = json!({ "dcc": exe.to_string_lossy(), "data_dir": data.map(|d| d.to_string_lossy().to_string()) });
            mods::ensure(&src, &app_home()?, !no_check, Some(&config))
        }
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

fn main() -> ExitCode {
    let Cli {
        data_dir: dir,
        command,
    } = Cli::parse();
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
    };
    emit(result)
}
