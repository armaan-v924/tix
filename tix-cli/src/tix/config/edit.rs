//! The mechanics `tix config add`, `remove`, and `unset` share: resolving
//! what a path addresses *without* creating it, and editing an array
//! without reformatting the one already in the file.
//!
//! The formatting work is the substance here. `toml_edit`'s own `push` and
//! `remove` are layout-agnostic — a push onto a multi-line array lands the
//! new element on the previous element's line, and a removal from the front
//! leaves the gap it made behind. Either one turns a `tix config` write into
//! a reformat of the user's file, which is the one thing the
//! format-preserving layer exists to prevent.

use crate::tix::config::ConfigKeyPath;
use tix_sdk::SdkError;
use tix_sdk::document::TixDocument;
use toml_edit::{Array, Item, TableLike, Value};

/// Parses a command-line value the way `tix config set` does: TOML when it
/// parses as TOML, a string otherwise.
pub fn parse_value(text: &str) -> Value {
    text.parse()
        .unwrap_or_else(|_| toml_edit::Value::from(text))
}

/// The array `key` addresses, created empty — along with any table on the
/// way to it — when the key is unset.
///
/// # Errors
///
/// [`SdkError::Message`] when `key` names something that is not a list, or
/// when a table on the path is occupied by a non-table.
pub fn array_for_add<'a>(
    document: &'a mut TixDocument,
    key: &ConfigKeyPath,
) -> Result<&'a mut Array, SdkError> {
    let table = document.table_at(&key.table_path())?;
    table
        .entry(key.leaf())
        .or_insert(toml_edit::value(Array::new()))
        .as_array_mut()
        .ok_or_else(|| not_a_list(key))
}

/// The array `key` addresses, as it already exists — nothing is created.
///
/// # Errors
///
/// [`SdkError::Message`] when the key is unset or is not a list. A removal
/// from a list that isn't there is a mistake worth hearing about, not a
/// silent no-op.
pub fn existing_array<'a>(
    document: &'a mut TixDocument,
    key: &ConfigKeyPath,
) -> Result<&'a mut Array, SdkError> {
    holding_table(document, key)?
        .get_mut(key.leaf())
        .ok_or_else(|| unset(key))?
        .as_array_mut()
        .ok_or_else(|| not_a_list(key))
}

/// The table holding `key`, creating no level of the path.
///
/// [`tix_sdk::document::TixDocument::table_at`] is the write-path
/// counterpart, and materializes what is missing; every command that edits
/// something already in the document wants this one instead, so a failed
/// edit cannot leave empty tables behind.
///
/// # Errors
///
/// [`SdkError::Message`] when a segment of the path is absent, or holds a
/// value rather than a table.
pub fn holding_table<'a>(
    document: &'a mut TixDocument,
    key: &ConfigKeyPath,
) -> Result<&'a mut dyn TableLike, SdkError> {
    let mut item: &mut Item = document.doc_mut().as_item_mut();
    for segment in key.table_path() {
        item = item.get_mut(segment).ok_or_else(|| unset(key))?;
    }
    item.as_table_like_mut().ok_or_else(|| {
        SdkError::Message(format!("'{key}' is not inside a table — nothing to edit"))
    })
}

/// The index of the first element equal in value to `value`.
pub fn position(array: &Array, value: &Value) -> Option<usize> {
    array.iter().position(|element| equivalent(element, value))
}

/// Appends `value`, matching the layout of what is already there: one
/// element per line in a multi-line array, inline in an inline one.
pub fn push_in_layout(array: &mut Array, mut value: Value) {
    let prefix = appended_prefix(array);
    let decor = value.decor_mut();
    decor.set_prefix(prefix);
    decor.set_suffix("");
    array.push_formatted(value);
}

