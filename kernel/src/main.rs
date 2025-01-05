#![cfg_attr(not(miri), no_std)]
#![cfg_attr(not(miri), no_main)]
#![cfg_attr(miri, allow(dead_code))]
#![cfg_attr(miri, allow(unused_imports))]
#![cfg_attr(miri, allow(unused_macros))]
#![cfg_attr(test, allow(dead_code))]
#![cfg_attr(test, allow(unused_imports))]
#![feature(nonzero_ops)]
#![feature(custom_test_frameworks)]
#![feature(let_chains)]
#![feature(vec_into_raw_parts)]
#![feature(assert_matches)]
#![feature(map_try_insert)]
#![feature(naked_functions)]
#![feature(new_range_api)]
#![feature(ptr_metadata)]
#![feature(macro_metavar_expr_concat)]
#![feature(generic_arg_infer)]
#![feature(str_from_raw_parts)]
#![test_runner(test::test_runner)]
#![reexport_test_harness_main = "test_main"]

use crate::{
    interrupts::plic, io::uart::QEMU_UART, memory::page_tables, pci::enumerate_devices,
    processes::timer,
};
use alloc::vec::Vec;
use asm::wfi_loop;
use cpu::Cpu;
use debugging::{backtrace, symbols};
use device_tree::get_devicetree_range;
use memory::page_tables::MappingDescription;
use processes::process_table;

mod asm;
mod assert;
mod autogenerated;
mod cpu;
mod debugging;
mod device_tree;
mod drivers;
mod interrupts;
mod io;
mod klibc;
mod logging;
mod memory;
mod net;
mod panic;
mod pci;
mod processes;
mod sbi;
mod syscalls;

mod test;

#[macro_use]
extern crate alloc;

#[unsafe(no_mangle)]
extern "C" fn kernel_init(hart_id: usize, device_tree_pointer: *const ()) -> ! {
    cpu::STARTING_CPU_ID.initialize(hart_id);

    QEMU_UART.lock().init();

    info!("Hello World from YaOS!\n");
    info!("Device Tree Pointer: {:p}", device_tree_pointer);

    let version = sbi::extensions::base_extension::sbi_get_spec_version();
    info!("SBI version {}.{}", version.major, version.minor);
    assert!(
        (version.major == 0 && version.minor >= 2) || version.major > 0,
        "Supported SBI Versions >= 0.2"
    );

    let num_cpus = sbi::extensions::hart_state_extension::get_number_of_harts();
    info!("Number of Cores: {num_cpus}");

    symbols::init();
    device_tree::init(device_tree_pointer);
    let device_tree_range = get_devicetree_range();

    memory::init_page_allocator(&[device_tree_range]);

    backtrace::init();
    processes::timer::init();

    #[cfg(test)]
    test_main();

    let pci_information = pci::parse().expect("pci information must be parsable");

    {
        let pci_space_64_bit = pci_information
            .get_first_range_for_type(pci::PCIBitField::MEMORY_SPACE_64_BIT_CODE)
            .expect("There must be a 64 bit allocation space.");
        let mut pci_allocator = pci::PCI_ALLOCATOR_64_BIT.lock();
        pci_allocator.init(pci_space_64_bit);
    }

    let mut runtime_mapping = Vec::new();

    runtime_mapping.push(MappingDescription {
        virtual_address_start: pci_information.pci_host_bridge_address,
        size: pci_information.pci_host_bridge_length,
        privileges: page_tables::XWRMode::ReadWrite,
        name: "PCI Space",
    });

    for range in &pci_information.ranges {
        runtime_mapping.push(MappingDescription {
            virtual_address_start: range.cpu_address,
            size: range.size,
            privileges: page_tables::XWRMode::ReadWrite,
            name: "PCI Range",
        });
    }

    memory::initialize_runtime_mappings(&runtime_mapping);

    process_table::init();

    Cpu::write_sscratch(Cpu::init(hart_id) as usize);

    Cpu::current().activate_kernel_page_table();

    plic::init_uart_interrupt(hart_id);

    let mut pci_devices = enumerate_devices(&pci_information);

    if let Some(network_device) = pci_devices.network_devices.pop() {
        let network_device = drivers::virtio::net::NetworkDevice::initialize(network_device)
            .expect("Initialization must work.");

        net::assign_network_device(network_device);
    }

    start_other_harts(hart_id, num_cpus);

    info!("kernel_init done! Enabling interrupts");

    prepare_for_scheduling();
}

#[no_mangle]
pub extern "C" fn prepare_for_scheduling() -> ! {
    // Enable all interrupts
    Cpu::write_sie(usize::MAX);

    // Enable global interrupts
    Cpu::csrs_sstatus(0b10);

    timer::set_timer(0);

    wfi_loop();
}

fn start_other_harts(current_hart_id: usize, number_of_cpus: usize) {
    extern "C" {
        fn start_hart();
    }
    for cpu_id in 0..number_of_cpus {
        if cpu_id == current_hart_id {
            continue;
        }

        info!("Starting cpu {cpu_id}");
        let cpu_struct = Cpu::init(cpu_id);
        sbi::extensions::hart_state_extension::start_hart(
            cpu_id,
            start_hart as usize,
            cpu_struct as usize,
        )
        .assert_success();
    }
}
