//! Turns a `clap::Command` tree into a documentation model.
//!
//! The model is the single intermediate every output format renders from, so
//! the man pages and the web reference cannot disagree about what the CLI
//! accepts. Nothing here is hand-maintained: adding a flag to `tix-cli`
//! changes this model on the next run, and `just check-docs` fails until the
//! rendered output is regenerated.

use clap::{Arg, ArgAction, Command};
use std::collections::HashMap;

/// Argument ids clap injects into every command.
///
/// Documented once, at the top of the reference, rather than repeated in the
/// option table of all fourteen leaf commands.
const IMPLICIT_ARGS: [&str; 2] = ["help", "version"];

/// One documented argument — positional or option, rendered the same way.
#[derive(Debug, Clone)]
pub struct ArgDoc {
    /// How the argument is written on the command line, e.g.
    /// `-d, --description <DESCRIPTION>` or `<KEY>...`.
    pub invocation: String,
    /// clap's help text, verbatim.
    pub help: Option<String>,
    /// Whether omitting the argument is an error.
    pub required: bool,
    /// The value used when the argument is absent, if clap declares one.
    pub default: Option<String>,
    /// The closed set of accepted values, for `ValueEnum`-backed arguments.
    pub possible_values: Vec<String>,
}

/// One documented command, and its subcommands.
#[derive(Debug, Clone)]
pub struct CommandDoc {
    /// The full invocation path, e.g. `tix ticket setup`.
    pub path: String,
    /// The leaf name, e.g. `setup`.
    pub name: String,
    /// clap's short description.
    pub about: Option<String>,
    /// clap's long description, when it differs from `about`.
    pub long_about: Option<String>,
    /// The synopsis line, derived rather than taken from `render_usage` so
    /// nested commands print their full path instead of a bare leaf name.
    pub usage: String,
    pub positionals: Vec<ArgDoc>,
    pub options: Vec<ArgDoc>,
    pub subcommands: Vec<CommandDoc>,
    /// Full paths of top-level aliases that dispatch here, e.g. `tix setup`
    /// for `tix ticket setup`. Detected structurally; see [`link_aliases`].
    pub aliases: Vec<String>,
}

impl CommandDoc {
    /// Whether this command dispatches to subcommands rather than doing work.
    pub fn is_group(&self) -> bool {
        !self.subcommands.is_empty()
    }
}

/// Whether a command belongs in generated documentation.
///
/// Excludes hidden commands, and clap's auto-generated `help`. The latter
/// matters more than it looks: `tix help` carries a child for every real
/// command, each with that command's own description, so leaving it in both
/// duplicates the whole tree and makes every top-level command look like an
/// alias of its `tix help <name>` twin.
pub fn is_documented(command: &Command) -> bool {
    !command.is_hide_set() && command.get_name() != "help"
}

/// Walks `command` into a [`CommandDoc`] tree rooted at `tix`.
///
/// `command` must already be built (`Command::build`), because clap fills in
/// implicit arguments and propagates globals at build time — walking an
/// unbuilt tree silently omits both.
pub fn document(command: &Command) -> CommandDoc {
    let mut root = walk(command, None);
    link_aliases(&mut root);
    root
}

/// Recursively documents `command`, whose parent is invoked as `parent_path`.
fn walk(command: &Command, parent_path: Option<&str>) -> CommandDoc {
    let name = command.get_name().to_string();
    let path = match parent_path {
        Some(parent) => format!("{parent} {name}"),
        None => name.clone(),
    };

    // Globals are declared once on the root and propagated by clap into every
    // subcommand; documenting them per-command would repeat five flags
    // fourteen times.
    let is_root = parent_path.is_none();
    let documented: Vec<&Arg> = command
        .get_arguments()
        .filter(|arg| !arg.is_hide_set())
        .filter(|arg| !IMPLICIT_ARGS.contains(&arg.get_id().as_str()))
        .filter(|arg| is_root || !arg.is_global_set())
        .collect();

    let (positionals, options): (Vec<&Arg>, Vec<&Arg>) =
        documented.into_iter().partition(|arg| arg.is_positional());

    let subcommands: Vec<CommandDoc> = command
        .get_subcommands()
        .filter(|sub| is_documented(sub))
        .map(|sub| walk(sub, Some(&path)))
        .collect();

    CommandDoc {
        usage: usage_line(&path, &positionals, &options, !subcommands.is_empty()),
        name,
        about: command.get_about().map(ToString::to_string),
        long_about: command
            .get_long_about()
            .map(ToString::to_string)
            .filter(|long| Some(long) != command.get_about().map(ToString::to_string).as_ref()),
        positionals: positionals.into_iter().map(describe_arg).collect(),
        options: options.into_iter().map(describe_arg).collect(),
        subcommands,
        path,
        aliases: Vec::new(),
    }
}

