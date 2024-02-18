pub mod config;
pub mod error;
pub mod s3;

use anyhow::anyhow;
use blake3::Hash;
use chrono::prelude::*;

use clap::{Parser, Subcommand};
use std::{
    fs::File,
    io::{BufReader, Error},
    path::Path,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run Backup
    Run {
        /// config file path
        #[arg(short, long, default_value = "./config.toml")]
        config: String,
    },

    /// Test Config
    Test {
        /// config file path
        #[arg(short, long, default_value = "./config.toml")]
        config: String,
    },
}

#[derive(Debug, Clone)]
pub struct CpsdFileName {
    pub prefix: String,
    pub name: String,
    pub datatime: String,
    pub hash: String,
    pub extension: String,
}

impl Default for CpsdFileName {
    fn default() -> Self {
        Self {
            prefix: "backup".to_string(),
            name: "name".to_string(),
            datatime: chrono::DateTime::format(&Utc::now(), "%Y_%m_%d-%H_%M_%S").to_string(),
            hash: "0000000".to_string(),
            extension: ".tar.zst".to_string(),
        }
    }
}

impl CpsdFileName {
    pub fn to_filename(&self) -> String {
        format!(
            "{}-{}-{}-{}{}",
            self.prefix, self.name, self.datatime, self.hash, self.extension
        )
    }

    pub fn try_from_filename<S>(s: S) -> Result<Self, anyhow::Error>
    where
        S: AsRef<str>,
    {
        let s = s.as_ref();
        let Some(ss) = s.strip_suffix(".tar.zst") else {
            return Err(anyhow!("parse failed from filename: {}", s));
        };
        let sp: Vec<&str> = ss.split('-').collect();
        if sp.len() != 4 {
            return Err(anyhow!("parse failed from filename: {}", s));
        }

        Ok(Self {
            prefix: sp[0].to_string(),
            name: sp[1].to_string(),
            datatime: sp[2].to_string(),
            hash: sp[3].to_string(),
            extension: ".tar.zst".to_string(),
        })
    }
}

pub fn init_tracing() {
    let default_tracing_level = if cfg!(debug_assertions) {
        "s_backup=debug"
    } else {
        "s_backup=info"
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_tracing_level.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

pub fn calc_hash_mmap_rayon<P>(path: P) -> Result<Hash, Error>
where
    P: AsRef<Path>,
{
    let mut hasher = blake3::Hasher::new();
    hasher.update_mmap_rayon(path)?;
    let hash = hasher.finalize();
    Ok(hash)
}

pub fn calc_hash_mmap<P>(path: P) -> Result<String, Error>
where
    P: AsRef<Path>,
{
    let mut hasher = blake3::Hasher::new();
    hasher.update_mmap(path)?;
    let hash = hasher.finalize();
    Ok(hash.to_string())
}

pub fn calc_hash<P>(path: P) -> Result<String, Error>
where
    P: AsRef<Path>,
{
    let reader = BufReader::new(File::open(path).unwrap());
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(reader)?;
    let hash = hasher.finalize();
    Ok(hash.to_string())
}
