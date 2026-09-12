//! Small PCRE2 boundary for already validated Rust strings. The general bytes
//! wrapper rechecks the entire subject on every match, making dense documents
//! quadratic. This API accepts only `&str` and character-boundary offsets, so
//! PCRE2_NO_UTF_CHECK is sound. Match state is owned per call; compiled patterns
//! are immutable and shared. Callouts bound backtracking across the whole block.
use pcre2_sys::*;
use std::{
    ffi::c_void,
    ptr::{self, NonNull},
};

pub struct Budget {
    remaining: usize,
    deadline: Option<std::time::Instant>,
    pub exhausted: bool,
}
impl Budget {
    pub fn new(units: usize) -> Self {
        Self {
            remaining: 1_000_000,
            deadline: (units > 4096)
                .then(|| std::time::Instant::now() + std::time::Duration::from_millis(25)),
            exhausted: false,
        }
    }
}

// pcre2-sys does not expose the callout setter. The callback's first parameter
// is an opaque pcre2_callout_block_8 pointer: we never access its versioned data.
unsafe extern "C" {
    fn pcre2_set_callout_8(
        context: *mut pcre2_match_context_8,
        callback: Option<unsafe extern "C" fn(*mut c_void, *mut c_void) -> i32>,
        data: *mut c_void,
    ) -> i32;
}
unsafe extern "C" fn consume(_: *mut c_void, data: *mut c_void) -> i32 {
    // SAFETY: captures_read_at supplies its exclusively borrowed live Budget;
    // PCRE2 invokes callbacks synchronously and does not retain them afterward.
    let budget = unsafe { &mut *data.cast::<Budget>() };
    if budget.remaining == 0
        || (budget.remaining.is_multiple_of(128)
            && budget
                .deadline
                .is_some_and(|deadline| std::time::Instant::now() >= deadline))
    {
        budget.exhausted = true;
        PCRE2_ERROR_CALLOUT
    } else {
        budget.remaining -= 1;
        0
    }
}

pub struct Pattern(NonNull<pcre2_code_8>);
// SAFETY: compilation/JIT finish before publication; PCRE2's compiled pattern
// is read-only during matches. Every matcher has its own context and data.
unsafe impl Send for Pattern {}
unsafe impl Sync for Pattern {}
impl Drop for Pattern {
    fn drop(&mut self) {
        // SAFETY: this pointer is uniquely owned compiled PCRE2 code.
        unsafe {
            pcre2_code_free_8(self.0.as_ptr());
        }
    }
}
impl Pattern {
    pub fn new(pattern: &str) -> Self {
        let mut error = 0;
        let mut offset = 0;
        // SAFETY: valid pattern bytes, explicit length, writable error outputs.
        let code = unsafe {
            pcre2_compile_8(
                pattern.as_ptr(),
                pattern.len(),
                PCRE2_UTF | PCRE2_AUTO_CALLOUT,
                &mut error,
                &mut offset,
                ptr::null_mut(),
            )
        };
        let code = NonNull::new(code).expect("static Markdown grammar compiles");
        // SAFETY: exclusive code ownership here. A platform without JIT falls
        // back to the interpreter with the same match/callout/heap limits.
        unsafe {
            pcre2_jit_compile_8(code.as_ptr(), PCRE2_JIT_COMPLETE);
        }
        Self(code)
    }

    pub fn capture_locations(&self) -> Captures {
        // SAFETY: initialized immutable pattern; default allocator.
        let data =
            unsafe { pcre2_match_data_create_from_pattern_8(self.0.as_ptr(), ptr::null_mut()) };
        let data = NonNull::new(data)
            .unwrap_or_else(|| std::alloc::handle_alloc_error(std::alloc::Layout::new::<usize>()));
        // SAFETY: independent match context using the default allocator.
        let context = unsafe { pcre2_match_context_create_8(ptr::null_mut()) };
        let context = NonNull::new(context)
            .unwrap_or_else(|| std::alloc::handle_alloc_error(std::alloc::Layout::new::<usize>()));
        // SAFETY: context is uniquely owned and valid. PCRE2 heap limit is KiB.
        unsafe {
            pcre2_set_match_limit_8(context.as_ptr(), 250_000);
            pcre2_set_depth_limit_8(context.as_ptr(), 256);
            pcre2_set_heap_limit_8(context.as_ptr(), 1024);
        }
        Captures {
            data,
            context,
            matched: false,
        }
    }

    pub fn captures_read_at(
        &self,
        captures: &mut Captures,
        text: &str,
        start: usize,
        budget: &mut Budget,
    ) -> Result<Option<Found>, ()> {
        if budget.exhausted || !text.is_char_boundary(start) {
            return Err(());
        }
        // SAFETY: text is valid UTF-8 and start is a character boundary, the
        // compiled pattern has UTF enabled, buffers have independent ownership,
        // and the synchronous callout borrows budget only until this returns.
        let count = unsafe {
            pcre2_set_callout_8(
                captures.context.as_ptr(),
                Some(consume),
                (budget as *mut Budget).cast(),
            );
            let count = pcre2_match_8(
                self.0.as_ptr(),
                text.as_ptr(),
                text.len(),
                start,
                PCRE2_NO_UTF_CHECK,
                captures.data.as_ptr(),
                captures.context.as_ptr(),
            );
            pcre2_set_callout_8(captures.context.as_ptr(), None, ptr::null_mut());
            count
        };
        captures.matched = count > 0;
        match count {
            PCRE2_ERROR_NOMATCH => Ok(None),
            1.. => {
                let (start, end) = captures.get(0).ok_or(())?;
                Ok(Some(Found { start, end }))
            }
            _ => {
                budget.exhausted = true;
                Err(())
            }
        }
    }
}

pub struct Found {
    start: usize,
    end: usize,
}
impl Found {
    pub fn start(&self) -> usize {
        self.start
    }
    pub fn end(&self) -> usize {
        self.end
    }
}
pub struct Captures {
    data: NonNull<pcre2_match_data_8>,
    context: NonNull<pcre2_match_context_8>,
    matched: bool,
}
impl Captures {
    pub fn get(&self, index: usize) -> Option<(usize, usize)> {
        // SAFETY: data remains owned and unchanged for this shared borrow;
        // PCRE2 reports the vector's allocated pair count before indexing.
        unsafe {
            if !self.matched || index >= pcre2_get_ovector_count_8(self.data.as_ptr()) as usize {
                return None;
            }
            let vector = pcre2_get_ovector_pointer_8(self.data.as_ptr());
            let start = *vector.add(index * 2);
            let end = *vector.add(index * 2 + 1);
            (start != usize::MAX && end != usize::MAX).then_some((start, end))
        }
    }
}
impl Drop for Captures {
    fn drop(&mut self) {
        // SAFETY: both allocations are uniquely owned and no match is active.
        unsafe {
            pcre2_match_data_free_8(self.data.as_ptr());
            pcre2_match_context_free_8(self.context.as_ptr());
        }
    }
}
