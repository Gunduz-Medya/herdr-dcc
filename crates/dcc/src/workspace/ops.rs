//! Save, build, rehydrate and close. All of it goes through the `Herdr` trait, so the same code
//! runs against the real app and against the fake in tests.
//!
//! Herdr facts this relies on (Phase 0):
//! - `terminal.open {folder, place}` splits the focused pane, so a split is `pane.focus` first.
//! - `space.new` and `tab.new` answer with the new terminal, the pane id is read from `herdr-app ls`.
//! - After a core restart Herdr keeps the structure and the folders, every terminal is a plain
//!   shell again. `rehydrate` only types the startup commands then.
//! - `space.close` sometimes leaves terminals running, so `close_space` checks and ends them.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::{Value, json};

use super::def::{Definition, Node, TabDef};
use crate::error::{Error, Result};
use crate::herdr::api::{Herdr, SpaceInfo, cwd_map, is_plain_shell};
use crate::store::Store;

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// A saved folder that no longer exists makes Herdr refuse the command. With this on, such a
    /// folder is replaced by the project folder and a warning is reported.
    pub check_dirs: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { check_dirs: true }
    }
}

/// A space by id or by name. A name used by more than one space must be given as an id.
pub fn find_space(h: &dyn Herdr, selector: &str) -> Result<SpaceInfo> {
    let spaces = h.spaces()?;
    let mut hits: Vec<SpaceInfo> = spaces
        .into_iter()
        .filter(|s| s.id == selector || s.name == selector)
        .collect();
    match hits.len() {
        0 => Err(Error::not_found(format!(
            "no space named or numbered {selector}"
        ))),
        1 => Ok(hits.remove(0)),
        n => Err(Error::conflict(format!(
            "{n} spaces are called {selector}, pass the id"
        ))),
    }
}

/// A space by name only (never by id), refusing a name that more than one space uses. Used where
/// commands are typed into terminals, so a name that matches the id of another space cannot redirect them.
pub fn find_space_by_name(h: &dyn Herdr, name: &str) -> Result<SpaceInfo> {
    let mut hits: Vec<SpaceInfo> = h.spaces()?.into_iter().filter(|s| s.name == name).collect();
    match hits.len() {
        0 => Err(Error::not_found(format!("no open space called {name}"))),
        1 => Ok(hits.remove(0)),
        n => Err(Error::conflict(format!(
            "{n} spaces are called {name}, close or rename the extra ones first"
        ))),
    }
}

/// `TAB=CMD` runs CMD in the first pane of that tab, `TAB:2=CMD` in the second.
pub fn parse_startups(items: &[String]) -> Result<HashMap<(String, usize), String>> {
    let mut map = HashMap::new();
    for item in items {
        let (key, cmd) = item
            .split_once('=')
            .ok_or_else(|| Error::usage(format!("startup needs TAB=COMMAND, got {item}")))?;
        let (tab, leaf) = match key.rsplit_once(':') {
            Some((t, n)) if n.parse::<usize>().is_ok_and(|n| n >= 1) => {
                (t.to_string(), n.parse::<usize>().unwrap_or(1) - 1)
            }
            _ => (key.to_string(), 0),
        };
        map.insert((tab, leaf), cmd.to_string());
    }
    Ok(map)
}

/// Reads a live space into a snapshot definition. The tree is a row, see the module docs.
pub fn save_snapshot(h: &dyn Herdr, selector: &str, startups: &[String]) -> Result<Definition> {
    let space = find_space(h, selector)?;
    let cwds = cwd_map(&h.terminals()?);
    let ups = parse_startups(startups)?;
    let mut tabs = Vec::new();
    for (i, tab) in space.tabs.iter().enumerate() {
        let panes = tab
            .terminals
            .iter()
            .enumerate()
            .map(|(j, t)| {
                (
                    cwds.get(&t.terminal)
                        .cloned()
                        .unwrap_or_else(|| space.folder.clone()),
                    ups.get(&(tab.name.clone(), j)).cloned(),
                )
            })
            .collect();
        let tree = Node::row(panes)
            .ok_or_else(|| Error::conflict(format!("tab {} has no terminals", tab.name)))?;
        tabs.push(TabDef {
            name: tab.name.clone(),
            named: tab.name != (i + 1).to_string(),
            tree,
        });
    }
    for (tab, leaf) in ups.keys() {
        let ok = tabs
            .iter()
            .find(|t| &t.name == tab)
            .is_some_and(|t| *leaf < t.tree.leaves().len());
        if !ok {
            return Err(Error::usage(format!(
                "startup refers to pane {} of tab {tab}, which does not exist",
                leaf + 1
            )));
        }
    }
    Ok(Definition {
        name: space.name,
        folder: space.folder,
        kind: "snapshot".to_string(),
        tabs,
    })
}

