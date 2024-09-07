# TODO how do I build for ARM?

FROM docker.io/lukemathwalker/cargo-chef:0.1.67-rust-1.81.0-alpine3.19 AS chef
WORKDIR /app
ARG CARGO_TERM_COLOR=always

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --target x86_64-unknown-linux-musl --recipe-path recipe.json

COPY . .
RUN cargo build --release --locked --target x86_64-unknown-linux-musl

FROM scratch
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/oxidicom /app/oxidicom

EXPOSE 11111
CMD ["/app/oxidicom"]
