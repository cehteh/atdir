use cbuffer::{RxBuffer, TxBuffer};
use libc::{c_int, c_uint, gid_t, mode_t, timespec, uid_t};
use log::{info, warn};
use std::ffi::CStr;
use std::io;
use std::mem::MaybeUninit;

#[derive(Debug)]
pub struct AtDir {
    root: c_int,
}

impl Drop for AtDir {
    fn drop(&mut self) {
        info!("dropped {:?} {}", self, self.root);
        unsafe {
            libc::close(self.root);
        }
    }
}

impl AtDir {
    pub fn new(path: &CStr) -> io::Result<AtDir> {
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_DIRECTORY | libc::O_CLOEXEC) };
        if fd < 0 {
            warn!("failed {}", io::Error::last_os_error());
            Err(io::Error::last_os_error())
        } else {
            info!("created {}", fd);
            Ok(AtDir { root: fd })
        }
    }

    fn ret_fd(fd: c_int) -> io::Result<c_int> {
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(fd)
        }
    }

    fn ret_err(success: c_int) -> io::Result<()> {
        if success < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub fn open(self, pathname: &CStr, flags: c_int, mode: c_int) -> io::Result<c_int> {
        Self::ret_fd(unsafe { libc::openat(self.root, pathname.as_ptr(), flags, mode) })
    }

    pub fn close(self, fd: c_int) -> io::Result<()> {
        Self::ret_err(unsafe { libc::close(fd) })
    }

    pub fn stat(self, pathname: &CStr, flags: c_int) -> io::Result<libc::stat> {
        let mut statbuf = MaybeUninit::uninit();
        let success =
            unsafe { libc::fstatat(self.root, pathname.as_ptr(), statbuf.as_mut_ptr(), flags) };
        if success == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { statbuf.assume_init() })
        }
    }

    pub fn access(self, pathname: &CStr, mode: c_int, flags: c_int) -> io::Result<bool> {
        let success = unsafe { libc::faccessat(self.root, pathname.as_ptr(), mode, flags) };
        if success == -1 {
            let last_error = io::Error::last_os_error();
            if last_error.kind() == io::ErrorKind::PermissionDenied {
                Ok(false)
            } else {
                Err(last_error)
            }
        } else {
            Ok(true)
        }
    }

    pub fn chmod(self, pathname: &CStr, mode: mode_t, flags: c_int) -> io::Result<()> {
        Self::ret_err(unsafe { libc::fchmodat(self.root, pathname.as_ptr(), mode, flags) })
    }

    pub fn chown(
        self,
        pathname: &CStr,
        owner: uid_t,
        group: gid_t,
        flags: c_int,
    ) -> io::Result<()> {
        Self::ret_err(unsafe { libc::fchownat(self.root, pathname.as_ptr(), owner, group, flags) })
    }

    pub fn mkdir(self, pathname: &CStr, mode: mode_t) -> io::Result<()> {
        Self::ret_err(unsafe { libc::mkdirat(self.root, pathname.as_ptr(), mode) })
    }

    pub fn link(
        self,
        oldpath: &CStr,
        newdir: Option<&AtDir>,
        newpath: &CStr,
        flags: c_int,
    ) -> io::Result<()> {
        let newdir = match newdir {
            Some(newdir) => newdir.root,
            None => self.root,
        };
        Self::ret_err(unsafe {
            libc::linkat(self.root, oldpath.as_ptr(), newdir, newpath.as_ptr(), flags)
        })
    }

    // attention: reverses order of arguments to be consistent with self.symlink(link, target) syntax
    pub fn symlink(self, linkpath: &CStr, target: &CStr) -> io::Result<()> {
        Self::ret_err(unsafe { libc::symlinkat(target.as_ptr(), self.root, linkpath.as_ptr()) })
    }

    pub fn readlink<'a>(
        self,
        pathname: &CStr,
        buf: &'a mut (dyn RxBuffer + 'a),
    ) -> io::Result<&'a [u8]> {
        unsafe {
            let (ptr, len) = buf.as_c_char();
            let len = libc::readlinkat(self.root, pathname.as_ptr(), ptr, len);
            match len {
                -1 => Err(io::Error::last_os_error()),
                size if size == len => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Buffer too small",
                )),
                _ => Ok(buf.rx_done(len as usize)),
            }
        }
    }

    pub fn statx(self, pathname: &CStr, flags: c_int, mask: c_uint) -> io::Result<libc::statx> {
        let mut statbuf = MaybeUninit::uninit();
        let success = unsafe {
            libc::statx(
                self.root,
                pathname.as_ptr(),
                flags,
                mask,
                statbuf.as_mut_ptr(),
            )
        };
        if success == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { statbuf.assume_init() })
        }
    }

    pub fn unlink(self, pathname: &CStr, flags: c_int) -> io::Result<()> {
        Self::ret_err(unsafe { libc::unlinkat(self.root, pathname.as_ptr(), flags) })
    }

    pub fn utimens(self, path: &CStr, times: &timespec, flag: c_int) -> io::Result<()> {
        Self::ret_err(unsafe { libc::utimensat(self.root, path.as_ptr(), times, flag) })
    }

    pub fn fgetxattr<'a>(
        filedes: c_int,
        name: &CStr,
        value: &'a mut (dyn RxBuffer + 'a),
    ) -> io::Result<&'a [u8]> {
        unsafe {
            //TODO: resize when requested
            let (ptr, len) = value.as_c_void();
            let len = libc::fgetxattr(filedes, name.as_ptr(), ptr, len);

            if len == -1 {
                Err(io::Error::last_os_error())
            } else {
                Ok(value.rx_done(len as usize))
            }
        }
    }

    pub fn fsetxattr(
        filedes: c_int,
        name: &CStr,
        value: &dyn TxBuffer,
        flags: c_int,
    ) -> io::Result<()> {
        let (ptr, len) = value.as_c_void();
        Self::ret_err(unsafe { libc::fsetxattr(filedes, name.as_ptr(), ptr, len, flags) })
    }

    pub fn fremovexattr(filedes: c_int, name: &CStr) -> io::Result<()> {
        Self::ret_err(unsafe { libc::fremovexattr(filedes, name.as_ptr()) })
    }

    pub fn flistxattr<'a>(
        filedes: c_int,
        list: &'a mut (dyn RxBuffer + 'a),
    ) -> io::Result<&'a [u8]> {
        unsafe {
            //TODO: resize when requested
            //TODO: iterators
            let (ptr, len) = list.as_c_char();
            let len = libc::flistxattr(filedes, ptr, len);

            if len == -1 {
                Err(io::Error::last_os_error())
            } else {
                Ok(list.rx_done(len as usize))
            }
        }
    }

    pub fn rename(self, oldname: &CStr, newroot: Option<AtDir>, newname: &CStr) -> io::Result<()> {
        Self::ret_err(unsafe {
            libc::renameat(
                self.root,
                oldname.as_ptr(),
                newroot.unwrap_or(self).root,
                newname.as_ptr(),
            )
        })
    }

    //PLANNED:
    //+ mknodat(2)
    //+ mkfifoat(3)
    //+ scandirat(3)
}
