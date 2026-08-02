//! Cross-platform disk space queries.
//!
//! Replaces `fs2::available_space` and `fs2::total_space`. Unix uses `statvfs`;
//! Windows uses `GetDiskFreeSpaceExW`.

use std::io;
use std::path::Path;

/// Available space (in bytes) for the filesystem containing `path`.
pub fn available_space(path: &Path) -> io::Result<u64> {
    space_impl(path).map(|s| s.available)
}

/// Total capacity (in bytes) of the filesystem containing `path`.
pub fn total_space(path: &Path) -> io::Result<u64> {
    space_impl(path).map(|s| s.total)
}

#[derive(Debug, Clone, Copy)]
struct Space {
    available: u64,
    total: u64,
}

#[cfg(unix)]
fn space_impl(path: &Path) -> io::Result<Space> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    unsafe {
        let mut buf: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut buf) != 0 {
            tracing::error!(
                operation = "space_impl",
                source_line = line!(),
                "space impl returned an error"
            );
            return Err(io::Error::last_os_error());
        }
        Ok(Space {
            available: buf.f_bavail as u64 * buf.f_frsize as u64,
            total: buf.f_blocks as u64 * buf.f_frsize as u64,
        })
    }
}

#[cfg(windows)]
fn space_impl(path: &Path) -> io::Result<Space> {
    use std::os::windows::ffi::OsStrExt;

    extern "system" {
        fn GetDiskFreeSpaceExW(
            lp_directory_name: *const u16,
            lp_free_bytes_available_to_caller: *mut u64,
            lp_total_number_of_bytes: *mut u64,
            lp_total_number_of_free_bytes: *mut u64,
        ) -> i32;
    }

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut available = 0u64;
    let mut total = 0u64;
    let mut free = 0u64;

    let rc = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free) };

    if rc == 0 {
        tracing::error!(
            operation = "GetDiskFreeSpaceExW",
            source_line = line!(),
            "GetDiskFreeSpaceExW returned an error"
        );
        return Err(io::Error::last_os_error());
    }

    Ok(Space { available, total })
}

#[cfg(not(any(unix, windows)))]
fn space_impl(_path: &Path) -> io::Result<Space> {
    // traci: allow -- unsupported platforms return a typed capability error to the caller.
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "disk space queries are not supported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_space_is_sane() {
        let total = total_space(Path::new(".")).expect("total_space");
        let available = available_space(Path::new(".")).expect("available_space");
        assert!(total > 0, "total space should be positive");
        assert!(
            available <= total,
            "available space should not exceed total"
        );
    }
}
