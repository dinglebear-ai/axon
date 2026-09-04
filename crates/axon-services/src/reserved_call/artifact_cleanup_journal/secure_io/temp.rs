pub(super) struct TempFile {
    directory: std::os::fd::RawFd,
    pub(super) name: std::ffi::CString,
    pub(super) published: bool,
}

impl TempFile {
    pub(super) fn new(directory: std::os::fd::RawFd, name: std::ffi::CString) -> Self {
        Self {
            directory,
            name,
            published: false,
        }
    }
}

#[allow(unsafe_code)]
impl Drop for TempFile {
    fn drop(&mut self) {
        if !self.published {
            unsafe { libc::unlinkat(self.directory, self.name.as_ptr(), 0) };
        }
    }
}
