# syntax=docker/dockerfile:1
# Linux amd64 manifest verified against NVIDIA's registry for R01.
ARG CUDA_IMAGE=nvidia/cuda:13.4.1-devel-ubuntu24.04@sha256:f0eec85d23f7fdff7b6d74feb1d63e1e5921b0a92c3adfd9847875f830eb0382
ARG UV_IMAGE=ghcr.io/astral-sh/uv:0.13.6@sha256:8b7774a14bb3abde914fda03efcb83f1f7e9163124e5c13e7fb4cc825293f5fa
FROM ${UV_IMAGE} AS uv
FROM ${CUDA_IMAGE} AS development

ARG RUST_VERSION=1.98.1
ARG DEBIAN_FRONTEND=noninteractive
SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential ca-certificates curl git pkg-config libssl-dev libudev-dev \
    python3 python3-venv \
    && rm -rf /var/lib/apt/lists/*

# Ubuntu images may already have UID 1000. Reuse it without creating duplicate
# identities. Source bind mounts normally belong to the WSL developer at UID 1000.
RUN if getent passwd 1000 >/dev/null; then \
        existing=$(getent passwd 1000 | cut -d: -f1); \
        usermod --login oscope --home /home/oscope "$existing"; \
    else useradd --create-home --uid 1000 --user-group oscope; fi \
    && mkdir -p /home/oscope/.cargo/registry /home/oscope/.cargo/git \
        /home/oscope/.cache/uv /workspace/target /opt/oscope \
    && chown -R 1000:1000 /home/oscope /workspace

ENV HOME=/home/oscope \
    CARGO_HOME=/home/oscope/.cargo \
    RUSTUP_HOME=/home/oscope/.rustup \
    UV_CACHE_DIR=/home/oscope/.cache/uv \
    UV_PYTHON_DOWNLOADS=never \
    CARGO_TARGET_DIR=/workspace/target \
    PATH=/home/oscope/.cargo/bin:/home/oscope/.local/bin:/usr/local/cuda/bin:${PATH}

USER oscope
RUN curl --proto '=https' --tlsv1.2 -fsSL \
        https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init \
        -o /tmp/rustup-init \
    && curl --proto '=https' --tlsv1.2 -fsSL \
        https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init.sha256 \
        -o /tmp/rustup-init.sha256 \
    && (cd /tmp && sha256sum --check rustup-init.sha256) \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal \
        --default-toolchain "${RUST_VERSION}" --component clippy --component rustfmt

RUN cargo install cargo-deny --version 0.20.2 --locked

USER root
COPY --from=uv /uv /uvx /usr/local/bin/
COPY rust-toolchain.toml /opt/oscope/rust-toolchain.toml
COPY --chmod=755 containers/check-environment.sh /usr/local/bin/oscope-check
COPY containers/gpu-probe.cu /opt/oscope/gpu-probe.cu
USER oscope
WORKDIR /workspace
CMD ["bash"]

FROM development AS backend-build
ARG SOURCE_REVISION=unknown
COPY --chown=1000:1000 . /workspace
RUN --mount=type=cache,id=oscope-runtime-registry,target=/home/oscope/.cargo/registry,uid=1000,gid=1000 \
    --mount=type=cache,id=oscope-runtime-git,target=/home/oscope/.cargo/git,uid=1000,gid=1000 \
    --mount=type=cache,id=oscope-runtime-target,target=/workspace/target,uid=1000,gid=1000 \
    if [[ "$SOURCE_REVISION" != unknown && ! "$SOURCE_REVISION" =~ ^[0-9a-f]{40}$ ]]; then exit 2; fi \
    && OSCOPE_BUILD_COMMIT="${SOURCE_REVISION:0:12}" CARGO_BUILD_JOBS=2 \
       cargo build --release --locked -p oscope-server -p ex-cuda-check \
    && mkdir -p /tmp/oscope-build \
    && cp target/x86_64-unknown-linux-gnu/release/oscope-server /tmp/oscope-build/ \
    && cp target/x86_64-unknown-linux-gnu/release/ex-cuda-check /tmp/oscope-build/oscope-cuda-check

FROM nvidia/cuda:13.4.1-runtime-ubuntu24.04@sha256:95e471b8be1ac7b962e6e6d6433bcfb15c3e93ef85253702f15c998565778187 AS runtime
ARG DEBIAN_FRONTEND=noninteractive
ARG SOURCE_REVISION=unknown
LABEL org.opencontainers.image.title="OScope Linux backend" \
      org.opencontainers.image.source="https://github.com/lucaswewa/OScope" \
      org.opencontainers.image.revision="${SOURCE_REVISION}"
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates python3 \
    && rm -rf /var/lib/apt/lists/* \
    && if getent passwd 1000 >/dev/null; then \
         existing=$(getent passwd 1000 | cut -d: -f1); \
         usermod --login oscope --home /home/oscope "$existing"; \
       else useradd --create-home --uid 1000 --user-group oscope; fi \
    && mkdir -p /home/oscope /var/lib/oscope/data /var/lib/oscope/scratch /etc/oscope /opt/oscope/sdk /usr/share/doc/oscope \
    && chown -R 1000:1000 /home/oscope /var/lib/oscope
COPY --from=uv /uv /usr/local/bin/uv
COPY --from=backend-build /tmp/oscope-build/ /usr/local/bin/
COPY python/oscope-ext/ /opt/oscope/sdk/
COPY LICENSE /usr/share/doc/oscope/LICENSE
COPY configs/runtime.toml /etc/oscope/oscope.toml
COPY --chmod=755 containers/runtime-entrypoint.sh /usr/local/bin/oscope-entrypoint
COPY --chmod=755 containers/healthcheck.py /usr/local/bin/oscope-healthcheck
ENV HOME=/home/oscope \
    OSCOPE_CONFIG=/var/lib/oscope/data/config/oscope.toml \
    UV_CACHE_DIR=/var/lib/oscope/data/cache/uv \
    UV_PYTHON_DOWNLOADS=never \
    CUDA_CACHE_PATH=/tmp/cuda-cache \
    NVIDIA_DRIVER_CAPABILITIES=compute,utility
USER 1000:1000
WORKDIR /var/lib/oscope
EXPOSE 8210
HEALTHCHECK --interval=5s --timeout=4s --start-period=30s --retries=6 CMD ["/usr/local/bin/oscope-healthcheck"]
ENTRYPOINT ["/usr/local/bin/oscope-entrypoint"]
CMD []
