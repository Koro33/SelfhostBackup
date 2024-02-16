FROM rust:latest as rust-builder
WORKDIR /build

# Copy Cargo files
COPY ./Cargo.toml ./

# Create fake main.rs file in src and build
RUN mkdir ./src && echo 'fn main() { panic!("Dummy Image Called!")}' > ./src/main.rs
RUN cargo build --release

# Copy source files over
RUN rm -rf ./src && rm -rf ./target/release
COPY ./src ./src

# The last modified attribute of main.rs needs to be updated manually,
# otherwise cargo won't rebuild it.
RUN touch -a -m ./src/main.rs
RUN cargo build --release

# Second stage putting the build result into a debian jessie-slim image
FROM debian:stable-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates zstd && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*
COPY --from=rust-builder /build/target/release/s-backup /usr/local/bin/
WORKDIR /usr/local/bin
CMD ["s-backup"]

