# syntax=docker/dockerfile:1.12

FROM rust:1.98-bookworm AS builder
WORKDIR /source
ENV CARGO_BUILD_JOBS=2
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY src ./src
COPY prompts ./prompts
RUN cargo build --locked --release --bin minimal-agent

FROM runtime-base
ARG VERSION=0.110.0
LABEL org.opencontainers.image.title="minimal-agent" \
      org.opencontainers.image.description="A small autonomous team-agent runtime powered by Rust" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.source="https://github.com/agnusdei1207/minimal-agent" \
      org.opencontainers.image.licenses="MIT"
COPY --from=builder /source/target/release/minimal-agent /usr/local/bin/minimal-agent
# Attack-methodology library the agent consults autonomously at runtime (ADR-0003).
# Shipped as on-disk files (not baked into the prompt) so agents browse only the
# relevant card, keeping the per-turn prompt small. World-readable for USER 10001.
COPY prompts/skills /opt/minimal-agent/skills
USER 10001:10001
WORKDIR /workspace
VOLUME ["/state"]
ENTRYPOINT ["minimal-agent"]
CMD ["--help"]
