/*
Lock free shared MEMORY RING BUFFER

This module implements a highly optimized, zero-copy ipc bridge using memory-mapped files (mmap).

* Single-Producer, Multi-Consumer (SPMC) topology.
* Lock-Free Concurrency: Uses atomic memory barriers (Acquire/Release) instead of Mutexes.
* Cache-Line Aligned: Structs are aligned to 64 bytes to prevent false-sharing CPU cache misses.
* Power-of-2 Bitwise Masking: Eliminates expensive modulo division.
*/

use std::{
    fs::OpenOptions,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use common::MessageAnalyticsWriter;
use memmap2::{MmapMut, MmapOptions};
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use thiserror::Error;

//Making ring buffer usuable for hft libraries...full testing done as part of other project...add videos
pub const SLOT_SIZE: usize = 64;
const MAGIC_NUM: u64 = 0x11223344_55667788;

//Aligned to 64 bytes. no crossing cache lines
#[repr(C, align(64))]
#[derive(Debug, Copy, Clone)]
pub struct Slot {
    pub timestamp: u64, //latency
    pub len: u16,       //lenght of itch data..2 bytes
    pub data: [u8; 54], //itch messages are max ~50bytes, padding is implicit.
}

#[repr(C, align(64))]
pub struct RingHeader {
    pub magic_num: AtomicU64, //ensure consumer attaches to correct memory
    // total messages written
    pub head: AtomicU64,
}

#[derive(Error, Debug)]
pub enum RingError {
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error), //std::io::Error inside a function that returns RingError, wrap it inside RingError::Io(..)
    #[error("Shared memory is not initialised.")]
    NotInitialized,
}

//#[derive(Error, Debug)]
pub struct ShmRing {
    mmap: MmapMut,
    capacity: usize,
}

#[derive(Debug)]
pub enum ReadResult {
    Success(Slot),
    Empty,
    Reset,
    // Means the writer was too fast and fully lapped us.
    // Contains the new message to jump to.
    Overlapped(u64),
    //as read copy completed slot was in progress of being overlapped
    Interrupted(u64),
}

impl ShmRing {
    pub fn create_producer(
        shared_mem_path: &str,
        size_to_power_of_two: usize,
    ) -> Result<Self, RingError> {
        if size_to_power_of_two == 0 || (size_to_power_of_two & (size_to_power_of_two - 1)) != 0 {
            panic!("Size has to power of 2...otherwise you dont get bitmask compare");
        }

        let path = PathBuf::from(shared_mem_path);

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;

        file.set_len(size_to_power_of_two as u64)?;

        let mut mmap = unsafe { MmapOptions::new().map_mut(&file)? };

        //you wont get 2GB, OS is optimized for efficiency... uses virtual memory pointing to nothing
        //on first write, OS check virtual address in the page table, nothing there, page fault, context switch
        //to kernel 4KB is allocated, rinse/wash/reepeat until 2GB is all written to.
        //All these page faults add to latency calc...
        let ptr = mmap.as_mut_ptr();
        let len = mmap.len();
        for i in (0..len).step_by(4096) {
            unsafe { ptr.add(i).write_volatile(0) };
        }
        println!("Ring buffer shared memory allocation complete.");

        //setup header
        let header = unsafe { &mut *(mmap.as_mut_ptr() as *mut RingHeader) };

        header.head.store(0, Ordering::SeqCst);

        header.magic_num.store(MAGIC_NUM, Ordering::SeqCst);

        let capacity = size_to_power_of_two / SLOT_SIZE;

        Ok(Self { mmap, capacity })
    }

    pub fn create_consumer(
        shared_mem_path: &str,
        size_to_power_of_two: usize,
    ) -> Result<Self, RingError> {
        if size_to_power_of_two == 0 || (size_to_power_of_two & (size_to_power_of_two - 1)) != 0 {
            panic!("Size has to power of 2...otherwise you dont get bitmask compare");
        }

        let path = PathBuf::from(shared_mem_path);
        let file = OpenOptions::new().read(true).write(true).open(&path)?;
        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };

