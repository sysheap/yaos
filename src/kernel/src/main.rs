#![cfg_attr(not(miri), no_std)]
#![cfg_attr(not(miri), no_main)]
#![cfg_attr(miri, allow(dead_code))]
#![cfg_attr(miri, allow(unused_imports))]
#![feature(panic_info_message)]
#![feature(strict_provenance)]
#![feature(nonzero_ops)]
#![feature(custom_test_frameworks)]
#![feature(const_mut_refs)]
#![feature(option_take_if)]
#![feature(non_null_convenience)]
#![feature(pointer_is_aligned)]
#![feature(exposed_provenance)]
#![feature(lazy_cell)]
#![feature(let_chains)]
#![feature(vec_into_raw_parts)]
#![test_runner(test::test_runner)]
#![reexport_test_harness_main = "test_main"]

use alloc::vec::Vec;

use crate::{
    interrupts::plic,
    io::uart::QEMU_UART,
    memory::{page_tables, RuntimeMapping},
    pci::enumerate_devices,
    processes::{scheduler, timer},
};

mod asm;
mod assert;
mod autogenerated;
mod cpu;
mod debug;
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

extern "C" {
    static HEAP_START: usize;
    static HEAP_SIZE: usize;
}

#[no_mangle]
extern "C" fn kernel_init(hart_id: usize, device_tree_pointer: *const ()) {
    QEMU_UART.lock().init();

    println!("Hello World from YaROS!\n");
    println!("Hart ID: {}", hart_id);
    println!("Device Tree Pointer: {:p}", device_tree_pointer);

    let version = sbi::extensions::base_extension::sbi_get_spec_version();
    info!("SBI version {}.{}", version.major, version.minor);
    assert!(
        (version.major == 0 && version.minor >= 2) || version.major > 0,
        "Supported SBI Versions >= 0.2"
    );

    let dtb = device_tree::parse_and_copy(device_tree_pointer);

    unsafe {
        info!("Initializing page allocator");
        info!(
            "Heap Start: {:#x}-{:#x} (size: {:#x})",
            HEAP_START,
            HEAP_START + HEAP_SIZE,
            HEAP_SIZE
        );
        memory::init_page_allocator(HEAP_START, HEAP_SIZE);
    }

    assert!(
        dtb.get_reserved_areas().is_empty(),
        "There should be no reserved memory regions"
    );

    #[cfg(test)]
    test_main();

    let parsed_structure_block = dtb
        .get_structure_block()
        .parse()
        .expect("DTB must be parsable");

    let pci_information =
        pci::parse(&parsed_structure_block).expect("pci information must be parsable");
    println!("pci information: {:#x?}", pci_information);

    {
        let pci_space_64_bit = pci_information
            .get_first_range_for_type(pci::PCIBitField::MEMORY_SPACE_64_BIT_CODE)
            .expect("There must be a 64 bit allocation space.");
        let mut pci_allocator = pci::PCI_ALLOCATOR_64_BIT.lock();
        pci_allocator.init(pci_space_64_bit);
    }

    let mut runtime_mapping = Vec::new();

    runtime_mapping.push(RuntimeMapping {
        virtual_address_start: pci_information.pci_host_bridge_address,
        size: pci_information.pci_host_bridge_length,
        privileges: page_tables::XWRMode::ReadWrite,
        name: "PCI Space",
    });

    for range in &pci_information.ranges {
        runtime_mapping.push(RuntimeMapping {
            virtual_address_start: range.cpu_address,
            size: range.size,
            privileges: page_tables::XWRMode::ReadWrite,
            name: "PCI Range",
        });
    }

    memory::initialize_runtime_mappings(&runtime_mapping);

    page_tables::activate_page_table(&page_tables::KERNEL_PAGE_TABLES.lock());

    interrupts::set_sscratch_to_kernel_trap_frame();

    plic::init_uart_interrupt();

    scheduler::initialize();

    let mut pci_devices = enumerate_devices(&pci_information);
    println!("Got {:#x?}", pci_devices);

    assert!(
        pci_devices.network_devices.len() == 1,
        "There should be one virtio net interface."
    );

    let network_device =
        drivers::virtio::net::NetworkDevice::initialize(pci_devices.network_devices.pop().unwrap())
            .expect("Initialization must work.");

    net::assign_network_device(network_device);

    loop {
        let packages = net::receive_packets();
        if !packages.is_empty() {
            println!("Received: {:x?}", packages);
        }
    }

    timer::set_timer(0);
}
