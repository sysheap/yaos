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

const SIE_STIE: usize = 5;
const SSTATUS_SPP: usize = 8;

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

macro_rules! read_csrr {
    ($name: ident) => {
        #[allow(dead_code)]
        pub fn ${concat(read_, $name)}() -> usize {
            if cfg!(miri) {
                return 0;
            }

            let $name: usize;
            unsafe {
                asm!(concat!("csrr {}, ", stringify!($name)), out(reg) $name);
            }
            $name
        }
    };
}

macro_rules! write_csrr {
    ($name: ident) => {
        #[allow(dead_code)]
        pub fn ${concat(write_, $name)}(value: usize)  {
            if cfg!(miri) {
                return ;
            }
            unsafe {
                asm!(concat!("csrw ", stringify!($name), ", {}"), in(reg) value);
            }
        }

        #[allow(dead_code)]
        pub fn ${concat(csrs_, $name)}(mask: usize)  {
            if cfg!(miri) {
                return ;
            }
            unsafe {
                asm!(concat!("csrs ", stringify!($name), ", {}"), in(reg) mask);
            }
        }

        #[allow(dead_code)]
        pub fn ${concat(csrc_, $name)}(mask: usize)  {
            if cfg!(miri) {
                return ;
            }
            unsafe {
                asm!(concat!("csrc ", stringify!($name), ", {}"), in(reg) mask);
            }
        }
    };
}

impl Cpu {
    read_csrr!(satp);
    read_csrr!(stval);
    read_csrr!(sepc);
    read_csrr!(scause);
    read_csrr!(sscratch);
    read_csrr!(sie);
    read_csrr!(sstatus);

    write_csrr!(satp);
    write_csrr!(sepc);
    write_csrr!(sscratch);
    write_csrr!(sstatus);
    write_csrr!(sie);

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

        Self::write_sscratch(static_cpu as usize);
    }

    fn get_per_cpu_data() -> *mut Self {
        let ptr = Self::read_sscratch() as *mut Self;
        assert!(!ptr.is_null() && ptr.is_aligned());
        ptr
    }

    pub fn current() -> CpuRefHolder {
        let ptr = Self::get_per_cpu_data();
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
        let ptr = Self::get_per_cpu_data();
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

    pub unsafe fn write_satp_and_fence(satp_val: usize) {
        Cpu::write_satp(satp_val);
        unsafe {
            asm!("sfence.vma");
        }
    }

    pub fn memory_fence() {
        unsafe {
            asm!("fence");
        }
    }

    pub unsafe fn disable_global_interrupts() {
        Self::csrc_sstatus(0b10);
        Self::write_sie(0);
    }

    pub fn wait_for_interrupt() {
        unsafe {
            asm!("wfi");
        }
    }

    pub fn is_timer_enabled() -> bool {
        let sie = Self::read_sie();
        (sie & (1 << SIE_STIE)) > 0
    }

    pub fn enable_timer_interrupt() {
        Self::csrs_sie(1 << SIE_STIE);
    }
    pub fn is_in_kernel_mode() -> bool {
        let sstatus = Self::read_sstatus();
        (sstatus & (1 << SSTATUS_SPP)) > 0
    }

    pub fn set_ret_to_kernel_mode(kernel_mode: bool) {
        if kernel_mode {
            Self::csrs_sstatus(1 << SSTATUS_SPP);
        } else {
            Self::csrc_sstatus(1 << SSTATUS_SPP);
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
