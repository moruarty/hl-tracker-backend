# ---- builder ----
FROM rust:1.85 as builder
WORKDIR /app

# Build MUSL (static) binary so runtime doesn't need glibc/openssl
RUN rustup target add x86_64-unknown-linux-musl \
 && apt-get update && apt-get install -y --no-install-recommends musl-tools ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# Name of your binary (matches Cargo package name)
ARG BIN=hyperliquid_tracker

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main(){}" > src/main.rs \
 && cargo build --release --locked --target x86_64-unknown-linux-musl \
 && rm -rf src

# Build your actual source
COPY src ./src
# Ensure crt-static for a true static binary
ENV RUSTFLAGS="-C target-feature=+crt-static"
RUN cargo build --release --locked --target x86_64-unknown-linux-musl

# ---- runtime ----
# Static binary -> use scratch; copy CA bundle so reqwest/rustls can validate HTTPS
FROM scratch

# Copy CA certs from builder (Debian provided by builder stage)
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

# Copy the compiled binary
ARG BIN=hyperliquid_tracker
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/${BIN} /usr/local/bin/app

# Container env
ENV RUST_LOG=info
# Cloud Run sets PORT; default to 8080 (your code already reads PORT)
ENV PORT=8080

EXPOSE 8080
# run as non-root (scratch has no passwd DB; UID is fine)
USER 10001
CMD ["/usr/local/bin/app"]
