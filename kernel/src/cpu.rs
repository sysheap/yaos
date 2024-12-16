use alloc::boxed::Box;
use core::arch::asm;

use common::syscalls::trap_frame::TrapFrame;

use crate::{
    klibc::sizes::KiB,
    memory::page_tables::{
        activate_page_table, get_satp_value_from_page_tables, RootPageTableHolder,
    },
};

const KERNEL_STACK_SIZE: usize = KiB(512);

// We need to make sure that the trap frame is the first member
// We store a pointer to his structure in sscratch and on an interrupt
// we're saving the context to the trap_frame, assuming it lies at offset
// 0x0 of the struct.
#[repr(C)]
pub struct PerCpuData {
    pub trap_frame: TrapFrame,
    kernel_page_tables_satp_value: usize, // We access this value in assembly, so don't move it
    pub cpu_id: u64,
    kernel_stack: *mut u8,
    page_tables: RootPageTableHolder,
}

impl PerCpuData {
    pub fn new(cpu_id: u64) -> Self {
        let kernel_stack =
            Box::into_raw(vec![0u8; KERNEL_STACK_SIZE].into_boxed_slice()) as *mut u8;
        let mut page_tables = RootPageTableHolder::new_with_kernel_mapping();

        let stack_start_virtual = (0usize).wrapping_sub(KERNEL_STACK_SIZE);

        page_tables.map(
            stack_start_virtual,
            kernel_stack as usize,
            KERNEL_STACK_SIZE,
            crate::memory::page_tables::XWRMode::ReadWrite,
            false,
            format!("KERNEL_STACK CPU {cpu_id}"),
        );

        let satp_value = get_satp_value_from_page_tables(&page_tables);

        Self {
            trap_frame: TrapFrame::zero(),
            kernel_page_tables_satp_value: satp_value,
            cpu_id,
            kernel_stack,
            page_tables,
        }
    }

    pub fn cpu_id() -> u64 {
        unsafe { (*get_per_cpu_data()).cpu_id }
    }

    pub fn get_kernel_page_table() -> &'static RootPageTableHolder {
        unsafe { &(*get_per_cpu_data()).page_tables }
    }

    pub fn activate_kernel_page_table() {
        activate_page_table(Self::get_kernel_page_table());
    }

    pub fn write_trap_frame(trap_frame: &TrapFrame) {
        unsafe { (*get_per_cpu_data()).trap_frame = *trap_frame };
    }

    pub fn read_trap_frame() -> TrapFrame {
        unsafe { (*get_per_cpu_data()).trap_frame }
    }
}

pub fn write_sscratch_register(value: *const PerCpuData) {
    unsafe {
        asm!("csrw sscratch, {}", in(reg) value);
    }
}

pub fn get_per_cpu_data() -> *mut PerCpuData {
    let ptr: *mut PerCpuData;
    unsafe {
        asm!("csrr {}, sscratch", out(reg) ptr);
    }
    ptr
}

pub fn write_sepc(value: usize) {
    unsafe {
        asm!("csrw sepc, {}", in(reg) value);
    }
}

pub fn read_sepc() -> usize {
    let sepc: usize;
    unsafe {
        asm!("csrr {}, sepc", out(reg) sepc);
    }
    sepc
}

pub unsafe fn write_satp_and_fence(satp_val: usize) {
    unsafe {
        asm!("csrw satp, {}", in(reg) satp_val);
        asm!("sfence.vma");
    }
}

pub fn read_satp() -> usize {
    if cfg!(miri) {
        return 0;
    }

    let satp: usize;
    unsafe {
        asm!("csrr {}, satp", out(reg) satp);
    }
    satp
}

pub fn memory_fence() {
    unsafe {
        asm!("fence");
    }
}

pub unsafe fn disable_global_interrupts() {
    unsafe {
        asm!(
            "csrc sstatus, {}", // Disable global interrupt flag
            "csrw sie, x0", // Clear any local enabled interrupts otherwise wfi just goes to the current pending interrupt
        in(reg) 0b10);
    }
}

pub fn wait_for_interrupt() {
    unsafe {
        asm!("wfi");
    }
}

const SIE_STIE: usize = 5;
const SSTATUS_SPP: usize = 8;

pub fn is_timer_enabled() -> bool {
    let sie: usize;
    unsafe { asm!("csrr {}, sie", out(reg) sie) }
    (sie & (1 << SIE_STIE)) > 0
}

pub fn enable_timer_interrupt() {
    unsafe {
        asm!("
                csrs sie, {}
            ", in(reg) (1 << SIE_STIE)
        )
    }
}

#[unsafe(no_mangle)]
#[naked]
pub extern "C" fn wfi_loop() {
    unsafe {
        core::arch::naked_asm!(
            "
        0:
            wfi
            j 0
        "
        )
    }
}

pub fn is_in_kernel_mode() -> bool {
    let value: usize;
    unsafe {
        asm!("csrr {0}, sstatus", out(reg) value);
    }
    (value & (1 << SSTATUS_SPP)) > 0
}

pub fn set_ret_to_kernel_mode(kernel_mode: bool) {
    if kernel_mode {
        unsafe {
            asm!("csrs sstatus, {}", in(reg) (1<<SSTATUS_SPP));
        }
    } else {
        unsafe {
            asm!("csrc sstatus, {}", in(reg) (1<<SSTATUS_SPP));
        }
    }
}
