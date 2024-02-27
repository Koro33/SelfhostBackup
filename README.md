# SelfhostBackup

![GitHub Release](https://img.shields.io/github/v/release/Koro33/s-backup) ![GitHub License](https://img.shields.io/github/license/Koro33/s-backup) ![GitHub Repo stars](https://img.shields.io/github/stars/Koro33/s-backup)

Use S3 (or compatible) storage service to backup your selfhosted service configurations.

## Motivation

Some of the cloud storage services(e.g. Backblaze, Cloudflare R2) provided free tier. Our individual users can use it free of charge, to backup small amount of data, e.g. selfhosted service configurations or data. This project aims to make it happen(and easy).

## Usage

### docker (recommend)

```sh
docker run -d \
  -v /path/to/config.toml:/app/config.toml \
  -v /path/to/backup:/backup_path_in_config:ro \
  ghcr.io/koro33/s-backup:latest \
  run
```

or docker compose, see [docker-compose.yml](./docker-compose.yml)

**A config file should be provided**. see [config.example.toml](./config.example.toml)

to test the config

```sh
docker run \
  -v /path/to/config.toml:/app/config.toml \
  ghcr.io/koro33/s-backup:latest \
  test
```

### Cli

```sh
# run according to config
s-backup run --config /path/to/config.toml

# test config
s-backup test --config /path/to/config.toml
```

## warning

- **For backblaze(b2) user** since versioning is enabled, the delete opration will not really delete the files. it will create a new file with the same fileanme and set it to hidden. If you really want to delete the old backup(to avoid beyond free tier storage capacity), go backet `Lifecycle Settings`, and set it to `Keep only the last version`. As the docs say, "This rule keeps only the most current version of a file. The previous version of the file is “hidden” for **one day** and then deleted"
