# Build stage
FROM rust:alpine3.20 AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev upx

# Set the working directory
WORKDIR /app

# Copy the project files
COPY . .

# Build the project
RUN cargo build --release
RUN upx /app/target/release/devicecheck

# Runtime stage
FROM alpine:3.16

# Copy the built binary from the builder stage
COPY --from=builder /app/target/release/devicecheck /bin/devicecheck

# Set the entrypoint
ENTRYPOINT ["/bin/devicecheck"]