//! A read only list of the AI tool configuration under a home folder. Only the file system
//! metadata is looked at (path, kind, size, time, link). No file is opened, so no content, no
//! secret and no setting value can reach the output. Files that look like credentials are not
//! listed, only counted.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::{Value, json};

const MAX_ENTRIES_PER_DIR: usize = 400;
const MAX_FILES_FOR_SIZE: usize = 2000;
const MAX_DEPTH: usize = 4;

#[derive(Clone, Copy)]
enum Shape {
    /// One file at a fixed path.
    File,
    /// Every entry directly inside a folder is one item (a skill folder, a rule file).
    Children,
    /// The folder itself is one item.
    Folder,
}

struct Spot {
    agent: &'static str,
    kind: &'static str,
    path: &'static str,
    shape: Shape,
}

const SPOTS: &[Spot] = &[
    Spot {
        agent: "claude",
        kind: "settings",
        path: ".claude/settings.json",
        shape: Shape::File,
    },
    Spot {
        agent: "claude",
        kind: "settings",
        path: ".claude/settings.local.json",
        shape: Shape::File,
    },
    Spot {
        agent: "claude",
        kind: "memory",
        path: ".claude/CLAUDE.md",
        shape: Shape::File,
    },
    Spot {
        agent: "claude",
        kind: "mcp",
        path: ".claude.json",
        shape: Shape::File,
    },
    Spot {
        agent: "claude",
        kind: "skill",
        path: ".claude/skills",
        shape: Shape::Children,
    },
    Spot {
        agent: "claude",
        kind: "agent",
        path: ".claude/agents",
        shape: Shape::Children,
    },
    Spot {
        agent: "claude",
        kind: "command",
        path: ".claude/commands",
        shape: Shape::Children,
    },
    Spot {
        agent: "claude",
        kind: "rule",
        path: ".claude/rules",
        shape: Shape::Children,
    },
    Spot {
        agent: "claude",
        kind: "hook",
        path: ".claude/hooks",
        shape: Shape::Children,
    },
    Spot {
        agent: "claude",
        kind: "plugin",
        path: ".claude/plugins",
        shape: Shape::Folder,
    },
    Spot {
        agent: "codex",
        kind: "settings",
        path: ".codex/config.toml",
        shape: Shape::File,
    },
    Spot {
        agent: "codex",
        kind: "memory",
        path: ".codex/AGENTS.md",
        shape: Shape::File,
    },
    Spot {
        agent: "codex",
        kind: "command",
        path: ".codex/prompts",
        shape: Shape::Children,
    },
    Spot {
        agent: "codex",
        kind: "skill",
        path: ".codex/skills",
        shape: Shape::Children,
    },
    Spot {
        agent: "codex",
        kind: "rule",
        path: ".codex/rules",
        shape: Shape::Children,
    },
    Spot {
        agent: "gemini",
        kind: "settings",
        path: ".gemini/settings.json",
        shape: Shape::File,
    },
    Spot {
        agent: "gemini",
        kind: "memory",
        path: ".gemini/GEMINI.md",
        shape: Shape::File,
    },
    Spot {
        agent: "gemini",
        kind: "command",
        path: ".gemini/commands",
        shape: Shape::Children,
    },
    Spot {
        agent: "cursor",
        kind: "rule",
        path: ".cursor/rules",
        shape: Shape::Children,
    },
    Spot {
        agent: "cursor",
        kind: "mcp",
        path: ".cursor/mcp.json",
        shape: Shape::File,
    },
    Spot {
        agent: "skillshare",
        kind: "settings",
        path: ".config/skillshare/config.yaml",
        shape: Shape::File,
    },
    Spot {
        agent: "skillshare",
        kind: "skill",
        path: ".config/skillshare/skills",
        shape: Shape::Children,
    },
];

/// Names that hold credentials. They are counted, never listed.
pub fn withheld(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "auth.json"
        || n.starts_with(".env")
        || n.ends_with(".key")
        || n.ends_with(".pem")
        || ["credential", "secret", "token", "password", "oauth"]
            .iter()
            .any(|w| n.contains(w))
}

#[derive(Default)]
struct Totals {
    withheld: u64,
    truncated: bool,
}