/// Removes the element at `index`, leaving the array's layout — and every
/// comment in it — intact.
///
/// A comment attaches to whatever comes *after* it, so one written at the
/// end of an element's line is stored in the prefix of the element below.
/// Removing an element therefore threatens a comment about the line above
/// it, which is why the prefix is taken apart rather than dropped: the
/// comment half moves to whatever now occupies the slot, and only the
/// whitespace half goes with the element.
pub fn remove_in_layout(array: &mut Array, index: usize) {
    let vacated = prefix_at(array, index);
    array.remove(index);
    let carried = comment_in(&vacated);

    if index < array.len() {
        let occupant = prefix_at(array, index);
        let kept = match index {
            // The first element's whitespace prefix is a property of the
            // position — it is whatever follows `[` — so the element that
            // moved up inherits it. A comment of its own is its own: it is
            // re-laid on a line of its own, where it reads as the note it
            // was rather than as an annotation on the opening bracket.
            0 => match comment_in(&occupant) {
                comment if comment.is_empty() => whitespace_in(&vacated),
                comment => format!(
                    "{}{}{}",
                    whitespace_in(&vacated),
                    comment.trim_start(),
                    whitespace_in(&occupant)
                ),
            },
            _ => occupant,
        };
        // A carried comment ends the line it was written on, so the
        // occupant's own leading newline would open a blank one.
        let kept = match carried.is_empty() {
            true => kept,
            false => kept.strip_prefix('\n').unwrap_or(&kept).to_string(),
        };
        if let Some(element) = array.get_mut(index) {
            element.decor_mut().set_prefix(format!("{carried}{kept}"));
        }
        return;
    }

    // Nothing follows the removed element, so a comment it carried belongs
    // to the closing bracket's decor now.
    if carried.is_empty() {
        // An array emptied of its last element closes up: `[]`, not a
        // bracket pair around the blank line the elements used to occupy.
        if array.is_empty() {
            array.set_trailing("");
        }
        return;
    }
    let trailing = trailing_of(array);
    let kept = match trailing.contains('#') {
        true => trailing,
        false => tail_of(&trailing).to_string(),
    };
    array.set_trailing(format!("{carried}{kept}"));
}

/// The decor prefix a newly appended element should carry.
///
/// Also relocates a comment trailing the last element: it sits between that
/// element's comma and the closing bracket, which is precisely where the new
/// element's prefix begins, so carrying it across keeps it on the line it
/// was written on instead of sliding down to the appended value.
fn appended_prefix(array: &mut Array) -> String {
    let Some(last) = array.len().checked_sub(1) else {
        return String::new(); // first element of an empty array: `["a"]`
    };
    let previous = prefix_at(array, last);

    // Only a multi-line array indents; an inline one takes the conventional
    // single space after the comma.
    let Some((_, indent)) = previous.rsplit_once('\n') else {
        return " ".to_string();
    };
    let indent = indent.to_string();

    let trailing = trailing_of(array);
    if !trailing.contains('#') {
        return format!("\n{indent}");
    }
    array.set_trailing(format!("\n{}", tail_of(&trailing)));
    format!("{trailing}{indent}")
}

/// The comment half of a decor prefix: everything up to and including the
/// newline that ends the last commented line, or nothing when the prefix is
/// pure whitespace. Concatenated with [`whitespace_in`] it is the prefix
/// again.
fn comment_in(prefix: &str) -> String {
    match prefix.contains('#') {
        true => prefix[..prefix.len() - tail_of(prefix).len()].to_string(),
        false => String::new(),
    }
}

/// The whitespace half of a decor prefix: what a comment in it leaves
/// behind, which is the indentation of the line the value sits on.
fn whitespace_in(prefix: &str) -> String {
    match prefix.contains('#') {
        true => tail_of(prefix).to_string(),
        false => prefix.to_string(),
    }
}

/// Whatever follows the last newline — the indentation of the final line.
fn tail_of(text: &str) -> &str {
    text.rsplit_once('\n').map_or(text, |(_, tail)| tail)
}

/// The array's trailing decor: the text between the last element's comma
/// and the closing bracket.
fn trailing_of(array: &Array) -> String {
    array.trailing().as_str().unwrap_or_default().to_string()
}

