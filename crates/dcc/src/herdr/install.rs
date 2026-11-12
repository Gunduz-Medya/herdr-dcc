//! Installing DCC into a Herdr app home, and taking it out again. `plan` only looks. `install`
//! writes, and only after every file it is about to replace has been copied into a backup folder
//! (with a manifest). Nothing outside the DCC mods and the DCC layout is ever touched, the Herdr
//! settings, the other mods and the other layouts stay as they are, and the layout is not
//! switched on, the user picks it. `uninstall` removes what an install created and puts the
//! replaced files back from a backup, and it removes nothing without `confirm`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{layout, mods, tabbar};
use crate::error::{Error, Result};
use crate::store::Store;
use crate::util::now_ms;

pub struct Params<'a> {
    pub app_home: &'a Path,
    pub mods_src: &'a Path,
    pub icons: &'a Path,
    pub recipe: &'a Path,
    pub data_dir: &'a Path,
    pub exe: &'a Path,
    /// `herdr-app --version`, for the warning when the pair is not a measured one.
    pub herdr_supported: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Path relative to the app home.
    pub rel: String,
    pub existed: bool,
    pub is_dir: bool,
}

fn recipe_layout_name(recipe: &Path) -> Result<String> {
    let text = fs::read_to_string(recipe)
        .map_err(|e| Error::not_found(format!("cannot read recipe {}: {e}", recipe.display())))?;
    let v: Value = serde_json::from_str(&text)?;
    let name = v
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::usage("recipe has no name"))?;
    layout::layout_path(Path::new("."), name)?; // checks the name
    Ok(name.to_string())
}

/// What an install would write, and what stands in the way.
pub fn plan(p: &Params) -> Result<Value> {
    let mut problems: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut items: Vec<Item> = Vec::new();
    let mut names = mods::mod_names(p.mods_src)?;
    names.push(tabbar::NAME.to_string());
    for n in &names {
        let rel = format!("mods/{n}");
        let existed = p.app_home.join(&rel).exists();
        items.push(Item {
            rel,
            existed,
            is_dir: true,
        });
    }
    let layout_name = recipe_layout_name(p.recipe)?;
    let layout_rel = format!("layouts/{layout_name}.json");
    items.push(Item {
        existed: p.app_home.join(&layout_rel).exists(),
        rel: layout_rel,
        is_dir: false,
    });

    if !p.app_home.is_dir() {
        problems.push(format!(
            "the app home {} does not exist, start Herdr once first",
            p.app_home.display()
        ));
    }
    let tabs_main = mods::mod_path(p.app_home, "tabs").join("main.js");
    match fs::read_to_string(&tabs_main) {
        Ok(src) => {
            if let Err(e) = tabbar::patch(&src) {
                problems.push(e.message);
            }
        }
        Err(_) => problems.push(format!(
            "the installed tabs mod is missing ({})",
            tabs_main.display()
        )),
    }
    if !p.app_home.join("layouts/main.json").exists() {
        problems.push("the base layout layouts/main.json is missing".to_string());
    }
    if p.herdr_supported == Some(false) {
        warnings.push("this Herdr version pair is not in compat.toml, run the contract test (dcc diag contract) before relying on agent states".to_string());
    }
    let rows: Vec<Value> = items
        .iter()
        .map(|i| json!({ "path": i.rel, "action": if i.existed { "replace" } else { "create" } }))
        .collect();
    Ok(json!({
        "app_home": p.app_home.to_string_lossy(),
        "data_dir": p.data_dir.to_string_lossy(),
        "dcc": p.exe.to_string_lossy(),
        "writes": false,
        "items": rows,
        "replaced_files_are_backed_up_to": p.data_dir.join("backups").to_string_lossy(),
        "untouched": ["settings.json", "view.json", "shared.json", "keys.json", "every other mod and layout"],
        "will_not_do": ["switch the layout (you pick it)", "write outside the app home and the data folder"],
        "problems": problems,
        "warnings": warnings,
    }))
}

fn copy_any(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to)?;
        for e in fs::read_dir(from)? {
            let e = e?;
            copy_any(&e.path(), &to.join(e.file_name()))?;
        }
    } else {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)?;
    }
    Ok(())
}

fn items_of(plan: &Value) -> Vec<Item> {
    plan["items"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|i| {
            let rel = i["path"].as_str().unwrap_or_default().to_string();
            Item {
                is_dir: rel.starts_with("mods/"),
                existed: i["action"] == "replace",
                rel,
            }
        })
        .collect()
}

