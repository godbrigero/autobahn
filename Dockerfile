# Build stage
FROM rust:1.81-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Create a new empty shell project
WORKDIR /usr/src/autobahn

# Copy the source code
COPY . .

# Build the project
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -m -u 1000 autobahn

# Create app directory
WORKDIR /app

# Copy the binary from builder
COPY --from=builder /usr/src/autobahn/target/release/autobahn /app/

# Copy configuration files
COPY config.toml /app/

# Set proper permissions
RUN chown -R autobahn:autobahn /app

# Switch to non-root user
USER autobahn

# Expose the port (adjust if your application uses a different port)
EXPOSE 8080

# Run the application
CMD ["./autobahn"] 