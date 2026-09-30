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
        fs::{self, File},
        os::windows::{io::AsRawHandle, process::CommandExt},
        path::Path,
        process::{Child, Command, Stdio},
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

    const FIXTURE_TEST: &str = "process_job::tests::process_tree_fixture";
    const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
    const EXIT_TIMEOUT: Duration = Duration::from_secs(5);

    // Always kill/reap direct children, including assertion and assignment failures.
    struct ChildGuard(Child);

    impl ChildGuard {
        fn wait_for_exit(&mut self, timeout: Duration) -> io::Result<bool> {
            let deadline = Instant::now() + timeout;
            loop {
                if self.0.try_wait()?.is_some() {
                    return Ok(true);
                }
                if Instant::now() >= deadline {
                    return Ok(false);
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.wait_for_exit(EXIT_TIMEOUT);
        }
    }

    fn fixture_command(role: &str, directory: &Path) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", FIXTURE_TEST, "--nocapture"])
            .env("MAGEKIT_TEST_PROCESS_ROLE", role)
            .env("MAGEKIT_TEST_PROCESS_DIR", directory)
            .stdin(Stdio::null())
            .creation_flags(0x08000000); // CREATE_NO_WINDOW
        command
    }

    // Reuse the test executable rather than relying on PowerShell startup,
    // quoting, execution policy, or a guessed delay before Job assignment.
    #[test]
    fn process_tree_fixture() {
        let Some(role) = std::env::var_os("MAGEKIT_TEST_PROCESS_ROLE") else {
            return;
        };
        let directory = std::path::PathBuf::from(
            std::env::var_os("MAGEKIT_TEST_PROCESS_DIR").expect("fixture directory"),
        );
        if role == "grandchild" {
            fs::write(
                directory.join("grandchild.pid"),
                std::process::id().to_string(),
            )
            .expect("write grandchild readiness");
            // A finite fallback lifetime avoids leaking an orphan if the test
            // runner itself dies. It is much longer than the exit assertion.
            thread::sleep(Duration::from_secs(60));
        } else if role == "parent" {
            let deadline = Instant::now() + STARTUP_TIMEOUT;
            while !directory.join("assigned").exists() {
                assert!(
                    Instant::now() < deadline,
                    "Job assignment handshake timed out"
                );
                thread::sleep(Duration::from_millis(10));
            }
            // The controller writes assigned only after AssignProcessToJobObject
            // succeeds, so this descendant necessarily inherits the tested Job.
            let mut grandchild = ChildGuard(
                fixture_command("grandchild", &directory)
                    .spawn()
                    .expect("spawn grandchild fixture"),
            );
            assert!(
                grandchild.wait_for_exit(Duration::from_secs(65)).unwrap(),
                "grandchild fixture exceeded its fallback lifetime"
            );
        } else {
            panic!("unknown process fixture role: {role:?}");
        }
    }

    #[test]
    fn closing_job_ends_nested_child_process() {
        let directory = tempfile::tempdir().unwrap();
        let log_path = directory.path().join("fixture.log");
        let log = File::create(&log_path).unwrap();
        let mut parent = ChildGuard(
            fixture_command("parent", directory.path())
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .spawn()
                .expect("spawn parent fixture"),
        );
        // Drop the Job before the ChildGuard during unwinding, so descendants
        // are cleaned up even when readiness or a later assertion fails.
        let job = ApplicationJob::create().unwrap();
        job.assign(parent.0.as_raw_handle())
            .expect("assign parent fixture to Job");
        fs::write(directory.path().join("assigned"), b"ready").unwrap();
        let diagnostics = || fs::read_to_string(&log_path).unwrap_or_default();
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        let grandchild_pid = loop {
            if let Ok(value) = fs::read_to_string(directory.path().join("grandchild.pid"))
                && let Ok(pid) = value.trim().parse::<u32>()
            {
                break pid;
            }
            let status = parent.0.try_wait().expect("query parent fixture status");
            assert!(
                status.is_none(),
                "parent fixture exited early: {status:?}\n{}",
                diagnostics()
            );
            assert!(
                Instant::now() < deadline,
                "grandchild readiness timed out (parent PID {})\n{}",
                parent.0.id(),
                diagnostics()
            );
            thread::sleep(Duration::from_millis(10));
        };
        // SAFETY: This PID is reported by our descendant after it starts;
        // request only query and synchronization rights, and retain the handle
        // so PID reuse cannot turn the termination assertion into a false pass.
        let raw = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | 0x0010_0000,
                0,
                grandchild_pid,
            )
        };
        assert!(
            !raw.is_null(),
            "open grandchild PID {grandchild_pid}: {}\n{}",
            io::Error::last_os_error(),
            diagnostics()
        );
        // SAFETY: OpenProcess returned an owned handle.
        let grandchild = unsafe { OwnedHandle::from_raw_handle(raw) };
        // Prove that the process was still alive immediately before closing
        // the Job; an early helper failure must not pass as successful cleanup.
        // SAFETY: grandchild owns a valid process handle.
        assert_eq!(
            unsafe { WaitForSingleObject(grandchild.as_raw_handle(), 0) },
            windows_sys::Win32::Foundation::WAIT_TIMEOUT,
            "grandchild exited before Job close\n{}",
            diagnostics()
        );
        drop(job);
        // SAFETY: grandchild remains a valid process handle after termination.
        assert_eq!(
            unsafe { WaitForSingleObject(grandchild.as_raw_handle(), 5_000) },
            WAIT_OBJECT_0,
            "Job close did not terminate grandchild PID {grandchild_pid}\n{}",
            diagnostics()
        );
        assert!(
            parent.wait_for_exit(EXIT_TIMEOUT).unwrap(),
            "Job close did not terminate parent fixture\n{}",
            diagnostics()
        );
    }
}
