//! Layered configuration loader.
//!
//! Precedence (each layer overrides the previous):
//! 1. Compiled-in defaults  (`Config::default()`)
//! 2. Global config         (`~/.config/rustpi/config.toml`)
//! 3. User config           (`~/.rustpi/config.toml`)
//! 4. Project config        (`.rustpi/config.toml` in cwd)
//! 5. Runtime overrides     (passed programmatically via `with_override`)
//!
//! Missing files are silently skipped.  Parse errors are surfaced as
//! `ConfigError::Parse`.

use std::path::PathBuf;

use tracing::debug;

use crate::{
    error::ConfigError,
    model::{
        CliConfig, Config, GlobalConfig, LoggingConfig, MemoryConfig, PolicyDefaults, ProjectConfig,
        TuiConfig, UserConfig,
    },
};

// ---------------------------------------------------------------------------
// Merge trait
// ---------------------------------------------------------------------------

/// Merges an override layer on top of a base layer.
///
/// Merge rules:
/// * `Option<T>` fields: if the override is `Some`, use it; otherwise keep the base.
/// * `Vec<T>` fields: if the override vec is non-empty, replace the base; otherwise keep the base.
/// * Scalar fields with defaults (bool, String, enum): always take the override, because
///   `Config::default()` provides the canonical default — any layer that was parsed from TOML
///   will have already set sensible values or left the field at the default.
trait Merge: Sized {
    fn merge(base: Self, over: Self) -> Self;
}

impl Merge for Config {
    fn merge(base: Self, over: Self) -> Self {
        Config {
            global: Merge::merge(base.global, over.global),
            providers: merge_vec(base.providers, over.providers),
            memory: Merge::merge(base.memory, over.memory),
            user: Merge::merge(base.user, over.user),
            project: Merge::merge(base.project, over.project),
            cli: Merge::merge(base.cli, over.cli),
            tui: Merge::merge(base.tui, over.tui),
            logging: Merge::merge(base.logging, over.logging),
            policy: Merge::merge(base.policy, over.policy),
        }
    }
}

impl Merge for GlobalConfig {
    fn merge(base: Self, over: Self) -> Self {
        GlobalConfig {
            default_provider: over.default_provider.or(base.default_provider),
            default_model: over.default_model.or(base.default_model),
            max_context_tokens: over.max_context_tokens.or(base.max_context_tokens),
            log_level: over.log_level.or(base.log_level),
        }
    }
}

impl Merge for MemoryConfig {
    fn merge(base: Self, over: Self) -> Self {
        MemoryConfig {
            obsidian_vault_path: over.obsidian_vault_path.or(base.obsidian_vault_path),
            session_backend: over.session_backend,
            qdrant_enabled: over.qdrant_enabled || base.qdrant_enabled,
            qdrant_url: over.qdrant_url.or(base.qdrant_url),
        }
    }
}

impl Merge for UserConfig {
    fn merge(base: Self, over: Self) -> Self {
        UserConfig {
            preferred_provider: over.preferred_provider.or(base.preferred_provider),
            preferred_model: over.preferred_model.or(base.preferred_model),
            theme: over.theme.or(base.theme),
            editor: over.editor.or(base.editor),
        }
    }
}

impl Merge for ProjectConfig {
    fn merge(base: Self, over: Self) -> Self {
        ProjectConfig {
            project_name: over.project_name.or(base.project_name),
            default_provider: over.default_provider.or(base.default_provider),
            default_model: over.default_model.or(base.default_model),
            allowed_tools: over.allowed_tools.or(base.allowed_tools),
            context_root: over.context_root.or(base.context_root),
        }
    }
}

impl Merge for CliConfig {
    fn merge(_base: Self, over: Self) -> Self {
        CliConfig {
            output_format: over.output_format,
            // Explicit `false` in an override file means "disable color/pager";
            // we can't distinguish that from a default, so we take the override.
            color: over.color,
            pager: over.pager,
        }
    }
}