struct Ctx<'a> {
    h: &'a dyn Herdr,
    project: String,
    opts: Options,
    warnings: Vec<String>,
    sent: Vec<Value>,
    skipped: Vec<Value>,
    terminals: Vec<String>,
    /// Terminals started in the project folder because their saved folder is gone. A startup
    /// command written for the saved folder is not typed there.
    substituted: HashSet<String>,
}

impl Ctx<'_> {
    /// The folder to start in, and whether it replaced the saved one.
    fn usable(&mut self, cwd: &str) -> (String, bool) {
        if !self.opts.check_dirs || Path::new(cwd).is_dir() {
            (cwd.to_string(), false)
        } else {
            self.warnings
                .push(format!("{cwd} is gone, used {}", self.project));
            (self.project.clone(), true)
        }
    }
}

fn pane_of(h: &dyn Herdr, terminal: &str) -> Result<String> {
    h.spaces()?
        .iter()
        .flat_map(|s| s.tabs.iter())
        .flat_map(|t| t.terminals.iter())
        .find(|t| t.terminal == terminal)
        .map(|t| t.pane.clone())
        .ok_or_else(|| Error::external(format!("herdr-app ls does not list terminal {terminal}")))
}

fn text(v: &Value, key: &str) -> Result<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Error::external(format!("herdr-app answered without {key}")))
}

fn realize(ctx: &mut Ctx, node: &Node, pane: &str, terminal: &str) -> Result<()> {
    match node {
        Node::Pane { startup, .. } => {
            ctx.terminals.push(terminal.to_string());
            if let Some(cmd) = startup {
                if ctx.substituted.contains(terminal) {
                    ctx.skipped.push(json!({ "terminal": terminal, "reason": "the saved folder is gone, the pane started in the project folder" }));
                } else {
                    ctx.h.send(terminal, cmd)?;
                    ctx.sent
                        .push(json!({ "terminal": terminal, "command": cmd }));
                }
            }
            Ok(())
        }
        Node::Split {
            direction,
            first,
            second,
        } => {
            // The second part takes its first pane from a split of this pane, then both parts grow.
            ctx.h.app_do("pane.focus", json!({ "pane": pane }))?;
            let (folder, substituted) = ctx.usable(second.first_cwd());
            let made = ctx.h.app_do(
                "terminal.open",
                json!({ "folder": folder, "place": direction.as_str() }),
            )?;
            let (new_pane, new_terminal) = (text(&made, "pane")?, text(&made, "terminal")?);
            if substituted {
                ctx.substituted.insert(new_terminal.clone());
            }
            realize(ctx, first, pane, terminal)?;
            realize(ctx, second, &new_pane, &new_terminal)
        }
    }
}

