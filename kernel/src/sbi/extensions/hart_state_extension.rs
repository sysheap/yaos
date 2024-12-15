use crate::sbi::{self};

const EID: u64 = 0x48534D;
const FID_GET_STATUS: u64 = 0x2;

pub fn get_number_of_harts() -> u64 {
    let mut harts = 0;

    loop {
        if sbi::sbi_call_1(EID, FID_GET_STATUS, harts).is_error() {
            break;
        }
        harts += 1;
    }

    harts
}
