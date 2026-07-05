FROM mcr.microsoft.com/devcontainers/rust:latest

WORKDIR /app

RUN cargo install sqlx-cli --no-default-features --features mysql && rustup component add clippy rust-src 
