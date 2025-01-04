use common::{
    net::UDPDescriptor,
    pointer::Pointer,
    ref_conversion::RefToPointer,
    syscalls::{
        kernel::KernelSyscalls, SysExecuteError, SysSocketError, SysWaitError, SyscallStatus,
    },
    unwrap_or_return,
};

use crate::{
    autogenerated::userspace_programs::PROGRAMS,
    debug,
    io::stdin_buf::STDIN_BUFFER,
    net::{udp::UdpHeader, ARP_CACHE, OPEN_UDP_SOCKETS},
    print, println,
    processes::{
        process::{Pid, NEVER_PID},
        process_table::ProcessRef,
        scheduler::{self},
    },
};

use super::validator::{UserspaceArgument, Validatable};

pub(super) struct SyscallHandler {
    process_exit: bool,
    current_process: ProcessRef,
    current_pid: Pid,
}

impl SyscallHandler {
    fn new() -> Self {
        let current_process = scheduler::THE.lock().get_current_process().clone();
        let current_pid = current_process.lock().get_pid();
        Self {
            process_exit: false,
            current_process,
            current_pid,
        }
    }

    pub fn current_process(&self) -> &ProcessRef {
        &self.current_process
    }
}

impl KernelSyscalls for SyscallHandler {
    type ArgWrapper<T: RefToPointer<T>> = UserspaceArgument<T>;

    fn sys_print_programs(&mut self) {
        for (name, _) in PROGRAMS {
            print!("{name} ");
        }
        println!("");
    }
    fn sys_panic(&mut self) {
        panic!("Userspace triggered kernel panic");
    }
    fn sys_write_char(&mut self, c: UserspaceArgument<char>) {
        print!("{}", *c);
    }

    fn sys_read_input(&mut self) -> Option<u8> {
        let mut stdin = STDIN_BUFFER.lock();
        stdin.pop()
    }
    fn sys_read_input_wait(&mut self) -> u8 {
        let input = STDIN_BUFFER.lock().pop();
        if let Some(input) = input {
            input
        } else {
            STDIN_BUFFER.lock().register_wakeup(self.current_pid);
            self.current_process.lock().set_waiting_on_syscall::<u8>();
            0
        }
    }

    fn sys_exit(&mut self, status: UserspaceArgument<isize>) {
        // We don't want to overwrite the next process trap frame
        self.process_exit = true;
        self.current_process = scheduler::THE.lock().get_dummy_process();
        self.current_pid = NEVER_PID;

        debug!("Exit process with status: {}\n", *status);
        scheduler::THE.lock().kill_current_process();
    }

    fn sys_execute(&mut self, name: UserspaceArgument<&str>) -> Result<u64, SysExecuteError> {
        let name = name.validate(self)?;

        if let Some(pid) = scheduler::THE.lock().start_program(name) {
            Ok(pid)
        } else {
            Err(SysExecuteError::InvalidProgram)
        }
    }

    fn sys_wait(&mut self, pid: UserspaceArgument<u64>) -> Result<(), SysWaitError> {
        if scheduler::THE.lock().let_current_process_wait_for(*pid) {
            Ok(())
        } else {
            Err(SysWaitError::InvalidPid)
        }
    }

    fn sys_mmap_pages(&mut self, number_of_pages: UserspaceArgument<usize>) -> *mut u8 {
        self.current_process.lock().mmap_pages(*number_of_pages)
    }

    fn sys_open_udp_socket(
        &mut self,
        port: UserspaceArgument<u16>,
    ) -> Result<UDPDescriptor, SysSocketError> {
        let socket = match OPEN_UDP_SOCKETS.lock().try_get_socket(*port) {
            None => return Err(SysSocketError::PortAlreadyUsed),
            Some(socket) => socket,
        };
        Ok(self.current_process.lock().put_new_udp_socket(socket))
    }

    fn sys_write_back_udp_socket(
        &mut self,
        descriptor: UserspaceArgument<UDPDescriptor>,
        buffer: UserspaceArgument<&[u8]>,
    ) -> Result<usize, SysSocketError> {
        let buffer = buffer.validate(self)?;

        descriptor.validate(self)?.with_lock(|socket| {
            let recv_ip = unwrap_or_return!(socket.get_from(), Err(SysSocketError::NoReceiveIPYet));
            let recv_port = unwrap_or_return!(
                socket.get_received_port(),
                Err(SysSocketError::NoReceiveIPYet)
            );

            // Get mac address of receiver
            // Since we already received a packet we should have it in the cache
            let destination_mac = *ARP_CACHE
                .lock()
                .get(&recv_ip)
                .expect("There must be a receiver mac already in the arp cache.");
            let constructed_packet = UdpHeader::create_udp_packet(
                recv_ip,
                recv_port,
                destination_mac,
                socket.get_port(),
                buffer,
            );
            crate::net::send_packet(constructed_packet);
            Ok(buffer.len())
        })
    }

    fn sys_read_udp_socket(
        &mut self,
        descriptor: UserspaceArgument<UDPDescriptor>,
        buffer: UserspaceArgument<&mut [u8]>,
    ) -> Result<usize, SysSocketError> {
        // Process packets
        crate::net::receive_and_process_packets();

        let buffer = buffer.validate(self)?;

        descriptor
            .validate(self)?
            .with_lock(|mut socket| Ok(socket.get_data(buffer)))
    }

    #[doc = r" Validate a pointer such that it is a valid userspace pointer"]
    fn validate_and_translate_pointer<PTR: Pointer>(&self, ptr: PTR) -> Option<PTR> {
        self.current_process.with_lock(|p| {
            let pt = p.get_page_table();
            if !pt.is_valid_userspace_ptr(ptr, true) {
                return None;
            }
            let physical_address = unwrap_or_return!(
                pt.translate_userspace_address_to_physical_address(ptr),
                None
            );
            Some(physical_address)
        })
    }
}

pub fn handle_syscall(nr: usize, arg: usize, ret: usize) -> Option<SyscallStatus> {
    let mut handler = SyscallHandler::new();
    let ret = handler.dispatch(nr, arg, ret);

    if handler.process_exit {
        None
    } else {
        Some(ret)
    }
}
