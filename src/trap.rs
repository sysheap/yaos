#[no_mangle]
extern "C" fn machine_mode_trap(mcause: usize, mtval: usize) {
    panic!(
        "Machine mode trap occurred! (mcause: {} (Reason: {})) (mtval: 0x{:x})",
        mcause,
        get_reason(mcause),
        mtval
    );
}

#[no_mangle]
extern "C" fn supervisor_mode_trap(mcause: usize, mtval: usize) {
    panic!(
        "Supervisor mode trap occurred! (mcause: {} (Reason: {})) (mtval: 0x{:x})",
        mcause,
        get_reason(mcause),
        mtval
    );
}

fn get_reason(mcause: usize) -> &'static str {
    let is_interrupt = (mcause >> 63) == 1;

    let mcause_with_cleared_interrupt_bit = (mcause << 1) >> 1;

    if is_interrupt {
        match mcause_with_cleared_interrupt_bit {
            0 => "Reserved",
            1 => "Supervisor software interrupt",
            2 => "Reserved",
            3 => "Machine software interrupt",
            4 => "Reserved",
            5 => "Supervisor timer interrupt",
            6 => "Reserved",
            7 => "Machine timer interrupt",
            8 => "Reserved",
            9 => "Supervisor external interrupt",
            10 => "Reserved",
            11 => "Machine external interrupt",
            12..=15 => "Reserved",
            _ => "Designated for platform use",
        }
    } else {
        match mcause_with_cleared_interrupt_bit {
            0 => "Instruction address misaligned",
            1 => "Instruction access fault",
            2 => "Illegal instruction",
            3 => "Breakpoint",
            4 => "Load address misaligned",
            5 => "Load access fault",
            6 => "Store/AMO address misaligned",
            7 => "Store/AMO access fault",
            8 => "Environment call from U-mode",
            9 => "Environment call from S-mode",
            10 => "Reserved",
            11 => "Environment call from M-Mode",
            12 => "Instruction page fault",
            13 => "Load page fault",
            14 => "Reserved",
            15 => "Store/AMO page fault",
            16..=23 => "Reserved",
            24..=31 => "Designated for custom use",
            32..=47 => "Reserved",
            48..=63 => "Designated for custom use",
            _ => "Reserved",
        }
    }
}
