//! The plan a `--dry-run` prints, in whichever `--output` format was asked
//! for.
//!
//! Every dry run builds a [`Plan`] — one [`Action`] per thing the real run
//! would do — and hands it to [`Plan::finish`] along with whether the real
//! run would have succeeded. The default format prints each action's
//! one-line `summary`; JSON and TOML print the whole plan, so a script reads
//! the same shape from every command:
//!
//! ```text
//! {
//!   "dry_run": true,
//!   "command": "destroy",
//!   "would_succeed": false,
//!   "actions": [
//!     { "action": "prune", "summary": "would prune worktree api (…)", "worktree": "api", … }
//!   ]
//! }
//! ```
//!
//! A plan the real run would not finish is still printed, up to the action
//! it would stop at, and then the error is returned as usual — stderr and a
//! non-zero exit — so `would_succeed` and the exit code always agree.

use crate::tix::utils::OutputType;
use serde::Serialize;
use tix_sdk::SdkError;

/// Everything a dry run found the real run would do.
#[derive(Debug, Serialize)]
pub struct Plan {
    /// Always `true`: marks the output as a plan rather than a result.
    dry_run: bool,
    /// The command planned, as typed (`destroy`, `config set`, …).
    command: &'static str,
    /// Whether the real run would get through every action and succeed.
    would_succeed: bool,
    /// What the real run would do, in order.
    actions: Vec<Action>,
}

/// One step of a [`Plan`]: what would happen, said once for people
/// (`summary`) and once for scripts (`action` plus its details).
#[derive(Debug, Serialize)]
pub struct Action {
    /// The kind of step, kebab-case (`prune`, `set`, `install`, …).
    action: &'static str,
    /// The line the default output prints.
    summary: String,
    /// What the step acts on: worktree, key, path, value, and so on.
    #[serde(flatten)]
    details: toml::Table,
}

impl Plan {
    /// An empty plan for `command`.
    pub fn new(command: &'static str) -> Self {
        Self {
            dry_run: true,
            command,
            would_succeed: true,
            actions: Vec::new(),
        }
    }

    /// Appends `action`.
    pub fn push(&mut self, action: Action) {
        self.actions.push(action);
    }

    /// Prints the plan in `output`, then returns `outcome`: what the real
    /// run would have returned once it got this far. An `Err` marks the
    /// plan as one that would not succeed.
    pub fn finish(
        mut self,
        output: OutputType,
        outcome: Result<(), SdkError>,
    ) -> Result<(), SdkError> {
        self.would_succeed = outcome.is_ok();
        self.print(output)?;
        outcome
    }

    fn print(&self, output: OutputType) -> Result<(), SdkError> {
        match output {
            OutputType::Default => {
                for action in &self.actions {
                    println!("{}", action.summary);
                }
            }
            OutputType::Json => {
                let json = serde_json::to_string_pretty(self)
                    .map_err(|e| SdkError::Message(format!("json conversion failed: {e}")))?;
                println!("{json}");
            }
            OutputType::Toml => print!("{}", toml::to_string(self)?),
        }
        Ok(())
    }
}

impl Action {
    /// A step of kind `action`, printed as `summary`.
    pub fn new(action: &'static str, summary: impl Into<String>) -> Self {
        Self {
            action,
            summary: summary.into(),
            details: toml::Table::new(),
        }
    }

    /// Adds the detail `name`.
    pub fn with(mut self, name: &str, value: impl Into<toml::Value>) -> Self {
        self.details.insert(name.to_string(), value.into());
        self
    }

    /// Adds the detail `name` when there is one to add.
    pub fn with_some(self, name: &str, value: Option<impl Into<toml::Value>>) -> Self {
        match value {
            Some(value) => self.with(name, value),
            None => self,
        }
    }

    /// Adds the path detail `name`, as the string the default output shows.
    pub fn with_path(self, name: &str, path: &std::path::Path) -> Self {
        self.with(name, path.display().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Plan {
        let mut plan = Plan::new("config set");
        plan.push(
            Action::new("set", "would set defaults.repositories = [\"api\"]")
                .with("key", "defaults.repositories")
                .with("value", toml::Value::Array(vec![toml::Value::from("api")]))
                .with_some("previous", None::<String>)
                .with_path("path", std::path::Path::new("/c/config.toml")),
        );
        plan
    }

    /// Details sit beside `action` and `summary`, absent ones are left out,
    /// and values keep their type.
    #[test]
    fn test_json_shape() {
        let value = serde_json::to_value(sample()).unwrap();
        assert_eq!(value["dry_run"], true);
        assert_eq!(value["command"], "config set");
        let action = &value["actions"][0];
        assert_eq!(action["action"], "set");
        assert_eq!(action["key"], "defaults.repositories");
        assert_eq!(action["value"], serde_json::json!(["api"]));
        assert!(action.get("previous").is_none());
    }

    /// The same plan renders as TOML that parses back to the same data.
    #[test]
    fn test_toml_round_trips() {
        let text = toml::to_string(&sample()).unwrap();
        let parsed: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(parsed["would_succeed"].as_bool(), Some(true));
        let action = &parsed["actions"].as_array().unwrap()[0];
        assert_eq!(action["path"].as_str(), Some("/c/config.toml"));
    }

    /// An error outcome is passed through and marks the plan.
    #[test]
    fn test_finish_reports_failure() {
        let plan = sample();
        let outcome = plan.finish(OutputType::Json, Err(SdkError::Message("no".into())));
        assert!(outcome.is_err());
    }
}
