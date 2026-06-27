# syntax=docker/dockerfile:1.7

# ---- stage 1: build WebUI (embedded into the Rust binary via rust-embed) ----
FROM node:20-alpine AS webui
WORKDIR /webui
COPY webui/package.json webui/package-lock.json ./
RUN npm ci
COPY webui/ ./
RUN npm run build

# ---- stage 2: build Rust release binary (native arch; buildx sets platform) ----
FROM rust:1-bookworm AS rust
WORKDIR /app
COPY Cargo.toml ./
COPY Cargo.lock* ./
COPY src/ ./src/
COPY migrations/ ./migrations/
COPY --from=webui /webui/dist ./webui/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release && \
    cp target/release/search-proxy /usr/local/bin/search-proxy

# ---- stage 3: minimal runtime ----
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=rust /usr/local/bin/search-proxy /usr/local/bin/search-proxy

WORKDIR /app
VOLUME ["/data"]
EXPOSE 8788
ENTRYPOINT ["/usr/local/bin/search-proxy"]
CMD ["serve"]
