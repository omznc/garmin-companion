//! Keep the data folder out of the iCloud and iTunes backup (iOS only).
//!
//! The cache holds health data. A backup copy of it is not wanted. This sets
//! `NSURLIsExcludedFromBackupKey`, which iOS applies to the whole folder.

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::error::CFErrorRef;
use core_foundation::string::CFStringRef;
use core_foundation::url::{CFURLRef, CFURL};
use std::os::raw::c_void;
use std::path::Path;

extern "C" {
    static kCFURLIsExcludedFromBackupKey: CFStringRef;
    fn CFURLSetResourcePropertyForKey(
        url: CFURLRef,
        key: CFStringRef,
        value: *const c_void,
        error: *mut CFErrorRef,
    ) -> u8;
}

/// Mark a folder as excluded from backup. A failure is logged, not fatal.
pub fn exclude_dir(dir: &Path) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("could not create {}: {e}", dir.display());
        return;
    }
    let Some(url) = CFURL::from_path(dir, true) else {
        return;
    };
    let value = CFBoolean::true_value();
    // SAFETY: the URL, the key and the value are valid for the whole call.
    let ok = unsafe {
        CFURLSetResourcePropertyForKey(
            url.as_concrete_TypeRef(),
            kCFURLIsExcludedFromBackupKey,
            value.as_concrete_TypeRef() as *const c_void,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        eprintln!("could not exclude {} from backup", dir.display());
    }
}
