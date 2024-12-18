use common::mutex::Mutex;

use crate::{
    autogenerated::userspace_programs::{INIT, PROGRAMS},
    cpu, debug, info,
    interrupts::{read_trap_frame, set_sscratch_to_kernel_trap_frame, write_trap_frame},
    klibc::{elf::ElfFile, macros::unwrap_or_return, runtime_initialized::RuntimeInitializedData},
    memory::page_tables::{KERNEL_PAGE_TABLES, activate_page_table},
    processes::{process::Process, timer},
    test::qemu_exit,
};

use super::{
    process::{Pid, ProcessState},
    process_table::{ProcessRef, ProcessTable},
};

pub static THE: RuntimeInitializedData<Mutex<Scheduler>> = RuntimeInitializedData::new();

pub fn init() {
    THE.initialize(Mutex::new(Scheduler::new()));
}

pub struct Scheduler {
    process_table: ProcessTable,
    current_process: ProcessRef,
}

impl Scheduler {
    fn new() -> Self {
        let mut process_table = ProcessTable::new();
        let current_process = process_table.get_dummy_process();

        let elf = ElfFile::parse(INIT).expect("Cannot parse ELF file");
        let process = Process::from_elf(&elf, "init");
        process_table.add_process(process);
        info!("Scheduler initialized and INIT process added to queue");

        Self {
            process_table,
            current_process,
        }
    }

    pub fn dump(&self) {
        self.process_table.dump();
    }

    pub fn get_current_process(&self) -> &ProcessRef {
        &self.current_process
    }

    pub fn get_process(&self, pid: Pid) -> Option<&ProcessRef> {
        self.process_table.get_process(pid)
    }

    pub fn schedule(&mut self) {
        debug!("Schedule next process");
        if self.prepare_next_process() {
            timer::set_timer(10);
            return;
        }
        activate_page_table(&KERNEL_PAGE_TABLES);
        timer::disable_timer();
        let addr = cpu::wfi_loop as *const () as usize;
        debug!("setting sepc={addr:#x}");
        cpu::write_sepc(addr);
        cpu::set_ret_to_kernel_mode(true);
        set_sscratch_to_kernel_trap_frame();
    }

    pub fn kill_current_process(&mut self) {
        let current_process = self.swap_current_with_dummy();

        activate_page_table(&KERNEL_PAGE_TABLES);
        let pid = current_process.lock().get_pid();
        drop(current_process);
        self.process_table.kill(pid);
    }

    pub fn let_current_process_wait_for(&self, pid: Pid) -> bool {
        let wait_for_process = unwrap_or_return!(self.process_table.get_process(pid), false);

        let mut current_process = self.current_process.lock();
        current_process.set_state(ProcessState::Waiting);
        current_process.set_syscall_return_code(0);

        wait_for_process
            .lock()
            .add_notify_on_die(current_process.get_pid());

        true
    }

    pub fn send_ctrl_c(&mut self) {
        self.queue_current_process_back();

        let highest_pid = self.process_table.get_highest_pid_without(&["yash"]);

        if let Some(pid) = highest_pid {
            activate_page_table(&KERNEL_PAGE_TABLES);
            self.process_table.kill(pid);
        }

        self.schedule();
    }

    pub fn get_dummy_process(&self) -> ProcessRef {
        self.process_table.get_dummy_process()
    }

    pub fn start_program(&mut self, name: &str) -> Option<Pid> {
        for (prog_name, elf) in PROGRAMS {
            if name == *prog_name {
                let elf = ElfFile::parse(elf).expect("Cannot parse ELF file");
                let process = Process::from_elf(&elf, prog_name);
                let pid = process.get_pid();
                self.process_table.add_process(process);
                return Some(pid);
            }
        }
        None
    }

    fn queue_current_process_back(&mut self) -> Pid {
        self.swap_current_with_dummy().with_lock(|mut p| {
            p.set_program_counter(cpu::read_sepc());
            p.set_in_kernel_mode(cpu::is_in_kernel_mode());
            p.set_register_state(&read_trap_frame());
            let pid = p.get_pid();
            debug!("Unscheduling PID={} NAME={}", pid, p.get_name());
            pid
        })
    }

    fn prepare_next_process(&mut self) -> bool {
        let old_pid = self.queue_current_process_back();

        if self.process_table.is_empty() {
            info!("No more processes to schedule, shutting down system");
            qemu_exit::exit_success();
        }

        let next_process = unwrap_or_return!(self.process_table.next_runnable(old_pid), false);

        next_process.with_lock(|p| {
            let pc = p.get_program_counter();

            write_trap_frame(p.get_register_state());
            cpu::write_sepc(pc);
            cpu::set_ret_to_kernel_mode(p.get_in_kernel_mode());
            activate_page_table(p.get_page_table());

            debug!("Scheduling PID={} NAME={}", p.get_pid(), p.get_name());
        });

        self.current_process = next_process;

        true
    }

    fn swap_current_with_dummy(&mut self) -> ProcessRef {
        let dummy_process = self.process_table.get_dummy_process();
        core::mem::replace(&mut self.current_process, dummy_process)
    }
}
