# syntax=docker/dockerfile:1.12

FROM rust:1.98-bookworm AS builder
WORKDIR /source
ENV CARGO_BUILD_JOBS=2
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
COPY prompts ./prompts
RUN cargo build --locked --release --bin pentesting

# CRITICAL REPOSITORY INVARIANT:
# Do not add package manager layers (apt or apt-get) to app.Dockerfile.
# All system tools and runtime libraries belong exclusively in runtime-base.Dockerfile.
FROM agnusdei1207/pentesting-runtime-base:latest AS runtime-base
FROM runtime-base
ARG VERSION=0.200.1
LABEL org.opencontainers.image.title="pentesting" \
      org.opencontainers.image.description="A small autonomous team-agent runtime powered by Rust" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.source="https://github.com/agnusdei1207/pentesting" \
      org.opencontainers.image.licenses="MIT"
COPY --from=builder /source/target/release/pentesting /usr/local/bin/pentesting
# Attack-methodology library the agent consults autonomously at runtime (INTENT-0003 비범위: doctrine·이미지 자산 계약).
# Shipped as on-disk files (not baked into the prompt) so agents browse only the
# relevant card, keeping the per-turn prompt small. World-readable for USER 10001.
COPY prompts/skills /opt/pentesting/skills
USER 10001:10001
WORKDIR /workspace
VOLUME ["/state"]
ENTRYPOINT ["pentesting"]
CMD ["--help"]

