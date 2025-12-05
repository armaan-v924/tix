# Copilot Instructions for tix

## Project Overview

**tix** is a Rust CLI tool for managing ticket-scoped git worktrees across multiple repositories. It helps developers maintain isolated contexts for different tasks (e.g., Jira tickets) without disrupting main development branches.

### Key Features
- Create ticket workspaces with isolated worktrees for multiple repos
- Automatic branch naming with sanitization (`<prefix>/<ticket>-<description>`)
- Per-ticket metadata tracking (`.tix/info.toml`)
- Safety checks to prevent data loss (clean working tree validation)
- Repository management (add, clone, configure)

## Architecture & Technology Stack

### Core Technologies
- **Language**: Rust (2024 edition)
- **CLI Framework**: clap (with derive macros)
- **Git Operations**: git2 (libgit2 bindings) - no shell commands
- **Configuration**: TOML via serde
- **Logging**: env_logger + log crate
- **Interactive Prompts**: dialoguer

### Key Modules
- `src/core/config.rs`: Configuration management (`Config` struct with TOML serialization)
- `src/core/git.rs`: Git operations (worktree creation, branch management, cleanup)
- `src/commands/`: Command implementations (setup, add, remove, destroy, etc.)

## Development Guidelines

### Code Style & Conventions

1. **Logging**: Always use `log` macros (`info!`, `warn!`, `error!`, `debug!`), never `println!` for status updates
2. **Error Handling**: Use `anyhow::Result` for error propagation with context
3. **Git Operations**: Always use `git2` crate, never shell out to git commands
4. **Path Handling**: Use `std::path::PathBuf` and handle cross-platform paths correctly
5. **Configuration**: All user-facing config in `~/.config/tix/config.toml` (or `XDG_CONFIG_HOME`)

### Branch Naming & Sanitization
- Format: `<branch_prefix>/<ticket>-<sanitized-description>`
- Sanitization rules: lowercase, alphanumeric only, single hyphens, trim trailing hyphens

### Metadata Management
Each ticket workspace contains `.tix/info.toml` with:
```toml
id = "JIRA-123"
description = "Feature description"
created_at = "2024-01-01T12:00:00Z"
branch = "feature/JIRA-123-feature-description"
repos = ["api", "web"]
repo_branches = { api = "feature/JIRA-123-feature-description", web = "feature/JIRA-123-feature-description" }
repo_worktrees = { api = "api", web = "web" }
```

### Safety Patterns
- **Clean Check**: Always verify working tree is clean before destructive operations
- **Stored Metadata**: Prefer stored branch/worktree names from metadata; warn on fallback to computed values
- **Location Validation**: Ensure user isn't inside a ticket directory before destroying it
- **Force Flag**: Only `destroy` command has `--force` to bypass safety checks

## Build, Test & Lint

### Building
```bash
cargo build          # Debug build
cargo build --release # Release build
```

### Testing
```bash
cargo test           # Run all tests (unit + integration)
cargo test --lib     # Unit tests only
cargo test --test cli_integration  # Specific integration test
```

**Testing Approach**:
- Unit tests embedded in modules
- Integration tests in `tests/` directory use temp git repos (no network required)
- Tests cover: git operations, sanitization, CLI commands (setup, add, remove, doctor)

### Linting
```bash
cargo clippy         # Lint checks
cargo fmt            # Format code
cargo fmt -- --check # Check formatting without modifying
```

## Common Commands & Workflows

### Core Commands
- `init`: Interactive configuration setup
- `setup <ticket>`: Create ticket workspace with worktrees
- `add <repo>`: Add repo worktree to existing ticket
- `remove <repo>`: Remove repo worktree from ticket
- `destroy <ticket>`: Delete entire ticket workspace
- `add-repo <repo>`: Register a new repository
- `setup-repos`: Clone all registered repos
- `config <key> [value]`: View/set configuration
- `doctor`: Validate configuration and report issues

### Verbosity Flags
- `-q, --quiet`: Suppress info logs, show only errors
- `-v, --verbose`: Show debug logs
- `-vv`: Show trace logs

## Important Implementation Notes

### Git Operations
1. **Worktree Creation**: 
   - Fetch and fast-forward before creating branches
   - Prefer `origin/HEAD` as base branch; warn if falling back to `HEAD`
   - Store per-repo branch and worktree names in metadata

2. **Branch Resolution**:
   - Check if branch exists in stored metadata first
   - Compute branch name if not found (with warning)
   - Create new branch from base ref or default branch

3. **Worktree Cleanup**:
   - Delete worktree directory first
   - Then prune using stored worktree name
   - Warn if falling back to computed worktree name

### Configuration Structure
```rust
struct Config {
    branch_prefix: String,           // e.g., "feature"
    github_base_url: String,          // e.g., "https://github.com"
    default_repository_owner: String, // e.g., "my-org"
    code_directory: PathBuf,          // Where repos are cloned
    tickets_directory: PathBuf,       // Where ticket workspaces live
    repositories: HashMap<String, RepoDefinition>, // alias -> {url, path}
}
```

### Repository URL Parsing
When adding repos (`add-repo`), parse input flexibly:
1. **Full URL**: `git@github.com:owner/repo.git` → use as-is
2. **Owner/Name**: `owner/repo` → construct using `github_base_url`
3. **Name Only**: `repo` → construct using `default_repository_owner`

## Testing Guidelines

### When Adding New Features
1. Add unit tests for helper functions
2. Add integration tests for CLI commands
3. Use `tempfile` crate for temporary directories in tests
4. Mock git operations with temp repos (no network dependencies)
5. Test both success and error paths

### Test File Locations
- Unit tests: In same file as code (within module with `#[cfg(test)]`)
- Integration tests: `tests/` directory
- Test utilities: `tests/common/` (if needed)

## Common Pitfalls to Avoid

1. **Don't** use `println!` for logging - use `log` macros
2. **Don't** shell out to git - use `git2` crate
3. **Don't** assume paths are Unix-style - use `PathBuf` and `Path` methods
4. **Don't** forget to check working tree is clean before destructive operations
5. **Don't** hardcode paths - use configuration and standard directories
6. **Don't** ignore stored metadata - always check `.tix/info.toml` first

## Release Process

- Releases are automated via GitHub Actions (`.github/workflows/release.yml`)
- Binary artifacts built for: Linux x86_64, macOS aarch64, Windows x86_64
- Self-update capability via `tix update` command (fetches from GitHub releases)

## Additional Resources

- `README.md`: User-facing documentation and usage examples
- `REQS.md`: Detailed project requirements and specifications
- `AGENTS.md`: Quick orientation guide for agents (condensed version of this file)
