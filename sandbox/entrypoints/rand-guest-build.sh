#!/bin/sh
# release: build the shim exactly as published (hc reproducible).
# debug:   a copy with -g added after -Os, for the DWARF line table only.
set -eu
J=/circuits/jobs/job
cp -r /opt/cargo-registry "$CARGO_HOME/registry"
elf_of() { find "$1/target/riscv32im-unknown-none-elf/release" -maxdepth 1 -type f -perm -u+x | head -1; }
case "$1" in
  release)
    rand-guest build "$J/shim" --max-words 65535 --out "$J/image.bin" ;;
  debug)
    rm -rf "$J/shim-dbg" && cp -r "$J/shim" "$J/shim-dbg" && rm -rf "$J/shim-dbg/target"
    sed -i -e 's/\.flag("-Os")/.flag("-Os").flag("-g")/' -e 's/"-Os",/"-Os", "-g",/' "$J/shim-dbg/build.rs"
    grep -q '"-g"' "$J/shim-dbg/build.rs" || { echo "could not add -g to build.rs" >&2; exit 66; }
    rand-guest build "$J/shim-dbg" --max-words 65535 --out "$J/image-dbg.bin"
    /opt/llvm/bin/llvm-dwarfdump --debug-line "$(elf_of "$J/shim-dbg")" > "$J/debug-line.txt" ;;
  *) echo "release|debug" >&2; exit 64 ;;
esac
