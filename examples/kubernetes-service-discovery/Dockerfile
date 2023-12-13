# syntax=docker/dockerfile:1.4
FROM rust:1.74.1 AS chef
WORKDIR /usr/app
RUN cargo install cargo-chef

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /usr/app/recipe.json recipe.json

# Build dependencies - this is the caching Docker layer!
RUN --mount=type=cache,target=~/.cargo \
    --mount=type=cache,target=/usr/app/target \
    cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . .
RUN --mount=type=cache,target=~/.cargo \
    --mount=type=cache,target=/usr/app/target \
    cargo build --release

FROM builder as test

RUN --mount=type=cache,target=~/.cargo \
    --mount=type=cache,target=/usr/app/target \
    cargo test --release

FROM builder AS release

# install the binaries
RUN --mount=type=cache,target=~/.cargo \
    --mount=type=cache,target=/usr/app/target \
    cargo install --locked --offline --frozen --path .

# final slim image
FROM frolvlad/alpine-glibc:alpine-3.17
WORKDIR /usr/local/bin

COPY --from=release /usr/local/cargo/bin/kubernetes-service-discovery .

CMD ["kubernetes-service-discovery"]