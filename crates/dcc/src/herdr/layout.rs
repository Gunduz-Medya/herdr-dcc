//! DCC's own layout. The layouts Herdr ships are read-only and replaced on every start, so DCC
//! builds its own: the shipped `main` layout plus the blocks of a recipe, written to
//! `<app home>/layouts/<name>.json`. It is rebuilt whenever the shipped layout, the recipe or the
//! Herdr app version changes (their hashes are kept in the database).
//!
//! Recipe: `{ "name": "dcc", "base": "main", "insert": [ { "into": "<block id>", "block": {..} } ],
//! "set": [ { "id": "<node id>", "fields": {..} } ] }`. `set` replaces fields of an existing node,
//! for example the mod a block shows.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::store::Store;
use crate::util::fnv1a_hex;

fn find_mut<'a>(node: &'a mut Value, id: &str) -> Option<&'a mut Value> {
    if node.get("id").and_then(Value::as_str) == Some(id) {
        return Some(node);
    }
    let children = node.get_mut("children")?.as_array_mut()?;
    children.iter_mut().find_map(|c| find_mut(c, id))
}

/// Applies the recipe to the base layout. A block whose id already exists in the target is
/// replaced, so applying twice gives the same result.
pub fn apply(base: &Value, recipe: &Value, app_version: &str) -> Result<Value> {
    let name = recipe
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::usage("recipe has no name"))?;
    let base_name = recipe.get("base").and_then(Value::as_str).unwrap_or("main");
    let mut out = base.clone();
    for insert in recipe
        .get("insert")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let into = insert
            .get("into")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::usage("insert without a target"))?;
        let block = insert
            .get("block")
            .cloned()
            .ok_or_else(|| Error::usage("insert without a block"))?;
        let block_id = block.get("id").and_then(Value::as_str).map(str::to_string);
        let target = find_mut(&mut out, into).ok_or_else(|| {
            Error::not_found(format!("the base layout has no block with id {into}"))
        })?;
        let children = target
            .as_object_mut()
            .ok_or_else(|| Error::usage("target is not an object"))?
            .entry("children")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| Error::usage("target children is not a list"))?;
        if let Some(id) = block_id {
            children.retain(|c| c.get("id").and_then(Value::as_str) != Some(id.as_str()));
        }
        children.push(block);
    }
    for set in recipe
        .get("set")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let id = set
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::usage("set without an id"))?;
        let fields = set
            .get("fields")
            .and_then(Value::as_object)
            .ok_or_else(|| Error::usage("set without fields"))?;
        let node = find_mut(&mut out, id)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| Error::not_found(format!("the base layout has no node with id {id}")))?;
        for (k, v) in fields {
            if k == "id" || k == "children" {
                return Err(Error::usage("set cannot change id or children"));
            }
            node.insert(k.clone(), v.clone());
        }
    }
    let obj = out
        .as_object_mut()
        .ok_or_else(|| Error::usage("layout is not an object"))?;
    obj.insert("id".into(), json!(name));
    obj.insert(
        "cloned_from".into(),
        json!({ "name": base_name, "version": app_version }),
    );
    Ok(out)
}

/// A layout name is a file name, so only `[A-Za-z0-9_-]` is allowed. A name with a dot or a
/// separator could point outside the layouts folder.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub fn layout_path(app_home: &Path, name: &str) -> Result<PathBuf> {
    if !valid_name(name) {
        return Err(Error::usage(format!(
            "invalid layout name {name:?}, use letters, digits, dash and underscore"
        )));
    }
    Ok(app_home.join("layouts").join(format!("{name}.json")))
}

