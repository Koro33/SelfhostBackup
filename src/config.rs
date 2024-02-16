use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub backup: Vec<Backup>,
    pub s3: S3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backup {
    pub name: String,
    pub path: String,
    pub exclude: Vec<String>,
    #[serde(default = "Backup::default_interval")]
    pub interval: usize,
    #[serde(default = "Backup::default_keep")]
    pub keep: usize,
}

impl Backup {
    fn default_interval() -> usize {
        24 * 60 * 60
    }

    fn default_keep() -> usize {
        7
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3 {
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    #[serde(default = "S3::default_root")]
    pub root: String,
}

impl S3 {
    fn default_root() -> String {
        "/backup".to_string()
    }
}

pub async fn read_config<P>(path: P) -> Result<Config, anyhow::Error>
where
    P: AsRef<Path>,
{
    let content = fs::read_to_string(path).await?;
    let config: Config = toml::from_str(&content)?;

    // check if there is a deplicated backup name
    if config
        .backup
        .iter()
        .map(|b| &b.name)
        .collect::<std::collections::HashSet<_>>()
        .len()
        != config.backup.len()
    {
        return Err(anyhow::anyhow!("deplicated backup name in config"));
    }

    // check if name contens `-`
    if config.backup.iter().any(|b| b.name.contains('-')) {
        return Err(anyhow::anyhow!("backup name can not contain `-`"));
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[tokio::test]
    async fn test_read_config() {
        let path = Path::new("./tests/config.toml");
        let config = read_config(path).await.unwrap();
        println!("{:#?}", config);
        assert_eq!(config.backup.len(), 2);
    }
}
