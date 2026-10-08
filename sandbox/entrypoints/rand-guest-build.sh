#!/bin/sh
# release: build the shim exactly as published (hc reproducible).
# debug:   a copy with -g added after -Os, for the DWARF line table only.
set -eu
J=/circuits/jobs/job
mkdir -p "$CARGO_HOME/registry"
cp -r /opt/shim-cargo-registry/index /opt/shim-cargo-registry/cache "$CARGO_HOME/registry/"
elf_of() { find "$1/target/riscv32im-unknown-none-elf/release" -maxdepth 1 -type f -perm -u+x | head -1; }
case "$1" in
  release)
    rand-guest build "$J/shim" --max-words 65535 --out "$J/image.bin" ;;
  debug)
    rm -rf "$J/shim-dbg" && mkdir "$J/shim-dbg"
    (cd "$J/shim" && tar -cf - --exclude=./target .) | (cd "$J/shim-dbg" && tar -xf -)
    sed -i -e 's/\.flag("-Os")/.flag("-Os").flag("-g")/' -e 's/b\.flag(format!("-ffile-prefix-map/b.flag("-g"); &/' "$J/shim-dbg/build.rs"
    # cargo's release profile strips debuginfo by default, which would drop the C objects' line table.
    awk '{print} /^\[profile\.release\]/{print "strip = \"none\""}' "$J/shim-dbg/Cargo.toml" > "$J/shim-dbg/Cargo.toml.new" && mv "$J/shim-dbg/Cargo.toml.new" "$J/shim-dbg/Cargo.toml"
    grep -q '^strip = "none"' "$J/shim-dbg/Cargo.toml" || { echo "could not keep debuginfo" >&2; exit 67; }
    grep -q '"-g"' "$J/shim-dbg/build.rs" || { echo "could not add -g to build.rs" >&2; exit 66; }
    rand-guest build "$J/shim-dbg" --max-words 65535 --out "$J/image-dbg.bin"
    /opt/llvm/bin/llvm-dwarfdump --debug-line "$(elf_of "$J/shim-dbg")" > "$J/debug-line.txt" ;;
  *) echo "release|debug" >&2; exit 64 ;;
esac
