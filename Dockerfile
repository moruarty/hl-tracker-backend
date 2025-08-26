# -------- builder stage --------
# This stage is correct and remains the same.
FROM rust:latest as builder

WORKDIR /app

# Install the MUSL C compiler which is required by the 'ring' crate
RUN apt-get update && apt-get install -y musl-tools

# Install the MUSL target for Rust
RUN rustup target add x86_64-unknown-linux-musl

# Cache deps using the MUSL target
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main(){}" > src/main.rs \
 && RUSTFLAGS='-C target-feature=-avx,-avx2,-sse4.1,-sse4.2' cargo build --release --target x86_64-unknown-linux-musl \
 && rm -rf src

# Now copy your actual source and build for MUSL
COPY src ./src
RUN RUSTFLAGS='-C target-feature=-avx,-avx2,-sse4.1,-sse4.2' cargo build --release --target x86_64-unknown-linux-musl

# -------- FINAL Distroless runtime stage --------
FROM gcr.io/distroless/static-debian11

# Copy the static binary from the builder stage into the root
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/hyperliquid_tracker /app

EXPOSE 8080

# The distroless image has a default non-root user 'nonroot' with UID 65532
USER 65532

# Set the entrypoint to our application using its full path
CMD ["/app"]