impl Merge for TuiConfig {
    fn merge(base: Self, over: Self) -> Self {
        TuiConfig {
            // Only override the theme if the override differs from the default.
            theme: if over.theme != TuiConfig::default().theme || base.theme == TuiConfig::default().theme {
                over.theme
            } else {
                base.theme
            },
            show_token_count: over.show_token_count,
            wrap_lines: over.wrap_lines,
        }
    }
}

impl Merge for LoggingConfig {
    fn merge(base: Self, over: Self) -> Self {
        LoggingConfig {
            level: if over.level != LoggingConfig::default().level
                || base.level == LoggingConfig::default().level
            {
                over.level
            } else {
                base.level
            },
            format: over.format,
            file: over.file.or(base.file),
        }
    }
}

impl Merge for PolicyDefaults {
    fn merge(_base: Self, over: Self) -> Self {
        // For policy we always take the override layer wholesale, because booleans
        // have no sentinel "unset" value.
        over
    }
}

/// For Vec fields: if the override is non-empty, use it; otherwise keep the base.
fn merge_vec<T>(base: Vec<T>, over: Vec<T>) -> Vec<T> {
    if over.is_empty() {
        base
    } else {
        over
    }
}

// ---------------------------------------------------------------------------
// ConfigLoader
// ---------------------------------------------------------------------------

