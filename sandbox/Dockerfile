# syntax=docker/dockerfile:1.7
# Rand Terminal's sandbox: every untrusted tool step runs here, without network.
# Build: sandbox/build.sh  (docker build --platform linux/amd64 -f sandbox/Dockerfile -t "$(sandbox/tag.sh)" .)
FROM debian:bookworm-slim
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
# translating and building the two reference programs, whose hc must equal the published ones
# (constraint set 11: evm2rv/README.md and sbpf2rv/tests/parity.rs at 41d64acd).
COPY circuits /circuits
# rand-guest and evm2rv build --locked: their lockfiles are complete at this pin (41d64acd). sbpf2rv's
# is not: it omits three edges of the rand_zkvm path crate (chacha20, rand_core, dilithium-core).
# Cargo adds those and changes no version; the hc pins below catch any drift.
RUN cd /circuits/rand-guest && cargo build --release --locked && install -m 0755 target/release/rand-guest /usr/local/bin/rand-guest \
 && cd /circuits/evm2rv && cargo build --release --locked && cd /circuits/sbpf2rv && cargo build --release
# The shim builds use their own CARGO_HOME so only their registry ships to run time.
# --allow-panic: both shims link core's panic machinery through the ABI harness, which rand-guest
# rejects since ZKVM-106 (a panic halts with out[7] = 0xdeadbeef, which a run's outputs show).
RUN set -e; export CARGO_HOME=/opt/shim-cargo; mkdir -p /circuits/jobs/warm; \
    /circuits/evm2rv/target/release/evm2rv /circuits/guests-compiled/evm/contracts/erc20.runtime.hex --out /circuits/jobs/warm/erc20 --chain-id 13; \
    rand-guest build /circuits/jobs/warm/erc20 --max-words 65535 --allow-panic | tee /tmp/erc20.txt; \
    grep -q "hc fd8dd3afdd785a991993c0f17dcca9a7c2c4b4c3bf213e2354d9aa1ffbf6e400" /tmp/erc20.txt; \
    /circuits/sbpf2rv/target/release/sbpf2rv /circuits/guests-compiled/sbpf/programs/spl_token.so --out /circuits/jobs/warm/spl --name spl-token; \
    rand-guest build /circuits/jobs/warm/spl --max-words 65535 --allow-panic | tee /tmp/spl.txt; \
    grep -q "hc 77919dab5ec22e58729064e0711fbf57dc040785eb0612a73e6ad0c4370a42b7" /tmp/spl.txt; \
    rm -rf /circuits/jobs/warm /circuits/*/target; \
    cp -r /opt/shim-cargo/registry /opt/shim-cargo-registry

COPY sandbox/entrypoints /opt/entrypoints
RUN chmod 0755 /opt/entrypoints/*.sh && mkdir -p /circuits/jobs/job && chown 10001:10001 /circuits/jobs/job \
 && /usr/sbin/useradd -u 10001 -M -d /tmp/home sandbox && chmod -R a+rX /opt /circuits
USER 10001:10001
WORKDIR /tmp
ENTRYPOINT ["/opt/entrypoints/dispatch.sh"]
