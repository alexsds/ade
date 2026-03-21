use std::ffi::CStr;
use std::path::PathBuf;

/// Get the foreground process group ID for a PTY master file descriptor.
/// Uses tcgetpgrp(3) to query the terminal's foreground process group.
pub fn foreground_pgid(master_fd: i32) -> Option<i32> {
    // SAFETY: tcgetpgrp is a standard POSIX call that returns the foreground
    // process group ID for the given terminal FD, or -1 on error.
    let pgid = unsafe { libc::tcgetpgrp(master_fd) };
    if pgid > 0 { Some(pgid) } else { None }
}

/// Get the process name for a given PID.
/// Uses proc_name(3) from macOS libproc.
pub fn process_name(pid: i32) -> Option<String> {
    let mut name_buf = [0u8; 256];
    // SAFETY: proc_name writes into the provided buffer up to the given size.
    // Returns the length written on success, or 0 on failure.
    let ret = unsafe {
        libc::proc_name(
            pid,
            name_buf.as_mut_ptr() as *mut libc::c_void,
            name_buf.len() as u32,
        )
    };
    if ret > 0 {
        let name = CStr::from_bytes_until_nul(&name_buf)
            .ok()?
            .to_str()
            .ok()?;
        Some(name.to_string())
    } else {
        None
    }
}

/// Get the current working directory for a given PID.
/// Uses proc_pidinfo with PROC_PIDVNODEPATHINFO.
pub fn process_cwd(pid: i32) -> Option<PathBuf> {
    // SAFETY: We zero-initialize the struct and pass its exact size.
    // proc_pidinfo writes into the struct on success.
    let mut vnode_info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let ret = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            &mut vnode_info as *mut _ as *mut libc::c_void,
            std::mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int,
        )
    };
    if ret <= 0 {
        return None;
    }
    // vip_path is [[c_char; 32]; 32] which is a contiguous 1024-byte buffer (MAXPATHLEN).
    // Cast to flat pointer and scan for nul terminator (Pitfall 5 from RESEARCH).
    let path_ptr = vnode_info.pvi_cdir.vip_path.as_ptr() as *const u8;
    // SAFETY: The vip_path array is 1024 bytes; we read up to that many bytes.
    let path_bytes = unsafe { std::slice::from_raw_parts(path_ptr, 1024) };
    let nul_pos = path_bytes.iter().position(|&b| b == 0)?;
    let path_str = std::str::from_utf8(&path_bytes[..nul_pos]).ok()?;
    Some(PathBuf::from(path_str))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_name_current_process() {
        let pid = std::process::id() as i32;
        let name = process_name(pid);
        assert!(name.is_some(), "process_name should return Some for current process");
        let name = name.unwrap();
        assert!(!name.is_empty(), "process_name should not be empty");
    }

    #[test]
    fn test_process_cwd_current_process() {
        let pid = std::process::id() as i32;
        let cwd = process_cwd(pid);
        assert!(cwd.is_some(), "process_cwd should return Some for current process");
        let cwd = cwd.unwrap();
        assert!(cwd.exists(), "process_cwd should return a path that exists on disk");
    }

    #[test]
    fn test_process_name_invalid_pid() {
        let name = process_name(-1);
        assert!(name.is_none(), "process_name should return None for invalid PID");
    }

    #[test]
    fn test_process_cwd_invalid_pid() {
        let cwd = process_cwd(-1);
        assert!(cwd.is_none(), "process_cwd should return None for invalid PID");
    }
}