/// Builds the space of a definition from nothing. Refuses when a space of that name is open.
pub fn build(h: &dyn Herdr, def: &Definition, opts: Options) -> Result<Value> {
    if def.tabs.is_empty() {
        return Err(Error::usage("the definition has no tabs"));
    }
    if h.spaces()?.iter().any(|s| s.name == def.name) {
        return Err(Error::conflict(format!(
            "a space called {} is already open, close it first (restore never makes duplicates)",
            def.name
        )));
    }
    let mut ctx = Ctx {
        h,
        project: def.folder.clone(),
        opts,
        warnings: Vec::new(),
        sent: Vec::new(),
        skipped: Vec::new(),
        terminals: Vec::new(),
        substituted: HashSet::new(),
    };
    let mut space_id = String::new();
    let mut tabs_report = Vec::new();
    for (i, tab) in def.tabs.iter().enumerate() {
        let (folder, substituted) = ctx.usable(tab.tree.first_cwd());
        let made = if i == 0 {
            let mut args = json!({ "name": def.name, "folder": folder });
            if tab.named {
                args["tab"] = json!(tab.name);
            }
            h.app_do("space.new", args)?
        } else {
            let mut args = json!({ "space": space_id, "folder": folder });
            if tab.named {
                args["name"] = json!(tab.name);
            }
            h.app_do("tab.new", args)?
        };
        if i == 0 {
            space_id = text(&made, "space")?;
        }
        let terminal = text(&made, "terminal")?;
        if substituted {
            ctx.substituted.insert(terminal.clone());
        }
        let pane = pane_of(h, &terminal)?;
        ctx.terminals.clear();
        realize(&mut ctx, &tab.tree, &pane, &terminal)?;
        tabs_report.push(json!({ "tab": tab.name, "terminals": ctx.terminals }));
    }
    Ok(
        json!({ "restored": def.name, "kind": def.kind, "space": space_id, "tabs": tabs_report, "startup_commands": ctx.sent, "skipped": ctx.skipped, "warnings": ctx.warnings }),
    )
}

/// For a space that survived a core restart. Types the startup commands into the terminals that
/// are plain shells again and still in their saved folder. Anything else is left alone and reported.
pub fn rehydrate(h: &dyn Herdr, def: &Definition) -> Result<Value> {
    // by name only, and a name used twice is refused, so commands cannot land in the wrong space
    let space = find_space_by_name(h, &def.name).map_err(|e| {
        if e.kind == "not_found" {
            Error::not_found(format!(
                "no open space called {}. Use restore to build it",
                def.name
            ))
        } else {
            e
        }
    })?;
    if space.tabs.len() != def.tabs.len() {
        return Err(Error::conflict(format!(
            "the open space has {} tabs, the definition {}",
            space.tabs.len(),
            def.tabs.len()
        )));
    }
    let terminals = h.terminals()?;
    let info: HashMap<&str, (&str, Option<&str>)> = terminals
        .iter()
        .map(|t| (t.address.as_str(), (t.program.as_str(), t.cwd.as_deref())))
        .collect();
    let (mut sent, mut skipped) = (Vec::new(), Vec::new());
    for (tab_def, tab_live) in def.tabs.iter().zip(&space.tabs) {
        let leaves = tab_def.tree.leaves();
        if leaves.len() != tab_live.terminals.len() {
            return Err(Error::conflict(format!(
                "tab {} has {} terminals, the definition {}",
                tab_live.name,
                tab_live.terminals.len(),
                leaves.len()
            )));
        }
        for ((saved_cwd, startup), t) in leaves.iter().zip(&tab_live.terminals) {
            let Some(cmd) = startup else { continue };
            match info.get(t.terminal.as_str()) {
                Some((program, cwd)) if is_plain_shell(program) && same_folder(*cwd, saved_cwd) => {
                    h.send(&t.terminal, cmd)?;
                    sent.push(json!({ "terminal": t.terminal, "command": cmd }));
                }
                Some((program, cwd)) => {
                    let reason = if !is_plain_shell(program) { "a program is already running" } else if !same_folder(*cwd, saved_cwd) { "the folder differs from the saved one" } else { "unknown" };
                    skipped.push(json!({ "terminal": t.terminal, "program": program, "reason": reason }));
                }
                None => skipped.push(json!({ "terminal": t.terminal, "program": Value::Null, "reason": "terminal is not live" })),
            }
        }
    }
    Ok(json!({ "rehydrated": def.name, "startup_commands": sent, "skipped": skipped }))
}

fn same_folder(live: Option<&str>, saved: &str) -> bool {
    let trim = |s: &str| s.trim_end_matches(['/', '\\']).to_string();
    live.is_some_and(|l| trim(l) == trim(saved))
}

/// Restores by saved name, building the space. Unknown names are an error.
pub fn restore_by_name(store: &Store, h: &dyn Herdr, name: &str, opts: Options) -> Result<Value> {
    let row = store
        .workspace(name)?
        .ok_or_else(|| Error::not_found(format!("no saved workspace called {name}")))?;
    let def: Definition = serde_json::from_str(&row.definition)?;
    build(h, &def, opts)
}