/// The decor prefix of the element at `index`, or the empty string.
fn prefix_at(array: &Array, index: usize) -> String {
    array
        .get(index)
        .and_then(|element| element.decor().prefix())
        .and_then(|prefix| prefix.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Whether two values mean the same thing, whatever they look like in the
/// file: `'backend'` and `"backend"` are one string, `+1` and `1` one
/// integer. Matching on the written form would make removal depend on how
/// the file happens to be typed.
fn equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::String(a), Value::String(b)) => a.value() == b.value(),
        (Value::Integer(a), Value::Integer(b)) => a.value() == b.value(),
        (Value::Float(a), Value::Float(b)) => a.value() == b.value(),
        (Value::Boolean(a), Value::Boolean(b)) => a.value() == b.value(),
        // Datetimes, and the nested arrays and inline tables a config could
        // hold: no cheap semantic comparison, so compare the literals with
        // their decor stripped.
        (a, b) => literal(a) == literal(b),
    }
}

/// A value's own text, without the whitespace and comments around it.
pub fn literal(value: &Value) -> String {
    let mut value = value.clone();
    let decor = value.decor_mut();
    decor.set_prefix("");
    decor.set_suffix("");
    value.to_string()
}

/// The array as it now reads, for the line a command prints back.
pub fn render(array: &Array) -> String {
    let mut array = array.clone();
    array.fmt();
    array.to_string().trim().to_string()
}

/// "That key holds nothing yet."
fn unset(key: &ConfigKeyPath) -> SdkError {
    SdkError::Message(format!("'{key}' is not set"))
}

