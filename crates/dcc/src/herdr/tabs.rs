//! Runs a program in a tab of its own. The tab is made first (a new tab in the named space, or a
//! new space when none is open), the program is started with `hc run` and the labels that put it
//! into that tab, and the shell the tab started with is closed once the program has joined.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::api::Herdr;
use crate::error::{Error, Result};
use crate::workspace::ops::find_space_by_name;

#[derive(Debug, Clone)]
pub struct TabRun {
    pub terminal: String,
    pub space_id: String,
    pub tab_id: String,
    pub warnings: Vec<String>,
}

pub fn tab_pane(
    h: &dyn Herdr,
    space_id: &str,
    tab_id: &str,
    terminal: &str,
) -> Result<Option<String>> {
    Ok(h.spaces()?
        .iter()
        .filter(|s| s.id == space_id)
        .flat_map(|s| s.tabs.iter())
        .filter(|t| t.id == tab_id)
        .flat_map(|t| t.terminals.iter())
        .find(|t| t.terminal == terminal)
        .map(|t| t.pane.clone()))
}

pub struct TabSpec<'a> {
    pub space: &'a str,
    pub tab_name: &'a str,
    pub cwd: &'a str,
    pub run_name: &'a str,
    /// Added to the two labels that place the terminal.
    pub extra_labels: &'a [(String, String)],
    pub argv: &'a [String],
    /// Names the program in the warning texts.
    pub what: &'a str,
}

pub fn run_in_new_tab(h: &dyn Herdr, t: &TabSpec) -> Result<TabRun> {
    let TabSpec {
        space,
        tab_name,
        cwd,
        run_name,
        extra_labels,
        argv,
        what,
    } = *t;
    let made = match find_space_by_name(h, space) {
        Ok(found) => h.app_do(
            "tab.new",
            json!({ "space": found.id, "name": tab_name, "folder": cwd }),
        )?,
        Err(e) if e.kind == "not_found" => h.app_do(
            "space.new",
            json!({ "name": space, "folder": cwd, "tab": tab_name }),
        )?,
        Err(e) => return Err(e),
    };
    let text = |k: &str| {
        made.get(k)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| Error::external(format!("herdr-app answered without {k}")))
    };
    let (space_id, tab_id, shell) = (text("space")?, text("tab")?, text("terminal")?);
    let shell_pane = tab_pane(h, &space_id, &tab_id, &shell)?;

    let mut labels = vec![
        ("herdr.space".to_string(), space_id.clone()),
        ("herdr.tab".to_string(), tab_id.clone()),
    ];
    labels.extend(extra_labels.iter().cloned());
    let terminal = h.run_labeled(run_name, cwd, &labels, argv)?;

    let mut warnings = Vec::new();
    let start = Instant::now();
    let joined = loop {
        if tab_pane(h, &space_id, &tab_id, &terminal)?.is_some() {
            break true;
        }
        if start.elapsed() > Duration::from_secs(4) {
            break false;
        }
        std::thread::sleep(Duration::from_millis(150));
    };
    match (joined, shell_pane) {
        (true, Some(pane)) => {
            if let Err(e) = h.app_do("pane.close", json!({ "pane": pane })) {
                warnings.push(format!("could not close the starting shell: {}", e.message));
            }
        }
        (false, _) => warnings.push(format!(
            "the {what} terminal did not show up in its tab within 4 s, the starting shell was left open"
        )),
        (true, None) => {}
    }
    Ok(TabRun {
        terminal,
        space_id,
        tab_id,
        warnings,
    })
}
