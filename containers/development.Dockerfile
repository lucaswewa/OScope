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

USER root
COPY --from=uv /uv /uvx /usr/local/bin/
COPY rust-toolchain.toml /opt/oscope/rust-toolchain.toml
COPY --chmod=755 containers/check-environment.sh /usr/local/bin/oscope-check
COPY containers/gpu-probe.cu /opt/oscope/gpu-probe.cu
USER oscope
WORKDIR /workspace
CMD ["bash"]
