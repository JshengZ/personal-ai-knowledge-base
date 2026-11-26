use anyhow::Result;

// The job handle is owned exclusively by the engine process, never inherited by
// children. Windows closes it even after taskkill /F, terminating tracked workers.
#[cfg(windows)]
pub fn track(pid: u32) -> Result<()> {
    use anyhow::{bail, Context};
    use std::{
        mem::{size_of, zeroed},
        sync::OnceLock,
    };
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
            Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE},
        },
    };
    static JOB: OnceLock<std::result::Result<usize, String>> = OnceLock::new();
    let job = JOB.get_or_init(|| unsafe {
        let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const _,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            let error = std::io::Error::last_os_error().to_string();
            CloseHandle(handle);
            return Err(error);
        }
        Ok(handle as usize)
    });
    let handle = match job {
        Ok(value) => *value,
        Err(error) => bail!("无法创建 Windows 子进程管理组：{error}"),
    };
    unsafe {
        let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
        if process.is_null() {
            return Err(std::io::Error::last_os_error()).context("无法管理子进程");
        }
        let assigned = AssignProcessToJobObject(handle as _, process);
        let error = std::io::Error::last_os_error();
        CloseHandle(process);
        if assigned == 0 {
            return Err(error).context("无法将子进程加入 Windows 生命周期管理组");
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn track(_pid: u32) -> Result<()> {
    Ok(())
}
