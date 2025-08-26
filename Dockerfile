# -------- builder stage --------
FROM rust:latest as builder

WORKDIR /app

# Install the MUSL C compiler which is required by the 'ring' crate
RUN apt-get update && apt-get install -y musl-tools

# Install the MUSL target for Rust
RUN rustup target add x86_64-unknown-linux-musl

# Cache deps using the MUSL target
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main(){}" > src/main.rs \
 # MODIFIED: Add RUSTFLAGS to create a more compatible binary
 && RUSTFLAGS='-C target-feature=-avx,-avx2,-sse4.1,-sse4.2' cargo build --release --target x86_64-unknown-linux-musl \
 && rm -rf src

# Now copy your actual source and build for MUSL
COPY src ./src
# MODIFIED: Add RUSTFLAGS here as well
RUN RUSTFLAGS='-C target-feature=-avx,-avx2,-sse4.1,-sse4.2' cargo build --release --target x86_64-unknown-linux-musl

# -------- runtime stage --------
FROM debian:bookworm-slim

# install CA certs for HTTPS
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 10001 appuser

# Copy the statically-linked binary
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/hyperliquid_tracker /usr/local/bin/app

# Ensure the binary is executable
RUN chmod +x /usr/local/bin/app

ENV RUST_LOG=info
EXPOSE 8080

# NEW: Set environment variables to point to the SSL certs for HTTPS requests
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV SSL_CERT_DIR=/etc/ssl/certs

USER appuser

CMD ["/usr/local/bin/app"]