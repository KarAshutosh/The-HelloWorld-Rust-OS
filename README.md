# Hello World OS

> An entire operating system, just to say "Hello World". Because why not?

A bare-metal OS written in Rust with a custom NASM bootloader. No libc. No std. Just pure systems programming.

## What It Does

1. BIOS loads bootloader from sector 1
2. Bootloader switches CPU to 32-bit protected mode
3. Bootloader loads Rust kernel from disk
4. Kernel writes to VGA text buffer (0xB8000)
5. Infinite loop

## Requirements

```bash
brew install nasm qemu
rustup default nightly
```

## Build & Run

```bash
./build.sh
qemu-system-i386 -fda target/os.img
```

## Project Structure

```
├── boot/boot.asm      # Bootloader (512 bytes)
├── src/main.rs        # Rust kernel
├── i686-hello_os.json # Custom target
├── linker.ld          # Linker script
└── build.sh           # Build script
```

## Why It's Good™

- `write_volatile` for memory-mapped I/O
- `#[repr(C)]` for predictable layout
- Spinlock for thread-safe VGA writer
- No red-zone, no SIMD
- Proper panic handler

## License

MIT
