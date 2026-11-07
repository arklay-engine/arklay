//! The bot's story parameters: `params/<game>.xml` declares them, and
//! `--bot-param NAME=VALUE` picks a value for one run.
//!
//! A parameter is a decision the game leaves to the player (answer Barry's
//! prompt, deliver the serum, free a cell), so that runs can be steered down
//! each branch of the story. Each one declares its allowed values and a
//! default, the choice the bot makes without the flag:
//!
//! ```xml
//! <params format="1">
//!   <param name="richard" values="save|leave" default="save"
//!          label="bring Richard the serum in 20D"/>
//! </params>
//! ```
//!
//! A room fact in `bot.xml` takes `when="NAME=VALUE"` and holds only while
//! the parameter has that value ([`When`]); Lua reads the values as
//! `bot:param(name)`. An undeclared name or an undeclared value, on the
//! command line or in a `when`, is an error.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use roxmltree::{Document, Node};

use super::meta::{at, check_attributes, check_leaf, elements};
use crate::budget;
use crate::pack::Pack;

/// The parameter file's format version.
const FORMAT: u32 = 1;
/// The largest accepted parameter file.
const MAX_BYTES: usize = 64 * 1024;

/// One declared parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decl {
    pub name: String,
    /// The allowed values, in declaration order.
    pub values: Vec<String>,
    /// The value without `--bot-param`: the bot's usual choice.
    pub default: String,
    /// What the decision is, for the run log and the docs.
    pub label: String,
}

/// The declared parameters and one run's values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Params {
    decls: Vec<Decl>,
    values: BTreeMap<String, String>,
}

impl Params {
    /// The declarations with every value at its default.
    pub fn new(decls: Vec<Decl>) -> Self {
        let values = decls
            .iter()
            .map(|decl| (decl.name.clone(), decl.default.clone()))
            .collect();
        Self { decls, values }
    }

    /// Set `name` to `value` (`--bot-param`): both must be declared.
    pub fn set(&mut self, name: &str, value: &str) -> Result<()> {
        let decl = self.decl(name)?;
        if !decl.values.iter().any(|allowed| allowed == value) {
            bail!(
                "bot parameter `{name}` has no value `{value}` (allowed: {})",
                decl.values.join(", ")
            );
        }
        self.values.insert(name.to_string(), value.to_string());
        Ok(())
    }

