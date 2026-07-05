FROM mcr.microsoft.com/devcontainers/rust:1.85-bookworm

WORKDIR /app

RUN cargo install sqlx-cli --version 0.8.3 --no-default-features --features mysql \
    && rustup component add clippy rust-src
