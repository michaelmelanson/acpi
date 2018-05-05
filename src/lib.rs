#![no_std]

#[cfg(test)]
extern crate std;

mod rsdp;
mod sdt;

use core::ops::Deref;
use core::ptr::NonNull;
use rsdp::Rsdp;

#[derive(Debug)]
pub enum AcpiError
{
    RsdpIncorrectSignature,
    RsdpInvalidOemId,
    RsdpInvalidChecksum,
}

/// Describes a physical mapping created by `AcpiHandler::map_physical_region` and unmapped by
/// `AcpiHandler::unmap_physical_region`. The region mapped must be at least `mem::size_of::<T>()`
/// bytes, but may be bigger.
pub struct PhysicalMapping<T>
{
    pub physical_start  : usize,
    pub virtual_start   : NonNull<T>,
    pub mapped_length   : usize,    // Differs from `region_length` if padding is added to align to page boundaries
}

impl<T> Deref for PhysicalMapping<T>
{
    type Target = T;

    fn deref(&self) -> &T
    {
        unsafe
        {
            self.virtual_start.as_ref()
        }
    }
}

/// The kernel must provide an implementation of this trait for `acpi` to interface with. It has
/// utility methods `acpi` uses to for e.g. mapping physical memory, but also an interface for
/// `acpi` to tell the kernel about the tables it's parsing, such as how the kernel should
/// configure the APIC or PCI routing.
pub trait AcpiHandler
{
    /// Given a starting physical address, map a region of physical memory that contains a `T`
    /// somewhere in the virtual address space. The address doesn't have to be page-aligned, so
    /// the implementation may have to add padding to either end.
    fn map_physical_region<T>(&mut self, physical_address : usize) -> PhysicalMapping<T>;

    /// Unmap the given physical mapping. Safe because we consume the mapping, and so it can't be
    /// used after being passed to this function.
    fn unmap_physical_region<T>(&mut self, region : PhysicalMapping<T>);
}

/// This is the entry point of `acpi`. Given the **physical** address of the RSDP, it parses all
/// the SDTs in the RSDT, calling the relevant handlers in the implementation's `AcpiHandler`.
pub fn parse_acpi<T>(handler : &mut T, rsdp_address : usize) -> Result<(), AcpiError>
    where T : AcpiHandler
{
    let rsdp_mapping = handler.map_physical_region::<Rsdp>(rsdp_address);
    (*rsdp_mapping).validate()?;

    // TODO: map the RSDT
    // TODO: parse the RSDT

    handler.unmap_physical_region(rsdp_mapping);
    Ok(())
}

/// This module tests against idealistic sets of ACPI tables, which we construct on the fly. This
/// should cover all standard-compliant implementations eventually, but does not guarantee we can
/// handle all hardware's tables, obviously.
#[cfg(test)]
mod constructed_table_tests
{
    use std::mem;
    use std::ptr::NonNull;
    use std::boxed::Box;
    use {AcpiHandler, PhysicalMapping, parse_acpi, rsdp::Rsdp};

    const OEM_ID : &[u8; 6] = b"RUST  ";

    /*
     * We use fake physical addresses to track what is being requested. When a particular table or
     * resource is requested, we just allocate it on the heap and return the "virtual address"
     * (a pointer onto the heap).
     */
    const RSDP_ADDRESS : usize = 0x0;
    const RSDT_ADDRESS : usize = 0x1;

    struct TestHandler { }

    impl AcpiHandler for TestHandler
    {
        fn map_physical_region<T>(&mut self, physical_address : usize) -> PhysicalMapping<T>
        {
            match physical_address
            {
                RSDP_ADDRESS =>
                {
                    let rsdp = Rsdp::make_testcase(*b"RSD PTR ",
                                                   None,
                                                   *OEM_ID,
                                                   0,
                                                   RSDT_ADDRESS as u32,
                                                   0,
                                                   0x0,
                                                   None,
                                                   [0, 0, 0]
                                                  );

                    PhysicalMapping
                    {
                        physical_start  : RSDP_ADDRESS,
                        virtual_start   : unsafe
                                          {
                                              NonNull::<T>::new_unchecked(Box::into_raw(Box::new(rsdp)) as *mut T)
                                          },
                        mapped_length   : mem::size_of::<Rsdp>(),
                    }
                },

                _ => panic!("ACPI requested invalid physical address: {:#x}", physical_address),
            }
        }

        fn unmap_physical_region<T>(&mut self, region : PhysicalMapping<T>)
        {
            match region.physical_start
            {
                RSDP_ADDRESS =>
                {
                    let _ = unsafe { Box::from_raw(region.virtual_start.as_ptr()) };
                },

                address => panic!("ACPI tried to unmap a region not created by test harness: {:#x}", address),
            }
        }
    }

    #[test]
    fn test_constructed_tables()
    {
        let mut test_handler = TestHandler { };
        match parse_acpi(&mut test_handler, RSDP_ADDRESS)
        {
            Ok(_) => (),
            Err(err) =>
            {
                panic!("Failed to parse ACPI: {:#?}", err);
            },
        }
    }
}