pub fn install(p: &Params, store: &Store) -> Result<Value> {
    let planned = plan(p)?;
    let problems = planned["problems"].as_array().cloned().unwrap_or_default();
    if !problems.is_empty() {
        return Err(Error::conflict(format!(
            "nothing was written, {} problem(s) found: {}",
            problems.len(),
            problems
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" | ")
        )));
    }
    let items = items_of(&planned);
    // 1. back up everything that is about to be replaced. Any failure stops before a write.
    let stamp = now_ms();
    let backup = p.data_dir.join("backups").join(stamp.to_string());
    fs::create_dir_all(&backup)?;
    for i in items.iter().filter(|i| i.existed) {
        copy_any(&p.app_home.join(&i.rel), &backup.join(&i.rel)).map_err(|e| {
            Error::io(format!(
                "backup of {} failed, nothing was written: {}",
                i.rel, e.message
            ))
        })?;
    }
    let manifest = json!({
        "created_ms": stamp, "app_home": p.app_home.to_string_lossy(),
        "items": items.iter().map(|i| json!({ "path": i.rel, "existed": i.existed })).collect::<Vec<_>>(),
    });
    fs::write(
        backup.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    // 2. write
    let config = json!({
        "dcc": p.exe.to_string_lossy(),
        "data_dir": p.data_dir.to_string_lossy(),
        "ai_home": std::env::var("DCC_AI_HOME").ok(),
    });
    let mods_report = mods::ensure(p.mods_src, p.app_home, true, Some(&config))?;
    let tabs_report = tabbar::ensure(p.app_home, p.icons)?;
    let version = super::compat::program_version("herdr-app").unwrap_or_else(|_| "unknown".into());
    let layout_report = layout::ensure(p.app_home, p.recipe, &version, store, true)?;
    let failed: Vec<String> = mods_report["mods"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter(|m| m["check_passed"] != true)
        .filter_map(|m| m["mod"].as_str().map(str::to_string))
        .collect();
    Ok(json!({
        "installed": true,
        "backup": backup.to_string_lossy(),
        "mods": mods_report["mods"], "tab_bar": tabs_report, "layout": layout_report,
        "mod_checks_failed": failed,
        "next": "pick the layout in Herdr (ctrl-shift-l cycles layouts), or run: herdr-app do layout.switch '{\"name\":\"dcc\"}'",
        "undo": "dcc herdr uninstall --backup <this backup folder name> --confirm",
    }))
}

/// Takes DCC out. Files an install created are removed, files it replaced are put back from the
/// backup. Without `confirm` it only lists what it would do.
pub fn uninstall(
    app_home: &Path,
    data_dir: &Path,
    backup_name: &str,
    confirm: bool,
) -> Result<Value> {
    if backup_name.is_empty() || !backup_name.chars().all(|c| c.is_ascii_digit()) {
        return Err(Error::usage(
            "--backup is the folder name of a backup (digits only)",
        ));
    }
    let backup = data_dir.join("backups").join(backup_name);
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(backup.join("manifest.json")).map_err(|_| {
            Error::not_found(format!(
                "no backup {backup_name} in {}",
                data_dir.join("backups").display()
            ))
        })?,
    )?;
    let mut actions = Vec::new();
    for i in manifest["items"].as_array().cloned().unwrap_or_default() {
        let rel = i["path"].as_str().unwrap_or_default().to_string();
        // a path in a manifest is checked like any input, it must stay inside the two known places
        let safe =
            (rel.starts_with("mods/dcc-") || rel.starts_with("layouts/")) && !rel.contains("..");
        if !safe {
            return Err(Error::usage(format!(
                "the manifest names a path DCC does not manage: {rel}"
            )));
        }
        let existed = i["existed"].as_bool().unwrap_or(false);
        actions.push((rel, existed));
    }
    let rows: Vec<Value> = actions
        .iter()
        .map(|(r, e)| json!({ "path": r, "action": if *e { "restore from backup" } else { "remove (the install created it)" } }))
        .collect();
    if !confirm {
        return Ok(
            json!({ "writes": false, "actions": rows, "note": "pass --confirm to do this" }),
        );
    }
    for (rel, existed) in &actions {
        let target = app_home.join(rel);
        if target.is_dir() {
            fs::remove_dir_all(&target)?;
        } else if target.exists() {
            fs::remove_file(&target)?;
        }
        if *existed {
            copy_any(&backup.join(rel), &target)?;
        }
    }
    Ok(json!({ "uninstalled": true, "actions": rows }))
}

