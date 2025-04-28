use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default)]
    pub console: ConsoleLogConfig,
    #[serde(default)]
    pub file: FileLogConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleLogConfig {
    #[serde(default = "default_console_enabled")]
    pub enabled: bool,
    #[serde(default = "default_rust_log")]
    pub rust_log: String,
    #[serde(default = "default_console_format")]
    pub format: String,
    #[serde(default = "default_include_target")]
    pub include_target: bool,
    #[serde(default = "default_include_thread_ids")]
    pub include_thread_ids: bool,
    #[serde(default = "default_include_thread_names")]
    pub include_thread_names: bool,
    #[serde(default = "default_include_file")]
    pub include_file: bool,
    #[serde(default = "default_include_line_number")]
    pub include_line_number: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLogConfig {
    #[serde(default = "default_file_enabled")]
    pub enabled: bool,
    #[serde(default = "default_rust_log")]
    pub rust_log: String,
    #[serde(default = "default_file_format")]
    pub format: String,
    #[serde(default = "default_file_path")]
    pub path: String,
    #[serde(default = "default_file_rotation")]
    pub rotation: String,
    #[serde(default = "default_include_target")]
    pub include_target: bool,
    #[serde(default = "default_include_thread_ids")]
    pub include_thread_ids: bool,
    #[serde(default = "default_include_thread_names")]
    pub include_thread_names: bool,
    #[serde(default = "default_include_file")]
    pub include_file: bool,
    #[serde(default = "default_include_line_number")]
    pub include_line_number: bool,
}

// Default functions for logging configuration
fn default_console_enabled() -> bool {
    true
}

fn default_rust_log() -> String {
    if cfg!(debug_assertions) {
        // Development environment - more verbose logging
        "warn,rpc_gateway_core=debug".to_string()
    } else {
        // Production environment - more conservative logging
        "warn,rpc_gateway_core=info".to_string()
    }
}

fn default_console_format() -> String {
    if cfg!(debug_assertions) {
        "text".to_string()
    } else {
        "json".to_string()
    }
}

fn default_file_enabled() -> bool {
    false
}

fn default_file_format() -> String {
    "json".to_string()
}

fn default_file_path() -> String {
    "logs/rpc-gateway.log".to_string()
}

fn default_file_rotation() -> String {
    "daily".to_string()
}

fn default_include_target() -> bool {
    false
}

fn default_include_thread_ids() -> bool {
    false
}

fn default_include_thread_names() -> bool {
    false
}

fn default_include_file() -> bool {
    true
}

fn default_include_line_number() -> bool {
    true
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            console: ConsoleLogConfig::default(),
            file: FileLogConfig::default(),
        }
    }
}

impl Default for ConsoleLogConfig {
    fn default() -> Self {
        Self {
            enabled: default_console_enabled(),
            rust_log: default_rust_log(),
            format: default_console_format(),
            include_target: default_include_target(),
            include_thread_ids: default_include_thread_ids(),
            include_thread_names: default_include_thread_names(),
            include_file: default_include_file(),
            include_line_number: default_include_line_number(),
        }
    }
}

impl Default for FileLogConfig {
    fn default() -> Self {
        Self {
            enabled: default_file_enabled(),
            rust_log: default_rust_log(),
            format: default_file_format(),
            path: default_file_path(),
            rotation: default_file_rotation(),
            include_target: default_include_target(),
            include_thread_ids: default_include_thread_ids(),
            include_thread_names: default_include_thread_names(),
            include_file: default_include_file(),
            include_line_number: default_include_line_number(),
        }
    }
}
