//! List all ticket workspaces.

use crate::core::config::Config;
use crate::core::ticket::Ticket;
use anyhow::{Context, Result};
use log::warn;
use std::fs;
use std::path::Path;

/// Run the list command.
pub fn run() -> Result<()> {
    let config = Config::load()?;
    
    // Check if tickets directory exists
    if !config.tickets_directory.exists() {
        warn!("Tickets directory does not exist: {:?}", config.tickets_directory);
        println!("No tickets found.");
        return Ok(());
    }

    // Collect all ticket directories
    let mut tickets = Vec::new();
    
    let entries = fs::read_dir(&config.tickets_directory)
        .context("Failed to read tickets directory")?;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            // Try to load ticket metadata
            match Ticket::load(&path) {
                Ok(ticket) => {
                    tickets.push((path, ticket.metadata));
                }
                Err(_) => {
                    // Skip directories that don't contain valid ticket metadata
                    continue;
                }
            }
        }
    }

    if tickets.is_empty() {
        println!("No tickets found.");
        return Ok(());
    }

    // Sort by ticket ID
    tickets.sort_by(|a, b| a.1.id.cmp(&b.1.id));

    // Display table header
    println!("{:<20} {:<40} {:<40} {}", 
        "TICKET", "DESCRIPTION", "PATH", "JIRA LINK");
    println!("{}", "-".repeat(140));

    // Display each ticket
    for (path, metadata) in tickets {
        let ticket_id = &metadata.id;
        let description = metadata.description.as_deref().unwrap_or("");
        let display_path = format_path_with_home(&path);
        let jira_link = format_jira_link(&config, ticket_id);

        println!("{:<20} {:<40} {:<40} {}", 
            ticket_id,
            truncate(description, 40),
            truncate(&display_path, 40),
            jira_link);
    }

    Ok(())
}

/// Replace the home directory prefix with ~ for display.
fn format_path_with_home(path: &Path) -> String {
    if let Some(home) = home::home_dir()
        && let Ok(stripped) = path.strip_prefix(&home) {
        return format!("~/{}", stripped.display());
    }
    path.display().to_string()
}

/// Format a Jira link if jira_base_url is configured.
fn format_jira_link(config: &Config, ticket_id: &str) -> String {
    match &config.jira_base_url {
        Some(base_url) => {
            let base = base_url.trim_end_matches('/');
            format!("{}/{}", base, ticket_id)
        }
        None => String::new(),
    }
}

/// Truncate a string to a maximum length, adding "..." if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    
    let char_count = s.chars().count();
    if char_count <= max_len {
        s.to_string()
    } else if max_len > 3 {
        let truncated: String = s.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    } else {
        s.chars().take(max_len).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn truncate_leaves_short_strings() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_shortens_long_strings() {
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn truncate_handles_zero_length() {
        assert_eq!(truncate("hello", 0), "");
    }

    #[test]
    fn truncate_handles_utf8_characters() {
        assert_eq!(truncate("こんにちは世界", 5), "こん...");
        assert_eq!(truncate("🎉🎊🎈", 2), "🎉🎊");
    }

    #[test]
    fn truncate_handles_small_max_len() {
        assert_eq!(truncate("hello", 3), "hel");
        assert_eq!(truncate("hello", 2), "he");
        assert_eq!(truncate("hello", 1), "h");
    }

    #[test]
    fn format_jira_link_returns_empty_when_not_configured() {
        let config = Config {
            jira_base_url: None,
            ..Default::default()
        };
        assert_eq!(format_jira_link(&config, "JIRA-123"), "");
    }

    #[test]
    fn format_jira_link_constructs_url_when_configured() {
        let config = Config {
            jira_base_url: Some("https://company.atlassian.net/browse".to_string()),
            ..Default::default()
        };
        assert_eq!(
            format_jira_link(&config, "JIRA-123"),
            "https://company.atlassian.net/browse/JIRA-123"
        );
    }

    #[test]
    fn format_jira_link_handles_trailing_slash() {
        let config = Config {
            jira_base_url: Some("https://company.atlassian.net/browse/".to_string()),
            ..Default::default()
        };
        assert_eq!(
            format_jira_link(&config, "JIRA-123"),
            "https://company.atlassian.net/browse/JIRA-123"
        );
    }

    #[test]
    fn format_path_with_home_uses_tilde() {
        if let Some(home) = home::home_dir() {
            let test_path = home.join("tickets/JIRA-123");
            let formatted = format_path_with_home(&test_path);
            assert!(formatted.starts_with("~/"));
        }
    }

    #[test]
    fn format_path_with_home_passthrough_non_home_paths() {
        let test_path = PathBuf::from("/tmp/tickets/JIRA-123");
        let formatted = format_path_with_home(&test_path);
        assert_eq!(formatted, "/tmp/tickets/JIRA-123");
    }
}
