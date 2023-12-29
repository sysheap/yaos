#![cfg_attr(miri, allow(unused_imports))]
use core::{
    alloc::{GlobalAlloc, Layout},
    marker::PhantomData,
    mem::offset_of,
    ptr::null_mut,
};

use common::mutex::Mutex;

use crate::{
    assert::static_assert_size,
    klibc::util::{align_up, minimum_amount_of_pages},
};

use super::{
    allocated_pages::{AllocatedPages, Ethernal, StaticAllocator, WhichAllocator},
    PAGE_SIZE,
};

type Link = Option<&'static mut FreeBlock>;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
struct AlignedSizeWithMetadata {
    size: usize,
}

impl AlignedSizeWithMetadata {
    const fn new() -> Self {
        Self { size: 0 }
    }

    const fn from_layout(layout: Layout) -> Self {
        let size = align_up(
            layout.size() + FreeBlock::METADATA_SIZE,
            FreeBlock::DATA_ALIGNMENT,
        );
        Self { size }
    }

    const fn from_pages(pages: usize) -> Self {
        Self {
            size: pages * PAGE_SIZE,
        }
    }

    const fn total_size(&self) -> usize {
        self.size
    }

    const fn data_size(&self) -> usize {
        self.size - FreeBlock::METADATA_SIZE
    }

    const fn get_remaining_size(&self, needed_size: AlignedSizeWithMetadata) -> Self {
        assert!(self.total_size() >= needed_size.total_size() + FreeBlock::MINIMUM_SIZE);
        Self {
            size: self.size - needed_size.size,
        }
    }
}

#[repr(C, align(8))]
struct FreeBlock {
    next: Link,
    size: AlignedSizeWithMetadata,
    data: u64,
}

static_assert_size!(FreeBlock, 24);

impl FreeBlock {
    const METADATA_SIZE: usize = offset_of!(FreeBlock, data);
    const DATA_ALIGNMENT: usize = 8;
    const MINIMUM_SIZE: usize = Self::METADATA_SIZE + Self::DATA_ALIGNMENT;

    const fn new() -> Self {
        Self {
            next: None,
            size: AlignedSizeWithMetadata::new(),
            data: 0,
        }
    }

    fn initialize(
        block_ptr: *mut FreeBlock,
        size: AlignedSizeWithMetadata,
    ) -> &'static mut FreeBlock {
        assert!(size.total_size() >= Self::MINIMUM_SIZE);

        let data_size = size.data_size();

        assert!(data_size >= Self::DATA_ALIGNMENT, "FreeBlock too small");
        assert!(
            data_size % Self::DATA_ALIGNMENT == 0,
            "FreeBlock not aligned (data_size={data_size})"
        );
        let block = unsafe { &mut *block_ptr };
        block.next = None;
        block.size = size;
        block
    }

    fn from_data_ptr(ptr: *mut u8) -> &'static mut FreeBlock {
        unsafe {
            let block_ptr = ptr.byte_sub(Self::METADATA_SIZE) as *mut FreeBlock;
            &mut *block_ptr
        }
    }

    fn get_data_ptr(&mut self) -> *mut u64 {
        &mut self.data
    }

    fn split(&mut self, requested_size: AlignedSizeWithMetadata) -> &'static mut FreeBlock {
        assert!(self.size.total_size() >= requested_size.total_size() + Self::MINIMUM_SIZE);
        assert!(requested_size.data_size() % Self::DATA_ALIGNMENT == 0);

        let remaining_size = self.size.get_remaining_size(requested_size);
        let new_block =
            unsafe { self.get_data_ptr().byte_add(requested_size.data_size()) as *mut FreeBlock };

        assert!(remaining_size.data_size() % Self::DATA_ALIGNMENT == 0);

        self.size = requested_size;

        Self::initialize(new_block, remaining_size)
    }
}

struct Heap<A: WhichAllocator> {
    genesis_block: FreeBlock,
    allocator: PhantomData<A>,
}

impl<A: WhichAllocator> Heap<A> {
    const fn new() -> Self {
        Self {
            genesis_block: FreeBlock::new(),
            allocator: PhantomData,
        }
    }

