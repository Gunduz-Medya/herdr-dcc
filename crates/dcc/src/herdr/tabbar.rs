//! DCC's tab bar. Herdr's own `tabs` mod draws the tab pills, the `+` menu and one button per side
//! panel (Files, Changes), and its source is read only. DCC does not ship a copy of it. This
//! module reads the `tabs` mod that is installed, adds one button per DCC panel after the Changes
//! button and writes the result as the mod `dcc-tabs` in the app home. The edit is three exact
//! text replacements. When one of them no longer matches (a new Herdr version changed the file)
//! nothing is written and the command fails, so a wrong tab bar never appears.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::herdr::mods::mod_path;

pub const NAME: &str = "dcc-tabs";

/// Panel id, label and icon file name. The ids are the ids of the DCC blocks in the layout recipe.
pub const PANELS: &[(&str, &str, &str)] = &[
    ("dcc-workspace", "Workspaces", "workspace.svg"),
    ("dcc-agents", "Agents", "agents.svg"),
    ("dcc-ssh", "SSH", "ssh.svg"),
    ("dcc-code", "Code", "code.svg"),
    ("dcc-config", "AI config", "config.svg"),
    ("dcc-usage", "Usage", "usage.svg"),
];

const ANCHOR_ENABLED: &str =
    r#"const enabled = shared("workspace.panels") ?? ["files", "changes"];"#;
const ANCHOR_BUTTON: &str = r#".when(enabled.includes("changes"), el => el.child(panelButton("changes", "Changes", "mods/changes/changes.svg")))"#;
const ANCHOR_FIND: &str = "function find(node, id) {";
const ANCHOR_RENDER: &str = "  render() {";
const ANCHOR_RETURN: &str = "    return h_flex().size_full().min_w(0).px(10).pl(lightsPadding)";

/// A method of the tab bar. At most two panels are open next to the terminal, the side panel
/// Herdr already has (slot a, Files and Changes live there too) and `dcc-panel-b` (slot b). A
/// panel that is open closes when its button is pressed again. A new one takes a free slot,
/// and when both are taken it replaces the one that was opened longer ago.
const TOGGLE_METHOD: &str = r#"  dccToggle(id) {
    const root = layout();
    const a = find(root, "panel"), ab = find(root, "panel-body");
    const b = find(root, "dcc-panel-b"), bb = find(root, "dcc-body-b");
    const aOpen = a && !a.hidden ? ab?.active ?? null : null;
    const bOpen = b && !b.hidden ? String(bb?.active ?? "").replace(/-b$/, "") || null : null;
    this.dccOrder = (this.dccOrder ?? []).filter(x => x === aOpen || x === bOpen);
    if (aOpen === id) { run("layout.hide", { id: "panel" }); return; }
    if (bOpen === id) { run("layout.hide", { id: "dcc-panel-b" }); return; }
    let slot = "a";
    if (aOpen) {
      if (!bOpen) slot = "b";
      else slot = this.dccOrder.indexOf(aOpen) <= this.dccOrder.indexOf(bOpen) ? "a" : "b";
    }
    if (slot === "a") { run("layout.select", { stack: "panel-body", child: id }); run("layout.show", { id: "panel" }); }
    else { run("layout.select", { stack: "dcc-body-b", child: `${id}-b` }); run("layout.show", { id: "dcc-panel-b" }); }
    this.dccOrder.push(id);
  }

"#;

/// Button of a DCC panel, defined inside `render` before the bar is built.
const BUTTON_DEF: &str = r#"    const dccOpenIds = [
      panel && !panel.hidden ? body?.active : null,
      find(root, "dcc-panel-b") && !find(root, "dcc-panel-b").hidden ? String(find(root, "dcc-body-b")?.active ?? "").replace(/-b$/, "") : null,
    ];
    const dccButton = (id, label, icon) => {
      const on = dccOpenIds.includes(id);
      return h_flex().id(`panel-${id}`).h(26).px(8).gap(5).flex_none().items_center().rounded(6)
        .text_size(12).bg(on ? t.selected : t.chrome).text_color(on ? t.foreground : t.muted)
        .hover(s => s.bg(t.hover)).on_mouse_down("left", stop)
        .on_click(() => this.dccToggle(id)).child(svg(icon).size(14)).child(label);
    };
"#;

fn panel_list() -> String {
    let rows: Vec<String> = PANELS
        .iter()
        .map(|(id, label, icon)| format!("[\"{id}\", \"{label}\", \"mods/{NAME}/{icon}\"]"))
        .collect();
    format!("const DCC_PANELS = [{}];\n", rows.join(", "))
}

/// The patched source, or an error naming the replacement that did not match.
pub fn patch(source: &str) -> Result<String> {
    for (anchor, what) in [
        (ANCHOR_FIND, "the find helper"),
        (ANCHOR_ENABLED, "the enabled panels line"),
        (ANCHOR_BUTTON, "the Changes button"),
        (ANCHOR_RENDER, "the render method"),
        (ANCHOR_RETURN, "the bar's return statement"),
    ] {
        if source.matches(anchor).count() != 1 {
            return Err(Error::conflict(format!(
                "the installed tabs mod does not look as expected ({what} not found once), so no DCC tab bar was written. The patch needs an update for this Herdr version."
            )));
        }
    }
    let out = source
        .replace(ANCHOR_FIND, &format!("{}{ANCHOR_FIND}", panel_list()))
        .replace(
            ANCHOR_ENABLED,
            r#"const enabled = [...(shared("workspace.panels") ?? ["files", "changes"]), ...DCC_PANELS.map(p => p[0])];"#,
        )
        .replace(
            ANCHOR_BUTTON,
            &format!(
                "{ANCHOR_BUTTON}\n      .children(DCC_PANELS.map(([id, label, icon]) => dccButton(id, label, icon)))"
            ),
        )
        .replace(ANCHOR_RENDER, &format!("{TOGGLE_METHOD}{ANCHOR_RENDER}"))
        .replace(ANCHOR_RETURN, &format!("{BUTTON_DEF}{ANCHOR_RETURN}"));
    Ok(out)
}

