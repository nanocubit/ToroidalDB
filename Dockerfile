# Multi-stage build for ToroidalDB
FROM rust:1.77-slim as builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/toroidal

# Copy source code
COPY . .

# Build the application
RUN cargo build --release

# Production stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    curl \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -r -s /bin/false toroidal

# Copy the binary from builder stage
COPY --from=builder /usr/src/toroidal/target/release/toroidal-db /usr/local/bin/toroidal-db

# Create data directory
RUN mkdir -p /data && chown toroidal:toroidal /data

# Copy static files
COPY static/ /static/

# Switch to non-root user
USER toroidal

# Expose ports
EXPOSE 5432 8443 9090

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:8443/health || exit 1

# Set environment variables
ENV TOROIDAL_DATA_DIR=/data
ENV RUST_LOG=info

# Volumes
VOLUME ["/data"]

# Run the application
CMD ["toroidal-db"]