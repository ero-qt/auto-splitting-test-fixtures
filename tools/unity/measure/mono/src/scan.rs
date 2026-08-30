//! Finding a value in live memory, and proving a candidate by agreement.

use std::ffi::c_int;

/// Every offset whose word equals the needle.
pub fn word_scan(base: *const u8, span: usize, needle: u64) -> Vec<u32> {
    let mut found = Vec::new();
    for at in (0..span).step_by(8) {
        let word = unsafe { base.add(at).cast::<u64>().read_unaligned() };
        if word == needle {
            found.push(at as u32);
        }
    }
    found
}

/// Every offset whose i32 equals the needle.
pub fn int_scan(base: *const u8, span: usize, needle: i32) -> Vec<u32> {
    let mut found = Vec::new();
    for at in (0..span.saturating_sub(3)).step_by(4) {
        let word = unsafe { base.add(at).cast::<i32>().read_unaligned() };
        if word == needle {
            found.push(at as u32);
        }
    }
    found
}

/// Every offset whose low three bits equal the needle's, byte-granular.
pub fn kind_scan(base: *const u8, span: usize, needle: u8) -> Vec<u32> {
    let mut found = Vec::new();
    for at in 0..span {
        let byte = unsafe { base.add(at).read() };
        if byte & 7 == needle & 7 {
            found.push(at as u32);
        }
    }
    found
}

/// Every offset whose byte equals the needle exactly.
pub fn byte_scan(base: *const u8, span: usize, needle: u8) -> Vec<u32> {
    let mut found = Vec::new();
    for at in 0..span {
        if unsafe { base.add(at).read() } == needle {
            found.push(at as u32);
        }
    }
    found
}

/// Whether a span can be read, so a scanned word that merely looks like a
/// pointer is never followed somewhere that faults.
///
/// The kernel is made to do the reading, by handing it the ends of the span
/// to copy out of. It answers for what a read would really do, where asking
/// whether the pages are mapped would let a guard page through and then fault
/// on it. The ends are enough: a span reaches across at most one boundary
/// worth caring about, and a wild pointer misses at its first byte.
pub fn readable(at: *const u8, span: usize) -> bool {
    if at.is_null() || span == 0 {
        return false;
    }

    let (out, back) = probe();
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
    let last = span - 1;

    let mut at_byte = 0;
    loop {
        let byte = at.wrapping_add(at_byte);
        let taken = unsafe { libc::write(out, byte.cast(), 1) };

        // Whatever the kernel accepted has to come back out, or the pipe fills
        // up and a later probe blocks on it forever.
        if taken == 1 {
            let mut sink = 0_u8;
            unsafe { libc::read(back, (&raw mut sink).cast(), 1) };
        } else {
            return false;
        }

        if at_byte == last {
            return true;
        }
        at_byte = at_byte.saturating_add(page).min(last);
    }
}

/// The pipe the probe copies into, one per thread so that two of them cannot
/// read each other's byte and then block waiting for their own.
fn probe() -> (c_int, c_int) {
    thread_local! {
        static PROBE: (c_int, c_int) = {
            let mut ends = [0_i32; 2];
            let opened = unsafe { libc::pipe(ends.as_mut_ptr()) };
            assert!(opened == 0, "the probe pipe did not open");
            (ends[1], ends[0])
        };
    }

    PROBE.with(|ends| *ends)
}

/// The offsets every witness agrees on.
pub fn agreed(witnesses: impl IntoIterator<Item = Vec<u32>>) -> Vec<u32> {
    let mut rounds = witnesses.into_iter();
    let mut agreed = rounds.next().unwrap_or_default();
    for round in rounds {
        agreed.retain(|at| round.contains(at));
    }
    agreed
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::ffi::c_void;
    use std::ptr;

    fn page() -> usize {
        unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize }
    }

    /// Two pages of anonymous memory, readable, to take apart per test.
    fn pages() -> *mut u8 {
        let at = unsafe {
            libc::mmap(
                ptr::null_mut(),
                2 * page(),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };
        assert!(at != libc::MAP_FAILED, "the test pages did not map");
        at.cast()
    }

    fn unmap(at: *mut u8) {
        unsafe { libc::munmap(at.cast::<c_void>(), 2 * page()) };
    }

    #[test]
    fn mapped_memory_reads() {
        let at = pages();
        assert!(readable(at, 1));
        assert!(readable(at, 2 * page()));
        unmap(at);
    }

    #[test]
    fn nothing_reads_from_nowhere() {
        assert!(!readable(ptr::null(), 1));
        assert!(!readable(pages(), 0));
    }

    #[test]
    fn unmapped_memory_does_not_read() {
        let at = pages();
        unmap(at);
        assert!(!readable(at, 1));
    }

    // A page can be mapped and still fault on a read, which is what a guard
    // page is. Asking only whether it is mapped answers yes and then faults.
    #[test]
    fn mapped_but_unreadable_memory_does_not_read() {
        let at = pages();
        assert_eq!(
            unsafe { libc::mprotect(at.cast::<c_void>(), page(), libc::PROT_NONE) },
            0,
        );
        assert!(!readable(at, 1));
        unmap(at);
    }

    // A span is only readable if all of it is. Checking its ends misses a
    // span that starts somewhere real and runs off into something that is
    // not, which is what an oversized claim looks like.
    #[test]
    fn spans_reaching_past_their_page_do_not_read() {
        let at = pages();
        assert_eq!(
            unsafe {
                libc::mprotect(
                    at.wrapping_add(page()).cast::<c_void>(),
                    page(),
                    libc::PROT_NONE,
                )
            },
            0,
        );

        assert!(readable(at, page()));
        assert!(!readable(at, 2 * page()));
        unmap(at);
    }

    #[test]
    fn scans_find_every_offset_holding_the_needle() {
        let mut bytes = vec![0_u8; 0x40];
        bytes[0x10..0x18].copy_from_slice(&0x1234_5678_9ABC_DEF0_u64.to_le_bytes());
        bytes[0x28..0x30].copy_from_slice(&0x1234_5678_9ABC_DEF0_u64.to_le_bytes());
        bytes[0x08..0x0C].copy_from_slice(&(-7_i32).to_le_bytes());
        bytes[0x21] = 0x15;

        let at = bytes.as_ptr();
        assert_eq!(word_scan(at, 0x40, 0x1234_5678_9ABC_DEF0), [0x10, 0x28]);
        assert_eq!(int_scan(at, 0x40, -7), [0x08]);
        assert_eq!(byte_scan(at, 0x40, 0x15), [0x21]);
    }

    // The kind scan matches the low bits, since the rest of the byte carries
    // flags that differ between two classes of the same kind. So 0x15 counts
    // and 0x03 does not, though both are one bit away from the needle.
    #[test]
    fn the_kind_scan_matches_the_low_bits_alone() {
        let bytes = [0x00_u8, 0x15, 0x03, 0x05];
        let found = kind_scan(bytes.as_ptr(), bytes.len(), 0x05);
        assert_eq!(found, [1, 3]);
    }

    #[test]
    fn agreement_keeps_only_what_every_witness_found() {
        let agreed = agreed([vec![0x8, 0x10, 0x18], vec![0x10, 0x18], vec![0x18, 0x20]]);
        assert_eq!(agreed, [0x18]);
        assert!(agreed_is_empty_without_witnesses());
    }

    fn agreed_is_empty_without_witnesses() -> bool {
        agreed(Vec::<Vec<u32>>::new()).is_empty()
    }
}
