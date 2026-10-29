//! The values bindings carry and the store screens read them from.
//!
//! The store is one string-keyed namespace shared by every screen on the
//! stack: `settings.*`, `saves.*`, `debug.*` and a Lua screen's own keys
//! are prefixes, not separate objects. A key may hold a [`Value::Map`] or
//! [`Value::List`], and a path reaches inside it (`saves.rows.3.tag`), so a
//! host can publish a whole table under one key and a binding can still
//! name one field of it.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::gui::canvas::Picture;

/// A picture with an optional grey twin, drawn while its row is not
/// selected.
#[derive(Debug, Clone)]
pub struct ImageValue {
    pub colour: Arc<Picture>,
    pub grey: Option<Arc<Picture>>,
}

/// A bound value.
#[derive(Debug, Clone, Default)]
pub enum Value {
    #[default]
    Nil,
    Bool(bool),
    Int(i64),
    Str(Arc<str>),
    Image(ImageValue),
    List(Arc<Vec<Value>>),
    Map(Arc<BTreeMap<String, Value>>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Image(a), Value::Image(b)) => {
                Arc::ptr_eq(&a.colour, &b.colour)
                    && match (&a.grey, &b.grey) {
                        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                        (None, None) => true,
                        _ => false,
                    }
            }
            (Value::List(a), Value::List(b)) => Arc::ptr_eq(a, b) || a == b,
            (Value::Map(a), Value::Map(b)) => Arc::ptr_eq(a, b) || a == b,
            _ => false,
        }
    }
}

impl Value {
    pub fn str(text: impl AsRef<str>) -> Self {
        Value::Str(Arc::from(text.as_ref()))
    }

    pub fn list(items: Vec<Value>) -> Self {
        Value::List(Arc::new(items))
    }

    /// A map from `(key, value)` pairs.
    pub fn map<K: Into<String>>(fields: impl IntoIterator<Item = (K, Value)>) -> Self {
        Value::Map(Arc::new(
            fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        ))
    }

    /// Whether the value counts as set: not nil, false, zero, empty text or
    /// an empty list.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(on) => *on,
            Value::Int(n) => *n != 0,
            Value::Str(s) => !s.is_empty(),
            Value::Image(_) => true,
            Value::List(items) => !items.is_empty(),
            Value::Map(_) => true,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            Value::Bool(on) => Some(i64::from(*on)),
            Value::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn as_list(&self) -> &[Value] {
        match self {
            Value::List(items) => items,
            _ => &[],
        }
    }

    /// One field of a map, or one element of a list by its index.
    pub fn field(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(fields) => fields.get(key),
            Value::List(items) => key.parse::<usize>().ok().and_then(|i| items.get(i)),
            _ => None,
        }
    }

    /// The value at `path` (dot-separated) inside this one.
    pub fn at(&self, path: &str) -> Option<&Value> {
        if path.is_empty() {
            return Some(self);
        }
        path.split('.')
            .try_fold(self, |value, key| value.field(key))
    }

    /// Replace the value at `path` inside this one, creating maps on the
    /// way; returns whether anything changed.
    fn set_at(&mut self, path: &str, value: Value) -> bool {
        let (head, rest) = match path.split_once('.') {
            Some((head, rest)) => (head, Some(rest)),
            None => (path, None),
        };
        match self {
            Value::List(items) => {
                let Ok(index) = head.parse::<usize>() else {
                    return false;
                };
                if index >= items.len() {
                    return false;
                }
                let items = Arc::make_mut(items);
                match rest {
                    Some(rest) => items[index].set_at(rest, value),
                    None if items[index] != value => {
                        items[index] = value;
                        true
                    }
                    None => false,
                }
            }
            Value::Map(_) | Value::Nil => {
                if matches!(self, Value::Nil) {
                    *self = Value::Map(Arc::default());
                }
                let Value::Map(fields) = self else {
                    unreachable!()
                };
                if rest.is_none() && fields.get(head) == Some(&value) {
                    return false;
                }
                let fields = Arc::make_mut(fields);
                let slot = fields.entry(head.to_string()).or_default();
                match rest {
                    Some(rest) => slot.set_at(rest, value),
                    None => {
                        *slot = value;
                        true
                    }
                }
            }
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil | Value::Image(_) | Value::List(_) | Value::Map(_) => Ok(()),
            Value::Bool(on) => write!(f, "{on}"),
            Value::Int(n) => write!(f, "{n}"),
            Value::Str(s) => f.write_str(s),
        }
    }
}

impl From<bool> for Value {
    fn from(on: bool) -> Self {
        Value::Bool(on)
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Int(n)
    }
}

impl From<i32> for Value {
    fn from(n: i32) -> Self {
        Value::Int(n.into())
    }
}

impl From<usize> for Value {
    fn from(n: usize) -> Self {
        Value::Int(n as i64)
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Self {
        Value::str(text)
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Value::str(text)
    }
}

/// The shared key-value store bindings read and widgets write.
#[derive(Debug, Default)]
pub struct Store {
    values: BTreeMap<String, Value>,
    revision: u64,
}

impl Store {
    /// The value at `path`: the longest stored key the path starts with,
    /// then the rest of the path inside it.
    pub fn get(&self, path: &str) -> Value {
        if let Some(value) = self.values.get(path) {
            return value.clone();
        }
        let mut end = path.len();
        while let Some(dot) = path[..end].rfind('.') {
            if let Some(value) = self.values.get(&path[..dot]) {
                return value.at(&path[dot + 1..]).cloned().unwrap_or_default();
            }
            end = dot;
        }
        Value::Nil
    }

    /// Store `value` at `path` (inside a stored table when a stored key is a
    /// prefix of it); returns whether anything changed.
    pub fn set(&mut self, path: &str, value: Value) -> bool {
        let mut end = path.len();
        while let Some(dot) = path[..end].rfind('.') {
            if let Some(table) = self.values.get_mut(&path[..dot]) {
                let changed = table.set_at(&path[dot + 1..], value);
                self.revision += u64::from(changed);
                return changed;
            }
            end = dot;
        }
        if self.values.get(path) == Some(&value) {
            return false;
        }
        self.values.insert(path.to_string(), value);
        self.revision += 1;
        true
    }

    /// Bumps whenever a value changes.
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_reach_inside_stored_tables() {
        let mut store = Store::default();
        store.set(
            "saves.rows",
            Value::list(vec![Value::map([("tag", Value::from("01"))])]),
        );
        assert_eq!(store.get("saves.rows.0.tag"), Value::from("01"));
        assert_eq!(store.get("saves.rows.1.tag"), Value::Nil);
        let revision = store.revision();
        assert!(store.set("saves.rows.0.tag", Value::from("02")));
        assert!(!store.set("saves.rows.0.tag", Value::from("02")));
        assert_eq!(store.revision(), revision + 1);
        assert_eq!(store.get("saves.rows.0.tag"), Value::from("02"));
    }

    #[test]
    fn truthiness() {
        assert!(!Value::Nil.truthy());
        assert!(!Value::from("").truthy());
        assert!(!Value::Int(0).truthy());
        assert!(Value::from("x").truthy());
        assert!(!Value::list(Vec::new()).truthy());
    }
}
