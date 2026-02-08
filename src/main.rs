#![no_std]
#![no_main]

use core::fmt::{self, Write};
use core::panic::PanicInfo;
use spin::Mutex;

const VGA: usize = 0xb8000;
const W: usize = 80;
const H: usize = 25;

#[repr(u8)]
#[derive(Clone, Copy)]
enum Color { Black = 0, Green = 2, LightGreen = 10, White = 15 }

#[repr(C)]
#[derive(Clone, Copy)]
struct Char { ascii: u8, color: u8 }

struct Writer { col: usize, row: usize, color: u8 }

impl Writer {
    const fn new() -> Self {
        Self { col: 0, row: 0, color: (Color::Black as u8) << 4 | Color::LightGreen as u8 }
    }

    fn put(&self, row: usize, col: usize, ch: Char) {
        unsafe {
            let ptr = (VGA as *mut Char).add(row * W + col);
            core::ptr::write_volatile(ptr, ch);
        }
    }

    fn clear(&mut self) {
        for r in 0..H {
            for c in 0..W {
                self.put(r, c, Char { ascii: b' ', color: self.color });
            }
        }
        self.col = 0;
        self.row = 0;
    }

    fn write_byte(&mut self, byte: u8) {
        if byte == b'\n' {
            self.col = 0;
            self.row += 1;
            return;
        }
        if self.col >= W {
            self.col = 0;
            self.row += 1;
        }
        if self.row < H {
            self.put(self.row, self.col, Char { ascii: byte, color: self.color });
            self.col += 1;
        }
    }

    fn set_color(&mut self, fg: Color, bg: Color) {
        self.color = (bg as u8) << 4 | fg as u8;
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            self.write_byte(if matches!(b, 0x20..=0x7e | b'\n') { b } else { 0xfe });
        }
        Ok(())
    }
}

static WRITER: Mutex<Writer> = Mutex::new(Writer::new());

macro_rules! println {
    () => ({ let _ = write!(WRITER.lock(), "\n"); });
    ($($arg:tt)*) => ({ let _ = writeln!(WRITER.lock(), $($arg)*); });
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    WRITER.lock().clear();

    println!("==========================================================================");
    println!("   _   _      _ _        __        __         _     _ _ ");
    println!("  | | | | ___| | | ___   \\ \\      / /__  _ __| | __| | |");
    println!("  | |_| |/ _ \\ | |/ _ \\   \\ \\ /\\ / / _ \\| '__| |/ _` | |");
    println!("  |  _  |  __/ | | (_) |   \\ V  V / (_) | |  | | (_| |_|");
    println!("  |_| |_|\\___|_|_|\\___/     \\_/\\_/ \\___/|_|  |_|\\__,_(_)");
    println!();
    println!("  By KarAshutosh. From Rust. With love. And zero dependencies on libc.");
    println!("==========================================================================");
    println!();

    WRITER.lock().set_color(Color::White, Color::Black);
    println!("  [OK] Booted into 32-bit protected mode");
    println!("  [OK] VGA text mode: 80x25");
    println!();

    WRITER.lock().set_color(Color::Green, Color::Black);
    println!("  This OS does exactly one thing, and it does it well.");
    println!();
    println!("  Now entering infinite loop. As one does.");

    loop { core::hint::spin_loop(); }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    WRITER.lock().set_color(Color::White, Color::Black);
    let _ = writeln!(WRITER.lock(), "\n!!! KERNEL PANIC !!!\n{}", info);
    loop { core::hint::spin_loop(); }
}
