//! Windows 应用进程树的生命周期保护。

use std::{
    io,
    mem::size_of,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
};
use windows_sys::Win32::System::{
    JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    },
    Threading::GetCurrentProcess,
};

/// 关闭最后一个 Job 句柄时，Windows 会结束作业内的所有子孙进程。
pub(crate) struct ApplicationJob(OwnedHandle);

impl ApplicationJob {
    pub(crate) fn attach() -> io::Result<Self> {
        let job = Self::create()?;
        // SAFETY: GetCurrentProcess 返回当前进程伪句柄，可用于 Job 分配。
        job.assign(unsafe { GetCurrentProcess() })?;
        Ok(job)
    }

    fn create() -> io::Result<Self> {
        // SAFETY: 空名称与安全属性创建仅供本进程持有的 Job。
        let raw = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateJobObjectW 返回了本进程拥有的新句柄，由 OwnedHandle 关闭。
        let job = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: limits 指向有效的固定大小 Win32 结构，Job 句柄在调用期间有效。
        if unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(job))
    }

    fn assign(&self, process: windows_sys::Win32::Foundation::HANDLE) -> io::Result<()> {
        // SAFETY: self 持有有效的 Job 句柄，调用方提供有效的进程句柄。
        if unsafe { AssignProcessToJobObject(self.0.as_raw_handle(), process) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::windows::{io::AsRawHandle, process::CommandExt},
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::WAIT_OBJECT_0,
        System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, WaitForSingleObject},
    };

    #[test]
    fn current_process_can_join_job() {
        if std::env::var_os("MAGEKIT_TEST_ATTACH_JOB").is_some() {
            // 此测试在子测试进程中运行；句柄交给进程退出时的系统清理。
            std::mem::forget(ApplicationJob::attach().unwrap());
            return;
        }
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "process_job::tests::current_process_can_join_job",
            ])
            .env("MAGEKIT_TEST_ATTACH_JOB", "1")
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "当前进程无法加入 Job: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn closing_job_ends_nested_child_process() {
        let marker =
            std::env::temp_dir().join(format!("magekit-job-test-{}.pid", std::process::id()));
        let _ = fs::remove_file(&marker);
        let job = ApplicationJob::create().unwrap();
        let script = "Start-Sleep -Milliseconds 500; $child = Start-Process -FilePath powershell.exe -ArgumentList '-NoProfile -NonInteractive -Command Start-Sleep -Seconds 30' -PassThru -WindowStyle Hidden; [IO.File]::WriteAllText($env:MAGEKIT_TEST_CHILD_PID, [string]$child.Id); Start-Sleep -Seconds 30";
        let mut parent = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("MAGEKIT_TEST_CHILD_PID", &marker)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .unwrap();
        job.assign(parent.as_raw_handle()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let grandchild_pid = loop {
            if let Ok(value) = fs::read_to_string(&marker)
                && let Ok(pid) = value.trim().parse::<u32>()
            {
                break pid;
            }
            assert!(Instant::now() < deadline, "子进程未启动");
            thread::sleep(Duration::from_millis(50));
        };
        // SAFETY: 由本测试启动的进程 PID，只请求查询和等待权限。
        let raw = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | 0x0010_0000,
                0,
                grandchild_pid,
            )
        };
        assert!(!raw.is_null(), "无法打开孙进程句柄");
        // SAFETY: OpenProcess 返回的新句柄由 OwnedHandle 负责关闭。
        let grandchild = unsafe { OwnedHandle::from_raw_handle(raw) };
        drop(job);
        // SAFETY: grandchild 是有效进程句柄，超时后返回等待结果。
        assert_eq!(
            unsafe { WaitForSingleObject(grandchild.as_raw_handle(), 5_000) },
            WAIT_OBJECT_0
        );
        assert!(parent.wait().is_ok());
        let _ = fs::remove_file(marker);
    }
}
