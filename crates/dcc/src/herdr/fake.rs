//! A pretend Herdr for tests. It keeps spaces, tabs, a split tree per tab and the terminals, and
//! understands the few `herdr-app do` commands the restore code uses. It follows what the real
//! app did in the Phase 0 tests (a new split takes the focus, `terminal.open` splits the focused
//! pane, `space.close` may or may not end the terminals).

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::{Value, json};

use super::api::{Herdr, SpaceInfo, TabInfo, TermRef, TerminalInfo};
use crate::error::{Error, Result};

#[derive(Debug, Clone)]
enum FNode {
    Leaf(String),
    Split {
        dir: String,
        first: Box<FNode>,
        second: Box<FNode>,
    },
}

#[derive(Debug, Clone)]
struct FTab {
    id: String,
    name: String,
    tree: FNode,
}

#[derive(Debug, Clone)]
struct FSpace {
    id: String,
    name: String,
    folder: String,
    tabs: Vec<FTab>,
}

#[derive(Debug, Clone)]
struct FTerm {
    cwd: String,
    program: String,
    live: bool,
}

#[derive(Default)]
struct State {
    spaces: Vec<FSpace>,
    terms: HashMap<String, FTerm>,
    pane_term: HashMap<String, String>,
    focus: Option<String>,
    counter: u32,
    log: Vec<String>,
    typed: Vec<(String, String)>,
    killed: Vec<String>,
    close_kills_terminals: bool,
}

pub struct FakeHerdr {
    s: RefCell<State>,
}

impl Default for FakeHerdr {
    fn default() -> Self {
        let f = Self {
            s: RefCell::new(State::default()),
        };
        f.s.borrow_mut().close_kills_terminals = true;
        f
    }
}

impl State {
    fn next(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{prefix}{}", self.counter)
    }

    fn new_terminal(&mut self, cwd: &str, program: &str) -> (String, String) {
        let term = self.next("t");
        let pane = self.next("p");
        self.terms.insert(
            term.clone(),
            FTerm {
                cwd: cwd.to_string(),
                program: program.to_string(),
                live: true,
            },
        );
        self.pane_term.insert(pane.clone(), term.clone());
        (term, pane)
    }

    fn leaves(node: &FNode, out: &mut Vec<String>) {
        match node {
            FNode::Leaf(p) => out.push(p.clone()),
            FNode::Split { first, second, .. } => {
                Self::leaves(first, out);
                Self::leaves(second, out);
            }
        }
    }

    fn split(node: &mut FNode, target: &str, dir: &str, new_pane: &str) -> bool {
        match node {
            FNode::Leaf(p) if p == target => {
                *node = FNode::Split {
                    dir: dir.to_string(),
                    first: Box::new(FNode::Leaf(p.clone())),
                    second: Box::new(FNode::Leaf(new_pane.to_string())),
                };
                true
            }
            FNode::Leaf(_) => false,
            FNode::Split { first, second, .. } => {
                Self::split(first, target, dir, new_pane)
                    || Self::split(second, target, dir, new_pane)
            }
        }
    }

    fn render(&self, node: &FNode) -> String {
        match node {
            FNode::Leaf(p) => {
                let t = &self.terms[&self.pane_term[p]];
                t.cwd.rsplit('/').next().unwrap_or_default().to_string()
            }
            FNode::Split { dir, first, second } => {
                format!("{dir}({},{})", self.render(first), self.render(second))
            }
        }
    }
}

fn arg<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

impl FakeHerdr {
    pub fn new() -> Self {
        Self::default()
    }

    /// With false, `space.close` leaves the terminals of the space running (the inconsistent
    /// behaviour seen in the spike).
    pub fn set_close_kills_terminals(&self, yes: bool) {
        self.s.borrow_mut().close_kills_terminals = yes;
    }

    /// Core restart as seen in Phase 0, the structure stays, every terminal becomes a plain shell.
    pub fn restart_core(&self) {
        for t in self.s.borrow_mut().terms.values_mut().filter(|t| t.live) {
            t.program = "zsh".into();
        }
    }

    pub fn set_program(&self, terminal: &str, program: &str) {
        if let Some(t) = self.s.borrow_mut().terms.get_mut(terminal) {
            t.program = program.into();
        }
    }

    /// The split tree of one tab as text, for example `down(right(a,b),c)`, leaves are folder names.
    pub fn shape(&self, space: &str, tab_index: usize) -> Option<String> {
        let s = self.s.borrow();
        let sp = s.spaces.iter().find(|x| x.name == space)?;
        Some(s.render(&sp.tabs.get(tab_index)?.tree))
    }

    pub fn typed(&self) -> Vec<(String, String)> {
        self.s.borrow().typed.clone()
    }

    pub fn killed(&self) -> Vec<String> {
        self.s.borrow().killed.clone()
    }

    pub fn log(&self) -> Vec<String> {
        self.s.borrow().log.clone()
    }

    pub fn live_terminals(&self) -> Vec<String> {
        let s = self.s.borrow();
        let mut v: Vec<String> = s
            .terms
            .iter()
            .filter(|(_, t)| t.live)
            .map(|(k, _)| k.clone())
            .collect();
        v.sort();
        v
    }

    /// Adds a space the way a person would, a shell in a folder.
    pub fn add_space(&self, name: &str, folder: &str) -> Value {
        self.app_do("space.new", json!({"name": name, "folder": folder}))
            .unwrap()
    }
}