/// Builds the synopsis, e.g. `tix ticket setup [OPTIONS] <KEY> [REPO_ALIASES]...`.
fn usage_line(path: &str, positionals: &[&Arg], options: &[&Arg], has_subcommands: bool) -> String {
    let mut parts = vec![path.to_string()];
    if !options.is_empty() {
        parts.push("[OPTIONS]".to_string());
    }
    for arg in positionals {
        parts.push(positional_form(arg));
    }
    if has_subcommands {
        parts.push("<COMMAND>".to_string());
    }
    parts.join(" ")
}

/// `<KEY>`, `[REPO_ALIASES]...` — angle brackets when required, square when
/// optional, an ellipsis when the argument accepts more than one value.
fn positional_form(arg: &Arg) -> String {
    let name = value_name(arg);
    let bracketed = if arg.is_required_set() {
        format!("<{name}>")
    } else {
        format!("[{name}]")
    };
    format!("{bracketed}{}", if accepts_many(arg) { "..." } else { "" })
}

/// The placeholder clap prints for an argument's value.
fn value_name(arg: &Arg) -> String {
    arg.get_value_names()
        .and_then(|names| names.first())
        .map(ToString::to_string)
        .unwrap_or_else(|| arg.get_id().as_str().to_uppercase())
}

/// Whether the argument may be given more than once.
///
/// `Vec<T>` fields become `ArgAction::Append`; `num_args` stays at 1 for
/// them, so the action is what distinguishes a list from a scalar.
fn accepts_many(arg: &Arg) -> bool {
    matches!(arg.get_action(), ArgAction::Append)
}

/// Whether the argument takes a value at all (as opposed to being a flag).
fn takes_value(arg: &Arg) -> bool {
    !matches!(
        arg.get_action(),
        ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count
    )
}

/// Renders one argument into its documented form.
fn describe_arg(arg: &Arg) -> ArgDoc {
    ArgDoc {
        invocation: if arg.is_positional() {
            positional_form(arg)
        } else {
            option_form(arg)
        },
        help: arg
            .get_long_help()
            .or_else(|| arg.get_help())
            .map(ToString::to_string),
        required: arg.is_required_set(),
        // Flags always default to "false", which says nothing a reader did
        // not already know from it being a flag.
        default: arg
            .get_default_values()
            .first()
            .filter(|_| takes_value(arg))
            .map(|value| value.to_string_lossy().into_owned()),
        possible_values: arg
            .get_possible_values()
            .iter()
            .filter(|value| !value.is_hide_set())
            .map(|value| value.get_name().to_string())
            .collect(),
    }
}

/// `-d, --description <DESCRIPTION>` — short and long spellings, then the
/// value placeholder for anything that is not a bare flag.
fn option_form(arg: &Arg) -> String {
    let mut spellings = Vec::new();
    if let Some(short) = arg.get_short() {
        spellings.push(format!("-{short}"));
    }
    if let Some(long) = arg.get_long() {
        spellings.push(format!("--{long}"));
    }
    let spelled = spellings.join(", ");
    if !takes_value(arg) {
        return spelled;
    }
    let name = value_name(arg);
    let repeat = if accepts_many(arg) { "..." } else { "" };
    format!("{spelled} <{name}>{repeat}")
}

/// Records the top-level aliases against the commands they dispatch to, and
/// drops them from the root's own subcommand list.
///
/// `tix setup` and `tix ticket setup` are two clap subcommands built from one
/// `Args` type, so a naive walk documents each twice. Rather than hardcode
/// the alias list — which would drift the moment one is added — a root-level
/// leaf is treated as an alias when a command of the same name and
/// description exists deeper in the tree.
fn link_aliases(root: &mut CommandDoc) {
    let nested: HashMap<String, String> = root
        .subcommands
        .iter()
        .filter(|sub| sub.is_group())
        .flat_map(|group| group.subcommands.iter())
        .map(|leaf| (alias_key(&leaf.name, &leaf.about), leaf.path.clone()))
        .collect();

    let mut resolved: Vec<(String, String)> = Vec::new();
    root.subcommands.retain(|sub| {
        let Some(target) = nested.get(&alias_key(&sub.name, &sub.about)) else {
            return true;
        };
        resolved.push((target.clone(), sub.path.clone()));
        false
    });

    for sub in &mut root.subcommands {
        attach_aliases(sub, &resolved);
    }
}

/// Records every alias in `resolved` against the command it dispatches to.
fn attach_aliases(command: &mut CommandDoc, resolved: &[(String, String)]) {
    for (target, alias) in resolved {
        if &command.path == target {
            command.aliases.push(alias.clone());
        }
    }
    for sub in &mut command.subcommands {
        attach_aliases(sub, resolved);
    }
}

/// Identity of a command for alias matching: two commands built from the same
/// `Args` type share both a name and a description.
fn alias_key(name: &str, about: &Option<String>) -> String {
    format!("{name}\u{0}{}", about.as_deref().unwrap_or_default())
}