fn modified_ms(m: &fs::Metadata) -> Option<i64> {
    let d = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(d.as_millis()).ok()
}

/// Sum of the sizes below a folder and the number of files, from metadata only. Links are not followed.
fn folder_size(dir: &Path, depth: usize, seen: &mut usize, hidden: &mut u64) -> u64 {
    let Ok(rd) = fs::read_dir(dir) else { return 0 };
    let mut sum = 0;
    for e in rd.flatten() {
        if *seen >= MAX_FILES_FOR_SIZE {
            break;
        }
        let Ok(m) = e.path().symlink_metadata() else {
            continue;
        };
        if withheld(&e.file_name().to_string_lossy()) {
            *hidden += 1;
            continue;
        }
        if m.is_dir() && depth < MAX_DEPTH {
            sum += folder_size(&e.path(), depth + 1, seen, hidden);
        } else if m.is_file() {
            *seen += 1;
            sum += m.len();
        }
    }
    sum
}

fn item(spot: &Spot, home: &Path, p: &Path, totals: &mut Totals) -> Option<Value> {
    let m = p.symlink_metadata().ok()?;
    let link = m.file_type().is_symlink();
    let (bytes, files) = if m.is_dir() {
        let mut seen = 0;
        let size = folder_size(p, 0, &mut seen, &mut totals.withheld);
        (size, Some(seen))
    } else {
        // a link is reported with the size of the link itself, nothing behind it is touched
        (m.len(), None)
    };
    let rel = p.strip_prefix(home).unwrap_or(p);
    Some(json!({
        "agent": spot.agent, "kind": spot.kind,
        "path": format!("~/{}", rel.to_string_lossy()),
        "bytes": bytes, "files": files, "modified_ms": modified_ms(&m),
        "symlink": link, "is_dir": m.is_dir(),
    }))
}

