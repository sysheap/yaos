use crate::processes::process_list;
use alloc::collections::VecDeque;
use common::mutex::Mutex;

pub static STDIN_BUFFER: Mutex<StdinBuffer> = Mutex::new(StdinBuffer::new());

pub struct StdinBuffer {
    data: VecDeque<u8>,
}

impl StdinBuffer {
    const fn new() -> Self {
        StdinBuffer {
            data: VecDeque::new(),
        }
    }

    pub fn push(&mut self, byte: u8) {
        if process_list::notify_input(byte) {
            // We already delivered the byte to the processes waiting for it
            return;
        }
        self.data.push_back(byte);
    }

    pub fn pop(&mut self) -> Option<u8> {
        self.data.pop_front()
    }
}
