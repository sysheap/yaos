use super::trap_cause::{exception::ENVIRONMENT_CALL_FROM_U_MODE, InterruptCause};
use crate::{
    cpu::{self, PerCpuData},
    debug,
    interrupts::plic::{self, InterruptSource},
    io::{stdin_buf::STDIN_BUFFER, uart},
    memory::page_tables::get_satp_value_from_page_tables,
    processes::{
        process::ProcessState,
        scheduler::{self},
    },
    syscalls::{self},
};
use common::syscalls::trap_frame::Register;
use core::panic;

#[no_mangle]
extern "C" fn get_process_satp_value() -> usize {
    scheduler::THE.with_lock(|s| {
        s.get_current_process()
            .with_lock(|p| get_satp_value_from_page_tables(p.get_page_table()))
    })
}

#[no_mangle]
extern "C" fn handle_timer_interrupt() {
    scheduler::THE.lock().schedule();
}

#[no_mangle]
fn handle_external_interrupt() {
    debug!("External interrupt occurred!");
    let plic_interrupt = plic::get_next_pending().expect("There should be a pending interrupt.");
    assert!(
        plic_interrupt == InterruptSource::Uart,
        "Plic interrupt should be uart."
    );

    let input = uart::read().expect("There should be input from the uart.");

    plic::complete_interrupt(plic_interrupt);

    match input {
        3 => scheduler::THE.lock().send_ctrl_c(),
        4 => crate::debugging::dump_current_state(),
        _ => STDIN_BUFFER.lock().push(input),
    }
}

fn handle_syscall(sepc: usize, per_cpu_data: &mut PerCpuData) {
    let trap_frame = &mut per_cpu_data.trap_frame;
    let nr = trap_frame[Register::a0];
    let arg1 = trap_frame[Register::a1];
    let arg2 = trap_frame[Register::a2];
    let arg3 = trap_frame[Register::a3];
    let ret = syscalls::handle_syscall(nr, arg1, arg2, arg3);
    if let Some((ret1, ret2)) = ret {
        trap_frame[Register::a0] = ret1;
        trap_frame[Register::a1] = ret2;
        cpu::write_sepc(sepc + 4); // Skip the ecall instruction
    }
    // In case our current process was set to waiting state we need to reschedule
    scheduler::THE.with_lock(|mut s| {
        if s.get_current_process().lock().get_state() == ProcessState::Waiting {
            s.schedule();
        }
    });
}

fn handle_unhandled_exception(
    cause: InterruptCause,
    stval: usize,
    sepc: usize,
    per_cpu_data: &mut PerCpuData,
) {
    let message= scheduler::THE.lock().get_current_process().with_lock(|p| {
        format!(
            "Unhandled exception!\nName: {}\nException code: {}\nstval: 0x{:x}\nsepc: 0x{:x}\nFrom Userspace: {}\nProcess name: {}\nTrap Frame: {:?}",
            cause.get_reason(),
            cause.get_exception_code(),
            stval,
            sepc,
            p.get_page_table().is_userspace_address(sepc),
            p.get_name(),
            per_cpu_data.trap_frame
        )
    });
    panic!("{}", message);
}

#[no_mangle]
extern "C" fn handle_exception(
    cause: InterruptCause,
    stval: usize,
    sepc: usize,
    per_cpu_data: &mut PerCpuData,
) {
    match cause.get_exception_code() {
        ENVIRONMENT_CALL_FROM_U_MODE => handle_syscall(sepc, per_cpu_data),
        _ => handle_unhandled_exception(cause, stval, sepc, per_cpu_data),
    }
}

#[no_mangle]
extern "C" fn handle_unimplemented(
    cause: InterruptCause,
    _stval: usize,
    sepc: usize,
    _trap_frame: &mut PerCpuData,
) {
    panic!(
        "Unimplemeneted trap occurred! (sepc: {:x?}) (cause: {:?})",
        sepc,
        cause.get_reason(),
    );
}