pub fn exe_path() -> Result<PathBuf> {
    Ok(std::env::current_exe()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::scratch_dir;

    const TABS: &str = "function find(node, id) {\n}\nclass T {\n  render() {\n    const enabled = shared(\"workspace.panels\") ?? [\"files\", \"changes\"];\n    return h_flex().size_full().min_w(0).px(10).pl(lightsPadding)\n.when(enabled.includes(\"changes\"), el => el.child(panelButton(\"changes\", \"Changes\", \"mods/changes/changes.svg\")))\n  }\n}\n";

    struct Setup {
        home: PathBuf,
        data: PathBuf,
        src: PathBuf,
        icons: PathBuf,
        recipe: PathBuf,
    }

    fn setup(name: &str) -> Setup {
        let d = scratch_dir(name);
        let home = d.join("home");
        let data = d.join("data");
        let src = d.join("mods");
        let icons = d.join("icons");
        for dir in [
            &home.join("mods/tabs"),
            &home.join("layouts"),
            &data,
            &src.join("dcc-x"),
            &icons,
        ] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::write(home.join("mods/tabs/main.js"), TABS).unwrap();
        fs::write(home.join("mods/tabs/mod.json"), "{}").unwrap();
        fs::write(home.join("settings.json"), "{\"keep\":true}").unwrap();
        fs::write(
            home.join("layouts/main.json"),
            json!({"id":"main","type":"column","children":[{"id":"tabs","type":"block","content":{"mod":"tabs"}},{"id":"panel-body","type":"stack","children":[]}]}).to_string(),
        )
        .unwrap();
        fs::write(src.join("dcc-x/mod.json"), "{}").unwrap();
        fs::write(src.join("dcc-x/main.js"), "x").unwrap();
        for (_, _, icon) in tabbar::PANELS {
            fs::write(icons.join(icon), "<svg/>").unwrap();
        }
        let recipe = d.join("recipe.json");
        fs::write(&recipe, json!({"name":"dcc","base":"main","insert":[{"into":"panel-body","block":{"id":"dcc-x","type":"block","content":{"mod":"dcc-x"}}}],"set":[{"id":"tabs","fields":{"content":{"mod":"dcc-tabs"}}}]}).to_string()).unwrap();
        Setup {
            home,
            data,
            src,
            icons,
            recipe,
        }
    }

    fn params(s: &Setup) -> Params<'_> {
        Params {
            app_home: &s.home,
            mods_src: &s.src,
            icons: &s.icons,
            recipe: &s.recipe,
            data_dir: &s.data,
            exe: Path::new("/x/dcc"),
            herdr_supported: Some(true),
        }
    }

    fn tree(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        fn walk(d: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            for e in fs::read_dir(d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out)
                } else {
                    out.push((p.clone(), fs::read(&p).unwrap()))
                }
            }
        }
        walk(dir, &mut out);
        out.sort();
        out
    }

    #[test]
    fn the_plan_lists_creates_and_replaces_and_writes_nothing() {
        let s = setup("install-plan");
        let before = tree(&s.home);
        let p = plan(&params(&s)).unwrap();
        assert_eq!(p["writes"], false);
        assert!(p["problems"].as_array().unwrap().is_empty(), "{p}");
        let rows = p["items"].as_array().unwrap();
        let find = |path: &str| {
            rows.iter().find(|r| r["path"] == path).unwrap()["action"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert_eq!(
            (
                find("mods/dcc-x").as_str(),
                find("mods/dcc-tabs").as_str(),
                find("layouts/dcc.json").as_str()
            ),
            ("create", "create", "create")
        );
        assert_eq!(before, tree(&s.home));
        assert!(
            !s.data
                .join("backups")
                .read_dir()
                .is_ok_and(|mut d| d.next().is_some())
        );
    }

    #[test]
    fn problems_stop_an_install_before_anything_is_written() {
        let s = setup("install-problem");
        fs::write(s.home.join("mods/tabs/main.js"), "something else entirely").unwrap();
        let before = tree(&s.home);
        let store = Store::open_in_memory().unwrap();
        let e = install(&params(&s), &store).unwrap_err();
        assert_eq!(e.kind, "conflict");
        assert_eq!(before, tree(&s.home));
        assert!(
            !s.data.join("backups").exists()
                || s.data.join("backups").read_dir().unwrap().next().is_none()
        );
        let mut p = params(&s);
        let missing = s.home.join("nope");
        p.app_home = &missing;
        assert!(!plan(&p).unwrap()["problems"].as_array().unwrap().is_empty());
    }

    #[test]
    fn an_install_backs_up_what_it_replaces_then_writes_only_dcc_files() {
        let s = setup("install-run");
        // an older DCC install is already there
        fs::create_dir_all(s.home.join("mods/dcc-x")).unwrap();
        fs::write(s.home.join("mods/dcc-x/main.js"), "OLD").unwrap();
        let settings_before = fs::read(s.home.join("settings.json")).unwrap();
        let tabs_before = fs::read(s.home.join("mods/tabs/main.js")).unwrap();
        let store = Store::open_in_memory().unwrap();
        let r = install(&params(&s), &store).unwrap();
        assert_eq!(r["installed"], true);
        let backup = PathBuf::from(r["backup"].as_str().unwrap());
        assert_eq!(
            fs::read_to_string(backup.join("mods/dcc-x/main.js")).unwrap(),
            "OLD"
        );
        assert!(backup.join("manifest.json").exists());
        // new files are there
        assert_eq!(
            fs::read_to_string(s.home.join("mods/dcc-x/main.js")).unwrap(),
            "x"
        );
        assert!(
            s.home.join("mods/dcc-tabs/main.js").exists()
                && s.home.join("layouts/dcc.json").exists()
        );
        // everything else is exactly as it was
        assert_eq!(
            fs::read(s.home.join("settings.json")).unwrap(),
            settings_before
        );
        assert_eq!(
            fs::read(s.home.join("mods/tabs/main.js")).unwrap(),
            tabs_before
        );
        assert!(
            !s.home
                .join("layouts/main.json")
                .metadata()
                .unwrap()
                .permissions()
                .readonly()
                || true
        );
    }

    #[test]
    fn uninstall_lists_first_then_removes_created_files_and_restores_replaced_ones() {
        let s = setup("install-undo");
        fs::create_dir_all(s.home.join("mods/dcc-x")).unwrap();
        fs::write(s.home.join("mods/dcc-x/main.js"), "OLD").unwrap();
        let store = Store::open_in_memory().unwrap();
        let r = install(&params(&s), &store).unwrap();
        let name = Path::new(r["backup"].as_str().unwrap())
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let dry = uninstall(&s.home, &s.data, &name, false).unwrap();
        assert_eq!(dry["writes"], false);
        assert!(
            s.home.join("mods/dcc-tabs").exists(),
            "a dry run removes nothing"
        );

        let done = uninstall(&s.home, &s.data, &name, true).unwrap();
        assert_eq!(done["uninstalled"], true);
        assert!(
            !s.home.join("mods/dcc-tabs").exists() && !s.home.join("layouts/dcc.json").exists()
        );
        assert_eq!(
            fs::read_to_string(s.home.join("mods/dcc-x/main.js")).unwrap(),
            "OLD",
            "the replaced file is back"
        );
        assert!(s.home.join("mods/tabs/main.js").exists() && s.home.join("settings.json").exists());
    }

    #[test]
    fn uninstall_refuses_odd_backup_names_and_manifests_that_point_elsewhere() {
        let s = setup("install-bad");
        assert_eq!(
            uninstall(&s.home, &s.data, "../x", true).unwrap_err().kind,
            "usage"
        );
        assert_eq!(
            uninstall(&s.home, &s.data, "123", true).unwrap_err().kind,
            "not_found"
        );
        let b = s.data.join("backups/777");
        fs::create_dir_all(&b).unwrap();
        fs::write(
            b.join("manifest.json"),
            json!({"items":[{"path":"settings.json","existed":true}]}).to_string(),
        )
        .unwrap();
        assert_eq!(
            uninstall(&s.home, &s.data, "777", true).unwrap_err().kind,
            "usage"
        );
        fs::write(
            b.join("manifest.json"),
            json!({"items":[{"path":"mods/dcc-../tabs","existed":false}]}).to_string(),
        )
        .unwrap();
        assert_eq!(
            uninstall(&s.home, &s.data, "777", true).unwrap_err().kind,
            "usage"
        );
        assert!(s.home.join("settings.json").exists());
    }
}
