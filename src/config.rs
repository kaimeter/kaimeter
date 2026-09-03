// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Configuration: environment variables with sane defaults, plus an optional
//! TOML file. Precedence: environment variables > TOML file > defaults.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Default listen address.
pub const DEFAULT_ADDR: &str = "127.0.0.1:8080";
/// Default data directory (SQLite database lives here).
pub const DEFAULT_DATA_DIR: &str = "./data";
/// Default locales directory.
pub const DEFAULT_LOCALES_DIR: &str = "./locales";
/// Optional TOML config file location.
pub const DEFAULT_CONFIG_PATH: &str = "./kaimeter.toml";

/// Mirror of [`Config`] used for deserializing the optional TOML file.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    addr: Option<String>,
    data_dir: Option<PathBuf>,
    locales_dir: Option<PathBuf>,
}

/// Fully-resolved runtime configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Socket address to bind, e.g. `127.0.0.1:8080`.
    pub addr: String,
    /// Directory for the embedded SQLite database (created on startup).
    pub data_dir: PathBuf,
    /// Directory containing `en.json`, `zh-CN.json`, `termbase.json`.
    pub locales_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: DEFAULT_ADDR.to_string(),
            data_dir: PathBuf::from(DEFAULT_DATA_DIR),
            locales_dir: PathBuf::from(DEFAULT_LOCALES_DIR),
        }
    }
}

impl Config {
    /// Load configuration: defaults <- TOML file (if present) <- environment.
    ///
    /// The TOML file is optional; a missing file is not an error. A file that
    /// exists but cannot be parsed *is* an error.
    pub fn load() -> anyhow::Result<Self> {
        Self::load_from(
            Path::new(DEFAULT_CONFIG_PATH),
            &std::env::vars().collect::<Vec<_>>(),
        )
    }

    /// Testable core of [`Config::load`].
    pub(crate) fn load_from(config_path: &Path, env: &[(String, String)]) -> anyhow::Result<Self> {
        let mut cfg = Config::default();

        // Layer 1: optional TOML file.
        if config_path.exists() {
            let raw = std::fs::read_to_string(config_path)?;
            let file: FileConfig = toml::from_str(&raw)
                .map_err(|e| anyhow::anyhow!("invalid TOML in {}: {e}", config_path.display()))?;
            if let Some(addr) = file.addr {
                cfg.addr = addr;
            }
            if let Some(data_dir) = file.data_dir {
                cfg.data_dir = data_dir;
            }
            if let Some(locales_dir) = file.locales_dir {
                cfg.locales_dir = locales_dir;
            }
        }

        // Layer 2: environment variables override the file.
        for (key, value) in env {
            match key.as_str() {
                "KAIMETER_ADDR" => cfg.addr = value.clone(),
                "KAIMETER_DATA_DIR" => cfg.data_dir = PathBuf::from(value),
                "KAIMETER_LOCALES_DIR" => cfg.locales_dir = PathBuf::from(value),
                _ => {}
            }
        }

        if cfg.addr.is_empty() {
            anyhow::bail!("KAIMETER_ADDR must not be empty");
        }
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn defaults_when_no_file_and_no_env() {
        // Point at a path that must not exist.
        let cfg = Config::load_from(Path::new("./definitely-missing-kaimeter.toml"), &env(&[]))
            .expect("config load");
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn env_overrides_defaults() {
        let cfg = Config::load_from(
            Path::new("./definitely-missing-kaimeter.toml"),
            &env(&[
                ("KAIMETER_ADDR", "0.0.0.0:9000"),
                ("KAIMETER_DATA_DIR", "/tmp/kai-data"),
                ("KAIMETER_LOCALES_DIR", "/tmp/kai-locales"),
            ]),
        )
        .expect("config load");
        assert_eq!(cfg.addr, "0.0.0.0:9000");
        assert_eq!(cfg.data_dir, PathBuf::from("/tmp/kai-data"));
        assert_eq!(cfg.locales_dir, PathBuf::from("/tmp/kai-locales"));
    }

    #[test]
    fn toml_layer_applies_and_env_wins_over_toml() {
        let dir = std::env::temp_dir().join("kaimeter-config-test");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("kaimeter.toml");
        std::fs::write(
            &path,
            "addr = \"127.0.0.1:7000\"\ndata_dir = \"/srv/kaimeter\"\n",
        )
        .expect("write toml");

        // No env: TOML values apply.
        let cfg = Config::load_from(&path, &env(&[])).expect("config load");
        assert_eq!(cfg.addr, "127.0.0.1:7000");
        assert_eq!(cfg.data_dir, PathBuf::from("/srv/kaimeter"));
        // locales_dir stays default.
        assert_eq!(cfg.locales_dir, PathBuf::from(DEFAULT_LOCALES_DIR));

        // Env beats TOML for the same key.
        let cfg = Config::load_from(&path, &env(&[("KAIMETER_ADDR", "127.0.0.1:8001")]))
            .expect("config load");
        assert_eq!(cfg.addr, "127.0.0.1:8001");
    }

    #[test]
    fn invalid_toml_is_an_error() {
        let dir = std::env::temp_dir().join("kaimeter-config-test-bad");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("kaimeter.toml");
        std::fs::write(&path, "addr = [not, valid]]]").expect("write toml");
        assert!(Config::load_from(&path, &env(&[])).is_err());
    }

    #[test]
    fn unknown_toml_keys_are_rejected() {
        let dir = std::env::temp_dir().join("kaimeter-config-test-unknown");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("kaimeter.toml");
        std::fs::write(&path, "bogus_key = 1\n").expect("write toml");
        assert!(Config::load_from(&path, &env(&[])).is_err());
    }
}
