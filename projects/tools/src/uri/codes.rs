use super::{
    image::Image,
    links::{self, Link},
    Result,
};
use std::ffi::{c_char, c_int, c_uint, c_ulong, c_void};
use std::sync::atomic::{AtomicBool, Ordering};

// The pinned nixpkgs ZBar C API. Scanner, image and symbol handles stay on
// this job's thread, and Rust owns the grayscale buffer throughout the scan.
#[link(name = "zbar")]
extern "C" {
    fn zbar_image_scanner_create() -> *mut c_void;
    fn zbar_image_scanner_destroy(scanner: *mut c_void);
    fn zbar_image_scanner_set_config(
        scanner: *mut c_void,
        symbol: c_int,
        config: c_int,
        value: c_int,
    ) -> c_int;
    fn zbar_image_create() -> *mut c_void;
    fn zbar_image_destroy(image: *mut c_void);
    fn zbar_image_set_format(image: *mut c_void, format: c_ulong);
    fn zbar_image_set_size(image: *mut c_void, width: c_uint, height: c_uint);
    fn zbar_image_set_data(
        image: *mut c_void,
        data: *const c_void,
        size: c_ulong,
        cleanup: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn zbar_scan_image(scanner: *mut c_void, image: *mut c_void) -> c_int;
    fn zbar_image_first_symbol(image: *const c_void) -> *const c_void;
    fn zbar_symbol_next(symbol: *const c_void) -> *const c_void;
    fn zbar_symbol_get_data(symbol: *const c_void) -> *const c_char;
    fn zbar_symbol_get_data_length(symbol: *const c_void) -> c_uint;
    fn zbar_symbol_get_loc_size(symbol: *const c_void) -> c_uint;
    fn zbar_symbol_get_loc_x(symbol: *const c_void, index: c_uint) -> c_int;
    fn zbar_symbol_get_loc_y(symbol: *const c_void, index: c_uint) -> c_int;
}

struct Scanner(*mut c_void);
impl Drop for Scanner {
    fn drop(&mut self) {
        unsafe { zbar_image_scanner_destroy(self.0) }
    }
}

struct Frame(*mut c_void);
impl Drop for Frame {
    fn drop(&mut self) {
        unsafe { zbar_image_destroy(self.0) }
    }
}

pub fn scan(image: &Image, output: &str, cancel: &AtomicBool) -> Result<Vec<Link>> {
    let gray = image.grayscale(0, image.height);
    if cancel.load(Ordering::Relaxed) {
        return Ok(vec![]);
    }
    let scanner = unsafe { zbar_image_scanner_create() };
    if scanner.is_null() {
        return Err("cannot create barcode scanner".into());
    }
    let scanner = Scanner(scanner);
    let frame = unsafe { zbar_image_create() };
    if frame.is_null() {
        return Err("cannot create barcode image".into());
    }
    let frame = Frame(frame);
    unsafe {
        // Enable supported symbologies and collect geometry. Scan the whole
        // output once: a QR code can be taller than an OCR strip's overlap.
        if zbar_image_scanner_set_config(scanner.0, 0, 0, 1) != 0
            || zbar_image_scanner_set_config(scanner.0, 0, 0x80, 1) != 0
        {
            return Err("cannot configure barcode scanner".into());
        }
        zbar_image_set_format(frame.0, u32::from_le_bytes(*b"Y800") as c_ulong);
        zbar_image_set_size(frame.0, image.width as c_uint, image.height as c_uint);
        zbar_image_set_data(frame.0, gray.as_ptr().cast(), gray.len() as c_ulong, None);
        if zbar_scan_image(scanner.0, frame.0) < 0 {
            return Err("barcode scan failed".into());
        }
        let mut found = vec![];
        let mut symbol = zbar_image_first_symbol(frame.0);
        while !symbol.is_null() && !cancel.load(Ordering::Relaxed) {
            let length = zbar_symbol_get_data_length(symbol) as usize;
            let data = zbar_symbol_get_data(symbol);
            let points = zbar_symbol_get_loc_size(symbol);
            if !data.is_null() && length > 0 && points > 0 {
                // Skip binary payloads that cannot be represented as text;
                // never silently replace bytes in what the user will copy.
                if let Ok(text) =
                    std::str::from_utf8(std::slice::from_raw_parts(data.cast(), length))
                {
                    let mut left = image.width as i32;
                    let mut top = image.height as i32;
                    let mut right = 0;
                    let mut bottom = 0;
                    for i in 0..points {
                        let x = zbar_symbol_get_loc_x(symbol, i).clamp(0, image.width as i32);
                        let y = zbar_symbol_get_loc_y(symbol, i).clamp(0, image.height as i32);
                        left = left.min(x);
                        right = right.max(x);
                        top = top.min(y);
                        bottom = bottom.max(y);
                    }
                    if right > left && bottom > top {
                        found.push(Link {
                            uri: links::destination(text).unwrap_or_default(),
                            text: text.into(),
                            code: true,
                            output: output.into(),
                            x0: f64::from(left) / image.width as f64,
                            y0: f64::from(top) / image.height as f64,
                            w: f64::from(right - left) / image.width as f64,
                            h: f64::from(bottom - top) / image.height as f64,
                            number: 0,
                        });
                    }
                }
            }
            symbol = zbar_symbol_next(symbol);
        }
        found.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
        Ok(found)
    }
}
