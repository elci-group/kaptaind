//! Cross-platform advisory file locking.
//!
//! Replaces `fs2::FileExt` for the lock/unlock operations used by Shark and the
//! AoC interceptor. Unix uses `flock(2)`; Windows uses `LockFileEx`/`UnlockFileEx`.

use std::fs::File;
use std::io;

/// Extension trait providing advisory file locks.
pub trait FileLockExt {
    /// Acquire an exclusive advisory lock on the file.
    fn lock_exclusive(&self) -> io::Result<()>;
    /// Release an advisory lock on the file.
    fn unlock(&self) -> io::Result<()>;
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::io::AsRawFd;

    impl FileLockExt for File {
        fn lock_exclusive(&self) -> io::Result<()> {
            let rc = unsafe { libc::flock(self.as_raw_fd(), libc::LOCK_EX) };
            if rc == 0 {
                Ok(())
            } else {
                let error = io::Error::last_os_error();
                tracing::error!(
                    ?error,
                    operation = "flock_lock",
                    "advisory file lock failed"
                );
                Err(error)
            }
        }

        fn unlock(&self) -> io::Result<()> {
            let rc = unsafe { libc::flock(self.as_raw_fd(), libc::LOCK_UN) };
            if rc == 0 {
                Ok(())
            } else {
                let error = io::Error::last_os_error();
                tracing::error!(
                    ?error,
                    operation = "flock_unlock",
                    "advisory file unlock failed"
                );
                Err(error)
            }
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::io::AsRawHandle;

    const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x0000_0002;

    #[repr(C)]
    struct OVERLAPPED {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        h_event: *mut libc::c_void,
    }

    extern "system" {
        fn LockFileEx(
            h_file: *mut libc::c_void,
            dw_flags: u32,
            dw_reserved: u32,
            n_number_of_bytes_to_lock_low: u32,
            n_number_of_bytes_to_lock_high: u32,
            lp_overlapped: *mut OVERLAPPED,
        ) -> i32;

        fn UnlockFileEx(
            h_file: *mut libc::c_void,
            dw_reserved: u32,
            n_number_of_bytes_to_unlock_low: u32,
            n_number_of_bytes_to_unlock_high: u32,
            lp_overlapped: *mut OVERLAPPED,
        ) -> i32;
    }

    fn lock_or_unlock<F>(file: &File, op: F) -> io::Result<()>
    where
        F: FnOnce(*mut libc::c_void, *mut OVERLAPPED) -> i32,
    {
        let mut overlapped = OVERLAPPED {
            internal: 0,
            internal_high: 0,
            offset: 0,
            offset_high: 0,
            h_event: std::ptr::null_mut(),
        };
        let rc = op(file.as_raw_handle(), &mut overlapped);
        if rc != 0 {
            Ok(())
        } else {
            let error = io::Error::last_os_error();
            tracing::error!(
                ?error,
                operation = "lock_file_ex",
                "Windows advisory file lock operation failed"
            );
            Err(error)
        }
    }

    impl FileLockExt for File {
        fn lock_exclusive(&self) -> io::Result<()> {
            lock_or_unlock(self, |handle, overlapped| unsafe {
                LockFileEx(
                    handle,
                    LOCKFILE_EXCLUSIVE_LOCK,
                    0,
                    u32::MAX,
                    u32::MAX,
                    overlapped,
                )
            })
        }

        fn unlock(&self) -> io::Result<()> {
            lock_or_unlock(self, |handle, overlapped| unsafe {
                UnlockFileEx(handle, 0, u32::MAX, u32::MAX, overlapped)
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;

    #[test]
    fn lock_unlock_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lockfile");
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.lock_exclusive().expect("lock");
        file.unlock().expect("unlock");
    }
}