    fn alloc(&mut self, layout: core::alloc::Layout) -> *mut u8 {
        let requested_size = AlignedSizeWithMetadata::from_layout(layout);
        let mut block = if let Some(block) = self.find_and_remove(requested_size) {
            block
        } else {
            let pages = minimum_amount_of_pages(requested_size.total_size());
            let allocation = if let Some(allocation) = AllocatedPages::<Ethernal, A>::zalloc(pages)
            {
                allocation
            } else {
                return null_mut();
            };
            FreeBlock::initialize(
                allocation.addr().cast().as_ptr(),
                AlignedSizeWithMetadata::from_pages(pages),
            )
        };

        // Make smaller if needed
        self.split_if_necessary(&mut block, requested_size);

        block.get_data_ptr() as *mut u8
    }

    fn dealloc(&mut self, ptr: *mut u8, layout: core::alloc::Layout) {
        let free_block = FreeBlock::from_data_ptr(ptr);
        assert!(free_block.next.is_none(), "Heap metadata corruption");
        assert!(
            free_block.size.data_size() >= layout.size(),
            "Heap metadata corruption"
        );
        self.insert(free_block);
    }

    fn insert(&mut self, block: &'static mut FreeBlock) {
        assert!(block.next.is_none(), "Heap metadata corruption");
        block.next = self.genesis_block.next.take();
        self.genesis_block.next = Some(block);
    }

    fn split_if_necessary(
        &mut self,
        block: &mut &'static mut FreeBlock,
        requested_size: AlignedSizeWithMetadata,
    ) {
        let current_block_size = block.size;
        assert!(current_block_size >= requested_size);
        if (current_block_size.total_size() - requested_size.total_size()) < FreeBlock::MINIMUM_SIZE
        {
            return;
        }
        let new_block = block.split(requested_size);
        self.insert(new_block);
    }

    fn find_and_remove(
        &mut self,
        requested_size: AlignedSizeWithMetadata,
    ) -> Option<&'static mut FreeBlock> {
        let mut previous_block = &mut self.genesis_block;
        loop {
            let block = previous_block
                .next
                .take_if(|block| block.size >= requested_size)
                .map(|block| {
                    previous_block.next = block.next.take();
                    block
                });
            if block.is_some() {
                return block;
            }
            if let Some(next) = &mut previous_block.next {
                previous_block = next;
            } else {
                break;
            }
        }
        None
    }
}

struct MutexHeap<A: WhichAllocator> {
    inner: Mutex<Heap<A>>,
}

impl<A: WhichAllocator> MutexHeap<A> {
    const fn new() -> Self {
        Self {
            inner: Mutex::new(Heap::new()),
        }
    }
}

unsafe impl<A: WhichAllocator> GlobalAlloc for MutexHeap<A> {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        self.inner.lock().alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        self.inner.lock().dealloc(ptr, layout)
    }
}

#[cfg(not(miri))]
#[global_allocator]
static HEAP: MutexHeap<StaticAllocator> = MutexHeap::new();

#[cfg(test)]
mod test {
    use core::alloc::GlobalAlloc;

    use common::mutex::Mutex;

    use crate::memory::{
        allocated_pages::WhichAllocator,
        page_allocator::{Page, PageAllocator},
    };

    use super::{FreeBlock, MutexHeap, PAGE_SIZE};

    const HEAP_PAGES: usize = 8;

    static mut PAGE_ALLOC_MEMORY: [u8; PAGE_SIZE * HEAP_PAGES] = [0; PAGE_SIZE * HEAP_PAGES];
    static PAGE_ALLOC: Mutex<PageAllocator> = Mutex::new(PageAllocator::new());

    struct TestAllocator;
    impl WhichAllocator for TestAllocator {
        fn allocate(number_of_pages: usize) -> Option<core::ptr::NonNull<Page>> {
            PAGE_ALLOC.lock().alloc(number_of_pages)
        }

        fn deallocate(pages: core::ptr::NonNull<Page>) {
            PAGE_ALLOC.lock().dealloc(pages)
        }
    }

    fn init_allocator() {
        unsafe {
            PAGE_ALLOC.lock().init(&mut PAGE_ALLOC_MEMORY);
        }
    }