        let header = unsafe { &*(mmap.as_ptr() as *const RingHeader) };

        if header.magic_num.load(Ordering::SeqCst) != MAGIC_NUM {
            return Err(RingError::NotInitialized);
        }

        let capacity = size_to_power_of_two / SLOT_SIZE;

        Ok(Self { mmap, capacity })
    }

    #[inline(always)]
    pub fn write(&mut self, data: &[u8]) {
        if data.len() > 54 {
            panic!(
                "Market data size of {} exceeds expected slot maximimum of 54.",
                data.len(),
            );
        }

        let header = unsafe { &*(self.mmap.as_ptr() as *const RingHeader) };
        let current_head_pos = header.head.load(Ordering::Relaxed);

        //let index = (current_head_pos as usize) % CAPACITY;
        let index = (current_head_pos as usize) & (self.capacity - 1);
        let offset = SLOT_SIZE + (index * SLOT_SIZE);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        let len = data.len().min(54); // Cap at new max size (54)

        unsafe {
            // Convert raw pointer (*mut Slot) to mutable reference (&mut Slot)
            let slot_ptr = self.mmap.as_mut_ptr().add(offset) as *mut Slot;
            let slot = &mut *slot_ptr;

            slot.timestamp = now as u64;
            slot.len = len as u16;
            slot.data[..len].copy_from_slice(&data[..len]);
        }

        header.head.store(current_head_pos + 1, Ordering::Release);
    }

    #[inline(always)]
    pub fn read(&self, consumer_sequence_num: u64) -> ReadResult {
        let header = unsafe { &*(self.mmap.as_ptr() as *const RingHeader) };

        let head_start = header.head.load(Ordering::Acquire); //dont allow CPU speculative read of data.

        if consumer_sequence_num >= head_start {
            return ReadResult::Empty;
        }

        if head_start < consumer_sequence_num {
            return ReadResult::Reset; //producer restarted/crashed...
        }

        //producer has overwritten, handle edge e.g. integer underflow
        if head_start.saturating_sub(consumer_sequence_num) > self.capacity as u64 {
            //oldest valid message...dont return head.
            return ReadResult::Overlapped(head_start.saturating_sub(self.capacity as u64));
        }

        //Attempt to trigger interrupt condition
        //if consumer_sequence_num % 1000 == 0 {
        //    println!("Attempt trigger interrupt condition");
        //    std::thread::sleep(std::time::Duration::from_millis(10));
        //}

        //let index = (consumer_sequence_num as usize) % CAPACITY; expensive 20-80 CPU cycles...
        // capacity = 512(has to be power of 2) consumerNum = 1432 ==> 512 mod 1437 = 413
        //    1 0 1 1 0 0 1 1 1 0 1   (Sequence: 1437)
        //  & 0 0 1 1 1 1 1 1 1 1 1   (Mask:     511)
        //  -----------------------
        //    0 0 1 1 0 0 1 1 1 1 1   (Result:   413)
        let index = (consumer_sequence_num as usize) & (self.capacity - 1);
        let offset = SLOT_SIZE + (index * SLOT_SIZE);

        let slot_ptr = unsafe { self.mmap.as_ptr().add(offset) as *const Slot };
        let slot_copy = unsafe { std::ptr::read(slot_ptr) };

        // ensure that the copy is "done" before we check head again...dont allow cpu to read head first,
        std::sync::atomic::fence(Ordering::Acquire);

        //Verify that producer didnt overlap during copy.
        let head_end = header.head.load(Ordering::Relaxed); //no barrier needed 

        //if difference is > capacity a full buffer over loop has occoured.
        if head_end.saturating_sub(consumer_sequence_num) > self.capacity as u64 {
            // The data is corrupt
            return ReadResult::Interrupted(head_end.saturating_sub(self.capacity as u64));
        }

        ReadResult::Success(slot_copy)
    }
}

impl MessageAnalyticsWriter for ShmRing {
    #[inline(always)]
    fn write(&mut self, data: &[u8]) {
        self.write(data);
    }
}
