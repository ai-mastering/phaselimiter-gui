# Debian 12 provides GCC 12 / Boost 1.74. Legacy DSP SDKs are checksum-pinned.
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential ca-certificates ccache clang cmake curl ffmpeg git \
        hyperfine libbenchmark-dev libboost-filesystem-dev libboost-iostreams-dev \
        libboost-math-dev libboost-serialization-dev libboost-system-dev \
        libpng-dev libsndfile1-dev ninja-build pkg-config python3 \
        xz-utils zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

RUN apt-get update \
    && apt-get install -y --no-install-recommends gdb jq procps ripgrep tmux \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://chatgpt.com/codex/install.sh | sh