/// Builds the layout file when needed. `force` rebuilds it anyway.
pub fn ensure(
    app_home: &Path,
    recipe_path: &Path,
    app_version: &str,
    store: &Store,
    force: bool,
) -> Result<Value> {
    let recipe_text = fs::read_to_string(recipe_path).map_err(|e| {
        Error::not_found(format!("cannot read recipe {}: {e}", recipe_path.display()))
    })?;
    let recipe: Value = serde_json::from_str(&recipe_text)?;
    let name = recipe
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::usage("recipe has no name"))?
        .to_string();
    let base_name = recipe
        .get("base")
        .and_then(Value::as_str)
        .unwrap_or("main")
        .to_string();
    let out_path = layout_path(app_home, &name)?;
    let base_path = layout_path(app_home, &base_name)?;
    let base_text = fs::read_to_string(&base_path).map_err(|e| {
        Error::not_found(format!(
            "cannot read the base layout {}: {e}",
            base_path.display()
        ))
    })?;
    let inputs = format!(
        "{}:{}:{}",
        fnv1a_hex(base_text.as_bytes()),
        fnv1a_hex(recipe_text.as_bytes()),
        app_version
    );
    let key = format!("layout.{name}.inputs");

    let reason = if force {
        Some("forced")
    } else if !out_path.exists() {
        Some("missing")
    } else {
        match store.state(&key)? {
            None => Some("unknown_origin"),
            Some(prev) if prev != inputs => Some("inputs_changed"),
            Some(_) => None,
        }
    };
    let Some(reason) = reason else {
        return Ok(
            json!({ "layout": name, "regenerated": false, "path": out_path.to_string_lossy() }),
        );
    };
    let base: Value = serde_json::from_str(&base_text)?;
    let built = apply(&base, &recipe, app_version)?;
    fs::write(&out_path, serde_json::to_string_pretty(&built)?)?;
    store.set_state(&key, &inputs)?;
    Ok(
        json!({ "layout": name, "regenerated": true, "reason": reason, "path": out_path.to_string_lossy() }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::scratch_dir;

    fn base() -> Value {
        json!({"id":"main","cloned_from":{"name":"x"},"type":"column","children":[{"id":"work","type":"row","children":[{"id":"files","type":"block"}]}]})
    }

    fn recipe() -> Value {
        json!({"name":"dcc","base":"main","insert":[{"into":"work","block":{"id":"dcc-workspace","type":"block","content":{"mod":"dcc-workspace"}}}]})
    }

    #[test]
    fn set_replaces_fields_of_an_existing_node_and_refuses_unknown_ones() {
        let r = json!({"name":"dcc","set":[{"id":"files","fields":{"content":{"mod":"other"}}}]});
        let out = apply(&base(), &r, "1").unwrap();
        assert_eq!(out["children"][0]["children"][0]["content"]["mod"], "other");
        let missing = json!({"name":"dcc","set":[{"id":"nope","fields":{"x":1}}]});
        assert_eq!(apply(&base(), &missing, "1").unwrap_err().kind, "not_found");
        let forbidden = json!({"name":"dcc","set":[{"id":"files","fields":{"children":[]}}]});
        assert_eq!(apply(&base(), &forbidden, "1").unwrap_err().kind, "usage");
    }

    #[test]
    fn inserts_the_block_and_renames() {
        let out = apply(&base(), &recipe(), "0.1.15").unwrap();
        assert_eq!(out["id"], "dcc");
        assert_eq!(out["cloned_from"]["version"], "0.1.15");
        let kids = out["children"][0]["children"].as_array().unwrap();
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[1]["id"], "dcc-workspace");
    }

    #[test]
    fn applying_twice_does_not_duplicate() {
        let once = apply(&base(), &recipe(), "v").unwrap();
        let twice = apply(&once, &recipe(), "v").unwrap();
        assert_eq!(
            twice["children"][0]["children"].as_array().unwrap().len(),
            2
        );
    }

    #[test]
    fn unknown_target_is_an_error() {
        let r = json!({"name":"dcc","insert":[{"into":"nope","block":{"id":"b"}}]});
        assert_eq!(apply(&base(), &r, "v").unwrap_err().kind, "not_found");
    }

    #[test]
    fn layout_names_cannot_leave_the_layouts_folder() {
        for bad in ["", "../x", "a/b", "a\\b", "a.b", "x y", &"n".repeat(65)] {
            assert!(layout_path(Path::new("/h"), bad).is_err(), "{bad}");
        }
        assert!(layout_path(Path::new("/h"), "dcc-2_b").is_ok());
        let dir = scratch_dir("layout-bad");
        let recipe_path = dir.join("r.json");
        fs::write(
            &recipe_path,
            json!({"name":"../evil","insert":[]}).to_string(),
        )
        .unwrap();
        let store = Store::open_in_memory().unwrap();
        assert_eq!(
            ensure(&dir, &recipe_path, "v", &store, false)
                .unwrap_err()
                .kind,
            "usage"
        );
        fs::write(
            &recipe_path,
            json!({"name":"ok","base":"../main","insert":[]}).to_string(),
        )
        .unwrap();
        assert_eq!(
            ensure(&dir, &recipe_path, "v", &store, false)
                .unwrap_err()
                .kind,
            "usage"
        );
    }

    #[test]
    fn ensure_regenerates_only_when_an_input_changes() {
        let dir = scratch_dir("layout");
        let home = dir.join("home");
        fs::create_dir_all(home.join("layouts")).unwrap();
        fs::write(home.join("layouts/main.json"), base().to_string()).unwrap();
        let recipe_path = dir.join("dcc.recipe.json");
        fs::write(&recipe_path, recipe().to_string()).unwrap();
        let store = Store::open_in_memory().unwrap();

        let first = ensure(&home, &recipe_path, "0.1.15", &store, false).unwrap();
        assert_eq!(
            (first["regenerated"].as_bool(), first["reason"].as_str()),
            (Some(true), Some("missing"))
        );
        let again = ensure(&home, &recipe_path, "0.1.15", &store, false).unwrap();
        assert_eq!(again["regenerated"], false);
        let newer_herdr = ensure(&home, &recipe_path, "0.1.16", &store, false).unwrap();
        assert_eq!(newer_herdr["reason"], "inputs_changed");
        // the shipped layout changes under us
        fs::write(
            home.join("layouts/main.json"),
            base().to_string().replace("files", "files2"),
        )
        .unwrap();
        assert_eq!(
            ensure(&home, &recipe_path, "0.1.16", &store, false).unwrap()["regenerated"],
            true
        );
        assert_eq!(
            ensure(&home, &recipe_path, "0.1.16", &store, true).unwrap()["reason"],
            "forced"
        );
    }
}