    /// The value of `name`, when it is declared.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// The run's values, `name=value` in declaration order, for the log.
    pub fn summary(&self) -> String {
        self.decls
            .iter()
            .map(|decl| format!("{}={}", decl.name, self.values[&decl.name]))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn decl(&self, name: &str) -> Result<&Decl> {
        self.decls
            .iter()
            .find(|decl| decl.name == name)
            .with_context(|| {
                let known: Vec<&str> = self.decls.iter().map(|decl| decl.name.as_str()).collect();
                format!(
                    "unknown bot parameter `{name}` (declared: {})",
                    if known.is_empty() {
                        "none".to_string()
                    } else {
                        known.join(", ")
                    }
                )
            })
    }

    /// Whether `when` holds for these values. A name or value `when` uses
    /// that is not declared is an error: a typo would otherwise drop or keep
    /// a fact silently.
    pub fn holds(&self, when: &When) -> Result<bool> {
        for cond in &when.conds {
            let decl = self.decl(&cond.name)?;
            for value in &cond.values {
                if !decl.values.contains(value) {
                    bail!(
                        "bot parameter `{}` has no value `{value}` (allowed: {})",
                        cond.name,
                        decl.values.join(", ")
                    );
                }
            }
            let value = &self.values[&cond.name];
            if cond.values.contains(value) == cond.negate {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// A fact's `when` attribute: conditions separated by spaces, all of which
/// must hold. Each is `NAME=VALUE` or `NAME!=VALUE`, and `VALUE` may list
/// alternatives as `a|b` (`richard=save|late`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct When {
    conds: Vec<Cond>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Cond {
    name: String,
    negate: bool,
    values: Vec<String>,
}

/// A parameter name or value: lower-case ASCII letters, digits, `_` and `-`.
fn is_word(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

impl When {
    pub fn parse(text: &str) -> Result<Self> {
        let mut conds = Vec::new();
        for part in text.split_whitespace() {
            let (name, negate, values) = if let Some((name, values)) = part.split_once("!=") {
                (name, true, values)
            } else if let Some((name, values)) = part.split_once('=') {
                (name, false, values)
            } else {
                bail!("`when` condition `{part}` is not NAME=VALUE or NAME!=VALUE");
            };
            let values: Vec<String> = values.split('|').map(str::to_string).collect();
            if !is_word(name) || !values.iter().all(|value| is_word(value)) {
                bail!(
                    "`when` condition `{part}`: names and values are lower-case letters, digits, `_` and `-`"
                );
            }
            conds.push(Cond {
                name: name.to_string(),
                negate,
                values,
            });
        }
        if conds.is_empty() {
            bail!("`when` is empty");
        }
        Ok(Self { conds })
    }
}

/// Parse `--bot-param NAME=VALUE`'s text into its two halves.
pub fn parse_assignment(text: &str) -> Result<(String, String), String> {
    let (name, value) = text
        .split_once('=')
        .ok_or_else(|| format!("`{text}` is not NAME=VALUE"))?;
    if !is_word(name) || !is_word(value) {
        return Err(format!(
            "`{text}`: names and values are lower-case letters, digits, `_` and `-`"
        ));
    }
    Ok((name.to_string(), value.to_string()))
}

/// The pack path of a game's parameter file.
pub fn path_for(game: &str) -> String {
    format!("params/{game}.xml")
}

/// Load the declarations for the pack's game (none when the pack carries
/// no parameter file), with `overrides` applied.
pub fn load(pack: &Pack, overrides: &[(String, String)]) -> Result<Params> {
    let mut params = Params::default();
    for game in super::knowledge::games(pack) {
        let path = path_for(&game);
        if !pack.contains(&path) {
            continue;
        }
        let bytes = pack
            .read(&path)
            .with_context(|| format!("cannot read {path}"))?;
        let text =
            std::str::from_utf8(&bytes).with_context(|| format!("{path} is not valid UTF-8"))?;
        params = Params::new(parse(text).with_context(|| path.clone())?);
        break;
    }
    for (name, value) in overrides {
        params.set(name, value).context("--bot-param")?;
    }
    Ok(params)
}

/// Parse one parameter document.
pub fn parse(text: &str) -> Result<Vec<Decl>> {
    budget::check_len(text.len(), MAX_BYTES, "bot params XML size")?;
    let document = Document::parse(text).map_err(|err| {
        anyhow::anyhow!(
            "params XML is not well-formed at line {}: {err}",
            err.pos().row
        )
    })?;
    let root = document.root_element();
    if root.tag_name().name() != "params" {
        bail!("{}: the root must be <params>", at(root));
    }
    check_attributes(root, &["format"])?;
    let format: u32 = super::meta::attr(root, "format")?;
    if format != FORMAT {
        bail!("{}: params format {format} is not {FORMAT}", at(root));
    }
    let mut decls: Vec<Decl> = Vec::new();
    for child in elements(root) {
        if child.tag_name().name() != "param" {
            bail!(
                "{}: unknown element <{}>",
                at(child),
                child.tag_name().name()
            );
        }
        let decl = parse_decl(child)?;
        if decls.iter().any(|held| held.name == decl.name) {
            bail!("{}: duplicate parameter `{}`", at(child), decl.name);
        }
        decls.push(decl);
    }
    Ok(decls)
}

fn parse_decl(node: Node<'_, '_>) -> Result<Decl> {
    check_attributes(node, &["name", "values", "default", "label"])?;
    check_leaf(node)?;
    let text = |name: &str| -> Result<String> {
        node.attribute(name)
            .map(str::to_string)
            .with_context(|| format!("{}: missing attribute `{name}`", at(node)))
    };
    let name = text("name")?;
    let values: Vec<String> = text("values")?.split('|').map(str::to_string).collect();
    let default = text("default")?;
    let label = text("label")?;
    if !is_word(&name) {
        bail!(
            "{}: parameter name `{name}` is not lower-case letters, digits, `_` and `-`",
            at(node)
        );
    }
    if values.len() < 2 || !values.iter().all(|value| is_word(value)) {
        bail!(
            "{}: `values` lists two or more words separated by `|`",
            at(node)
        );
    }
    if values
        .iter()
        .enumerate()
        .any(|(i, value)| values[..i].contains(value))
    {
        bail!("{}: `values` repeats a value", at(node));
    }
    if !values.contains(&default) {
        bail!("{}: default `{default}` is not one of `values`", at(node));
    }
    if label.trim().is_empty() {
        bail!("{}: `label` is empty", at(node));
    }
    Ok(Decl {
        name,
        values,
        default,
        label,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<params format="1">
        <param name="richard" values="save|leave" default="save" label="the serum to Richard"/>
        <param name="go_first" values="yes|no" default="yes" label="Barry: I can go first"/>
    </params>"#;

    fn params() -> Params {
        Params::new(parse(DOC).unwrap())
    }

    #[test]
    fn declarations_parse_and_default() {
        let params = params();
        assert_eq!(params.get("richard"), Some("save"));
        assert_eq!(params.get("go_first"), Some("yes"));
        assert_eq!(params.summary(), "richard=save go_first=yes");
        for bad in [
            r#"<params format="1"><param name="a" values="x" default="x" label="l"/></params>"#,
            r#"<params format="1"><param name="a" values="x|y" default="z" label="l"/></params>"#,
            r#"<params format="1"><param name="A" values="x|y" default="x" label="l"/></params>"#,
            r#"<params format="1"><param name="a" values="x|x" default="x" label="l"/></params>"#,
            r#"<params format="1"><param name="a" values="x|y" default="x"/></params>"#,
            r#"<params format="1"><param name="a" values="x|y" default="x" label=" "/></params>"#,
            r#"<params format="2"/>"#,
            r#"<params format="1"><param name="a" values="x|y" default="x" label="l"/><param name="a" values="x|y" default="x" label="l"/></params>"#,
            r#"<params format="1"><other/></params>"#,
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn set_checks_names_and_values() {
        let mut params = params();
        params.set("richard", "leave").unwrap();
        assert_eq!(params.get("richard"), Some("leave"));
        assert!(params.set("richrd", "leave").is_err());
        assert!(params.set("richard", "maybe").is_err());
    }

    #[test]
    fn when_holds_for_every_condition() {
        let mut params = params();
        let when = |text| When::parse(text).unwrap();
        assert!(params.holds(&when("richard=save")).unwrap());
        assert!(!params.holds(&when("richard!=save")).unwrap());
        assert!(
            params
                .holds(&when("richard=save|leave go_first=yes"))
                .unwrap()
        );
        params.set("go_first", "no").unwrap();
        assert!(!params.holds(&when("richard=save go_first=yes")).unwrap());
        assert!(params.holds(&when("go_first!=yes")).unwrap());
        // Undeclared names and values are errors, not a silent false.
        assert!(params.holds(&when("richrd=save")).is_err());
        assert!(params.holds(&when("richard=sav")).is_err());
        for bad in [
            "",
            "richard",
            "richard=",
            "=save",
            "Richard=save",
            "richard=save|",
        ] {
            assert!(When::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn assignments_parse() {
        assert_eq!(
            parse_assignment("richard=leave"),
            Ok(("richard".to_string(), "leave".to_string()))
        );
        for bad in ["richard", "richard=", "=x", "Richard=x", "a=b c"] {
            assert!(parse_assignment(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_shipped_parameter_file_parses() {
        let path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot/params/re1.xml");
        let text = std::fs::read_to_string(&path).unwrap();
        parse(&text).unwrap();
    }

    /// Every `when` in the shipped room files names a declared parameter
    /// and value: checked by loading each file with the shipped
    /// declarations, for both characters.
    #[test]
    fn every_shipped_when_names_a_declared_parameter() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot");
        let decls = parse(&std::fs::read_to_string(root.join("params/re1.xml")).unwrap()).unwrap();
        let params = Params::new(decls);
        let mut checked = 0;
        for entry in std::fs::read_dir(root.join("room")).unwrap() {
            let path = entry.unwrap().path().join("bot.xml");
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for jill in [true, false] {
                super::super::meta::parse_for(&text, Some(jill), Some(&params))
                    .unwrap_or_else(|err| panic!("{}: {err:#}", path.display()));
            }
            checked += 1;
        }
        assert!(checked >= 40, "only {checked} room files found");
    }
}
