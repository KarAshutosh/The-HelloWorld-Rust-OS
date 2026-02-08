; boot.asm - Bootloader: real mode -> protected mode -> kernel

[bits 16]
[org 0x7c00]

KERNEL_OFFSET equ 0x1000

start:
    ; Set up segments and stack
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7c00

    ; Save boot drive
    mov [BOOT_DRIVE], dl

    ; Load kernel (32 sectors from sector 2)
    mov bx, KERNEL_OFFSET
    mov ah, 0x02
    mov al, 32
    mov ch, 0
    mov dh, 0
    mov cl, 2
    mov dl, [BOOT_DRIVE]
    int 0x13
    jc $                    ; Hang on error

    ; Enter protected mode
    cli
    lgdt [gdt_descriptor]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp 0x08:protected_mode

BOOT_DRIVE: db 0

gdt_start:
    dq 0                    ; Null descriptor
gdt_code:
    dw 0xffff, 0            ; Limit, Base low
    db 0, 10011010b, 11001111b, 0
gdt_data:
    dw 0xffff, 0
    db 0, 10010010b, 11001111b, 0
gdt_end:

gdt_descriptor:
    dw gdt_end - gdt_start - 1
    dd gdt_start

[bits 32]
protected_mode:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov esp, 0x90000
    jmp KERNEL_OFFSET

times 510 - ($ - $$) db 0
dw 0xaa55
