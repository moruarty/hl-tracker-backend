# -------- builder stage --------
FROM rust:1.85 as builder
# ^ use a recent stable; 1.85+ supports edition2024

WORKDIR /app

# Cache deps
COPY Cargo.toml Cargo.lock ./
# Create a dummy main to build deps, then remove it
RUN mkdir -p src && echo "fn main(){}" > src/main.rs \
 && cargo fetch \
 && cargo build --release \
 && rm -rf src

# Now copy your actual source and build
COPY src ./src
RUN cargo build --release

# -------- runtime stage --------
FROM debian:bookworm-slim

# install CA certs for HTTPS
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 10001 appuser

# Copy the statically-linked (or mostly) binary from builder
COPY --from=builder /app/target/release/hyperliquid_tracker /usr/local/bin/app

ENV RUST_LOG=info
EXPOSE 8080
USER appuser

# IMPORTANT: your main.rs must bind 0.0.0.0:8080 (you already set this)
CMD ["/usr/local/bin/app"]
