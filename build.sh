#!/bin/bash
set -e
export PATH="$HOME/.cargo/bin:$PATH"

cargo build --release
~/.rustup/toolchains/nightly-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/bin/llvm-objcopy \
    -O binary target/i686-hello_os/release/hello-os target/kernel.bin
nasm -f bin boot/boot.asm -o target/boot.bin
cat target/boot.bin target/kernel.bin > target/os.img
dd if=/dev/zero bs=1 count=$((16384 - $(stat -f%z target/os.img))) >> target/os.img 2>/dev/null

echo "Done! Run: qemu-system-i386 -fda target/os.img"
