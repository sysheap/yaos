use core::arch::{asm, global_asm};

global_asm!(include_str!("boot.S"));
global_asm!(include_str!("trap.S"));
global_asm!(include_str!("powersave.S"));
global_asm!(include_str!("panic.S"));

#[unsafe(no_mangle)]
pub fn asm_panic_rust() {
    let ra: usize;
    unsafe {
        asm!("mv {}, ra", out(reg)ra);
    }
    panic!("Panic from asm code (ra={ra:#x})");
}