    fn create_heap() -> MutexHeap<TestAllocator> {
        init_allocator();
        MutexHeap::<TestAllocator>::new()
    }

    fn alloc<T>(heap: &MutexHeap<TestAllocator>) -> *mut T {
        let layout = core::alloc::Layout::new::<T>();
        let ptr = unsafe { heap.alloc(layout) as *mut T };
        if ptr.is_null() {
            return ptr;
        }
        let free_block = FreeBlock::from_data_ptr(ptr as *mut u8);
        assert!(free_block.next.is_none());
        assert!(free_block.size.data_size() >= core::mem::size_of::<T>());
        assert!(free_block.size.data_size() % FreeBlock::DATA_ALIGNMENT == 0);
        assert!(ptr as usize % FreeBlock::DATA_ALIGNMENT == 0);
        ptr
    }

    fn dealloc<T>(heap: &MutexHeap<TestAllocator>, ptr: *mut T) {
        let layout = core::alloc::Layout::new::<T>();
        unsafe { heap.dealloc(ptr as *mut u8, layout) };
    }

    #[test_case]
    fn empty_heap() {
        let heap = create_heap();
        assert!(heap.inner.lock().genesis_block.next.is_none());
    }

    #[test_case]
    fn single_allocation() {
        let heap = create_heap();
        let ptr = alloc::<u8>(&heap);
        assert!(!ptr.is_null());
        let heap = heap.inner.lock();
        let free_block = heap.genesis_block.next.as_ref().unwrap();
        assert!(free_block.next.is_none());
        assert_eq!(
            free_block.size.total_size(),
            PAGE_SIZE - FreeBlock::METADATA_SIZE - FreeBlock::DATA_ALIGNMENT
        );
    }

    #[test_case]
    fn split_block() {
        let heap = create_heap();
        let ptr1 = alloc::<u8>(&heap);
        assert!(!ptr1.is_null());

        let ptr2 = alloc::<u8>(&heap);
        assert!(!ptr2.is_null());

        let heap = heap.inner.lock();
        let free_block = heap.genesis_block.next.as_ref().unwrap();
        assert!(free_block.next.is_none());
        assert_eq!(
            free_block.size.total_size(),
            PAGE_SIZE - (2 * FreeBlock::METADATA_SIZE) - (2 * FreeBlock::DATA_ALIGNMENT)
        );
    }

    #[test_case]
    fn deallocation() {
        let heap = create_heap();
        let ptr = alloc::<u8>(&heap);
        assert!(!ptr.is_null());
        dealloc(&heap, ptr);
        let heap = heap.inner.lock();
        let free_block1 = heap.genesis_block.next.as_ref().unwrap();
        assert_eq!(free_block1.size.data_size(), FreeBlock::DATA_ALIGNMENT);

        let free_block2 = free_block1.next.as_ref().unwrap();
        assert!(free_block2.next.is_none());
        assert_eq!(
            free_block2.size.total_size(),
            PAGE_SIZE - FreeBlock::METADATA_SIZE - FreeBlock::DATA_ALIGNMENT
        );
    }

    #[test_case]
    fn alloc_exhaustion() {
        let heap = create_heap();
        // One page is metadata
        let ptr = alloc::<[u8; ((HEAP_PAGES - 1) * PAGE_SIZE) - FreeBlock::METADATA_SIZE]>(&heap);
        assert!(!ptr.is_null());

        let ptr2 = alloc::<u8>(&heap);
        assert!(ptr2.is_null());

        let heap_lock = heap.inner.lock();
        assert!(heap_lock.genesis_block.next.is_none());
        drop(heap_lock);

        dealloc(&heap, ptr);

        let ptr = alloc::<u8>(&heap);
        assert!(!ptr.is_null());

        let heap_lock = heap.inner.lock();
        let free_block = heap_lock.genesis_block.next.as_ref().unwrap();
        assert!(free_block.next.is_none());
        assert_eq!(
            free_block.size.total_size(),
            ((HEAP_PAGES - 1) * PAGE_SIZE) - FreeBlock::METADATA_SIZE - FreeBlock::DATA_ALIGNMENT
        );
    }
}
