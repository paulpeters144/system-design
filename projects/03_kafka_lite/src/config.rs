use serde::{Deserialize, Deserializer};
use config::{Config, ConfigError, File};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub broker_port: u16,
    pub log_dir: PathBuf,
    pub log_level: String,
    #[serde(deserialize_with = "deserialize_bytes")]
    pub segment_size_limit: u64,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let s = Config::builder()
            .set_default("broker_port", 8080)?
            .set_default("log_level", "info")?
            .add_source(File::with_name("config.yaml").required(false))
            .add_source(config::Environment::with_prefix("KAFKA_LITE"))
            .build()?;

        s.try_deserialize()
    }
}

fn deserialize_bytes<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    let s = s.trim().to_uppercase();

    if let Some(val) = s.strip_suffix("MB") {
        val.trim()
            .parse::<u64>()
            .map(|v| v * 1024 * 1024)
            .map_err(serde::de::Error::custom)
    } else if let Some(val) = s.strip_suffix("GB") {
        val.trim()
            .parse::<u64>()
            .map(|v| v * 1024 * 1024 * 1024)
            .map_err(serde::de::Error::custom)
    } else if let Some(val) = s.strip_suffix("KB") {
        val.trim()
            .parse::<u64>()
            .map(|v| v * 1024)
            .map_err(serde::de::Error::custom)
    } else {
        s.parse::<u64>().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct TestConfig {
        #[serde(deserialize_with = "deserialize_bytes")]
        bytes: u64,
    }

    #[test]
    fn test_deserialize_bytes() {
        let cases = vec![
            ("100", 100),
            ("1KB", 1024),
            ("1MB", 1024 * 1024),
            ("1GB", 1024 * 1024 * 1024),
            (" 10 MB ", 10 * 1024 * 1024),
        ];

        for (input, expected) in cases {
            let yaml = format!("bytes: \"{}\"", input);
            let config: TestConfig = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(config.bytes, expected, "Failed for input: {}", input);
        }
    }
}