impl Herdr for FakeHerdr {
    fn spaces(&self) -> Result<Vec<SpaceInfo>> {
        let s = self.s.borrow();
        Ok(s.spaces
            .iter()
            .map(|sp| SpaceInfo {
                id: sp.id.clone(),
                name: sp.name.clone(),
                folder: sp.folder.clone(),
                tabs: sp
                    .tabs
                    .iter()
                    .map(|t| {
                        let mut panes = Vec::new();
                        State::leaves(&t.tree, &mut panes);
                        TabInfo {
                            id: t.id.clone(),
                            name: t.name.clone(),
                            terminals: panes
                                .into_iter()
                                .map(|p| TermRef {
                                    terminal: s.pane_term[&p].clone(),
                                    pane: p,
                                })
                                .collect(),
                        }
                    })
                    .collect(),
            })
            .collect())
    }

    fn terminals(&self) -> Result<Vec<TerminalInfo>> {
        let s = self.s.borrow();
        let mut v: Vec<TerminalInfo> = s
            .terms
            .iter()
            .filter(|(_, t)| t.live)
            .map(|(a, t)| TerminalInfo {
                address: a.clone(),
                cwd: Some(t.cwd.clone()),
                program: t.program.clone(),
                live: true,
            })
            .collect();
        v.sort_by(|a, b| a.address.cmp(&b.address));
        Ok(v)
    }

    fn app_do(&self, name: &str, args: Value) -> Result<Value> {
        let mut s = self.s.borrow_mut();
        s.log.push(format!("{name} {args}"));
        match name {
            "space.new" => {
                let folder = arg(&args, "folder").unwrap_or("/").to_string();
                let sp_name = arg(&args, "name").unwrap_or("space").to_string();
                let tab_name = arg(&args, "tab").unwrap_or("1").to_string();
                let (term, pane) = s.new_terminal(&folder, "zsh");
                let (sid, tid) = (s.next("sp"), s.next("tb"));
                s.spaces.push(FSpace {
                    id: sid.clone(),
                    name: sp_name,
                    folder,
                    tabs: vec![FTab {
                        id: tid.clone(),
                        name: tab_name,
                        tree: FNode::Leaf(pane.clone()),
                    }],
                });
                s.focus = Some(pane);
                Ok(json!({"space": sid, "tab": tid, "terminal": term}))
            }
            "tab.new" => {
                let sid = arg(&args, "space")
                    .ok_or_else(|| Error::usage("tab.new needs a space"))?
                    .to_string();
                let folder = arg(&args, "folder").unwrap_or("/").to_string();
                let (term, pane) = s.new_terminal(&folder, "zsh");
                let tid = s.next("tb");
                let n = s
                    .spaces
                    .iter()
                    .find(|x| x.id == sid)
                    .ok_or_else(|| Error::not_found("no such space"))?
                    .tabs
                    .len()
                    + 1;
                let tab_name = arg(&args, "name")
                    .map(str::to_string)
                    .unwrap_or_else(|| n.to_string());
                let sp = s.spaces.iter_mut().find(|x| x.id == sid).unwrap();
                sp.tabs.push(FTab {
                    id: tid.clone(),
                    name: tab_name,
                    tree: FNode::Leaf(pane.clone()),
                });
                s.focus = Some(pane);
                Ok(json!({"space": sid, "tab": tid, "terminal": term}))
            }
            "pane.focus" => {
                let pane =
                    arg(&args, "pane").ok_or_else(|| Error::usage("pane.focus needs a pane"))?;
                if !s.pane_term.contains_key(pane) {
                    return Err(Error::not_found("no such pane"));
                }
                s.focus = Some(pane.to_string());
                Ok(json!({"ok": true}))
            }
            "terminal.open" => {
                let folder = arg(&args, "folder").unwrap_or("/").to_string();
                let place = arg(&args, "place").unwrap_or("tab");
                let program = args
                    .get("argv")
                    .and_then(|a| a.get(0))
                    .and_then(Value::as_str)
                    .unwrap_or("zsh")
                    .to_string();
                let focus = s
                    .focus
                    .clone()
                    .ok_or_else(|| Error::usage("nothing is focused"))?;
                let (term, pane) = s.new_terminal(&folder, &program);
                if place == "right" || place == "down" {
                    let mut done = false;
                    for sp in &mut s.spaces {
                        for tab in &mut sp.tabs {
                            done = done || State::split(&mut tab.tree, &focus, place, &pane);
                        }
                    }
                    if !done {
                        return Err(Error::not_found("the focused pane is in no tab"));
                    }
                }
                s.focus = Some(pane.clone());
                Ok(json!({"pane": pane, "terminal": term}))
            }
            "space.close" => {
                let id = arg(&args, "id")
                    .ok_or_else(|| Error::usage("space.close needs an id"))?
                    .to_string();
                let idx = s
                    .spaces
                    .iter()
                    .position(|x| x.id == id)
                    .ok_or_else(|| Error::not_found("no such space"))?;
                let sp = s.spaces.remove(idx);
                if s.close_kills_terminals {
                    for tab in &sp.tabs {
                        let mut panes = Vec::new();
                        State::leaves(&tab.tree, &mut panes);
                        for p in panes {
                            let t = s.pane_term[&p].clone();
                            if let Some(x) = s.terms.get_mut(&t) {
                                x.live = false;
                            }
                        }
                    }
                }
                Ok(json!({"ok": true}))
            }
            other => Err(Error::usage(format!("the fake does not know {other}"))),
        }
    }

    fn send(&self, terminal: &str, text: &str) -> Result<()> {
        let mut s = self.s.borrow_mut();
        if !s.terms.get(terminal).map(|t| t.live).unwrap_or(false) {
            return Err(Error::not_found("no such live terminal"));
        }
        s.typed.push((terminal.to_string(), text.to_string()));
        Ok(())
    }

    fn kill(&self, terminal: &str) -> Result<()> {
        let mut s = self.s.borrow_mut();
        s.killed.push(terminal.to_string());
        if let Some(t) = s.terms.get_mut(terminal) {
            t.live = false;
        }
        Ok(())
    }
}
