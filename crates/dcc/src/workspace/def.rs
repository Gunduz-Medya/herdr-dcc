use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Right,
    Down,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Right => "right",
            Direction::Down => "down",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    Pane {
        cwd: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        startup: Option<String>,
    },
    Split {
        direction: Direction,
        first: Box<Node>,
        second: Box<Node>,
    },
}

impl Node {
    pub fn pane(cwd: &str, startup: Option<&str>) -> Node {
        Node::Pane {
            cwd: cwd.to_string(),
            startup: startup.map(str::to_string),
        }
    }

    /// Panes in tree order, the order `herdr-app ls` lists them in.
    pub fn leaves(&self) -> Vec<(&str, Option<&str>)> {
        match self {
            Node::Pane { cwd, startup } => vec![(cwd.as_str(), startup.as_deref())],
            Node::Split { first, second, .. } => {
                let mut v = first.leaves();
                v.extend(second.leaves());
                v
            }
        }
    }

    pub fn first_cwd(&self) -> &str {
        match self {
            Node::Pane { cwd, .. } => cwd,
            Node::Split { first, .. } => first.first_cwd(),
        }
    }

    /// A row of panes, split to the right one after the other.
    pub fn row(panes: Vec<(String, Option<String>)>) -> Option<Node> {
        let mut it = panes.into_iter().rev();
        let (cwd, startup) = it.next()?;
        let mut node = Node::Pane { cwd, startup };
        for (cwd, startup) in it {
            node = Node::Split {
                direction: Direction::Right,
                first: Box::new(Node::Pane { cwd, startup }),
                second: Box::new(node),
            };
        }
        Some(node)
    }

    /// Sets the startup command of the nth leaf (0 based). Returns false when there is no such leaf.
    pub fn set_startup(&mut self, mut n: usize, command: &str) -> bool {
        fn walk(node: &mut Node, n: &mut usize, command: &str) -> bool {
            match node {
                Node::Pane { startup, .. } => {
                    if *n == 0 {
                        *startup = Some(command.to_string());
                        true
                    } else {
                        *n -= 1;
                        false
                    }
                }
                Node::Split { first, second, .. } => {
                    walk(first, n, command) || walk(second, n, command)
                }
            }
        }
        walk(self, &mut n, command)
    }

    pub fn replace_cwd(&mut self, from: &str, to: &str) {
        match self {
            Node::Pane { cwd, .. } => *cwd = cwd.replace(from, to),
            Node::Split { first, second, .. } => {
                first.replace_cwd(from, to);
                second.replace_cwd(from, to);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabDef {
    pub name: String,
    /// False when the tab was called by its number (1, 2, ...).
    pub named: bool,
    pub tree: Node,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    /// Templates leave these two empty, they are filled in when a template is applied.
    #[serde(default)]
    pub folder: String,
    /// `snapshot` or `template`.
    #[serde(default)]
    pub kind: String,
    pub tabs: Vec<TabDef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_leaves_keep_their_order() {
        let n = Node::row(vec![
            ("a".into(), None),
            ("b".into(), Some("x".into())),
            ("c".into(), None),
        ])
        .unwrap();
        assert_eq!(n.leaves(), vec![("a", None), ("b", Some("x")), ("c", None)]);
        assert!(Node::row(vec![]).is_none());
        assert_eq!(n.first_cwd(), "a");
    }

    #[test]
    fn startup_by_leaf_index() {
        let mut n = Node::row(vec![("a".into(), None), ("b".into(), None)]).unwrap();
        assert!(n.set_startup(1, "lazygit"));
        assert!(!n.set_startup(5, "x"));
        assert_eq!(n.leaves()[1].1, Some("lazygit"));
    }

    #[test]
    fn json_round_trip_is_tagged_and_compact() {
        let d = Definition {
            name: "w".into(),
            folder: "/f".into(),
            kind: "template".into(),
            tabs: vec![TabDef {
                name: "t".into(),
                named: true,
                tree: Node::row(vec![
                    ("/f".into(), None),
                    ("/g".into(), Some("nvim".into())),
                ])
                .unwrap(),
            }],
        };
        let text = serde_json::to_string(&d).unwrap();
        assert!(text.contains("\"type\":\"split\"") && text.contains("\"direction\":\"right\""));
        assert!(!text.contains("null"));
        assert_eq!(serde_json::from_str::<Definition>(&text).unwrap(), d);
    }
}
