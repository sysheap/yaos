macro_rules! getter_address {
    ($name:ident) => {
        #[cfg(not(miri))]
        pub fn $name() -> usize {
            unsafe extern "C" {
                static $name: usize;
            }
            core::ptr::addr_of!($name) as usize
        }
        #[cfg(miri)]
        pub fn $name() -> usize {
            // When running under Miri we don't have any sections
            // Just choose any value which does not collide with any
            // other mappings
            common::util::align_down(u32::MAX as usize, $crate::memory::PAGE_SIZE)
        }
    };
}

macro_rules! getter {
    ($name:ident) => {
        // The linker generates magic variables which marks section start and end in the form
        // __start_SECTION and __stop_SECTION
        getter_address!(${concat(__start_, $name)});
        getter_address!(${concat(__stop_, $name)});
        pub fn ${concat($name, _size)}() -> usize {
            Self::${concat(__stop_, $name)}() - Self::${concat(__start_, $name)}()
        }
        pub fn ${concat($name, _range)}() -> core::ops::Range<usize> {
            Self::${concat(__start_, $name)}()..Self::${concat(__stop_, $name)}()
        }
    };
}

// Idea taken by https://veykril.github.io/tlborm/decl-macros/building-blocks/counting.html
macro_rules! count_idents {
    () => { 0 };
    ($first:ident $($rest:ident)*) => {1 + count_idents!($($rest)*)};
}

macro_rules! sections {
    ($($name:ident, $xwr:expr;)*) => {
        use $crate::memory::page_tables::MappingDescription;
        use $crate::memory::page_tables::XWRMode;
        use $crate::memory::PAGE_SIZE;
        use $crate::debugging;
        use common::util::align_up;

        pub struct LinkerInformation;

        #[allow(dead_code)]
        impl LinkerInformation {
            $(getter!($name);)*

            // We don't know the end of the symbols yet because it
            // will be binary patched
            getter_address!(__start_symbols);

            // The heap will start directly page aligned after the symbols
            pub fn __start_heap() -> usize {
                align_up(debugging::symbols::symbols_end(), PAGE_SIZE)
            }

            #[cfg(not(miri))]
            pub fn all_mappings() -> [MappingDescription; count_idents!($($name)*)] {
                [
                    $(MappingDescription {
                      virtual_address_start: LinkerInformation::${concat(__start_, $name)}(),
                      size: LinkerInformation::${concat($name, _size)}(),
                      privileges: $xwr,
                      name: stringify!($name)
                    },)*
                ]
            }
            #[cfg(miri)]
            pub fn all_mappings() -> [MappingDescription; 0] {
                // When running under Miri we don't have any sections
                []
            }
        }
    };
}

sections! {
    text, XWRMode::ReadExecute;
    rodata, XWRMode::ReadOnly;
    eh_frame, XWRMode::ReadOnly;
    data, XWRMode::ReadWrite;
    bss, XWRMode::ReadWrite;
    kernel_stack, XWRMode::ReadWrite;
}