pub fn scan(home: &Path) -> Value {
    let mut items: Vec<Value> = Vec::new();
    let mut totals = Totals::default();
    for spot in SPOTS {
        let base: PathBuf = home.join(spot.path);
        match spot.shape {
            Shape::File | Shape::Folder => {
                if let Some(v) = item(spot, home, &base, &mut totals) {
                    items.push(v);
                }
            }
            Shape::Children => {
                let Ok(rd) = fs::read_dir(&base) else {
                    continue;
                };
                let mut names: Vec<_> = rd.flatten().collect();
                names.sort_by_key(|e| e.file_name());
                if names.len() > MAX_ENTRIES_PER_DIR {
                    totals.truncated = true;
                    names.truncate(MAX_ENTRIES_PER_DIR);
                }
                for e in names {
                    let name = e.file_name().to_string_lossy().to_string();
                    if withheld(&name) {
                        totals.withheld += 1;
                        continue;
                    }
                    if name.starts_with('.') && name != ".system" {
                        continue;
                    }
                    if let Some(v) = item(spot, home, &e.path(), &mut totals) {
                        items.push(v);
                    }
                }
            }
        }
    }
    let mut agents: Vec<String> = items
        .iter()
        .filter_map(|i| i["agent"].as_str().map(str::to_string))
        .collect();
    agents.dedup();
    agents.sort();
    agents.dedup();
    let summary: Vec<Value> = agents
        .iter()
        .map(|a| {
            let mine: Vec<&Value> = items.iter().filter(|i| i["agent"] == a.as_str()).collect();
            let mut kinds = serde_json::Map::new();
            for i in &mine {
                let k = i["kind"].as_str().unwrap_or("?").to_string();
                let n = kinds.get(&k).and_then(Value::as_u64).unwrap_or(0) + 1;
                kinds.insert(k, json!(n));
            }
            json!({ "agent": a, "items": mine.len(), "by_kind": kinds,
                    "bytes": mine.iter().filter_map(|i| i["bytes"].as_u64()).sum::<u64>() })
        })
        .collect();
    json!({ "agents": summary, "items": items, "withheld_credential_like": totals.withheld,
            "truncated": totals.truncated, "content_read": false })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::scratch_dir;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn put(home: &Path, rel: &str, text: &str) {
        let p = home.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    #[test]
    fn lists_paths_kinds_and_sizes_per_agent() {
        let h = scratch_dir("inv");
        put(&h, ".claude/settings.json", "{}");
        put(&h, ".claude/CLAUDE.md", "12345");
        put(&h, ".claude/skills/alpha/SKILL.md", "abc");
        put(&h, ".claude/skills/alpha/notes/more.md", "abcd");
        put(&h, ".claude/skills/beta/SKILL.md", "z");
        put(&h, ".claude/commands/c.md", "x");
        put(&h, ".codex/config.toml", "x");
        let v = scan(&h);
        let items = v["items"].as_array().unwrap();
        let find = |p: &str| {
            items
                .iter()
                .find(|i| i["path"] == p)
                .unwrap_or_else(|| panic!("{p}"))
        };
        assert_eq!(find("~/.claude/CLAUDE.md")["kind"], "memory");
        assert_eq!(find("~/.claude/CLAUDE.md")["bytes"], 5);
        let alpha = find("~/.claude/skills/alpha");
        assert_eq!(
            (
                alpha["kind"].as_str(),
                alpha["bytes"].as_u64(),
                alpha["files"].as_u64()
            ),
            (Some("skill"), Some(7), Some(2))
        );
        assert_eq!(find("~/.codex/config.toml")["agent"], "codex");
        let claude = v["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["agent"] == "claude")
            .unwrap();
        assert_eq!(claude["by_kind"]["skill"], 2);
        assert!(
            items
                .iter()
                .all(|i| i["path"].as_str().unwrap().starts_with("~/"))
        );
    }

    #[test]
    fn no_file_is_opened_and_no_content_reaches_the_output() {
        let h = scratch_dir("inv-noread");
        put(&h, ".claude/settings.json", "TOP-SECRET-MARKER-123");
        put(&h, ".claude/CLAUDE.md", "TOP-SECRET-MARKER-456");
        // a file nobody can read is still listed, because only metadata is used
        let p = h.join(".claude/settings.json");
        fs::set_permissions(&p, fs::Permissions::from_mode(0o000)).unwrap();
        let v = scan(&h);
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        let text = v.to_string();
        assert!(!text.contains("TOP-SECRET-MARKER"));
        assert!(
            v["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["path"] == "~/.claude/settings.json")
        );
        assert_eq!(v["content_read"], false);
    }

    #[test]
    fn credential_like_names_are_counted_not_listed() {
        let h = scratch_dir("inv-cred");
        put(&h, ".claude/skills/ok/SKILL.md", "x");
        put(&h, ".claude/skills/ok/api-token.txt", "x");
        put(&h, ".claude/skills/oauth-helper/SKILL.md", "x");
        put(&h, ".claude/skills/.env", "x");
        let v = scan(&h);
        let text = v.to_string();
        assert!(
            !text.contains("oauth-helper") && !text.contains("api-token") && !text.contains(".env")
        );
        assert_eq!(v["withheld_credential_like"], 3);
        let ok = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["path"] == "~/.claude/skills/ok")
            .unwrap();
        assert_eq!(
            ok["bytes"], 1,
            "the credential like file inside is not counted either"
        );
    }

    #[test]
    fn links_are_reported_and_not_followed() {
        let h = scratch_dir("inv-link");
        let outside = scratch_dir("inv-link-target");
        put(&outside, "big.txt", &"x".repeat(5000));
        fs::create_dir_all(h.join(".claude/skills")).unwrap();
        symlink(&outside, h.join(".claude/skills/linked")).unwrap();
        let v = scan(&h);
        let l = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["path"] == "~/.claude/skills/linked")
            .unwrap();
        assert_eq!(l["symlink"], true);
        assert!(
            l["bytes"].as_u64().unwrap() < 1000,
            "the target's size must not be counted"
        );
        assert_eq!(l["is_dir"], false);
    }

    #[test]
    fn an_empty_home_gives_an_empty_inventory() {
        let h = scratch_dir("inv-empty");
        let v = scan(&h);
        assert!(v["items"].as_array().unwrap().is_empty());
        assert!(v["agents"].as_array().unwrap().is_empty());
    }
}
