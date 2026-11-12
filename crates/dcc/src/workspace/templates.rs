//! Built in templates. A template is a definition with `{folder}` placeholders in its folders.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::util::fnv1a_hex;
use crate::workspace::def::Definition;

const BUILTIN: &[(&str, &str)] = &[("code", include_str!("../../../../templates/code.json"))];

pub fn names() -> Vec<&'static str> {
    BUILTIN.iter().map(|(n, _)| *n).collect()
}

/// A named definition built from a template. Every `{folder}` becomes `folder`.
pub fn instantiate(template: &str, name: &str, folder: &str) -> Result<Definition> {
    let (_, text) = BUILTIN
        .iter()
        .find(|(n, _)| *n == template)
        .ok_or_else(|| {
            Error::not_found(format!(
                "no template called {template}. Known templates are {}",
                names().join(", ")
            ))
        })?;
    let mut def: Definition = serde_json::from_str(text)?;
    def.name = name.to_string();
    def.folder = folder.to_string();
    def.kind = "template".to_string();
    for tab in &mut def.tabs {
        tab.tree.replace_cwd("{folder}", folder);
    }
    Ok(def)
}

/// What links the shell pane of a code tab to its neovim. Neovim listens on `socket`, the shell
/// gets that path in `DCC_NVIM_SOCKET` and an `e` function that sends a file to it.
pub struct EditorLink {
    pub socket: PathBuf,
    pub dcc: PathBuf,
}

/// Single quotes for a shell, so a path with spaces or quotes stays one word.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The socket a workspace's neovim listens on. A hash of the name keeps the path short (a unix
/// socket path is limited to about 100 bytes) and free of odd characters.
pub fn editor_socket(data_dir: &Path, name: &str) -> PathBuf {
    data_dir
        .join("nvim")
        .join(format!("{}.sock", fnv1a_hex(name.as_bytes())))
}

/// Like `instantiate`, and for the code template it also starts neovim with `--listen` and sets
/// the shell pane up so that `e <file>` opens the file in that neovim. The commands are typed into
/// the panes, and are stored in the definition like any other startup command.
pub fn instantiate_linked(
    template: &str,
    name: &str,
    folder: &str,
    link: &EditorLink,
) -> Result<Definition> {
    let mut def = instantiate(template, name, folder)?;
    if template != "code" {
        return Ok(def);
    }
    let socket = shell_quote(&link.socket.to_string_lossy());
    let dcc = shell_quote(&link.dcc.to_string_lossy());
    for tab in &mut def.tabs {
        for startup in tab.tree.startups_mut() {
            match startup.as_deref() {
                Some("nvim") => *startup = Some(format!("nvim --listen {socket}")),
                None => {
                    *startup = Some(format!(
                        "export DCC_NVIM_SOCKET={socket}; e() {{ {dcc} code edit \"$@\"; }}; clear"
                    ));
                }
                _ => {}
            }
        }
    }
    Ok(def)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_template_has_three_panes_and_two_programs() {
        let d = instantiate("code", "proj", "/work/proj").unwrap();
        assert_eq!((d.name.as_str(), d.kind.as_str()), ("proj", "template"));
        let leaves = d.tabs[0].tree.leaves();
        assert_eq!(
            leaves,
            vec![
                ("/work/proj", Some("nvim")),
                ("/work/proj", Some("lazygit")),
                ("/work/proj", None)
            ]
        );
    }

    #[test]
    fn a_linked_code_tab_starts_neovim_with_a_socket_and_sets_up_the_shell() {
        let link = EditorLink {
            socket: PathBuf::from("/data/nvim/abc.sock"),
            dcc: PathBuf::from("/opt/my tools/dcc"),
        };
        let d = instantiate_linked("code", "proj", "/work/proj", &link).unwrap();
        let leaves = d.tabs[0].tree.leaves();
        assert_eq!(leaves[0].1, Some("nvim --listen '/data/nvim/abc.sock'"));
        assert_eq!(leaves[1].1, Some("lazygit"));
        let shell = leaves[2].1.unwrap();
        assert!(shell.contains("export DCC_NVIM_SOCKET='/data/nvim/abc.sock'"));
        assert!(
            shell.contains("'/opt/my tools/dcc' code edit \"$@\""),
            "{shell}"
        );
        assert!(shell.ends_with("clear"));
    }

    #[test]
    fn quoting_keeps_odd_paths_in_one_word() {
        assert_eq!(shell_quote("/a b/c"), "'/a b/c'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn the_socket_path_is_short_and_depends_on_the_name() {
        let a = editor_socket(Path::new("/d"), "proj-one");
        let b = editor_socket(Path::new("/d"), "proj-two");
        assert_ne!(a, b);
        assert!(a.to_string_lossy().len() < 40);
        assert!(a.starts_with("/d/nvim"));
    }

    #[test]
    fn unknown_template_lists_the_known_ones() {
        let e = instantiate("nope", "x", "/").unwrap_err();
        assert_eq!(e.kind, "not_found");
        assert!(e.message.contains("code"));
    }
}
