#![cfg_attr(not(miri), no_std)]
#![cfg_attr(not(miri), no_main)]
#![feature(panic_info_message)]
#![feature(strict_provenance)]
#![feature(nonzero_ops)]
#![feature(custom_test_frameworks)]
#![feature(const_mut_refs)]
#![feature(offset_of)]
#![feature(option_take_if)]
#![test_runner(test::test_runner)]
#![reexport_test_harness_main = "test_main"]

use alloc::rc::Rc;

use crate::{
    interrupts::plic,
    io::uart::QEMU_UART,
    memory::page_tables::{self, RootPageTableHolder},
    processes::{scheduler, timer},
};

mod asm;
mod assert;
mod autogenerated;
mod cpu;
mod interrupts;
mod io;
mod klibc;
mod logging;
mod memory;
mod panic;
mod processes;
mod sbi;
mod syscalls;

mod test;

#[macro_use]
extern crate alloc;

extern "C" {
    static HEAP_START: usize;
    static HEAP_SIZE: usize;
}

#[no_mangle]
extern "C" fn kernel_init() {
    QEMU_UART.lock().init();

    println!("Hello World from YaROS!\n");

    let version = sbi::extensions::base_extension::sbi_get_spec_version();
    info!("SBI version {}.{}", version.major, version.minor);
    assert!(
        (version.major == 0 && version.minor >= 2) || version.major > 0,
        "Supported SBI Versions >= 0.2"
    );

    unsafe {
        info!("Initializing page allocator");
        memory::init_page_allocator(HEAP_START as *mut u8, HEAP_SIZE);
    }

    #[cfg(test)]
    test_main();

    page_tables::activate_page_table(Rc::new(RootPageTableHolder::new_with_kernel_mapping()));
    interrupts::set_sscratch_to_kernel_trap_frame();

    plic::init_uart_interrupt();

    scheduler::initialize();
    timer::set_timer(0);
}
