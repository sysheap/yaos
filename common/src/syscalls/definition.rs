use crate::{net::UDPDescriptor, scalar_enum};

use super::macros::syscalls;

#[derive(Debug)]
#[repr(usize)]
pub enum SysWaitError {
    InvalidPid,
}

#[derive(Debug)]
#[repr(usize)]
pub enum SysExecuteError {
    InvalidPtr,
    InvalidProgram,
}

#[derive(Debug)]
#[repr(usize)]
pub enum SysSocketError {
    PortAlreadyUsed,
    InvalidPtr,
    InvalidDescriptor,
    NoReceiveIPYet,
}

scalar_enum! {
    #[repr(usize)]
    #[derive(Debug, PartialEq, Eq)]
    pub enum SyscallStatus {
        Success,
        InvalidSyscallNumber,
        InvalidArgPtr,
        InvalidRetPtr,
    }
}

syscalls!(
    sys_write_char(c: char) -> ();
    sys_read_input() -> Option<u8>;
    sys_read_input_wait() -> u8;
    sys_exit(status: isize) -> ();
    // TODO: Implement slice as argument using a wrapper
    sys_execute<'a>(name: &'a str) -> Result<u64, SysExecuteError>;
    sys_wait(pid: u64) -> Result<(), SysWaitError>;
    sys_mmap_pages(number_of_pages: usize) -> *mut u8;
    sys_open_udp_socket(port: u16) -> Result<UDPDescriptor, SysSocketError>;
    sys_write_back_udp_socket<'a>(descriptor: UDPDescriptor, buffer: &'a [u8]) -> Result<usize, SysSocketError>;
    sys_read_udp_socket<'a>(descriptor: UDPDescriptor, buffer: &'a mut [u8]) -> Result<usize, SysSocketError>;
    sys_panic() -> ();
    sys_print_programs() -> ();
);