/// "That key is not a list." Names `unset` because reaching for `remove` on
/// a scalar is nearly always an attempt to clear it.
fn not_a_list(key: &ConfigKeyPath) -> SdkError {
    SdkError::Message(format!(
        "'{key}' is not a list — `tix config add`/`remove` work on list \
         elements; use `tix config unset {key}` to remove the key itself"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `edit` against the array in `src` and returns the rewritten
    /// document, so each test asserts on the file the user would be left
    /// with rather than on decor internals.
    fn rewrite(src: &str, edit: impl Fn(&mut Array)) -> String {
        let mut document: toml_edit::DocumentMut = src.parse().unwrap();
        edit(document["defaults"]["repositories"].as_array_mut().unwrap());
        document.to_string()
    }

    const MULTI_LINE: &str = "\
[defaults]
repositories = [
    \"backend\",
    \"frontend\",
]
";

    /// An append to a multi-line array gets its own line, at the indent the
    /// existing elements use.
    #[test]
    fn test_push_keeps_one_element_per_line() {
        let out = rewrite(MULTI_LINE, |array| {
            push_in_layout(array, parse_value("infra"))
        });
        assert_eq!(
            out,
            "\
[defaults]
repositories = [
    \"backend\",
    \"frontend\",
    \"infra\",
]
"
        );
    }

    /// An append to an inline array stays inline.
    #[test]
    fn test_push_keeps_inline_arrays_inline() {
        let out = rewrite(
            "[defaults]\nrepositories = [\"backend\", \"frontend\"]\n",
            |array| push_in_layout(array, parse_value("infra")),
        );
        assert_eq!(
            out,
            "[defaults]\nrepositories = [\"backend\", \"frontend\", \"infra\"]\n"
        );
    }

    /// The first element of an array created by the append needs no
    /// separator before it.
    #[test]
    fn test_push_onto_empty_array() {
        let out = rewrite("[defaults]\nrepositories = []\n", |array| {
            push_in_layout(array, parse_value("infra"))
        });
        assert_eq!(out, "[defaults]\nrepositories = [\"infra\"]\n");
    }

    /// A comment trailing the last element stays on that element's line
    /// instead of sliding onto the appended one.
    #[test]
    fn test_push_leaves_a_trailing_comment_where_it_was() {
        let out = rewrite(
            "[defaults]\nrepositories = [\n    \"backend\",   # the api\n]\n",
            |array| push_in_layout(array, parse_value("infra")),
        );
        assert_eq!(
            out,
            "\
[defaults]
repositories = [
    \"backend\",   # the api
    \"infra\",
]
"
        );
    }

    /// Removing the first element leaves the survivor where the removed one
    /// stood, not indented by the gap it left behind.
    #[test]
    fn test_remove_first_element_closes_the_gap() {
        let out = rewrite(
            "[defaults]\nrepositories = [\"backend\", \"frontend\"]\n",
            |array| remove_in_layout(array, 0),
        );
        assert_eq!(out, "[defaults]\nrepositories = [\"frontend\"]\n");
    }

    /// A multi-line array stays multi-line through a removal.
    #[test]
    fn test_remove_keeps_multi_line_layout() {
        let out = rewrite(MULTI_LINE, |array| remove_in_layout(array, 0));
        assert_eq!(
            out,
            "\
[defaults]
repositories = [
    \"frontend\",
]
"
        );
    }

    /// A comment the surviving first element carries is the user's text and
    /// outlives the element removed from in front of it.
    #[test]
    fn test_remove_keeps_a_survivors_comment() {
        let out = rewrite(
            "[defaults]\nrepositories = [\n    \"backend\",\n    # the ui\n    \"frontend\",\n]\n",
            |array| remove_in_layout(array, 0),
        );
        assert_eq!(
            out,
            "\
[defaults]
repositories = [
    # the ui
    \"frontend\",
]
"
        );
    }

    /// Removing the first element promotes a comment that trailed it onto a
    /// line of its own, rather than leaving it hanging off the bracket.
    #[test]
    fn test_remove_first_relays_a_trailing_comment() {
        let out = rewrite(
            "[defaults]\nrepositories = [\n    \"backend\",   # the api\n    \"frontend\",\n]\n",
            |array| remove_in_layout(array, 0),
        );
        assert_eq!(
            out,
            "\
[defaults]
repositories = [
    # the api
    \"frontend\",
]
"
        );
    }

    /// A comment at the end of an element's line is stored in the *next*
    /// element's prefix, so removing that next element must not take the
    /// comment with it.
    #[test]
    fn test_remove_keeps_the_comment_of_the_line_above() {
        let out = rewrite(
            "[defaults]\nrepositories = [\n    \"backend\",   # the api\n    \"frontend\",\n]\n",
            |array| remove_in_layout(array, 1),
        );
        assert_eq!(
            out,
            "\
[defaults]
repositories = [
    \"backend\",   # the api
]
"
        );
    }

    /// The same, one element further in: the comment lands in front of
    /// whatever moved up into the slot.
    #[test]
    fn test_remove_moves_a_carried_comment_to_the_next_element() {
        let out = rewrite(
            "[defaults]\nrepositories = [\n    \"backend\",   # the api\n    \"frontend\",\n    \"infra\",\n]\n",
            |array| remove_in_layout(array, 1),
        );
        assert_eq!(
            out,
            "\
[defaults]
repositories = [
    \"backend\",   # the api
    \"infra\",
]
"
        );
    }

    /// An array emptied of its last element closes up.
    #[test]
    fn test_remove_last_element_collapses_the_array() {
        let out = rewrite(MULTI_LINE, |array| {
            remove_in_layout(array, 0);
            remove_in_layout(array, 0);
        });
        assert_eq!(out, "[defaults]\nrepositories = []\n");
    }

    /// Elements match on value, not on how they are quoted.
    #[test]
    fn test_position_matches_by_value() {
        let document: toml_edit::DocumentMut =
            "[defaults]\nrepositories = ['backend', \"frontend\"]\n"
                .parse()
                .unwrap();
        let array = document["defaults"]["repositories"].as_array().unwrap();
        assert_eq!(position(array, &parse_value("backend")), Some(0));
        assert_eq!(position(array, &parse_value("frontend")), Some(1));
        assert_eq!(position(array, &parse_value("infra")), None);
    }

    /// A duplicate resolves to the first of its occurrences, so repeated
    /// removals undo repeated appends in order.
    #[test]
    fn test_position_finds_the_first_duplicate() {
        let document: toml_edit::DocumentMut = "[defaults]\nrepositories = [\"a\", \"b\", \"a\"]\n"
            .parse()
            .unwrap();
        let array = document["defaults"]["repositories"].as_array().unwrap();
        assert_eq!(position(array, &parse_value("a")), Some(0));
    }
}
