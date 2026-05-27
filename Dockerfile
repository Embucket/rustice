# Multi-stage Dockerfile optimized for caching and minimal final image size
FROM rust:bookworm AS builder

WORKDIR /app

# Install required system dependencies
RUN apt-get update && apt-get install -y \
    cmake \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy source code
COPY . .

# Build a glibc-static release binary so the runtime image does not need libc packages.
# This target-specific setting avoids applying crt-static to host proc-macro crates.
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C target-feature=+crt-static"

# Build the application, optionally enabling experimental features
ARG ENABLE_EXPERIMENTAL=false
RUN mkdir -p /app/runtime-data && \
    if [ "$ENABLE_EXPERIMENTAL" = "true" ]; then \
        echo "Building embucketd with experimental features enabled"; \
        cargo build --release --target x86_64-unknown-linux-gnu --bin embucketd --features experimental; \
    else \
        echo "Building embucketd without experimental features"; \
        cargo build --release --target x86_64-unknown-linux-gnu --bin embucketd --no-default-features; \
    fi

# Stage 4: Final runtime image
FROM gcr.io/distroless/static-debian13:nonroot AS runtime

# Set working directory
WORKDIR /app

# Copy the binary and required files
COPY --from=builder /app/target/x86_64-unknown-linux-gnu/release/embucketd ./embucketd
COPY --from=builder /app/rest-catalog-open-api.yaml ./rest-catalog-open-api.yaml
COPY --chown=65532:65532 --from=builder /app/runtime-data ./data

# Expose port (adjust as needed)
EXPOSE 8080
EXPOSE 3000

ENV OBJECT_STORE_BACKEND=file
ENV FILE_STORAGE_PATH=data/
ENV BUCKET_HOST=0.0.0.0
ENV JWT_SECRET=63f4945d921d599f27ae4fdf5bada3f1

# Default command
CMD ["./embucketd"]
