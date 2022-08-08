//! Define the type of an identifier.
use serde::{Deserialize, Serialize};
use std::{fmt, hash::Hash};

use crate::position::TermPos;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(into = "String", from = "String")]
pub struct Ident {
    pub label: String,
    pub pos: TermPos,
}

/// Special character used for generating fresh identifiers. It must be syntactically impossible to
/// use to write in a standard Nickel program, to avoid name clashes.
pub const GEN_PREFIX: char = '%';

impl PartialOrd for Ident {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.label.partial_cmp(&other.label)
    }
}

impl Ord for Ident {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.label.cmp(&other.label)
    }
}

impl PartialEq for Ident {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label
    }
}

impl Eq for Ident {}

impl Hash for Ident {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.label.hash(state);
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.label)
    }
}

impl<F> From<F> for Ident
where
    String: From<F>,
{
    fn from(val: F) -> Self {
        Ident {
            label: String::from(val),
            pos: TermPos::None,
        }
    }
}

// We can't satisfy clippy and implement `From<Ident> for String`. Otherwise, the generic
// implementation above will give a second way of deriving `From<Ident> for Ident`:
//
// - the identity, provided by core (impl From<T> for T)
// - `String::from::<Ident>` -> `Ident::from::<String>`, given by the implementation above.
//
// And the compiler is unhappy (and the second implementation would silently erase the position).
// Hence, we disable the clippy lint, because being able to write `Ident::from("foo")` is nice.
#[allow(clippy::from_over_into)]
impl Into<String> for Ident {
    fn into(self) -> String {
        self.label
    }
}

impl Ident {
    pub fn is_generated(&self) -> bool {
        self.label.starts_with(GEN_PREFIX)
    }
}

impl AsRef<str> for Ident {
    fn as_ref(&self) -> &str {
        &self.label
    }
}