/// Loads and merges configuration from multiple file-system layers.
///
/// # Example
/// ```
/// use config_core::loader::ConfigLoader;
///
/// let config = ConfigLoader::new().load().unwrap();
/// println!("{:?}", config.logging.level);
/// ```
pub struct ConfigLoader {
    global_path: Option<PathBuf>,
    user_path: Option<PathBuf>,
    project_path: Option<PathBuf>,
    override_config: Option<Config>,
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigLoader {
    /// Create a new loader with default OS-appropriate paths.
    pub fn new() -> Self {
        let home = std::env::var("HOME").ok().map(PathBuf::from);

        let global_path = home
            .as_ref()
            .map(|h| h.join(".config").join("rustpi").join("config.toml"));

        let user_path = home.map(|h| h.join(".rustpi").join("config.toml"));

        let project_path = std::env::current_dir()
            .ok()
            .map(|d| d.join(".rustpi").join("config.toml"));

        Self {
            global_path,
            user_path,
            project_path,
            override_config: None,
        }
    }

    /// Override the global config file path.
    pub fn with_global_path(mut self, path: PathBuf) -> Self {
        self.global_path = Some(path);
        self
    }

    /// Override the user config file path.
    pub fn with_user_path(mut self, path: PathBuf) -> Self {
        self.user_path = Some(path);
        self
    }

    /// Override the project config file path.
    pub fn with_project_path(mut self, path: PathBuf) -> Self {
        self.project_path = Some(path);
        self
    }

    /// Supply a programmatic override that is applied last (highest precedence).
    pub fn with_override(mut self, cfg: Config) -> Self {
        self.override_config = Some(cfg);
        self
    }

    /// Load and merge all config layers.  Missing files are silently skipped.
    pub fn load(&self) -> Result<Config, ConfigError> {
        let mut config = Config::default();

        for path in [&self.global_path, &self.user_path, &self.project_path]
            .into_iter()
            .flatten()
        {
            match load_file(path) {
                Ok(layer) => {
                    debug!(path = %path.display(), "loaded config layer");
                    config = Merge::merge(config, layer);
                }
                Err(ConfigError::NotFound(_)) => {
                    debug!(path = %path.display(), "config file not found, skipping");
                }
                Err(e) => return Err(e),
            }
        }

        if let Some(over) = &self.override_config {
            config = Merge::merge(config, over.clone());
        }

        Ok(config)
    }

    /// Parse a `Config` directly from a TOML string (useful for tests).
    pub fn from_toml_str(s: &str) -> Result<Config, ConfigError> {
        toml::from_str(s).map_err(|e| ConfigError::Parse(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_file(path: &std::path::Path) -> Result<Config, ConfigError> {
    if !path.exists() {
        return Err(ConfigError::NotFound(path.display().to_string()));
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|e| ConfigError::Parse(format!("could not read {}: {e}", path.display())))?;
    toml::from_str(&contents).map_err(|e| ConfigError::Parse(e.to_string()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DefaultPolicy, LogFormat, OutputFormat};
    use std::io::Write;

    // Helper: write a TOML string to a temp file in the project dir so we
    // don't use /tmp (which is forbidden by the environment rules).
    fn write_temp_toml(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    // Return a scratch directory under the crate's target/ dir.
    fn scratch_dir(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-scratch")
            .join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ------------------------------------------------------------------
    // 1. Default values
    // ------------------------------------------------------------------
    #[test]
    fn default_config_has_sensible_values() {
        let cfg = Config::default();
        assert_eq!(cfg.logging.level, "info");
        assert_eq!(cfg.tui.theme, "default");
        assert!(cfg.cli.color);
        assert!(cfg.cli.pager);
        assert!(cfg.tui.show_token_count);
        assert!(cfg.tui.wrap_lines);
        assert!(!cfg.policy.require_approval_for_file_writes);
        assert!(!cfg.policy.require_approval_for_shell_commands);
        assert!(cfg.providers.is_empty());
    }

    // ------------------------------------------------------------------
    // 2. from_toml_str — valid TOML
    // ------------------------------------------------------------------
    #[test]
    fn from_toml_str_parses_valid_toml() {
        let toml = r#"
            [logging]
            level = "debug"

            [cli]
            color = false
            pager = false
        "#;

        let cfg = ConfigLoader::from_toml_str(toml).unwrap();
        assert_eq!(cfg.logging.level, "debug");
        assert!(!cfg.cli.color);
        assert!(!cfg.cli.pager);
    }

    // ------------------------------------------------------------------
    // 3. from_toml_str — invalid TOML
    // ------------------------------------------------------------------
    #[test]
    fn from_toml_str_returns_parse_error_on_invalid_toml() {
        let bad = "this is [not valid toml {{{{";
        let err = ConfigLoader::from_toml_str(bad).unwrap_err();
        assert!(matches!(err, ConfigError::Parse(_)));
    }

    // ------------------------------------------------------------------
    // 4. Merge: project default_provider overrides user
    // ------------------------------------------------------------------
    #[test]
    fn merge_project_overrides_user_provider() {
        let user_toml = r#"
            [user]
            preferred_provider = "openai"
        "#;
        let project_toml = r#"
            [project]
            default_provider = "local-llm"
        "#;

        let user_cfg = ConfigLoader::from_toml_str(user_toml).unwrap();
        let project_cfg = ConfigLoader::from_toml_str(project_toml).unwrap();

        let merged: Config = Merge::merge(user_cfg, project_cfg);
        // User preferred_provider should survive
        assert_eq!(
            merged.user.preferred_provider.as_ref().map(|p| p.0.as_str()),
            Some("openai")
        );
        // Project default_provider was set by the project layer
        assert_eq!(
            merged.project.default_provider.as_ref().map(|p| p.0.as_str()),
            Some("local-llm")
        );
    }

    // ------------------------------------------------------------------
    // 5. Missing file is silently skipped
    // ------------------------------------------------------------------
    #[test]
    fn missing_config_file_is_skipped() {
        let dir = scratch_dir("missing-file");
        let nonexistent = dir.join("does_not_exist.toml");

        let result = ConfigLoader::new()
            .with_global_path(nonexistent.clone())
            .with_user_path(nonexistent.clone())
            .with_project_path(nonexistent)
            .load();

        assert!(result.is_ok());
    }

    // ------------------------------------------------------------------
    // 6. Vec fields — override wins when non-empty
    // ------------------------------------------------------------------
    #[test]
    fn vec_providers_override_wins_when_nonempty() {
        let base_toml = r#"
            [[providers]]
            id = "base-provider"
            kind = "open_ai_compatible"
            [providers.auth]
            kind = "api_key"
            env_var = "BASE_KEY"
        "#;
        let over_toml = r#"
            [[providers]]
            id = "override-provider"
            kind = "gemini"
            [providers.auth]
            kind = "device_code"
        "#;

        let base_cfg = ConfigLoader::from_toml_str(base_toml).unwrap();
        let over_cfg = ConfigLoader::from_toml_str(over_toml).unwrap();

        let merged = Merge::merge(base_cfg, over_cfg);
        assert_eq!(merged.providers.len(), 1);
        assert_eq!(merged.providers[0].id.0.as_str(), "override-provider");
    }

    // ------------------------------------------------------------------
    // 7. Vec providers — empty override keeps base
    // ------------------------------------------------------------------
    #[test]
    fn vec_providers_keeps_base_when_override_empty() {
        let base_toml = r#"
            [[providers]]
            id = "base-provider"
            kind = "open_ai_compatible"
            [providers.auth]
            kind = "api_key"
            env_var = "BASE_KEY"
        "#;
        let over_toml = r#"
            [logging]
            level = "warn"
        "#;

        let base_cfg = ConfigLoader::from_toml_str(base_toml).unwrap();
        let over_cfg = ConfigLoader::from_toml_str(over_toml).unwrap();

        let merged = Merge::merge(base_cfg, over_cfg);
        assert_eq!(merged.providers.len(), 1);
        assert_eq!(merged.providers[0].id.0.as_str(), "base-provider");
        assert_eq!(merged.logging.level, "warn");
    }

    // ------------------------------------------------------------------
    // 8. Precedence order: global < user < project
    // ------------------------------------------------------------------
    #[test]
    fn precedence_global_user_project() {
        let dir = scratch_dir("precedence");

        let global = write_temp_toml(
            &dir,
            "global.toml",
            r#"[logging]
level = "error"
"#,
        );
        let user = write_temp_toml(
            &dir,
            "user.toml",
            r#"[logging]
level = "warn"
"#,
        );
        let project = write_temp_toml(
            &dir,
            "project.toml",
            r#"[logging]
level = "debug"
"#,
        );

        let cfg = ConfigLoader::new()
            .with_global_path(global)
            .with_user_path(user)
            .with_project_path(project)
            .load()
            .unwrap();

        // Project layer is last, so it wins.
        assert_eq!(cfg.logging.level, "debug");
    }

    // ------------------------------------------------------------------
    // 9. OutputFormat and LogFormat enums parse correctly
    // ------------------------------------------------------------------
    #[test]
    fn enum_fields_parse_correctly() {
        let toml = r#"
            [cli]
            output_format = "json"

            [logging]
            format = "json"

            [policy]
            default_tool_policy = "deny"
        "#;
        let cfg = ConfigLoader::from_toml_str(toml).unwrap();
        assert!(matches!(cfg.cli.output_format, OutputFormat::Json));
        assert!(matches!(cfg.logging.format, LogFormat::Json));
        assert!(matches!(cfg.policy.default_tool_policy, DefaultPolicy::Deny));
    }

    // ------------------------------------------------------------------
    // 10. Loader reads files from disk
    // ------------------------------------------------------------------
    #[test]
    fn loader_reads_files_from_disk() {
        let dir = scratch_dir("from-disk");

        let path = write_temp_toml(
            &dir,
            "config.toml",
            r#"[tui]
theme = "dracula"
show_token_count = false
"#,
        );

        let cfg = ConfigLoader::new()
            .with_global_path(dir.join("nonexistent.toml"))
            .with_user_path(path)
            .with_project_path(dir.join("nonexistent2.toml"))
            .load()
            .unwrap();

        assert_eq!(cfg.tui.theme, "dracula");
        assert!(!cfg.tui.show_token_count);
    }
}
