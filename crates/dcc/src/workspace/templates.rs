//! Built in templates. A template is a definition with `{folder}` placeholders in its folders.

use crate::error::{Error, Result};
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
    fn unknown_template_lists_the_known_ones() {
        let e = instantiate("nope", "x", "/").unwrap_err();
        assert_eq!(e.kind, "not_found");
        assert!(e.message.contains("code"));
    }
}
