use alloc::boxed::Box;
use core::{
    arch::asm,
    cell::Cell,
    ops::{Deref, DerefMut},
};

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
pub struct Cpu {
    trap_frame: TrapFrame,
    kernel_page_tables_satp_value: usize, // We access this value in assembly, so don't move it
    cpu_id: usize,
    kernel_stack: *mut u8,
    kernel_page_tables: RootPageTableHolder,
    mutable_reference_alive: Cell<bool>,
}

impl Cpu {
    pub fn init(cpu_id: usize) {
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

        let cpu = Box::new(Self {
            trap_frame: TrapFrame::zero(),
            kernel_page_tables_satp_value: satp_value,
            cpu_id,
            kernel_stack,
            kernel_page_tables: page_tables,
            mutable_reference_alive: Cell::new(false),
        });

        let static_cpu = Box::leak(cpu) as *mut Cpu;

        write_sscratch_register(static_cpu);
    }

    pub fn current() -> CpuRefHolder {
        let ptr = get_per_cpu_data();
        // SAFETY: The pointer points to a static and is therefore always valid.
        let mutable_reference_alive = unsafe { &(*ptr).mutable_reference_alive };
        let old = mutable_reference_alive.replace(true);
        assert!(
            !old,
            "There must be only one valid mutable reference to the current cpu struct."
        );
        // SAFETY: The pointer points to a static and is therefore always valid.
        unsafe { CpuRefHolder(&mut *ptr) }
    }

    pub unsafe fn current_nevertheless() -> CpuRefHolder {
        let ptr = get_per_cpu_data();
        unsafe { CpuRefHolder(&mut *ptr) }
    }

    pub fn cpu_id(&self) -> usize {
        self.cpu_id
    }

    pub fn activate_kernel_page_table(&self) {
        activate_page_table(&self.kernel_page_tables);
    }

    pub fn kernel_page_table(&self) -> &RootPageTableHolder {
        &self.kernel_page_tables
    }

    pub fn trap_frame_mut(&mut self) -> &mut TrapFrame {
        &mut self.trap_frame
    }

    pub fn trap_frame(&self) -> &TrapFrame {
        &self.trap_frame
    }
}

pub fn write_sscratch_register(value: *const Cpu) {
    unsafe {
        asm!("csrw sscratch, {}", in(reg) value);
    }
}

pub fn get_per_cpu_data() -> *mut Cpu {
    let ptr: *mut Cpu;
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

pub fn read_scause() -> usize {
    let scause: usize;
    unsafe {
        asm!("csrr {}, scause", out(reg) scause);
    }
    scause
}

pub fn read_stval() -> usize {
    let stval: usize;
    unsafe {
        asm!("csrr {}, stval", out(reg) stval);
    }
    stval
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

pub struct CpuRefHolder(&'static mut Cpu);

impl Drop for CpuRefHolder {
    fn drop(&mut self) {
        self.0.mutable_reference_alive.set(false);
    }
}

impl Deref for CpuRefHolder {
    type Target = Cpu;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl DerefMut for CpuRefHolder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}