pub fn rehydrate_by_name(store: &Store, h: &dyn Herdr, name: &str) -> Result<Value> {
    let row = store
        .workspace(name)?
        .ok_or_else(|| Error::not_found(format!("no saved workspace called {name}")))?;
    let def: Definition = serde_json::from_str(&row.definition)?;
    rehydrate(h, &def)
}

/// Closes a space and makes sure its terminals are really gone.
pub fn close_space(h: &dyn Herdr, selector: &str) -> Result<Value> {
    let space = find_space(h, selector)?;
    let terminals: Vec<String> = space
        .tabs
        .iter()
        .flat_map(|t| t.terminals.iter().map(|x| x.terminal.clone()))
        .collect();
    h.app_do("space.close", json!({ "id": space.id, "confirmed": true }))?;
    let live: HashSet<String> = h.terminals()?.into_iter().map(|t| t.address).collect();
    let mut killed = Vec::new();
    for t in &terminals {
        if live.contains(t) {
            h.kill(t)?;
            killed.push(t.clone());
        }
    }
    let still: Vec<String> = h
        .terminals()?
        .into_iter()
        .map(|t| t.address)
        .filter(|a| terminals.contains(a))
        .collect();
    if !still.is_empty() {
        return Err(Error::external(format!(
            "terminals still run after closing {}: {}",
            space.name,
            still.join(", ")
        )));
    }
    Ok(json!({ "closed": space.name, "terminals": terminals, "killed": killed }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herdr::fake::FakeHerdr;
    use crate::util::scratch_dir;
    use crate::workspace::templates;

    fn offline() -> Options {
        Options { check_dirs: false }
    }

    fn typed_commands(h: &FakeHerdr) -> Vec<String> {
        h.typed().into_iter().map(|(_, c)| c).collect()
    }

    #[test]
    fn the_code_template_rebuilds_exactly() {
        let h = FakeHerdr::new();
        let def = templates::instantiate("code", "proj", "/work/proj").unwrap();
        let r = build(&h, &def, offline()).unwrap();
        assert_eq!(h.shape("proj", 0).unwrap(), "down(right(proj,proj),proj)");
        assert_eq!(r["tabs"][0]["terminals"].as_array().unwrap().len(), 3);
        assert_eq!(typed_commands(&h), vec!["nvim", "lazygit"]);
        assert_eq!(r["warnings"], json!([]));
    }

    #[test]
    fn three_tabs_with_nested_splits() {
        let h = FakeHerdr::new();
        let split = |d, a: Node, b: Node| Node::Split {
            direction: d,
            first: Box::new(a),
            second: Box::new(b),
        };
        use crate::workspace::Direction::{Down, Right};
        let def = Definition {
            name: "multi".into(),
            folder: "/p".into(),
            kind: "template".into(),
            tabs: vec![
                TabDef {
                    name: "api".into(),
                    named: true,
                    tree: Node::pane("/p/api", Some("echo api")),
                },
                TabDef {
                    name: "web".into(),
                    named: true,
                    tree: split(
                        Right,
                        Node::pane("/p/web", None),
                        split(
                            Down,
                            Node::pane("/p/ui", Some("echo ui")),
                            Node::pane("/p/css", None),
                        ),
                    ),
                },
                TabDef {
                    name: "3".into(),
                    named: false,
                    tree: Node::pane("/p/misc", None),
                },
            ],
        };
        let r = build(&h, &def, offline()).unwrap();
        assert_eq!(h.shape("multi", 0).unwrap(), "api");
        assert_eq!(h.shape("multi", 1).unwrap(), "right(web,down(ui,css))");
        assert_eq!(h.shape("multi", 2).unwrap(), "misc");
        assert_eq!(h.live_terminals().len(), 5);
        assert_eq!(r["tabs"].as_array().unwrap().len(), 3);
        let spaces = h.spaces().unwrap();
        assert_eq!(
            spaces[0]
                .tabs
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            vec!["api", "web", "3"]
        );
        assert_eq!(typed_commands(&h), vec!["echo api", "echo ui"]);
    }

    #[test]
    fn a_snapshot_is_a_row_and_keeps_folders_and_startups() {
        let h = FakeHerdr::new();
        h.add_space("a", "/x/a");
        h.app_do("terminal.open", json!({"folder": "/x/b", "place": "right"}))
            .unwrap();
        h.app_do("terminal.open", json!({"folder": "/x/c", "place": "down"}))
            .unwrap();
        let d = save_snapshot(&h, "a", &["1=nvim".into(), "1:3=lazygit".into()]).unwrap();
        assert_eq!(d.kind, "snapshot");
        assert!(!d.tabs[0].named);
        assert_eq!(
            d.tabs[0].tree.leaves(),
            vec![
                ("/x/a", Some("nvim")),
                ("/x/b", None),
                ("/x/c", Some("lazygit"))
            ]
        );
        close_space(&h, "a").unwrap();
        build(&h, &d, offline()).unwrap();
        assert_eq!(h.shape("a", 0).unwrap(), "right(a,right(b,c))");
    }

    #[test]
    fn bad_startup_references_are_refused() {
        let h = FakeHerdr::new();
        h.add_space("a", "/x/a");
        assert_eq!(
            save_snapshot(&h, "a", &["9=x".into()]).unwrap_err().kind,
            "usage"
        );
        assert_eq!(
            save_snapshot(&h, "a", &["1:4=x".into()]).unwrap_err().kind,
            "usage"
        );
        assert_eq!(
            save_snapshot(&h, "a", &["nonsense".into()])
                .unwrap_err()
                .kind,
            "usage"
        );
        assert_eq!(
            save_snapshot(&h, "ghost", &[]).unwrap_err().kind,
            "not_found"
        );
    }

    #[test]
    fn restore_refuses_a_duplicate_and_an_unknown_name() {
        let h = FakeHerdr::new();
        let store = Store::open_in_memory().unwrap();
        let def = templates::instantiate("code", "proj", "/work/proj").unwrap();
        store
            .save_workspace(
                "proj",
                "/work/proj",
                "template",
                &serde_json::to_string(&def).unwrap(),
            )
            .unwrap();
        restore_by_name(&store, &h, "proj", offline()).unwrap();
        assert_eq!(
            restore_by_name(&store, &h, "proj", offline())
                .unwrap_err()
                .kind,
            "conflict"
        );
        assert_eq!(
            restore_by_name(&store, &h, "ghost", offline())
                .unwrap_err()
                .kind,
            "not_found"
        );
        assert_eq!(h.spaces().unwrap().len(), 1);
    }

    #[test]
    fn rehydrate_types_into_shells_and_leaves_programs_alone() {
        let h = FakeHerdr::new();
        let def = templates::instantiate("code", "proj", "/work/proj").unwrap();
        build(&h, &def, offline()).unwrap();
        h.restart_core();
        let before = typed_commands(&h).len();
        let terms: Vec<String> = h.spaces().unwrap()[0].tabs[0]
            .terminals
            .iter()
            .map(|t| t.terminal.clone())
            .collect();
        h.set_program(&terms[1], "lazygit"); // already running again
        let r = rehydrate(&h, &def).unwrap();
        assert_eq!(typed_commands(&h)[before..], ["nvim"]);
        assert_eq!(r["skipped"][0]["terminal"], json!(terms[1]));
        assert_eq!(r["skipped"][0]["reason"], "a program is already running");
        assert_eq!(h.shape("proj", 0).unwrap(), "down(right(proj,proj),proj)");
    }

    #[test]
    fn rehydrate_needs_a_matching_open_space() {
        let h = FakeHerdr::new();
        let def = templates::instantiate("code", "proj", "/work/proj").unwrap();
        assert_eq!(rehydrate(&h, &def).unwrap_err().kind, "not_found");
        h.add_space("proj", "/work/proj");
        let two_tabs = Definition {
            tabs: vec![def.tabs[0].clone(), def.tabs[0].clone()],
            ..def.clone()
        };
        assert_eq!(rehydrate(&h, &two_tabs).unwrap_err().kind, "conflict");
        assert_eq!(rehydrate(&h, &def).unwrap_err().kind, "conflict"); // one pane open, three expected
    }

    #[test]
    fn close_ends_terminals_that_space_close_left_running() {
        let h = FakeHerdr::new();
        h.set_close_kills_terminals(false);
        let def = templates::instantiate("code", "proj", "/work/proj").unwrap();
        build(&h, &def, offline()).unwrap();
        let r = close_space(&h, "proj").unwrap();
        assert_eq!(r["killed"].as_array().unwrap().len(), 3);
        assert!(h.live_terminals().is_empty());
        assert!(h.spaces().unwrap().is_empty());
        // and when the close already ended them, nothing is killed
        h.set_close_kills_terminals(true);
        build(&h, &def, offline()).unwrap();
        assert_eq!(close_space(&h, "proj").unwrap()["killed"], json!([]));
    }

    #[test]
    fn a_missing_folder_falls_back_to_the_project_folder() {
        let h = FakeHerdr::new();
        let project = scratch_dir("ws-fallback");
        let def = Definition {
            name: "w".into(),
            folder: project.to_string_lossy().into(),
            kind: "snapshot".into(),
            tabs: vec![TabDef {
                name: "1".into(),
                named: false,
                tree: Node::pane("/definitely/not/here", None),
            }],
        };
        let r = build(&h, &def, Options::default()).unwrap();
        assert_eq!(r["warnings"].as_array().unwrap().len(), 1);
        let leaf = project.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(h.shape("w", 0).unwrap(), leaf);
    }

    #[test]
    fn a_duplicate_name_must_be_closed_by_id() {
        let h = FakeHerdr::new();
        h.add_space("twin", "/a");
        h.add_space("twin", "/b");
        assert_eq!(find_space(&h, "twin").unwrap_err().kind, "conflict");
        let id = h.spaces().unwrap()[0].id.clone();
        assert!(close_space(&h, &id).is_ok());
    }

    #[test]
    fn rehydrate_refuses_a_duplicate_name_and_a_changed_folder() {
        let h = FakeHerdr::new();
        let def = templates::instantiate("code", "proj", "/work/proj").unwrap();
        build(&h, &def, offline()).unwrap();
        h.restart_core();
        // a second space with the same name must stop the command
        h.add_space("proj", "/elsewhere");
        assert_eq!(rehydrate(&h, &def).unwrap_err().kind, "conflict");
        assert!(
            h.typed().len() == 2,
            "only the two commands of build exist, nothing new was typed"
        );
        // a definition whose folders differ from the live ones types nothing
        let h2 = FakeHerdr::new();
        build(&h2, &def, offline()).unwrap();
        h2.restart_core();
        let moved = templates::instantiate("code", "proj", "/other/place").unwrap();
        let before = h2.typed().len();
        let r = rehydrate(&h2, &moved).unwrap();
        assert_eq!(h2.typed().len(), before);
        assert_eq!(r["skipped"].as_array().unwrap().len(), 2);
        assert_eq!(
            r["skipped"][0]["reason"],
            "the folder differs from the saved one"
        );
    }

    #[test]
    fn no_startup_command_is_typed_where_the_folder_was_replaced() {
        let h = FakeHerdr::new();
        let project = scratch_dir("ws-nostartup");
        let good = project.to_string_lossy().to_string();
        let def = Definition {
            name: "w".into(),
            folder: good.clone(),
            kind: "snapshot".into(),
            tabs: vec![TabDef {
                name: "1".into(),
                named: false,
                tree: Node::Split {
                    direction: crate::workspace::Direction::Right,
                    first: Box::new(Node::pane("/definitely/not/here", Some("typed-by-mistake"))),
                    second: Box::new(Node::pane(&good, Some("echo fine"))),
                },
            }],
        };
        let r = build(&h, &def, Options::default()).unwrap();
        assert_eq!(typed_commands(&h), vec!["echo fine"]);
        assert_eq!(r["skipped"].as_array().unwrap().len(), 1);
        assert_eq!(r["warnings"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn rehydrate_matches_by_name_only() {
        let h = FakeHerdr::new();
        let def = templates::instantiate("code", "proj", "/work/proj").unwrap();
        build(&h, &def, offline()).unwrap();
        h.restart_core();
        // a definition named like the id of the open space must not match that space
        let id = h.spaces().unwrap()[0].id.clone();
        let by_id = Definition {
            name: id,
            ..def.clone()
        };
        assert_eq!(rehydrate(&h, &by_id).unwrap_err().kind, "not_found");
    }
}
