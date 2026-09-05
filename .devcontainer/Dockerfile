FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends default-mysql-client \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install sqlx-cli --version 0.8.3 --no-default-features --features mysql \
    && rustup component add clippy rust-src
