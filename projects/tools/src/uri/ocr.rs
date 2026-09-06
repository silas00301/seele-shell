use super::{image::Image, links::Word, Result};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

// Tesseract's stable C API, supplied by nixpkgs. Keeping this boundary here
// avoids a new crate graph and lets every worker retain its own warmed engine.
#[link(name = "tesseract")]
extern "C" {
    fn TessBaseAPICreate() -> *mut c_void;
    fn TessBaseAPIDelete(api: *mut c_void);
    fn TessBaseAPIInit2(
        api: *mut c_void,
        path: *const c_char,
        language: *const c_char,
        mode: c_int,
    ) -> c_int;
    fn TessBaseAPISetVariable(api: *mut c_void, name: *const c_char, value: *const c_char)
        -> c_int;
    fn TessBaseAPISetPageSegMode(api: *mut c_void, mode: c_int);
    fn TessBaseAPISetImage(
        api: *mut c_void,
        data: *const u8,
        width: c_int,
        height: c_int,
        channels: c_int,
        stride: c_int,
    );
    fn TessBaseAPISetSourceResolution(api: *mut c_void, ppi: c_int);
    fn TessBaseAPIRecognize(api: *mut c_void, monitor: *mut c_void) -> c_int;
    fn TessBaseAPIGetIterator(api: *mut c_void) -> *mut c_void;
    fn TessBaseAPIClear(api: *mut c_void);
    fn TessResultIteratorGetPageIterator(iter: *mut c_void) -> *mut c_void;
    fn TessResultIteratorGetUTF8Text(iter: *const c_void, level: c_int) -> *mut c_char;
    fn TessResultIteratorNext(iter: *mut c_void, level: c_int) -> c_int;
    fn TessResultIteratorDelete(iter: *mut c_void);
    fn TessPageIteratorBoundingBox(
        iter: *const c_void,
        level: c_int,
        left: *mut c_int,
        top: *mut c_int,
        right: *mut c_int,
        bottom: *mut c_int,
    ) -> c_int;
    fn TessPageIteratorIsAtBeginningOf(iter: *const c_void, level: c_int) -> c_int;
    fn TessDeleteText(text: *const c_char);
    fn TessMonitorCreate() -> *mut c_void;
    fn TessMonitorDelete(monitor: *mut c_void);
    fn TessMonitorSetCancelFunc(
        monitor: *mut c_void,
        callback: unsafe extern "C" fn(*mut c_void, c_int) -> bool,
    );
    fn TessMonitorSetCancelThis(monitor: *mut c_void, data: *mut c_void);
    fn TessMonitorSetDeadlineMSecs(monitor: *mut c_void, deadline: c_int);
}

pub struct Ocr(*mut c_void);

impl Ocr {
    pub fn new() -> Result<Self> {
        // The API is created, used and dropped on one worker thread. No raw
        // handle or iterator crosses a thread boundary.
        unsafe {
            let api = Self(TessBaseAPICreate());
            if api.0.is_null() {
                return Err("cannot allocate OCR engine".into());
            }
            api.set("debug_file", "/dev/null")?;
            api.set("load_system_dawg", "0")?;
            api.set("load_freq_dawg", "0")?;
            let path = std::env::var("TESSDATA_PREFIX")
                .ok()
                .map(CString::new)
                .transpose()?;
            if TessBaseAPIInit2(
                api.0,
                path.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                c"eng".as_ptr(),
                1,
            ) != 0
            {
                return Err("cannot load nixpkgs English OCR data".into());
            }
            TessBaseAPISetPageSegMode(api.0, 11); // sparse screen text, no orientation pass
            Ok(api)
        }
    }

    fn set(&self, name: &str, value: &str) -> Result {
        let name = CString::new(name)?;
        let value = CString::new(value)?;
        if unsafe { TessBaseAPISetVariable(self.0, name.as_ptr(), value.as_ptr()) } == 0 {
            return Err("unsupported OCR setting".into());
        }
        Ok(())
    }

    pub fn words(
        &mut self,
        image: &Image,
        y: usize,
        height: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<Word>> {
        unsafe extern "C" fn cancelled(data: *mut c_void, _: c_int) -> bool {
            // `cancel` is borrowed for the entire synchronous Recognize call.
            unsafe { &*data.cast::<AtomicBool>() }.load(Ordering::Relaxed)
        }
        let mut words = Vec::new();
        // Image validates dimensions and payload length, and callers create
        // strips wholly inside it. SetImage copies pixels into the engine.
        if y + height > image.height || height == 0 {
            return Err("invalid OCR strip".into());
        }
        unsafe {
            TessBaseAPISetImage(
                self.0,
                image.bytes.as_ptr().add(image.offset + y * image.width * 3),
                image.width as c_int,
                height as c_int,
                3,
                (image.width * 3) as c_int,
            );
            TessBaseAPISetSourceResolution(self.0, 96);
            let monitor = TessMonitorCreate();
            if monitor.is_null() {
                TessBaseAPIClear(self.0);
                return Err("cannot allocate OCR monitor".into());
            }
            TessMonitorSetCancelThis(monitor, (cancel as *const AtomicBool).cast_mut().cast());
            TessMonitorSetCancelFunc(monitor, cancelled);
            TessMonitorSetDeadlineMSecs(monitor, 5000);
            let status = TessBaseAPIRecognize(self.0, monitor);
            TessMonitorDelete(monitor);
            if status != 0 || cancel.load(Ordering::Relaxed) {
                TessBaseAPIClear(self.0);
                return Err("OCR cancelled or timed out".into());
            }
            let iter = TessBaseAPIGetIterator(self.0);
            if !iter.is_null() {
                let page = TessResultIteratorGetPageIterator(iter);
                loop {
                    let text = TessResultIteratorGetUTF8Text(iter, 3); // word
                    if !text.is_null() {
                        let mut word = Word {
                            text: CStr::from_ptr(text).to_string_lossy().into_owned(),
                            left: 0,
                            top: 0,
                            right: 0,
                            bottom: 0,
                            line_start: TessPageIteratorIsAtBeginningOf(page, 2) != 0,
                        };
                        TessDeleteText(text);
                        if TessPageIteratorBoundingBox(
                            page,
                            3,
                            &mut word.left,
                            &mut word.top,
                            &mut word.right,
                            &mut word.bottom,
                        ) != 0
                        {
                            words.push(word);
                        }
                    }
                    if TessResultIteratorNext(iter, 3) == 0 {
                        break;
                    }
                }
                TessResultIteratorDelete(iter);
            }
            TessBaseAPIClear(self.0); // release pixels and text, retain the model
        }
        Ok(words)
    }
}

impl Drop for Ocr {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { TessBaseAPIDelete(self.0) };
        }
    }
}
