# important: must be running miniChRIS-docker and in the minichris-local network

FROM docker.io/lukemathwalker/cargo-chef:0.1.66-rust-1.78-alpine3.18 AS chef
WORKDIR /app
ARG CARGO_TERM_COLOR=always

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --target x86_64-unknown-linux-musl --recipe-path recipe.json

COPY . .
# need to set DATABASE_URL for sqlx crate to do compile-time validation of SQL commands
ARG DATABASE_URL
RUN cargo build --release --locked --target x86_64-unknown-linux-musl

FROM scratch
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/oxidicom /app/oxidicom

LABEL org.opencontainers.image.authors="Jennings Zhang <jennings.zhang@childrens.harvard.edu>, FNNDSC <dev@babyMRI.org>" \
    org.opencontainers.image.url="https://github.com/FNNDSC/oxidicom" \
    org.opencontainers.image.licenses="MIT" \
    org.opencontainers.image.title="oxidicom" \
    org.opencontainers.image.description="DICOM file receiver for ChRIS backend"

EXPOSE 11111
CMD ["/app/oxidicom"]
