#[derive(Default)]
pub struct LinkerInformation {
    pub text_start: usize,
    pub text_end: usize,
    pub rodata_start: usize,
    pub rodata_end: usize,
    pub data_start: usize,
    pub data_end: usize,
    pub heap_start: usize,
    pub heap_size: usize,
    pub eh_frame_start: usize,
    pub eh_frame_size: usize,
}

impl LinkerInformation {
    pub fn new() -> Self {
        extern "C" {
            static mut TEXT_START: usize;
            static mut TEXT_END: usize;
            static mut RODATA_START: usize;
            static mut RODATA_END: usize;
            static mut DATA_START: usize;
            static mut DATA_END: usize;

            static mut HEAP_START: usize;
            static mut HEAP_SIZE: usize;

            static mut EH_FRAME_START: usize;
            static mut EH_FRAME_SIZE: usize;
        }

        if cfg!(miri) {
            Self::default()
        } else {
            // SAFETY: We only read information from the linker built into the binary
            // this is always safe
            unsafe {
                Self {
                    text_start: TEXT_START,
                    text_end: TEXT_END,
                    rodata_start: RODATA_START,
                    rodata_end: RODATA_END,
                    data_start: DATA_START,
                    data_end: DATA_END,
                    heap_start: HEAP_START,
                    heap_size: HEAP_SIZE,
                    eh_frame_start: EH_FRAME_START,
                    eh_frame_size: EH_FRAME_SIZE,
                }
            }
        }
    }

    pub fn text_size(&self) -> usize {
        self.text_end - self.text_start
    }

    pub fn rodata_size(&self) -> usize {
        self.rodata_end - self.rodata_start
    }

    pub fn data_size(&self) -> usize {
        self.data_end - self.data_start
    }
}