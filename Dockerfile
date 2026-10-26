# Debian 12 provides GCC 12 / Boost 1.74. Legacy DSP SDKs are checksum-pinned.
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential ca-certificates ccache clang cmake curl ffmpeg git \
        hyperfine libbenchmark-dev libboost-filesystem-dev libboost-iostreams-dev \
        libboost-math-dev libboost-serialization-dev libboost-system-dev \
        libpng-dev libsndfile1-dev ninja-build pkg-config python3 \
        xz-utils zlib1g-dev gdb jq procps ripgrep tmux \
        libx11-dev libx11-xcb-dev libxkbcommon-dev libxkbcommon-x11-dev \
        libwayland-dev xvfb xauth python3-xlib python3-venv lld \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

# Keep Cargo artifacts on the container filesystem, off the host bind mount.
ENV CARGO_TARGET_DIR=/root/.cache/phaselimiter-target

RUN curl -fsSL https://chatgpt.com/codex/install.sh | sh

# Rust GUI development. No macOS SDK is downloaded; provide your own SDKROOT
# when cross-building. The backend's DSP dependencies are separate.
RUN curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o /tmp/rustup-init.sh \
    && sh /tmp/rustup-init.sh -y --profile minimal --default-toolchain 1.99.1 \
    && rm /tmp/rustup-init.sh
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup component add rustfmt clippy
RUN ln -s /usr/bin/clang /usr/local/bin/clang-cl \
    && python3 -m venv /opt/phaselimiter-cross \
    && /opt/phaselimiter-cross/bin/pip install cargo-xwin==0.23.1
ENV PATH="/opt/phaselimiter-cross/bin:${PATH}"
