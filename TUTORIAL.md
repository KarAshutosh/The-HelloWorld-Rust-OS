# Building a "Hello World" Operating System in Rust: A Complete Guide

## Table of Contents
1. [Introduction](#introduction)
2. [Prerequisites & Concepts](#prerequisites--concepts)
3. [The Boot Process](#the-boot-process)
4. [The Bootloader](#the-bootloader)
5. [Protected Mode](#protected-mode)
6. [The Rust Kernel](#the-rust-kernel)
7. [VGA Text Mode](#vga-text-mode)
8. [Build System](#build-system)
9. [Debugging Journey](#debugging-journey)
10. [Lessons Learned](#lessons-learned)

---

## Introduction

This document teaches you how to build a bare-metal operating system that displays "Hello World" - from scratch. No libraries, no standard library, no libc. Just you, the CPU, and raw memory.

**What you'll learn:**
- How computers boot
- x86 real mode vs protected mode
- Writing a bootloader in assembly
- Bare-metal Rust programming
- Memory-mapped I/O (VGA text buffer)
- Cross-compilation and custom targets
- Debugging OS development issues

**Final result:** A ~16KB bootable disk image that displays ASCII art.

---

## Prerequisites & Concepts

### Hardware We're Targeting
- **Architecture:** i686 (32-bit x86)
- **Display:** VGA text mode (80x25 characters)
- **Boot method:** BIOS (not UEFI)

### Why 32-bit?
64-bit (x86_64) requires more setup:
- Long mode transition
- Paging must be enabled
- More complex GDT

32-bit protected mode is simpler for learning.

### Tools Required
```bash
# macOS
brew install nasm qemu
rustup default nightly
rustup component add rust-src llvm-tools-preview
```

- **NASM:** Assembler for the bootloader
- **QEMU:** x86 emulator for testing
- **Rust nightly:** Required for `#![no_std]` and build-std
- **rust-src:** Source code to compile core library
- **llvm-tools:** For `llvm-objcopy` to extract raw binary

---

## The Boot Process

When you press the power button, here's what happens:

```
┌─────────────────────────────────────────────────────────────────┐
│ 1. CPU starts in 16-bit "real mode"                             │
│    - Can only address 1MB of memory                             │
│    - No memory protection                                       │
│    - Instruction pointer starts at 0xFFFF0 (BIOS ROM)           │
├─────────────────────────────────────────────────────────────────┤
│ 2. BIOS runs POST (Power-On Self Test)                          │
│    - Checks hardware                                            │
│    - Initializes devices                                        │
├─────────────────────────────────────────────────────────────────┤
│ 3. BIOS loads boot sector                                       │
│    - Reads first 512 bytes from disk                            │
│    - Loads it to memory address 0x7C00                          │
│    - Checks for boot signature (0x55AA at bytes 510-511)        │
│    - Jumps to 0x7C00                                            │
├─────────────────────────────────────────────────────────────────┤
│ 4. Our bootloader runs (boot.asm)                               │
│    - Still in 16-bit real mode                                  │
│    - Loads kernel from disk to 0x1000                           │
│    - Switches to 32-bit protected mode                          │
│    - Jumps to kernel                                            │
├─────────────────────────────────────────────────────────────────┤
│ 5. Our kernel runs (main.rs)                                    │
│    - Now in 32-bit protected mode                               │
│    - Writes to VGA buffer at 0xB8000                            │
│    - Loops forever                                              │
└─────────────────────────────────────────────────────────────────┘
```

### Memory Map at Boot

```
0x00000000 ┌─────────────────────┐
           │ Interrupt Vector    │
           │ Table (IVT)         │
0x00000400 ├─────────────────────┤
           │ BIOS Data Area      │
0x00000500 ├─────────────────────┤
           │ Free memory         │
0x00001000 ├─────────────────────┤ ← We load kernel here
           │ Our Kernel          │
           │                     │
0x00007C00 ├─────────────────────┤ ← BIOS loads bootloader here
           │ Our Bootloader      │
0x00007E00 ├─────────────────────┤
           │ Free memory         │
0x0009FC00 ├─────────────────────┤
           │ Extended BIOS Data  │
0x000A0000 ├─────────────────────┤
           │ Video Memory        │
0x000B8000 │ ← VGA Text Buffer   │
0x000C0000 ├─────────────────────┤
           │ BIOS ROM            │
0x00100000 └─────────────────────┘ ← 1MB limit in real mode
```

---

## The Bootloader

### Full Annotated Code

```asm
; boot.asm - Bootloader: real mode -> protected mode -> kernel

[bits 16]           ; Tell assembler we're writing 16-bit code
[org 0x7c00]        ; Tell assembler our code will be loaded at 0x7C00
                    ; This affects how labels are calculated

KERNEL_OFFSET equ 0x1000    ; Where we'll load the kernel in memory

start:
    ; ─────────────────────────────────────────────────────────────
    ; STEP 1: Set up segment registers
    ; ─────────────────────────────────────────────────────────────
    ; In real mode, memory address = (segment * 16) + offset
    ; We want segment:offset to just equal offset, so set segments to 0
    
    xor ax, ax      ; ax = 0 (faster than mov ax, 0)
    mov ds, ax      ; Data segment = 0
    mov es, ax      ; Extra segment = 0
    mov ss, ax      ; Stack segment = 0
    mov sp, 0x7c00  ; Stack pointer = 0x7C00 (grows downward)
                    ; Stack is below bootloader, won't overwrite it
    
    mov [BOOT_DRIVE], dl    ; BIOS passes boot drive number in DL
                            ; Save it - we need it to read more sectors

    ; ─────────────────────────────────────────────────────────────
    ; STEP 2: Load kernel from disk using BIOS interrupt
    ; ─────────────────────────────────────────────────────────────
    ; INT 0x13, AH=0x02: Read sectors from disk
    ;
    ; Parameters:
    ;   AH = 0x02 (read sectors function)
    ;   AL = number of sectors to read
    ;   CH = cylinder number (0-indexed)
    ;   CL = sector number (1-indexed!) 
    ;   DH = head number
    ;   DL = drive number
    ;   ES:BX = destination address
    ;
    ; Returns:
    ;   CF = set on error
    ;   AL = number of sectors actually read
    
    mov bx, KERNEL_OFFSET   ; ES:BX = 0x0000:0x1000 = 0x1000
    mov ah, 0x02            ; BIOS read sectors function
    mov al, 32              ; Read 32 sectors (16KB)
    mov ch, 0               ; Cylinder 0
    mov dh, 0               ; Head 0
    mov cl, 2               ; Start at sector 2 (sector 1 is boot sector)
    mov dl, [BOOT_DRIVE]    ; Drive number
    int 0x13                ; Call BIOS
    jc $                    ; If carry flag set (error), hang forever
                            ; '$' means current address - infinite loop

    ; ─────────────────────────────────────────────────────────────
    ; STEP 3: Enter protected mode
    ; ─────────────────────────────────────────────────────────────
    
    cli                     ; Disable interrupts
                            ; We're changing CPU mode - interrupts would crash
    
    lgdt [gdt_descriptor]   ; Load Global Descriptor Table
                            ; This defines memory segments for protected mode
    
    mov eax, cr0            ; CR0 is a control register
    or eax, 1               ; Set bit 0 (Protection Enable)
    mov cr0, eax            ; Write back - NOW WE'RE IN PROTECTED MODE
                            ; But we're still running 16-bit code!
    
    jmp 0x08:protected_mode ; Far jump to 32-bit code
                            ; 0x08 = offset of code segment in GDT
                            ; This jump:
                            ;   1. Flushes the CPU pipeline
                            ;   2. Loads CS with our code segment selector
                            ;   3. Starts executing 32-bit code

BOOT_DRIVE: db 0            ; Storage for boot drive number

; ─────────────────────────────────────────────────────────────────
; Global Descriptor Table (GDT)
; ─────────────────────────────────────────────────────────────────
; In protected mode, segments aren't just multiplied by 16.
; Instead, segment registers hold "selectors" that index into the GDT.
; Each GDT entry describes a memory segment's base, limit, and permissions.

gdt_start:
    dq 0                    ; Null descriptor (required, index 0)
                            ; CPU will fault if you try to use it

gdt_code:                   ; Code segment descriptor (index 0x08)
    ; This describes a segment for executable code
    dw 0xffff               ; Limit bits 0-15 (max = 0xFFFFF with granularity)
    dw 0                    ; Base bits 0-15
    db 0                    ; Base bits 16-23
    db 10011010b            ; Access byte:
                            ;   1 = present (segment is valid)
                            ;   00 = ring 0 (kernel privilege)
                            ;   1 = code/data segment (not system)
                            ;   1 = executable (code segment)
                            ;   0 = not conforming
                            ;   1 = readable
                            ;   0 = not accessed
    db 11001111b            ; Flags + Limit bits 16-19:
                            ;   1 = granularity (limit in 4KB pages)
                            ;   1 = 32-bit segment
                            ;   0 = not 64-bit
                            ;   0 = reserved
                            ;   1111 = limit bits 16-19
    db 0                    ; Base bits 24-31

gdt_data:                   ; Data segment descriptor (index 0x10)
    dw 0xffff
    dw 0
    db 0
    db 10010010b            ; Same as code but:
                            ;   0 = not executable (data segment)
                            ;   1 = writable
    db 11001111b
    db 0

gdt_end:

gdt_descriptor:
    dw gdt_end - gdt_start - 1  ; Size of GDT minus 1
    dd gdt_start                 ; Address of GDT

; ─────────────────────────────────────────────────────────────────
; 32-bit Protected Mode Code
; ─────────────────────────────────────────────────────────────────

[bits 32]                   ; Now we're writing 32-bit code

protected_mode:
    ; Set up segment registers with data segment selector
    mov ax, 0x10            ; 0x10 = offset of data segment in GDT
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov esp, 0x90000        ; Set up stack at 576KB
                            ; Plenty of room, won't hit kernel

    jmp KERNEL_OFFSET       ; Jump to our Rust kernel at 0x1000!

; ─────────────────────────────────────────────────────────────────
; Boot Sector Padding and Signature
; ─────────────────────────────────────────────────────────────────

times 510 - ($ - $$) db 0   ; Pad with zeros to byte 510
                            ; $ = current position
                            ; $$ = start of section
dw 0xaa55                   ; Boot signature (little-endian: 0x55 0xAA)
                            ; BIOS checks for this to identify bootable disk
```

### Key Concepts Explained

#### Why `[org 0x7c00]`?
The assembler needs to know where the code will be loaded to calculate label addresses correctly. Without this, `jmp` and `call` instructions would jump to wrong addresses.

#### Why save the boot drive?
BIOS passes the boot drive number in `DL`. We need this to read more sectors. If we don't save it, it might get overwritten.

#### Why `jmp 0x08:protected_mode`?
This is a "far jump" that:
1. Changes the code segment register (CS) to 0x08 (our code segment)
2. Flushes the CPU's instruction pipeline (which still has 16-bit instructions)
3. Starts fetching 32-bit instructions

---

## Protected Mode

### Real Mode vs Protected Mode

| Feature | Real Mode | Protected Mode |
|---------|-----------|----------------|
| Address size | 20-bit (1MB) | 32-bit (4GB) |
| Memory protection | None | Yes |
| Privilege levels | None | 4 rings (0-3) |
| Segment meaning | Base address | GDT selector |
| Interrupts | BIOS available | Must set up IDT |

### The Global Descriptor Table (GDT)

The GDT is an array of 8-byte entries describing memory segments:

```
┌─────────────────────────────────────────────────────────────────┐
│ Byte 7 │ Byte 6 │ Byte 5 │ Byte 4 │ Byte 3-2 │ Byte 1-0        │
├────────┼────────┼────────┼────────┼──────────┼─────────────────┤
│ Base   │ Flags  │ Access │ Base   │ Base     │ Limit           │
│ 31:24  │ Limit  │ Byte   │ 23:16  │ 15:0     │ 15:0            │
│        │ 19:16  │        │        │          │                 │
└─────────────────────────────────────────────────────────────────┘
```

We create a "flat" memory model where both code and data segments span all 4GB. This is the simplest setup and what most modern OSes use.



---

## The Rust Kernel

### Why Rust for OS Development?

1. **No garbage collector** - Predictable performance
2. **No runtime** - Can run on bare metal
3. **Memory safety** - Prevents many bugs at compile time
4. **Zero-cost abstractions** - High-level code, low-level performance
5. **`#![no_std]`** - Can compile without standard library

### The Challenge: No Standard Library

Normal Rust programs use `std`, which depends on an OS. We ARE the OS, so we can't use it. We use `#![no_std]` which gives us only `core` - the OS-independent parts of Rust.

**What we lose:**
- `println!` (needs OS for stdout)
- `Vec`, `String` (need heap allocator)
- `std::fs`, `std::net` (need OS)
- Panic unwinding (needs runtime)

**What we keep:**
- Primitive types (`u8`, `i32`, etc.)
- `Option`, `Result`
- Iterators
- `core::fmt` (formatting)
- `core::ptr` (raw pointers)

### Full Annotated Kernel Code

```rust
// ═══════════════════════════════════════════════════════════════════
// CRATE ATTRIBUTES
// ═══════════════════════════════════════════════════════════════════

#![no_std]      // Don't link the Rust standard library
#![no_main]     // Don't use the normal entry point (main)
                // We define our own: _start

// ═══════════════════════════════════════════════════════════════════
// IMPORTS
// ═══════════════════════════════════════════════════════════════════

use core::fmt::{self, Write};   // Formatting traits (no heap needed)
use core::panic::PanicInfo;     // Panic handler info
use spin::Mutex;                // Spinlock for thread safety
                                // (even though we're single-threaded,
                                // it's good practice and prevents
                                // compiler optimizations that could break things)

// ═══════════════════════════════════════════════════════════════════
// CONSTANTS
// ═══════════════════════════════════════════════════════════════════

const VGA: usize = 0xb8000;     // VGA text buffer physical address
                                // This is memory-mapped I/O:
                                // Writing here directly updates the screen
const W: usize = 80;            // Screen width in characters
const H: usize = 25;            // Screen height in characters

// ═══════════════════════════════════════════════════════════════════
// VGA COLOR DEFINITIONS
// ═══════════════════════════════════════════════════════════════════

#[repr(u8)]                     // Store as exactly 1 byte
#[derive(Clone, Copy)]          // Allow bitwise copying
enum Color {
    Black = 0,
    Green = 2,
    LightGreen = 10,
    White = 15,
}

// ═══════════════════════════════════════════════════════════════════
// VGA CHARACTER STRUCTURE
// ═══════════════════════════════════════════════════════════════════

#[repr(C)]                      // Use C memory layout (predictable)
#[derive(Clone, Copy)]          // Allow bitwise copying
struct Char {
    ascii: u8,                  // ASCII character code
    color: u8,                  // Color byte: high nibble = bg, low = fg
}

// VGA text buffer layout:
// Each character cell is 2 bytes:
// ┌─────────┬─────────┐
// │ Byte 0  │ Byte 1  │
// │ ASCII   │ Color   │
// │ char    │ attr    │
// └─────────┴─────────┘
//
// Color attribute byte:
// ┌───┬───┬───┬───┬───┬───┬───┬───┐
// │ 7 │ 6 │ 5 │ 4 │ 3 │ 2 │ 1 │ 0 │
// ├───┴───┴───┴───┼───┴───┴───┴───┤
// │  Background   │  Foreground   │
// └───────────────┴───────────────┘
// Bit 7: Blink (if enabled) or bright background

// ═══════════════════════════════════════════════════════════════════
// VGA WRITER
// ═══════════════════════════════════════════════════════════════════

struct Writer {
    col: usize,                 // Current column (0-79)
    row: usize,                 // Current row (0-24)
    color: u8,                  // Current color attribute
}

impl Writer {
    /// Create a new writer with default colors
    const fn new() -> Self {
        Self {
            col: 0,
            row: 0,
            // Color: light green on black
            color: (Color::Black as u8) << 4 | Color::LightGreen as u8,
        }
    }

    /// Write a character at a specific position
    /// 
    /// CRITICAL: We use write_volatile here!
    /// 
    /// Normal writes might be optimized away by the compiler:
    /// - "This memory is never read, skip the write"
    /// - "Combine multiple writes into one"
    /// 
    /// write_volatile tells the compiler:
    /// - "This write has side effects you don't know about"
    /// - "Do exactly what I say, don't optimize"
    fn put(&self, row: usize, col: usize, ch: Char) {
        unsafe {
            // Calculate address: base + (row * width + col) * 2 bytes
            let ptr = (VGA as *mut Char).add(row * W + col);
            
            // Volatile write - compiler won't optimize this away
            core::ptr::write_volatile(ptr, ch);
        }
    }

    /// Clear the entire screen
    fn clear(&mut self) {
        for r in 0..H {
            for c in 0..W {
                self.put(r, c, Char { ascii: b' ', color: self.color });
            }
        }
        self.col = 0;
        self.row = 0;
    }

    /// Write a single byte to the screen
    fn write_byte(&mut self, byte: u8) {
        // Handle newline
        if byte == b'\n' {
            self.col = 0;
            self.row += 1;
            return;
        }
        
        // Handle line wrap
        if self.col >= W {
            self.col = 0;
            self.row += 1;
        }
        
        // Only write if we're still on screen
        // (Simple version - no scrolling)
        if self.row < H {
            self.put(self.row, self.col, Char { 
                ascii: byte, 
                color: self.color 
            });
            self.col += 1;
        }
    }

    /// Change the current color
    fn set_color(&mut self, fg: Color, bg: Color) {
        self.color = (bg as u8) << 4 | fg as u8;
    }
}

// ═══════════════════════════════════════════════════════════════════
// IMPLEMENT fmt::Write TRAIT
// ═══════════════════════════════════════════════════════════════════
// This lets us use write!() and writeln!() macros with our Writer

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            // Only write printable ASCII or newline
            // Replace other bytes with a placeholder (■)
            self.write_byte(
                if matches!(b, 0x20..=0x7e | b'\n') { b } else { 0xfe }
            );
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════
// GLOBAL WRITER WITH SPINLOCK
// ═══════════════════════════════════════════════════════════════════

// Why a spinlock?
// 1. We need a global writer (no heap for dynamic allocation)
// 2. Global mutable state needs synchronization
// 3. Spinlock is simple and works without OS support
//
// The spinlock ensures only one "thread" can write at a time.
// Even though we're single-threaded, this:
// - Prevents compiler from making unsafe optimizations
// - Is correct if we ever add interrupts
// - Is good practice

static WRITER: Mutex<Writer> = Mutex::new(Writer::new());

// ═══════════════════════════════════════════════════════════════════
// PRINTLN MACRO
// ═══════════════════════════════════════════════════════════════════

macro_rules! println {
    // println!() - just newline
    () => ({ let _ = write!(WRITER.lock(), "\n"); });
    
    // println!("format", args...) - formatted output
    ($($arg:tt)*) => ({ let _ = writeln!(WRITER.lock(), $($arg)*); });
}

// ═══════════════════════════════════════════════════════════════════
// ENTRY POINT
// ═══════════════════════════════════════════════════════════════════

#[no_mangle]                    // Don't mangle the name - bootloader calls "_start"
pub extern "C" fn _start() -> ! {   // extern "C" = use C calling convention
                                    // -> ! = never returns (diverging function)
    
    WRITER.lock().clear();      // Clear screen (removes bootloader messages)

    // Print our glorious ASCII art
    println!("========================================================================");
    println!("   _   _      _ _        __        __         _     _ _ ");
    println!("  | | | | ___| | | ___   \\ \\      / /__  _ __| | __| | |");
    println!("  | |_| |/ _ \\ | |/ _ \\   \\ \\ /\\ / / _ \\| '__| |/ _` | |");
    println!("  |  _  |  __/ | | (_) |   \\ V  V / (_) | |  | | (_| |_|");
    println!("  |_| |_|\\___|_|_|\\___/     \\_/\\_/ \\___/|_|  |_|\\__,_(_)");
    println!();
    println!("  From Rust. With love. And zero dependencies on libc.");
    println!("========================================================================");
    println!();

    WRITER.lock().set_color(Color::White, Color::Black);
    println!("  [OK] Booted into 32-bit protected mode");
    println!("  [OK] VGA text mode: 80x25");
    println!();

    WRITER.lock().set_color(Color::Green, Color::Black);
    println!("  This OS does exactly one thing, and it does it well.");
    println!();
    println!("  Now entering infinite loop. As one does.");

    // Infinite loop - OS never exits
    loop { 
        core::hint::spin_loop();    // Hint to CPU: we're spinning
                                    // CPU can save power
    }
}

// ═══════════════════════════════════════════════════════════════════
// PANIC HANDLER
// ═══════════════════════════════════════════════════════════════════

// Required for #![no_std] - what to do when panic!() is called
// Normal Rust unwinds the stack, but we disabled that (panic = "abort")
// So we just print the error and hang

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    WRITER.lock().set_color(Color::White, Color::Black);
    let _ = writeln!(WRITER.lock(), "\n!!! KERNEL PANIC !!!\n{}", info);
    loop { core::hint::spin_loop(); }
}
```

### Why `#[repr(C)]`?

Rust's default struct layout is undefined - the compiler can reorder fields for efficiency. For hardware interfaces, we need exact control:

```rust
// Without repr(C) - compiler might reorder!
struct Bad {
    a: u8,
    b: u32,
    c: u8,
}
// Could be laid out as: b, a, c (to reduce padding)

// With repr(C) - guaranteed order
#[repr(C)]
struct Good {
    a: u8,      // offset 0
    b: u32,     // offset 4 (with 3 bytes padding)
    c: u8,      // offset 8
}
```

For our `Char` struct, we need `ascii` at offset 0 and `color` at offset 1 - exactly what VGA expects.

### Why `write_volatile`?

```rust
// This might be optimized away:
let ptr = 0xB8000 as *mut u8;
unsafe { *ptr = b'A'; }
// Compiler: "Nobody reads this memory, skip the write"

// This won't be optimized away:
unsafe { core::ptr::write_volatile(ptr, b'A'); }
// Compiler: "I don't know what this does, better do it"
```

Memory-mapped I/O looks like normal memory to the compiler, but writes have side effects (updating the screen). `write_volatile` ensures the write actually happens.



---

## VGA Text Mode

### Memory Layout

VGA text mode buffer starts at physical address `0xB8000`. It's 80×25 characters = 4000 bytes (2 bytes per character).

```
Address     Content
0xB8000     Row 0, Col 0: [ASCII][Color]
0xB8002     Row 0, Col 1: [ASCII][Color]
...
0xB809E     Row 0, Col 79: [ASCII][Color]
0xB80A0     Row 1, Col 0: [ASCII][Color]
...
0xB8F9E     Row 24, Col 79: [ASCII][Color]
```

### Color Codes

```
Value   Color           Value   Color (Bright)
0x0     Black           0x8     Dark Gray
0x1     Blue            0x9     Light Blue
0x2     Green           0xA     Light Green
0x3     Cyan            0xB     Light Cyan
0x4     Red             0xC     Light Red
0x5     Magenta         0xD     Pink
0x6     Brown           0xE     Yellow
0x7     Light Gray      0xF     White
```

### Example: Writing "Hi" in Yellow on Blue

```rust
// Address calculation: 0xB8000 + (row * 80 + col) * 2
let base = 0xB8000 as *mut u8;

unsafe {
    // 'H' at row 0, col 0
    *base.add(0) = b'H';           // ASCII
    *base.add(1) = 0x1E;           // Yellow (E) on Blue (1)
    
    // 'i' at row 0, col 1
    *base.add(2) = b'i';
    *base.add(3) = 0x1E;
}
```

---

## Build System

### Custom Target Specification

We can't use standard targets like `x86_64-unknown-linux-gnu` because they assume an OS exists. We create a custom target:

**i686-hello_os.json:**
```json
{
    "llvm-target": "i686-unknown-none",
    "data-layout": "e-m:e-p:32:32-...",
    "arch": "x86",
    "target-endian": "little",
    "target-pointer-width": 32,
    "target-c-int-width": 32,
    "os": "none",                    // No OS!
    "executables": true,
    "linker-flavor": "ld.lld",
    "linker": "rust-lld",
    "panic-strategy": "abort",       // No unwinding
    "disable-redzone": true,         // Important for interrupts
    "features": "-mmx,-sse,-sse2..." // Disable SIMD
}
```

#### Why disable the red zone?

The "red zone" is a 128-byte area below the stack pointer that functions can use without adjusting SP. It's an optimization, but it's dangerous for OS code:

```
Without red zone disabled:
┌─────────────────┐
│ Stack frame     │
├─────────────────┤ ← SP
│ Red zone (128B) │ ← Function uses this
│ (not allocated) │
└─────────────────┘

If an interrupt fires:
┌─────────────────┐
│ Stack frame     │
├─────────────────┤ ← SP
│ Red zone data   │ ← CORRUPTED by interrupt!
│ Interrupt frame │ ← CPU pushes here
└─────────────────┘
```

#### Why disable SIMD (SSE/MMX)?

SIMD instructions use special registers (XMM0-XMM15). If we use them:
1. We must save/restore them on context switches
2. We must save/restore them on interrupts
3. It's complex and we don't need SIMD for "Hello World"

### Cargo Configuration

**.cargo/config.toml:**
```toml
[build]
target = "i686-hello_os.json"       # Use our custom target

[unstable]
build-std = ["core", "compiler_builtins", "alloc"]  # Compile std library
build-std-features = ["compiler-builtins-mem"]       # Include memcpy, etc.
json-target-spec = true                              # Allow JSON targets

[target.i686-hello_os]
rustflags = ["-C", "link-arg=-Tlinker.ld"]          # Use our linker script
```

### Linker Script

**linker.ld:**
```ld
ENTRY(_start)                   /* Entry point symbol */
SECTIONS {
    . = 0x1000;                 /* Start at address 0x1000 */
                                /* Must match KERNEL_OFFSET in bootloader! */
    
    .text : {                   /* Code section */
        *(.text._start)         /* Put _start first */
        *(.text .text.*)        /* Then other code */
    }
    .rodata : { *(.rodata .rodata.*) }  /* Read-only data (strings) */
    .data : { *(.data .data.*) }        /* Initialized data */
    .bss : { *(.bss .bss.*) }           /* Uninitialized data */
}
```

### Build Script

**build.sh:**
```bash
#!/bin/bash
set -e                          # Exit on error
export PATH="$HOME/.cargo/bin:$PATH"

# 1. Compile Rust kernel to ELF
cargo build --release

# 2. Extract raw binary from ELF
#    ELF has headers, sections, symbols - we just want the code
llvm-objcopy -O binary \
    target/i686-hello_os/release/hello-os \
    target/kernel.bin

# 3. Assemble bootloader
nasm -f bin boot/boot.asm -o target/boot.bin

# 4. Concatenate: bootloader + kernel = disk image
cat target/boot.bin target/kernel.bin > target/os.img

# 5. Pad to 16KB (bootloader reads 32 sectors)
dd if=/dev/zero bs=1 count=$((16384 - $(stat -f%z target/os.img))) \
    >> target/os.img 2>/dev/null
```

---

## Debugging Journey

This section documents every problem encountered and how it was solved.

### Problem 1: Rust Toolchain Conflicts

**Symptom:**
```
error: the option `Z` is only accepted on the nightly compiler
```

**Cause:** Homebrew-installed Rust (`/opt/homebrew/bin/rustc`) was being used instead of rustup-managed nightly.

**Diagnosis:**
```bash
which rustc
# /opt/homebrew/bin/rustc  ← Wrong!

rustc --version
# rustc 1.93.0 (stable)    ← Wrong!
```

**Solution:** Prepend rustup's bin directory to PATH:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

**Lesson:** Always check which toolchain is being used. `rust-toolchain.toml` only works with rustup's cargo.

---

### Problem 2: Target Specification Format

**Symptom:**
```
error: target-pointer-width: invalid type: string "64", expected u16
```

**Cause:** Newer Rust nightly changed the JSON format. String values like `"32"` must now be integers `32`.

**Wrong:**
```json
"target-pointer-width": "32"
```

**Correct:**
```json
"target-pointer-width": 32
```

**Lesson:** Target spec format evolves. Check current documentation.

---

### Problem 3: SSE/Soft-Float Incompatibility

**Symptom:**
```
error: target feature `soft-float` is incompatible with the ABI
error: target feature `sse2` is required by the ABI but gets disabled
```

**Cause:** x86_64 ABI requires SSE2 for floating-point. We tried to disable it AND use soft-float.

**Solution:** Use i686 (32-bit) instead, which doesn't require SSE:
```json
"features": "-mmx,-sse,-sse2,-sse3,-ssse3,-sse4.1,-sse4.2,-avx,-avx2"
```

**Lesson:** 32-bit x86 is simpler for bare-metal because it has fewer ABI requirements.

---

### Problem 4: JSON Target Spec Flag

**Symptom:**
```
error: `.json` target specs require -Zjson-target-spec
```

**Cause:** Newer Rust requires explicit opt-in for JSON target files.

**Solution:** Add to `.cargo/config.toml`:
```toml
[unstable]
json-target-spec = true
```

---

### Problem 5: Missing llvm-objcopy

**Symptom:**
```
./build.sh: line 25: llvm-objcopy: command not found
```

**Cause:** `llvm-objcopy` isn't in PATH. It's installed by rustup but in a non-standard location.

**Diagnosis:**
```bash
find ~/.rustup -name "llvm-objcopy"
# /Users/.../.rustup/toolchains/nightly-.../lib/rustlib/.../bin/llvm-objcopy
```

**Solution:** Use full path in build script:
```bash
~/.rustup/toolchains/nightly-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/bin/llvm-objcopy
```

---

### Problem 6: Bootloader Crate Compatibility

**Symptom:**
```
error: could not compile `serde_core` due to 5829 previous errors
```

**Cause:** The `bootloader` crate (v0.11) has dependencies that don't compile on bleeding-edge nightly.

**Solution:** Abandon the bootloader crate approach. Write our own NASM bootloader instead.

**Lesson:** External crates can break on nightly. For OS dev, simpler is often better.

---

### Problem 7: Linker Script Not Applied

**Symptom:** Kernel compiled but wasn't placed at address 0x1000.

**Cause:** Cargo wasn't passing the linker script to the linker.

**Diagnosis:**
```bash
llvm-objdump -d target/.../hello-os | head -20
# Showed wrong addresses
```

**Solution:** Add rustflags to `.cargo/config.toml`:
```toml
[target.i686-hello_os]
rustflags = ["-C", "link-arg=-Tlinker.ld"]
```

---

### Problem 8: "Booting from Hard Disk..." Then Nothing

**Symptom:** QEMU showed "Booting from Hard Disk..." but kernel didn't run.

**Diagnosis Steps:**

1. **Check boot signature:**
```bash
hexdump -C target/os.img | grep "000001f0"
# Should show: ... 55 aa
```
✓ Boot signature was correct.

2. **Check bootloader size:**
```bash
ls -la target/boot.bin
# Should be exactly 512 bytes
```
✓ Size was correct.

3. **Check kernel is in image:**
```bash
hexdump -C target/os.img | head -40
# Should see kernel code after offset 0x200
```
✓ Kernel was present.

4. **Add debug output to bootloader:**
```asm
; Write 'P' to screen to confirm protected mode
mov byte [0xb8000], 'P'
mov byte [0xb8001], 0x0f
```

5. **Try different QEMU options:**
```bash
# Instead of:
qemu-system-i386 -drive format=raw,file=target/os.img

# Try:
qemu-system-i386 -fda target/os.img
```

**Root Cause:** QEMU's `-drive` option was treating the image as a hard disk, but our bootloader was designed for floppy disk geometry (CHS addressing).

**Solution:** Use `-fda` (floppy disk A) instead of `-drive`.

**Lesson:** Boot media type matters. Floppy and hard disk have different sector layouts.

---

### Problem 9: Borrow Checker Issues

**Symptom:**
```
error[E0503]: cannot use `self.row` because it was mutably borrowed
```

**Cause:** Original code tried to use `self.buffer()[self.row][self.col]` which borrows `self` mutably for `buffer()` and then tries to read `self.row`.

**Wrong:**
```rust
fn buffer(&mut self) -> &mut [[Char; W]; H] {
    unsafe { &mut *(VGA as *mut _) }
}

fn write_byte(&mut self, byte: u8) {
    self.buffer()[self.row][self.col] = ch;  // Error!
    //   ↑ borrows self mutably
    //              ↑ tries to read self.row
}
```

**Solution:** Don't return a reference. Use raw pointers directly:
```rust
fn put(&self, row: usize, col: usize, ch: Char) {
    unsafe {
        let ptr = (VGA as *mut Char).add(row * W + col);
        core::ptr::write_volatile(ptr, ch);
    }
}
```

**Lesson:** For memory-mapped I/O, raw pointers are often cleaner than references.



---

## Lessons Learned

### 1. Start Simple
We initially tried to use the `bootloader` crate for convenience. It broke on nightly Rust. Writing our own 50-line bootloader was ultimately simpler and more educational.

### 2. Understand Every Layer
When something doesn't work, you need to understand:
- What the CPU is doing
- What the BIOS expects
- What the assembler produces
- What the linker does
- What the compiler generates

### 3. Use Hexdump Liberally
```bash
hexdump -C target/os.img | head -50
```
This shows you exactly what's in your binary. Is the boot signature there? Is the kernel where you expect?

### 4. Add Debug Output Early
In the bootloader, printing characters or writing to VGA confirms how far execution gets:
```asm
mov byte [0xb8000], '1'  ; Made it to step 1
; ... more code ...
mov byte [0xb8002], '2'  ; Made it to step 2
```

### 5. Check Your Tools
```bash
which rustc
rustc --version
which cargo
```
Multiple Rust installations can cause subtle issues.

### 6. Read Error Messages Carefully
```
error: target feature `sse2` is required by the ABI
```
This tells you exactly what's wrong. The x86_64 ABI requires SSE2. Solution: use i686.

### 7. QEMU Options Matter
- `-fda file` = floppy disk
- `-hda file` = hard disk
- `-drive file=...,format=raw` = generic drive

Different options affect how BIOS sees the disk.

### 8. Memory Layout is Critical
If your linker script says `0x1000` but your bootloader jumps to `0x2000`, nothing works. These must match exactly.

### 9. Volatile is Not Optional
For memory-mapped I/O, always use `write_volatile`. The compiler will optimize away "useless" writes otherwise.

### 10. repr(C) for Hardware Interfaces
Rust's default struct layout is undefined. Use `#[repr(C)]` when interfacing with hardware or other languages.

---

## Appendix A: File Listing

```
os-2/
├── .cargo/
│   └── config.toml         # Cargo configuration
├── boot/
│   └── boot.asm            # NASM bootloader (512 bytes)
├── src/
│   └── main.rs             # Rust kernel
├── target/
│   ├── boot.bin            # Assembled bootloader
│   ├── kernel.bin          # Raw kernel binary
│   └── os.img              # Final bootable image
├── build.sh                # Build script
├── Cargo.toml              # Rust package manifest
├── Cargo.lock              # Dependency lock file
├── i686-hello_os.json      # Custom target specification
├── linker.ld               # Linker script
├── rust-toolchain.toml     # Rust toolchain specification
├── README.md               # Project readme
└── TUTORIAL.md             # This document
```

---

## Appendix B: Quick Reference

### Build Commands
```bash
./build.sh                                    # Build everything
qemu-system-i386 -fda target/os.img          # Run in QEMU
```

### Debug Commands
```bash
# Check boot signature
hexdump -C target/os.img | grep "000001f0"

# Disassemble kernel
llvm-objdump -d target/i686-hello_os/release/hello-os | head -100

# Check file sizes
ls -la target/*.bin

# View raw image
hexdump -C target/os.img | head -50
```

### QEMU Options
```bash
# Basic run
qemu-system-i386 -fda target/os.img

# With debug console
qemu-system-i386 -fda target/os.img -monitor stdio

# Stop on boot (for debugging)
qemu-system-i386 -fda target/os.img -S -s
# Then connect with: gdb -ex "target remote :1234"
```

---

## Appendix C: Common Errors and Solutions

| Error | Cause | Solution |
|-------|-------|----------|
| `no_std` crate cannot use `std` | Using std types | Use `core` equivalents |
| `cannot find crate core` | Wrong target | Add `build-std` to config |
| `target-pointer-width: invalid type` | Old JSON format | Use integers, not strings |
| `sse2 required by ABI` | x86_64 requires SSE | Use i686 instead |
| `json target specs require flag` | Missing config | Add `json-target-spec = true` |
| Boot signature not found | Wrong padding | Ensure `times 510-($-$$) db 0` |
| Kernel not running | Wrong load address | Match linker.ld and bootloader |
| Screen not updating | Missing volatile | Use `write_volatile` |
| Struct layout wrong | Rust reordering | Add `#[repr(C)]` |

---

## Appendix D: Further Reading

1. **OSDev Wiki** - https://wiki.osdev.org/
   - Comprehensive OS development resource
   - Covers everything from bootloaders to filesystems

2. **Writing an OS in Rust** - https://os.phil-opp.com/
   - Excellent blog series on Rust OS development
   - More advanced than this tutorial

3. **Intel Software Developer Manuals**
   - Definitive x86 reference
   - Volume 3: System Programming Guide

4. **The Little Book About OS Development**
   - Free online book
   - Good introduction to x86 OS concepts

---

## Conclusion

You've built a complete operating system from scratch:

1. **Bootloader** (50 lines of assembly)
   - Loads kernel from disk
   - Switches to protected mode
   - Sets up GDT and segments

2. **Kernel** (100 lines of Rust)
   - Writes to VGA text buffer
   - Implements `fmt::Write` for formatting
   - Handles panics gracefully

3. **Build System** (10 lines of shell)
   - Cross-compiles Rust to bare metal
   - Extracts raw binary
   - Creates bootable disk image

The result: a 16KB bootable image that displays "Hello World" in glorious ASCII art.

Is it practical? No.
Is it educational? Absolutely.
Is it mass of a mass of a mass of a cool? You bet.

---

*"In the beginning, there was the BIOS, and the BIOS said 'load sector 1 to 0x7C00', and it was good."*
