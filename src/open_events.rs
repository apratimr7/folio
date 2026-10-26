//! Documents the desktop asks us to open while we run: on macOS, Finder's "Open", "Open With" and double-clicks
//! (and `open -a folio file.pdf`) arrive as an Apple Event, not as command-line arguments.
//!
//! winit's application delegate doesn't handle `application:openURLs:`, so the method is added to `NSObject` (the
//! delegate's base class) before the app starts; AppKit then delivers the files to it. Other platforms pass files
//! on the command line, so this is empty there.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static PENDING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static WAKE: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Files received so far.
pub fn take() -> Vec<PathBuf> {
    std::mem::take(&mut *PENDING.lock().unwrap_or_else(|e| e.into_inner()))
}

/// How to wake the UI when a file arrives (set once the window exists).
pub fn set_waker(wake: impl Fn() + Send + Sync + 'static) {
    let _ = WAKE.set(Box::new(wake));
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn push(paths: Vec<PathBuf>) {
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).extend(paths);
    if let Some(wake) = WAKE.get() {
        wake();
    }
}

#[cfg(target_os = "macos")]
pub fn install() {
    use std::ffi::{CStr, c_char};

    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::{msg_send, sel};

    /// `- (void)application:(NSApplication *)app openURLs:(NSArray<NSURL *> *)urls`
    unsafe extern "C-unwind" fn open_urls(_this: *mut AnyObject, _cmd: Sel, _app: *mut AnyObject, urls: *mut AnyObject) {
        if urls.is_null() {
            return;
        }
        let mut paths = Vec::new();
        unsafe {
            let n: usize = msg_send![urls, count];
            for i in 0..n {
                let url: *mut AnyObject = msg_send![urls, objectAtIndex: i];
                let is_file: bool = msg_send![url, isFileURL];
                if !is_file {
                    continue;
                }
                let path: *mut AnyObject = msg_send![url, path];
                let utf8: *const c_char = if path.is_null() { std::ptr::null() } else { msg_send![path, UTF8String] };
                if !utf8.is_null() {
                    paths.push(PathBuf::from(CStr::from_ptr(utf8).to_string_lossy().into_owned()));
                }
            }
        }
        push(paths);
    }

    let Some(class) = AnyClass::get(c"NSObject") else { return };
    // SAFETY: the function matches the method's type encoding (`v@:@@`: void, self, _cmd, two objects), and
    // `class_addMethod` leaves classes that already implement the selector alone.
    unsafe {
        let imp: Imp = std::mem::transmute(open_urls as unsafe extern "C-unwind" fn(_, _, _, _));
        objc2::ffi::class_addMethod(
            class as *const AnyClass as *mut AnyClass,
            sel!(application:openURLs:),
            imp,
            c"v@:@@".as_ptr(),
        );
    }
}

#[cfg(not(target_os = "macos"))]
pub fn install() {}
