use core::{fmt::Debug, panic};

use crate::{
    cpu, debug,
    interrupts::plic::{self, InterruptSource},
    io::uart,
    memory::page_tables,
    print, println,
    processes::{scheduler, timer},
    syscalls::handle_syscall,
};

use super::trap_cause::InterruptCause;
use super::trap_cause::{exception::ENVIRONMENT_CALL_FROM_U_MODE, interrupt::*};

#[repr(C)]
pub struct TrapFrame {
    registers: [usize; 32],
    floating_registers: [usize; 32],
}

impl Debug for TrapFrame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Trap Frame[
            x0\t(zero):\t\t0x{:x}
            x1\t(ra):\t\t0x{:x}
            x2\t(sp):\t\t0x{:x}
            x3\t(gp):\t\t0x{:x}
            x4\t(tp):\t\t0x{:x}
            x5\t(t0):\t\t0x{:x}
            x6\t(t1):\t\t0x{:x}
            x7\t(t2):\t\t0x{:x}
            x8\t(s0/fp):\t0x{:x}
            x9\t(s1):\t\t0x{:x}
            x10\t(a0):\t\t0x{:x}
            x11\t(a1):\t\t0x{:x}
            x12\t(a2):\t\t0x{:x}
            x13\t(a3):\t\t0x{:x}
            x14\t(a4):\t\t0x{:x}
            x15\t(a5):\t\t0x{:x}
            x16\t(a6):\t\t0x{:x}
            x17\t(a7):\t\t0x{:x}
            x18\t(s2):\t\t0x{:x}
            x19\t(s3):\t\t0x{:x}
            x20\t(s4):\t\t0x{:x}
            x21\t(s5):\t\t0x{:x}
            x22\t(s6):\t\t0x{:x}
            x23\t(s7):\t\t0x{:x}
            x24\t(s8):\t\t0x{:x}
            x25\t(s9):\t\t0x{:x}
            x26\t(s10):\t\t0x{:x}
            x27\t(s11):\t\t0x{:x}
            x28\t(t3):\t\t0x{:x}
            x29\t(t4):\t\t0x{:x}
            x30\t(t5):\t\t0x{:x}
            x31\t(t6):\t\t0x{:x}
            ]",
            self.registers[0],
            self.registers[1],
            self.registers[2],
            self.registers[3],
            self.registers[4],
            self.registers[5],
            self.registers[6],
            self.registers[7],
            self.registers[8],
            self.registers[9],
            self.registers[10],
            self.registers[11],
            self.registers[12],
            self.registers[13],
            self.registers[14],
            self.registers[15],
            self.registers[16],
            self.registers[17],
            self.registers[18],
            self.registers[19],
            self.registers[20],
            self.registers[21],
            self.registers[22],
            self.registers[23],
            self.registers[24],
            self.registers[25],
            self.registers[26],
            self.registers[27],
            self.registers[28],
            self.registers[29],
            self.registers[30],
            self.registers[31]
        )
    }
}

#[repr(usize)]
#[allow(non_camel_case_types)]
pub enum Register {
    zero = 0,
    ra = 1,
    sp = 2,
    gp = 3,
    tp = 4,
    t0 = 5,
    t1 = 6,
    t2 = 7,
    s0_fp = 8,
    s1 = 9,
    a0 = 10,
    a1 = 11,
    a2 = 12,
    a3 = 13,
    a4 = 14,
    a5 = 15,
    a6 = 16,
    a7 = 17,
    s2 = 18,
    s3 = 19,
    s4 = 20,
    s5 = 21,
    s6 = 22,
    s7 = 23,
    s8 = 24,
    s9 = 25,
    s10 = 26,
    s11 = 27,
    t3 = 28,
    t4 = 29,
    t5 = 30,
    t6 = 31,
}

impl core::ops::Index<Register> for TrapFrame {
    type Output = usize;

    fn index(&self, index: Register) -> &Self::Output {
        &self.registers[index as usize]
    }
}

impl core::ops::IndexMut<Register> for TrapFrame {
    fn index_mut(&mut self, index: Register) -> &mut Self::Output {
        &mut self.registers[index as usize]
    }
}

impl TrapFrame {
    pub const fn zero() -> Self {
        Self {
            registers: [0; 32],
            floating_registers: [0; 32],
        }
    }
}

#[no_mangle]
extern "C" fn supervisor_mode_trap(
    cause: InterruptCause,
    stval: usize,
    sepc: usize,
    trap_frame: &mut TrapFrame,
) {
    if cause.is_interrupt() {
        handle_interrupt(cause, stval, sepc, trap_frame);
    } else {
        handle_exception(cause, stval, sepc, trap_frame);
    }
}

fn handle_exception(cause: InterruptCause, stval: usize, sepc: usize, trap_frame: &mut TrapFrame) {
    match cause.get_exception_code() {
        ENVIRONMENT_CALL_FROM_U_MODE => {
            handle_syscall(trap_frame);
            cpu::write_sepc(sepc + 4); // Skip the ecall instruction
        }
        _ => {
            panic!(
                "Unhandled exception! (Name: {}) (Exception code: {}) (stval: 0x{:x}) (sepc: 0x{:x}) (From Userspace: {})",
                cause.get_reason(),
                cause.get_exception_code(),
                stval,
                sepc,
                page_tables::is_userspace_address(sepc)
            );
        }
    }
}

fn handle_interrupt(cause: InterruptCause, stval: usize, sepc: usize, trap_frame: &TrapFrame) {
    match cause.get_exception_code() {
        SUPERVISOR_TIMER_INTERRUPT => handle_supervisor_timer_interrupt(),
        SUPERVISOR_EXTERNAL_INTERRUPT => handle_external_interrupt(),
        _ => {
            panic!("Unknwon interrupt! (Name: {})", cause.get_reason());
        }
    }
}

fn handle_supervisor_timer_interrupt() {
    debug!("Supervisor timer interrupt occurred!");
    timer::set_timer(1);
    scheduler::schedule();
}

fn handle_external_interrupt() {
    debug!("External interrupt occurred!");
    let plic_interrupt = plic::get_next_pending().expect("There should be a pending interrupt.");
    assert!(
        plic_interrupt == InterruptSource::Uart,
        "Plic interrupt should be uart."
    );

    let input = uart::read().expect("There should be input from the uart.");

    match input {
        8 => {
            // This is a backspace, so we
            // essentially have to write a space and
            // backup again:
            print!("{} {}", 8 as char, 8 as char);
        }
        10 | 13 => {
            // Newline or carriage-return
            println!();
        }
        _ => {
            print!("{}", input as char);
        }
    };

    plic::complete_interrupt(plic_interrupt);
}
