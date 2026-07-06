# ═══════════════════════════════════════════════════════════════════════════
# RAT Agent — Multi-stage Dockerfile
# Builds agentic-memory and rat-pipeline binaries.
# Uses cargo-chef for efficient dependency caching.
# ═══════════════════════════════════════════════════════════════════════════

# ── Stage 1: Planner (dependency caching via cargo-chef) ─────────────────
FROM rust:1.96-slim-bookworm AS planner
WORKDIR /app

# Install cargo-chef for dependency caching
RUN cargo install cargo-chef

# Install system deps needed for build
RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 2: Builder ──────────────────────────────────────────────────────
FROM rust:1.96-slim-bookworm AS builder
WORKDIR /app

RUN cargo install cargo-chef

RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY --from=planner /app/recipe.json recipe.json
# Build only dependencies (cached layer)
RUN cargo chef cook --release --recipe-path recipe.json -p agentic-memory -p rat-pipeline

# Copy full source and build targeted packages
COPY . .
RUN cargo build --release -p agentic-memory -p rat-pipeline

# Strip debug info
RUN strip /app/target/release/agentic-memory && \
    strip /app/target/release/rat-pipeline

# ── Stage 3: Runtime ──────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime
WORKDIR /app

# ca-certificates for Binance HTTPS, curl for healthcheck, libssl3 for reqwest
RUN apt-get update && apt-get install -y \
    ca-certificates curl libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/agentic-memory /app/agentic-memory
COPY --from=builder /app/target/release/rat-pipeline /app/rat-pipeline

EXPOSE 3111

# Default: run memory service (overridden by docker-compose for pipeline)
CMD ["/app/agentic-memory"]
