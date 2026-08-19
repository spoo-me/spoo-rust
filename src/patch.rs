//! Tri-state PATCH semantics.

use serde::{Serialize, Serializer};

/// The three things a PATCH field can say: leave the stored value alone,
/// clear it, or replace it.
///
/// The update endpoint gives `null` a per-field meaning (clear the password,
/// remove the expiry, move back to the default domain), so "send null" and
/// "send nothing" must stay distinguishable. `Option<T>` cannot carry that
/// distinction; this type can. Update builders use it internally and expose
/// it through paired methods (`password(...)` / `remove_password()`), so most
/// callers never name it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Patch<T> {
    /// Do not send the field: the stored value stays as it is.
    #[default]
    Keep,
    /// Send an explicit `null`: clear or reset the field.
    Null,
    /// Send a new value.
    Set(T),
}

impl<T> Patch<T> {
    /// Whether this field should be omitted from the wire entirely.
    pub fn is_keep(&self) -> bool {
        matches!(self, Patch::Keep)
    }
}

impl<T> From<T> for Patch<T> {
    fn from(value: T) -> Self {
        Patch::Set(value)
    }
}

impl<T: Serialize> Serialize for Patch<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            // Keep is skipped at the struct level via
            // `skip_serializing_if = "Patch::is_keep"`; serializing it
            // anyway degrades to null rather than failing.
            Patch::Keep | Patch::Null => serializer.serialize_none(),
            Patch::Set(value) => value.serialize(serializer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Body {
        #[serde(skip_serializing_if = "Patch::is_keep")]
        password: Patch<String>,
        #[serde(skip_serializing_if = "Patch::is_keep")]
        max_clicks: Patch<u32>,
        #[serde(skip_serializing_if = "Patch::is_keep")]
        alias: Patch<String>,
    }

    #[test]
    fn wire_bytes_distinguish_all_three_states() {
        let body = Body {
            password: Patch::Null,
            max_clicks: Patch::Set(10),
            alias: Patch::Keep,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert_eq!(json, r#"{"password":null,"max_clicks":10}"#);
    }
}
