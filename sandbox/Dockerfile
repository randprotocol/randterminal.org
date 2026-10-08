# syntax=docker/dockerfile:1.7
# Rand Terminal's sandbox: every untrusted tool step runs here, without network.
# Build: sandbox/build.sh  (docker build --platform linux/amd64 -f sandbox/Dockerfile -t terminal-sandbox:<rev> .)
FROM --platform=linux/amd64 debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl xz-utils bzip2 build-essential pkg-config libssl-dev libudev-dev git \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:/opt/solana-release/bin:/opt/llvm/bin:/usr/local/bin:/usr/bin:/bin
RUN curl --proto '=https' -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.98.1 \
    && rustup target add riscv32im-unknown-none-elf --toolchain 1.98.1

# clang 23.1.1: the hc of every image is a function of this compiler.
RUN curl -sSfL https://github.com/llvm/llvm-project/releases/download/llvmorg-23.1.1/LLVM-23.1.1-Linux-X64.tar.xz \
      | tar -xJ -C /opt && ln -s /opt/LLVM-23.1.1-Linux-X64 /opt/llvm
ENV CLANG=/opt/llvm/bin/clang

# solc 0.8.37, pinned by hash (the same pin as fullnode's ci.yml).
RUN curl -sSfL -o /usr/local/bin/solc https://github.com/ethereum/solidity/releases/download/v0.8.37/solc-static-linux \
    && echo "5de843c2c93563cc66425c99a4fb13fdbf32b4c4ae07469480faaf126e14404a  /usr/local/bin/solc" | sha256sum -c \
    && chmod 0755 /usr/local/bin/solc

# cargo-build-sbf from agave v4.3.0, platform-tools v1.54 fetched once into /opt/sbf-home.
RUN curl -sSfL https://github.com/anza-xyz/agave/releases/download/v4.3.0/solana-release-x86_64-unknown-linux-gnu.tar.bz2 \
      | tar -xj -C /opt
COPY sandbox/solana-template /opt/solana-template
RUN cd /opt/solana-template && HOME=/opt/sbf-home cargo vendor /opt/solana-vendor > /tmp/vendor.toml \
    && mkdir -p .cargo && cp /tmp/vendor.toml .cargo/config.toml \
    && HOME=/opt/sbf-home cargo-build-sbf --tools-version v1.54 --offline --sbf-out-dir /tmp/warm \
    && test -f /tmp/warm/program.so && rm -rf /tmp/warm target

# circuits at the pinned revision; rand-guest built once; the shim crates' registry warmed by
# translating and building the two reference programs, whose hc must equal the published ones.
COPY circuits /circuits
RUN cd /circuits/rand-guest && cargo build --release --locked && install -m 0755 target/release/rand-guest /usr/local/bin/rand-guest \
 && cd /circuits/evm2rv && cargo build --release --locked && cd /circuits/sbpf2rv && cargo build --release --locked
RUN set -e; mkdir -p /circuits/jobs/warm; \
    /circuits/evm2rv/target/release/evm2rv /circuits/guests-compiled/evm/contracts/erc20.runtime.hex --out /circuits/jobs/warm/erc20 --chain-id 13; \
    rand-guest build /circuits/jobs/warm/erc20 --max-words 65535 | tee /tmp/erc20.txt; \
    grep -q "hc a0feae92a7311c9562495717100eb7aea71270a38ed435d06dc31387e0ea8ff6" /tmp/erc20.txt; \
    /circuits/sbpf2rv/target/release/sbpf2rv /circuits/guests-compiled/sbpf/programs/spl_token.so --out /circuits/jobs/warm/spl --name spl-token; \
    rand-guest build /circuits/jobs/warm/spl --max-words 65535 | tee /tmp/spl.txt; \
    grep -q "hc 8ca905ae3c62f503de7de86829040f323e098b53dba5fffe09b42f1aad16758b" /tmp/spl.txt; \
    rm -rf /circuits/jobs/warm /circuits/*/target; \
    cp -r /opt/cargo/registry /opt/cargo-registry

COPY sandbox/entrypoints /opt/entrypoints
RUN chmod 0755 /opt/entrypoints/*.sh && mkdir -p /circuits/jobs/job && chown 10001:10001 /circuits/jobs/job \
 && useradd -u 10001 -M -d /tmp/home sandbox && chmod -R a+rX /opt /circuits
USER 10001:10001
WORKDIR /tmp
ENTRYPOINT ["/opt/entrypoints/dispatch.sh"]
