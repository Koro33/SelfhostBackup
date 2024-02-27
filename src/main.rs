use s_backup::{
    calc_hash_mmap_rayon,
    config::{read_config, Backup},
    init_tracing,
    s3::init_s3,
    Args, Commands, CpsdFileName,
};

use anyhow::{anyhow, Result};
use clap::Parser;
use humansize::{FormatSize, DECIMAL};
use opendal::Operator;
use std::{os::linux::fs::MetadataExt, path::Path, process::Stdio};
use tempfile::TempDir;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    signal,
    task::spawn_blocking,
};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let args = Args::parse();

    match &args.command {
        Commands::Run { config } => {
            run(config).await?;
            graceful_shutdown().await;
        }
        Commands::Test { config } => test(config).await?,
    }

    Ok(())
}

async fn run(config_path: &str) -> Result<()> {
    let config = read_config(config_path).await.map_err(|e| {
        tracing::error!("read config failed at `{}`: {}", config_path, e);
        e
    })?;

    let s3_op = init_s3(&config.s3).await?;

    for b in &config.backup {
        let b_clone = b.clone();
        let s3_op_clone = s3_op.clone();
        let _handle = tokio::spawn(async move {
            period_backup(&b_clone, &s3_op_clone).await.unwrap();
        });
    }

    Ok(())
}

async fn test(config_path: &str) -> Result<()> {
    // check config file syntax
    let config = read_config(config_path).await.map_err(|e| {
        tracing::error!("❌ read config failed at `{}`: {}", config_path, e);
        e
    })?;
    tracing::info!("✅ config file syntax is ok");

    // check s3 instance
    let s3_op = init_s3(&config.s3).await?;
    s3_op.check().await.map_err(|e| {
        tracing::error!("❌ connect to s3 service failed: {}", e);
        e
    })?;
    tracing::info!("✅ connect to s3 service successfully");

    Ok(())
}

async fn period_backup(b: &Backup, s3_oprator: &Operator) -> Result<()> {
    loop {
        let _ = backup(b, s3_oprator).await.map_err(|e| {
            tracing::error!("backup failed: {}", e);
            e
        });

        let next_run_date = chrono::Utc::now() + chrono::Duration::seconds(b.interval as i64);
        tracing::info!(
            "[{}] The next backup will be run at {}",
            b.name,
            next_run_date.to_rfc2822()
        );

        tokio::time::sleep(std::time::Duration::from_secs(b.interval as u64)).await
    }
}

async fn backup(b: &Backup, s3_oprator: &Operator) -> Result<()> {
    tracing::info!("[{}] Start backup", b.name);

    let entries = s3_oprator.list("/").await.map_err(|e| {
        tracing::error!("connect to s3 failed: {}", e);
        e
    })?;

    let backup_source = Path::new(&b.path);

    let temp_dir = TempDir::new()?;

    let temp_dest = temp_dir.path().join("cpsd_tmp.tar.zst");

    // Compress with subprocess
    let mut cmd = Command::new("tar");

    let mut args = vec![
        "--zstd".to_string(),
        "-cf".to_string(),
        temp_dest
            .to_str()
            .ok_or(anyhow!("failed to convert path to string"))?
            .to_string(),
    ];
    args.extend(b.exclude.iter().map(|e| format!("--exclude={}", e)));
    args.push("-C".to_string());
    args.push(
        backup_source
            .to_str()
            .ok_or(anyhow!("failed to convert path to string"))?
            .to_string(),
    );
    args.push(".".to_string());

    cmd.args(args).kill_on_drop(true);

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("failed to spawn command");
    let stderr = child.stderr.take().expect("failed to get child's stderr");
    let mut stderr_reader = BufReader::new(stderr).lines();
    while let Some(line) = stderr_reader.next_line().await? {
        tracing::warn!("{}", line);
    }

    let exit_code = child
        .wait()
        .await?
        .code()
        .ok_or(anyhow!("tar is terminated by signal"))
        .map_err(|e| {
            tracing::error!("failed to get exit code: {}", e);
            e
        })?;

    match exit_code {
        // ignore exitcode 1, to fix `tar: file changed as we read it`
        0 | 1 => {}
        _ => return Err(anyhow!("failed to compress")),
    }

    // Calculate blake3 hash
    let temp_dest_clone = temp_dest.clone();
    let calc_hash_task = spawn_blocking(move || calc_hash_mmap_rayon(temp_dest_clone).unwrap());
    let hash = calc_hash_task.await.map_err(|e| {
        tracing::error!("failed to calc hash: {}", e);
        e
    })?;

    tracing::debug!("compressed file hash: {}", hash.to_hex());

    // rename
    let cpsd_f = CpsdFileName {
        name: b.name.clone(),
        hash: hash.to_hex()[57..].to_string(),
        ..Default::default()
    };

    let backup_dest = temp_dir.path().join(cpsd_f.to_filename());

    tokio::fs::rename(&temp_dest, &backup_dest).await?;

    // reader for compressed file
    let compressed_file = tokio::fs::File::open(&backup_dest).await?;
    let metadata = compressed_file.metadata().await?;
    tracing::info!(
        "compressed to `{}` success, size {}",
        backup_dest.display(),
        metadata.st_size().format_size(DECIMAL)
    );
    let mut reader = BufReader::new(compressed_file);

    // writer for s3
    let backup_path = Path::new("/").join(cpsd_f.to_filename());
    let mut writer = s3_oprator
        .writer_with(backup_path.to_str().unwrap())
        .buffer(8 * 1024 * 1024)
        .await
        .unwrap();

    // start upload
    tracing::info!("start uploading...",);
    match tokio::io::copy(&mut reader, &mut writer).await {
        Ok(_) => writer.close().await?,
        Err(_e) => {
            writer.abort().await?;
        }
    }

    tracing::info!("backup to `{}` success", backup_path.display(),);

    // remove old backup
    let to_remove = find_remove_files(&entries, &b.name, b.keep);
    if !to_remove.is_empty() {
        s3_oprator.remove(to_remove.clone()).await?;
        tracing::info!("removed old backup: {:?}", to_remove);
    }

    Ok(())
}

fn find_remove_files(entries: &[opendal::Entry], name: &str, keep: usize) -> Vec<String> {
    // backup_file_list order old -> new
    let to_remove: Vec<String> = entries
        .iter()
        .filter(|x| {
            x.metadata().mode().is_file()
                && x.path().starts_with("backup")
                && x.path().split('-').nth(1).is_some_and(|x| x == name)
        })
        .map(|x| x.path())
        .rev()
        .skip(keep - 1)
        .map(|x| x.to_string())
        .collect();
    to_remove
}

async fn graceful_shutdown() {
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
        tracing::warn!("signal terminate received");
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install signal handler");
        tracing::warn!("signal ctrl_c received");
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[cfg(test)]
mod tests {}
