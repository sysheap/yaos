extern crate alloc;
extern crate macros;

use alloc::vec::Vec;
use macros::syscalls;

// pub struct UserpointerMut<T> {
//     ptr: *mut T,
// }

// fn handle_write_char(c: u8) -> isize {
//     0
// }

// fn handle_share_vec(vec: UserpointerMut<Vec<u8>>, additional_data: usize) -> isize {
//     0
// }

syscalls!(
    WRITE_CHAR(handle_write_char)(c: u8);
    SHARE_VEC(handle_share_vec)(vec: &mut Vec<u8>, additional_data: usize);
);

// mod userspace_helper {
//     use core::arch::asm;

//     pub fn syscall_1(nr: usize, arg1: usize) -> isize {
//         let ret: isize;
//         unsafe {
//             asm!("ecall",
//                 in("a7") nr,
//                 in("a0") arg1,
//                 lateout("a0") ret,
//             );
//         }
//         ret
//     }

//     pub fn syscall_2(nr: usize, arg1: usize, arg2: usize) -> isize {
//         let ret: isize;
//         unsafe {
//             asm!("ecall",
//                 in("a7") nr,
//                 in("a0") arg1,
//                 in("a1") arg2,
//                 lateout("a0") ret,
//             );
//         }
//         ret
//     }
// }

// mod userspace {
//     extern crate alloc;

//     use alloc::vec::Vec;

//     use super::{
//         shared::{SYS_SHARE_VEC_NR, SYS_WRITE_CHAR_NR},
//         userspace_helper::{syscall_1, syscall_2},
//     };

//     pub fn sys_write_char(c: u8) {
//         syscall_1(SYS_WRITE_CHAR_NR, c as usize);
//     }

//     pub fn sys_share_vec(vec: &mut Vec<u8>, additional_data: usize) -> isize {
//         syscall_2(SYS_SHARE_VEC_NR, vec.as_ptr() as usize, additional_data)
//     }
// }

// mod shared {
//     pub const SYS_WRITE_CHAR_NR: usize = 0;
//     pub const SYS_SHARE_VEC_NR: usize = 1;
// }

// mod kernel {
//     extern crate alloc;

//     use alloc::vec::Vec;

//     use super::{
//         kernel_impl::{sys_share_vec_impl, sys_write_char_impl},
//         shared::{SYS_SHARE_VEC_NR, SYS_WRITE_CHAR_NR},
//     };

//     struct TrapFrame {
//         a0: usize,
//         a1: usize,
//         a7: usize,
//     }

//     pub struct Userpointer<T> {
//         ptr: *mut T,
//     }

//     fn syscall_handler(trap_frame: &mut TrapFrame) -> isize {
//         match trap_frame.a0 {
//             SYS_WRITE_CHAR_NR => sys_write_char_impl(trap_frame.a0 as u8),
//             SYS_SHARE_VEC_NR => sys_share_vec_impl(
//                 Userpointer {
//                     ptr: trap_frame.a0 as *mut Vec<usize>,
//                 },
//                 trap_frame.a1,
//             ),
//             _ => panic!("Unknown syscall number: {}", trap_frame.a7),
//         }
//     }
// }

// mod kernel_impl {
//     extern crate alloc;

//     use alloc::vec::Vec;

//     use super::kernel::Userpointer;

//     pub fn sys_write_char_impl(c: u8) -> isize {
//         0
//     }

//     pub fn sys_share_vec_impl(vec: Userpointer<Vec<usize>>, additional_data: usize) -> isize {
//         0
//     }
// }
