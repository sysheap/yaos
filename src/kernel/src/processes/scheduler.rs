use core::cell::RefCell;

use alloc::rc::Rc;
use common::mutex::Mutex;

use crate::autogenerated::userspace_programs::{INIT, PROGRAMS};
use crate::klibc::elf::ElfFile;
use crate::memory::page_tables;
use crate::processes::process_list::{self, notify_died};
use crate::test::qemu_exit;
use crate::{cpu, debug};

use super::process::{Pid, Process, ProcessState};
use super::process_list::add_process;

pub fn initialize() {
    let elf = ElfFile::parse(INIT).expect("Cannot parse ELF file");
    let process = Process::from_elf(&elf);
    add_process(process);
}

static CURRENT_PROCESS: Mutex<Option<Rc<RefCell<Process>>>> = Mutex::new(None);

extern "C" {
    fn restore_user_context() -> !;
}

pub fn schedule() -> ! {
    debug!("Schedule next process");
    prepare_next_process();
    unsafe {
        restore_user_context();
    }
}

pub fn kill_current_process() {
    debug!("Killing current process");
    let current_process = CURRENT_PROCESS.lock().take();
    if let Some(current_process) = current_process {
        notify_died(current_process.borrow().get_pid());
        debug!("{:?}", current_process);
    }
    schedule();
}

pub fn let_current_process_wait_for(pid: Pid) -> bool {
    if !process_list::does_pid_exits(pid) {
        return false;
    }
    {
        let current_process_lock = CURRENT_PROCESS.lock();
        let current_process = current_process_lock
            .as_ref()
            .expect("There should be a process.");

        let mut current_process = current_process.borrow_mut();
        current_process.set_state(ProcessState::WaitingFor(pid));
        current_process.set_syscall_return_code(0);
    }
    schedule();
}

pub fn schedule_program(name: &str) -> Option<Pid> {
    for (prog_name, elf) in PROGRAMS {
        if name == *prog_name {
            let elf = ElfFile::parse(elf).expect("Cannot parse ELF file");
            let process = Process::from_elf(&elf);
            let pid = process.get_pid();
            add_process(process);
            return Some(pid);
        }
    }
    None
}

fn prepare_next_process() {
    let current_process = CURRENT_PROCESS.lock().take();

    if let Some(current_process) = current_process {
        current_process
            .borrow_mut()
            .set_program_counter(cpu::read_sepc());
        debug!("Saved context to current process");
        debug!("Current process: {:?}", current_process);
        process_list::enqueue(current_process);
    }

    let next_process_ref = if let Some(next_process) = process_list::next_runnable() {
        next_process
    } else {
        debug!("No more processes to run, shutting down");
        qemu_exit::exit_success();
    };

    {
        let next_process = next_process_ref.borrow();

        let trap_frame_ptr = next_process.register_state_ptr();
        let pc = next_process.get_program_counter();
        let page_table = next_process.get_page_table();

        cpu::write_sscratch_register(trap_frame_ptr);
        cpu::write_sepc(pc);

        page_tables::activate_page_table(page_table);

        debug!("Next process: {:?}", next_process);
    }

    *CURRENT_PROCESS.lock() = Some(next_process_ref);
}