/// Writes `<app home>/mods/dcc-tabs` from the installed `tabs` mod and the icons in `icons`.
pub fn ensure(app_home: &Path, icons: &Path) -> Result<Value> {
    let src = mod_path(app_home, "tabs");
    let source = fs::read_to_string(src.join("main.js")).map_err(|e| {
        Error::not_found(format!(
            "cannot read the installed tabs mod ({}): {e}",
            src.display()
        ))
    })?;
    let patched = patch(&source)?;
    let dst = mod_path(app_home, NAME);
    fs::create_dir_all(&dst)?;
    let mut copied = 0;
    // the window icons and the mod description come from the installed mod as they are
    for entry in fs::read_dir(&src)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "main.js" || name.starts_with('.') || !entry.file_type()?.is_file() {
            continue;
        }
        let keep = matches!(name.as_str(), "mod.json") || name.ends_with(".svg");
        if keep {
            fs::copy(entry.path(), dst.join(&name))?;
            copied += 1;
        }
    }
    fs::write(dst.join("main.js"), patched)?;
    for (_, _, icon) in PANELS {
        let from = icons.join(icon);
        fs::copy(&from, dst.join(icon)).map_err(|e| {
            Error::not_found(format!("cannot read the icon {}: {e}", from.display()))
        })?;
        copied += 1;
    }
    Ok(
        json!({ "mod": NAME, "panels": PANELS.iter().map(|p| p.0).collect::<Vec<_>>(), "files": copied + 1 }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::scratch_dir;

    fn source() -> String {
        format!(
            "import x;\n{ANCHOR_FIND}\n}}\nclass T {{\n{ANCHOR_RENDER}\n  {ANCHOR_ENABLED}\n{ANCHOR_RETURN}\n      {ANCHOR_BUTTON}\n      .when(w, 1);\n}}\n}}\n"
        )
    }

    #[test]
    fn the_patch_adds_a_button_per_panel_and_lists_them_as_enabled() {
        let out = patch(&source()).unwrap();
        for (id, label, _) in PANELS {
            assert!(out.contains(&format!("[\"{id}\", \"{label}\"")), "{id}");
        }
        assert!(out.contains("...DCC_PANELS.map(p => p[0])"));
        assert!(out.contains(".children(DCC_PANELS.map("));
        // at most two panels: a toggle that closes an open one and replaces the older of two
        assert!(out.contains("dccToggle(id) {") && out.contains("dccButton(id, label, icon)"));
        assert!(out.contains("\"dcc-panel-b\"") && out.contains("dcc-body-b"));
        assert!(out.find("dccToggle(id) {").unwrap() < out.find(ANCHOR_RENDER).unwrap());
        assert!(out.find("const dccButton").unwrap() < out.find(ANCHOR_RETURN).unwrap());
        assert!(out.find("const DCC_PANELS").unwrap() < out.find(ANCHOR_FIND).unwrap());
        // the original lines are still there
        assert!(out.contains(ANCHOR_BUTTON));
    }

    #[test]
    fn a_changed_source_is_refused_and_nothing_is_written() {
        let bad = source().replace("changes", "diffs");
        assert_eq!(patch(&bad).unwrap_err().kind, "conflict");
        let doubled = format!("{}{}", source(), source());
        assert_eq!(patch(&doubled).unwrap_err().kind, "conflict");
        let d = scratch_dir("tabbar-refuse");
        let home = d.join("home");
        fs::create_dir_all(mod_path(&home, "tabs")).unwrap();
        fs::write(mod_path(&home, "tabs").join("main.js"), bad).unwrap();
        assert!(ensure(&home, &d).is_err());
        assert!(!mod_path(&home, NAME).exists());
    }

    #[test]
    fn ensure_writes_the_mod_with_icons_and_the_original_assets() {
        let d = scratch_dir("tabbar-ok");
        let home = d.join("home");
        let tabs = mod_path(&home, "tabs");
        fs::create_dir_all(&tabs).unwrap();
        fs::write(tabs.join("main.js"), source()).unwrap();
        fs::write(tabs.join("mod.json"), "{\"place\":\"none\"}").unwrap();
        fs::write(tabs.join("win-min.svg"), "<svg/>").unwrap();
        fs::write(tabs.join("jsconfig.json"), "{}").unwrap();
        let icons = d.join("icons");
        fs::create_dir_all(&icons).unwrap();
        for (_, _, icon) in PANELS {
            fs::write(icons.join(icon), "<svg/>").unwrap();
        }
        let r = ensure(&home, &icons).unwrap();
        assert_eq!(r["mod"], NAME);
        let out = mod_path(&home, NAME);
        assert!(
            fs::read_to_string(out.join("main.js"))
                .unwrap()
                .contains("DCC_PANELS")
        );
        assert!(out.join("win-min.svg").exists() && out.join("mod.json").exists());
        assert!(out.join("code.svg").exists());
        assert!(
            !out.join("jsconfig.json").exists(),
            "Herdr writes its own typing files"
        );
        // the installed mod is untouched
        assert!(
            !fs::read_to_string(tabs.join("main.js"))
                .unwrap()
                .contains("DCC_PANELS")
        );
    }
}
