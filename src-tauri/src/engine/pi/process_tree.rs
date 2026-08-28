//! Ownership boundary for the production Pi process and its containment handle.
//!
//! Windows Job Objects give the sidecar a containment primitive whose emptiness
//! can be confirmed, including for descendants that detach from the console.
//! macOS starts a new session (`setsid`) and signals that process group on
//! terminate. That contract is weaker than a Job Object: descendants that leave
//! the session may leak, and emptiness of the group is not proof that every
//! historical child is gone. Other targets fail closed rather than claiming
//! Job Object isolation.

use std::io;

use tokio::process::{ChildStderr, ChildStdin, ChildStdout};

use super::CleanupBudget;

pub(super) struct PiProcessStdio {
    pub(super) stdin: ChildStdin,
    pub(super) stdout: ChildStdout,
    pub(super) stderr: Option<ChildStderr>,
}

pub(super) struct PiProcessSpawnError {
    _source: io::Error,
    cleanup_confirmed: bool,
    cleanup_budget: Option<CleanupBudget>,
}

impl PiProcessSpawnError {
    fn before_child(source: io::Error) -> Self {
        Self {
            _source: source,
            cleanup_confirmed: true,
            cleanup_budget: None,
        }
    }

    fn after_child(
        source: io::Error,
        cleanup_confirmed: bool,
        cleanup_budget: CleanupBudget,
    ) -> Self {
        Self {
            _source: source,
            cleanup_confirmed,
            cleanup_budget: Some(cleanup_budget),
        }
    }

    pub(super) fn cleanup_confirmed(&self) -> bool {
        self.cleanup_confirmed
    }

    pub(super) fn cleanup_budget(&self) -> Option<CleanupBudget> {
        self.cleanup_budget
    }
}

#[cfg(windows)]
mod platform {
    use std::{
        ffi::c_void,
        io,
        mem::size_of,
        os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle},
        ptr,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use tokio::process::{Child, Command};

    use super::{CleanupBudget, PiProcessSpawnError, PiProcessStdio};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_SUSPENDED: u32 = 0x0000_0004;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
    const JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION_CLASS: i32 = 1;
    const JOB_OBJECT_BASIC_PROCESS_ID_LIST_CLASS: i32 = 3;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: i32 = 9;
    const ERROR_INVALID_PARAMETER: i32 = 87;
    const ERROR_MORE_DATA: i32 = 234;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_TIMEOUT: u32 = 258;

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct JobObjectBasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct JobObjectExtendedLimitInformation {
        basic_limit_information: JobObjectBasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[repr(C)]
    #[derive(Default)]
    struct JobObjectBasicAccountingInformation {
        total_user_time: i64,
        total_kernel_time: i64,
        this_period_total_user_time: i64,
        this_period_total_kernel_time: i64,
        total_page_fault_count: u32,
        total_processes: u32,
        active_processes: u32,
        total_terminated_processes: u32,
    }

    #[repr(C)]
    struct JobObjectBasicProcessIdListHeader {
        number_of_assigned_processes: u32,
        number_of_process_ids_in_list: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "CreateJobObjectW"]
        fn create_job_object_w(attributes: *const c_void, name: *const u16) -> *mut c_void;
        #[link_name = "SetInformationJobObject"]
        fn set_information_job_object(
            job: *mut c_void,
            information_class: i32,
            information: *const c_void,
            information_length: u32,
        ) -> i32;
        #[link_name = "AssignProcessToJobObject"]
        fn assign_process_to_job_object(job: *mut c_void, process: *mut c_void) -> i32;
        #[link_name = "TerminateJobObject"]
        fn terminate_job_object(job: *mut c_void, exit_code: u32) -> i32;
        #[link_name = "QueryInformationJobObject"]
        fn query_information_job_object(
            job: *mut c_void,
            information_class: i32,
            information: *mut c_void,
            information_length: u32,
            return_length: *mut u32,
        ) -> i32;
        #[link_name = "OpenProcess"]
        fn open_process(access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
        #[link_name = "WaitForSingleObject"]
        fn wait_for_single_object(handle: *mut c_void, milliseconds: u32) -> u32;
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        #[link_name = "NtResumeProcess"]
        fn nt_resume_process(process: *mut c_void) -> i32;
    }

    #[async_trait]
    trait ChildProcess: Send {
        fn raw_handle(&self) -> Option<RawHandle>;
        fn take_stdio(&mut self) -> io::Result<PiProcessStdio>;
        fn start_kill(&mut self) -> io::Result<()>;
        async fn wait(&mut self) -> io::Result<()>;
    }

    struct TokioChild(Child);

    #[async_trait]
    impl ChildProcess for TokioChild {
        fn raw_handle(&self) -> Option<RawHandle> {
            self.0.raw_handle()
        }

        fn take_stdio(&mut self) -> io::Result<PiProcessStdio> {
            Ok(PiProcessStdio {
                stdin: self.0.stdin.take().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "Pi RPC stdin is unavailable")
                })?,
                stdout: self.0.stdout.take().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "Pi RPC stdout is unavailable")
                })?,
                stderr: self.0.stderr.take(),
            })
        }

        fn start_kill(&mut self) -> io::Result<()> {
            self.0.start_kill()
        }

        async fn wait(&mut self) -> io::Result<()> {
            self.0.wait().await.map(|_| ())
        }
    }

    #[async_trait]
    trait ProcessTreeOperations: Send + Sync {
        fn assign(&self, process: RawHandle) -> io::Result<()>;
        fn resume(&self, process: RawHandle) -> io::Result<()>;
        fn snapshot_processes(&self) -> io::Result<Vec<OwnedHandle>>;
        fn terminate(&self) -> io::Result<()>;
        async fn confirm_terminated(
            &self,
            processes: &[OwnedHandle],
            cleanup: CleanupBudget,
        ) -> io::Result<()>;
        fn close(&self);
    }

    struct WindowsJobOperations {
        job: Mutex<Option<OwnedHandle>>,
    }

    impl WindowsJobOperations {
        fn new() -> io::Result<Self> {
            let raw_job = unsafe { create_job_object_w(ptr::null(), ptr::null()) };
            if raw_job.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = unsafe { OwnedHandle::from_raw_handle(raw_job) };
            let mut information = JobObjectExtendedLimitInformation::default();
            information.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = unsafe {
                set_information_job_object(
                    job.as_raw_handle(),
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                    ptr::from_ref(&information).cast(),
                    size_of::<JobObjectExtendedLimitInformation>() as u32,
                )
            };
            if configured == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self {
                job: Mutex::new(Some(job)),
            })
        }

        fn process_ids(&self) -> io::Result<Vec<u32>> {
            let job = self.job.lock().unwrap();
            let job = job
                .as_ref()
                .ok_or_else(|| io::Error::other("Pi Job handle is closed"))?;
            let mut word_capacity = 64_usize;
            loop {
                let mut buffer = vec![0_usize; word_capacity];
                let queried = unsafe {
                    query_information_job_object(
                        job.as_raw_handle(),
                        JOB_OBJECT_BASIC_PROCESS_ID_LIST_CLASS,
                        buffer.as_mut_ptr().cast(),
                        (buffer.len() * size_of::<usize>()) as u32,
                        ptr::null_mut(),
                    )
                };
                let header =
                    unsafe { &*buffer.as_ptr().cast::<JobObjectBasicProcessIdListHeader>() };
                if queried != 0 {
                    let count = header.number_of_process_ids_in_list as usize;
                    let available = (buffer.len() * size_of::<usize>()
                        - size_of::<JobObjectBasicProcessIdListHeader>())
                        / size_of::<usize>();
                    if count > available {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Pi Job returned an oversized process list",
                        ));
                    }
                    let ids = unsafe {
                        std::slice::from_raw_parts(
                            buffer
                                .as_ptr()
                                .cast::<u8>()
                                .add(size_of::<JobObjectBasicProcessIdListHeader>())
                                .cast::<usize>(),
                            count,
                        )
                    };
                    return ids
                        .iter()
                        .map(|id| {
                            u32::try_from(*id)
                                .map_err(|_| io::Error::other("Pi Job returned invalid process id"))
                        })
                        .collect();
                }
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(ERROR_MORE_DATA) {
                    return Err(error);
                }
                let required_bytes = size_of::<JobObjectBasicProcessIdListHeader>()
                    + header.number_of_assigned_processes as usize * size_of::<usize>();
                word_capacity = word_capacity
                    .saturating_mul(2)
                    .max(required_bytes.div_ceil(size_of::<usize>()));
            }
        }

        fn active_processes(&self) -> io::Result<Option<u32>> {
            let job = self.job.lock().unwrap();
            let Some(job) = job.as_ref() else {
                return Ok(None);
            };
            let mut information = JobObjectBasicAccountingInformation::default();
            let queried = unsafe {
                query_information_job_object(
                    job.as_raw_handle(),
                    JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION_CLASS,
                    ptr::from_mut(&mut information).cast(),
                    size_of::<JobObjectBasicAccountingInformation>() as u32,
                    ptr::null_mut(),
                )
            };
            if queried == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Some(information.active_processes))
        }
    }

    #[async_trait]
    impl ProcessTreeOperations for WindowsJobOperations {
        fn assign(&self, process: RawHandle) -> io::Result<()> {
            let job = self.job.lock().unwrap();
            let job = job
                .as_ref()
                .ok_or_else(|| io::Error::other("Pi Job handle is closed"))?;
            if unsafe { assign_process_to_job_object(job.as_raw_handle(), process) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        fn resume(&self, process: RawHandle) -> io::Result<()> {
            let status = unsafe { nt_resume_process(process) };
            if status < 0 {
                return Err(io::Error::other(format!(
                    "NtResumeProcess failed with NTSTATUS {status:#010x}"
                )));
            }
            Ok(())
        }

        fn snapshot_processes(&self) -> io::Result<Vec<OwnedHandle>> {
            let mut handles = Vec::new();
            for process_id in self.process_ids()? {
                let raw_process = unsafe { open_process(SYNCHRONIZE, 0, process_id) };
                if raw_process.is_null() {
                    let error = io::Error::last_os_error();
                    if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER) {
                        continue;
                    }
                    return Err(error);
                }
                handles.push(unsafe { OwnedHandle::from_raw_handle(raw_process) });
            }
            Ok(handles)
        }

        fn terminate(&self) -> io::Result<()> {
            let job = self.job.lock().unwrap();
            let job = job
                .as_ref()
                .ok_or_else(|| io::Error::other("Pi Job handle is closed"))?;
            if unsafe { terminate_job_object(job.as_raw_handle(), 1) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        async fn confirm_terminated(
            &self,
            processes: &[OwnedHandle],
            cleanup: CleanupBudget,
        ) -> io::Result<()> {
            tokio::time::timeout_at(cleanup.deadline(), async {
                loop {
                    let mut all_signaled = true;
                    for process in processes {
                        match unsafe { wait_for_single_object(process.as_raw_handle(), 0) } {
                            WAIT_OBJECT_0 => {}
                            WAIT_TIMEOUT => all_signaled = false,
                            _ => return Err(io::Error::last_os_error()),
                        }
                    }
                    if all_signaled && self.active_processes()?.is_none_or(|active| active == 0) {
                        return Ok(());
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Pi Job did not empty"))?
        }

        fn close(&self) {
            drop(self.job.lock().unwrap().take());
        }
    }

    pub(crate) struct PiProcessTree {
        child: Box<dyn ChildProcess>,
        operations: Arc<dyn ProcessTreeOperations>,
    }

    impl PiProcessTree {
        pub(crate) async fn spawn(command: &mut Command) -> Result<Self, PiProcessSpawnError> {
            command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
            let operations =
                Arc::new(WindowsJobOperations::new().map_err(PiProcessSpawnError::before_child)?);
            let child = Box::new(TokioChild(
                command.spawn().map_err(PiProcessSpawnError::before_child)?,
            ));
            Self::contain_spawned_child(child, operations).await
        }

        async fn contain_spawned_child(
            child: Box<dyn ChildProcess>,
            operations: Arc<dyn ProcessTreeOperations>,
        ) -> Result<Self, PiProcessSpawnError> {
            let tree = Self { child, operations };
            let Some(process) = tree.child.raw_handle() else {
                let error = io::Error::new(
                    io::ErrorKind::NotFound,
                    "spawned Pi process has no process handle",
                );
                return Err(tree.cleanup_spawn_failure(error).await);
            };
            if let Err(error) = tree.operations.assign(process) {
                return Err(tree.cleanup_spawn_failure(error).await);
            }
            if let Err(error) = tree.operations.resume(process) {
                return Err(tree.cleanup_spawn_failure(error).await);
            }
            Ok(tree)
        }

        async fn cleanup_spawn_failure(self, source: io::Error) -> PiProcessSpawnError {
            let cleanup = CleanupBudget::new();
            match self.terminate_and_confirm(false, cleanup).await {
                Ok(()) => PiProcessSpawnError::after_child(source, true, cleanup),
                Err(error) => PiProcessSpawnError::after_child(error, false, cleanup),
            }
        }

        pub(crate) fn take_stdio(&mut self) -> io::Result<PiProcessStdio> {
            self.child.take_stdio()
        }

        pub(crate) async fn terminate_and_confirm(
            mut self,
            allow_graceful_exit: bool,
            cleanup: CleanupBudget,
        ) -> io::Result<()> {
            let mut cleanup_errors = Vec::new();
            let mut child_reaped = false;
            if allow_graceful_exit {
                match tokio::time::timeout_at(cleanup.deadline(), self.child.wait()).await {
                    Ok(Ok(())) => child_reaped = true,
                    Ok(Err(error)) => cleanup_errors.push(error),
                    Err(_) => {}
                }
            }

            let processes = match self.operations.snapshot_processes() {
                Ok(processes) => processes,
                Err(error) => {
                    cleanup_errors.push(error);
                    Vec::new()
                }
            };
            let mut job_closed = false;
            if let Err(error) = self.operations.terminate() {
                cleanup_errors.push(error);
                self.operations.close();
                job_closed = true;
            }
            if !child_reaped {
                if let Err(error) = self.child.start_kill() {
                    cleanup_errors.push(error);
                }
                match tokio::time::timeout_at(cleanup.deadline(), self.child.wait()).await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => cleanup_errors.push(error),
                    Err(_) => cleanup_errors.push(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Pi child did not exit",
                    )),
                }
            }
            if let Err(error) = self
                .operations
                .confirm_terminated(&processes, cleanup)
                .await
            {
                cleanup_errors.push(error);
                if !job_closed {
                    self.operations.close();
                    job_closed = true;
                    if let Err(error) = self
                        .operations
                        .confirm_terminated(&processes, cleanup)
                        .await
                    {
                        cleanup_errors.push(error);
                    }
                }
            }
            if !job_closed {
                self.operations.close();
            }

            if cleanup_errors.is_empty() {
                Ok(())
            } else {
                Err(io::Error::other(
                    "Pi process cleanup could not be confirmed",
                ))
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use std::{
            io,
            os::windows::io::{OwnedHandle, RawHandle},
            sync::{Arc, Mutex},
        };

        use async_trait::async_trait;

        use super::{
            ChildProcess, CleanupBudget, PiProcessStdio, PiProcessTree, ProcessTreeOperations,
        };

        #[derive(Clone, Copy)]
        enum Failure {
            None,
            Assign,
            Resume,
            Terminate,
            Query,
        }

        struct ScriptedChild {
            calls: Arc<Mutex<Vec<&'static str>>>,
            wait_fails: bool,
        }

        impl Drop for ScriptedChild {
            fn drop(&mut self) {
                self.calls.lock().unwrap().push("drop_child");
            }
        }

        #[async_trait]
        impl ChildProcess for ScriptedChild {
            fn raw_handle(&self) -> Option<RawHandle> {
                Some(1_usize as RawHandle)
            }

            fn take_stdio(&mut self) -> io::Result<PiProcessStdio> {
                Err(io::Error::other("scripted child has no stdio"))
            }

            fn start_kill(&mut self) -> io::Result<()> {
                self.calls.lock().unwrap().push("kill");
                Ok(())
            }

            async fn wait(&mut self) -> io::Result<()> {
                self.calls.lock().unwrap().push("wait");
                if self.wait_fails {
                    Err(io::Error::other("scripted wait failure"))
                } else {
                    Ok(())
                }
            }
        }

        struct ScriptedOperations {
            failure: Failure,
            calls: Arc<Mutex<Vec<&'static str>>>,
        }

        type ScriptedParts = (
            Box<dyn ChildProcess>,
            Arc<dyn ProcessTreeOperations>,
            Arc<Mutex<Vec<&'static str>>>,
        );

        impl Drop for ScriptedOperations {
            fn drop(&mut self) {
                self.calls.lock().unwrap().push("drop_operations");
            }
        }

        #[async_trait]
        impl ProcessTreeOperations for ScriptedOperations {
            fn assign(&self, _process: RawHandle) -> io::Result<()> {
                self.calls.lock().unwrap().push("assign");
                if matches!(self.failure, Failure::Assign) {
                    Err(io::Error::other("scripted assign failure"))
                } else {
                    Ok(())
                }
            }

            fn resume(&self, _process: RawHandle) -> io::Result<()> {
                self.calls.lock().unwrap().push("resume");
                if matches!(self.failure, Failure::Resume) {
                    Err(io::Error::other("scripted resume failure"))
                } else {
                    Ok(())
                }
            }

            fn snapshot_processes(&self) -> io::Result<Vec<OwnedHandle>> {
                self.calls.lock().unwrap().push("query");
                if matches!(self.failure, Failure::Query) {
                    Err(io::Error::other("scripted query failure"))
                } else {
                    Ok(Vec::new())
                }
            }

            fn terminate(&self) -> io::Result<()> {
                self.calls.lock().unwrap().push("terminate");
                if matches!(self.failure, Failure::Terminate) {
                    Err(io::Error::other("scripted terminate failure"))
                } else {
                    Ok(())
                }
            }

            async fn confirm_terminated(
                &self,
                _processes: &[OwnedHandle],
                _cleanup: CleanupBudget,
            ) -> io::Result<()> {
                self.calls.lock().unwrap().push("confirm");
                Ok(())
            }

            fn close(&self) {
                self.calls.lock().unwrap().push("close");
            }
        }

        fn scripted_parts(failure: Failure) -> ScriptedParts {
            scripted_parts_with_wait_failure(failure, false)
        }

        fn scripted_parts_with_wait_failure(failure: Failure, wait_fails: bool) -> ScriptedParts {
            let calls = Arc::new(Mutex::new(Vec::new()));
            (
                Box::new(ScriptedChild {
                    calls: Arc::clone(&calls),
                    wait_fails,
                }),
                Arc::new(ScriptedOperations {
                    failure,
                    calls: Arc::clone(&calls),
                }),
                calls,
            )
        }

        #[tokio::test]
        async fn process_tree_spawn_reaps_child_when_assign_fails() {
            let (child, operations, calls) = scripted_parts(Failure::Assign);
            assert!(
                PiProcessTree::contain_spawned_child(child, operations)
                    .await
                    .is_err()
            );
            assert_eq!(
                &*calls.lock().unwrap(),
                &[
                    "assign",
                    "query",
                    "terminate",
                    "kill",
                    "wait",
                    "confirm",
                    "close",
                    "drop_child",
                    "drop_operations"
                ]
            );
        }

        #[tokio::test]
        async fn process_tree_spawn_reaps_child_when_resume_fails() {
            let (child, operations, calls) = scripted_parts(Failure::Resume);
            assert!(
                PiProcessTree::contain_spawned_child(child, operations)
                    .await
                    .is_err()
            );
            assert_eq!(
                &*calls.lock().unwrap(),
                &[
                    "assign",
                    "resume",
                    "query",
                    "terminate",
                    "kill",
                    "wait",
                    "confirm",
                    "close",
                    "drop_child",
                    "drop_operations"
                ]
            );
        }

        #[tokio::test]
        async fn process_tree_spawn_reports_unconfirmed_when_partial_cleanup_cannot_reap() {
            let (child, operations, calls) =
                scripted_parts_with_wait_failure(Failure::Assign, true);
            let error = match PiProcessTree::contain_spawned_child(child, operations).await {
                Err(error) => error,
                Ok(_) => panic!("assign failure must fail spawn"),
            };
            assert!(!error.cleanup_confirmed());
            assert_eq!(
                &*calls.lock().unwrap(),
                &[
                    "assign",
                    "query",
                    "terminate",
                    "kill",
                    "wait",
                    "confirm",
                    "close",
                    "drop_child",
                    "drop_operations"
                ]
            );
        }

        #[tokio::test]
        async fn process_tree_reports_unconfirmed_when_terminate_fails() {
            let (child, operations, calls) = scripted_parts(Failure::Terminate);
            let tree = PiProcessTree { child, operations };
            assert!(
                tree.terminate_and_confirm(false, CleanupBudget::new())
                    .await
                    .is_err()
            );
            assert_eq!(
                &*calls.lock().unwrap(),
                &[
                    "query",
                    "terminate",
                    "close",
                    "kill",
                    "wait",
                    "confirm",
                    "drop_child",
                    "drop_operations"
                ]
            );
        }

        #[tokio::test]
        async fn process_tree_reports_unconfirmed_when_query_fails() {
            let (child, operations, calls) = scripted_parts(Failure::Query);
            let tree = PiProcessTree { child, operations };
            assert!(
                tree.terminate_and_confirm(false, CleanupBudget::new())
                    .await
                    .is_err()
            );
            assert_eq!(
                &*calls.lock().unwrap(),
                &[
                    "query",
                    "terminate",
                    "kill",
                    "wait",
                    "confirm",
                    "close",
                    "drop_child",
                    "drop_operations"
                ]
            );
        }

        #[tokio::test]
        async fn process_tree_drops_all_handles_before_reporting_cleanup_success() {
            let (child, operations, calls) = scripted_parts(Failure::None);
            let tree = PiProcessTree { child, operations };
            tree.terminate_and_confirm(false, CleanupBudget::new())
                .await
                .unwrap();
            assert_eq!(
                &*calls.lock().unwrap(),
                &[
                    "query",
                    "terminate",
                    "kill",
                    "wait",
                    "confirm",
                    "close",
                    "drop_child",
                    "drop_operations"
                ]
            );
        }
    }
}
#[cfg(target_os = "macos")]
mod platform {
    use std::{io, time::Duration};

    use tokio::process::{Child, Command};

    use super::{CleanupBudget, PiProcessSpawnError, PiProcessStdio};

    // kill(-pgid, …) addresses the session we created with setsid(2).
    const ESRCH: i32 = 3;
    const SIGKILL: i32 = 9;

    unsafe extern "C" {
        fn setsid() -> i32;
        fn kill(pid: i32, signal: i32) -> i32;
    }

    pub(crate) struct PiProcessTree {
        child: Child,
        pgid: i32,
    }

    impl PiProcessTree {
        pub(crate) async fn spawn(command: &mut Command) -> Result<Self, PiProcessSpawnError> {
            // New session so the sidecar is session and process-group leader.
            // This is not a Windows Job Object: a descendant that calls setsid()
            // or setpgid() leaves the group and will not be reaped.
            // SAFETY: pre_exec runs in the forked child before exec. The
            // closure only calls setsid(2), which is async-signal-safe.
            unsafe {
                command.pre_exec(|| {
                    if setsid() == -1 {
                        Err(io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                });
            }
            let child = command.spawn().map_err(PiProcessSpawnError::before_child)?;
            let Some(pid) = child.id() else {
                return Err(abandon_spawned_child(
                    child,
                    io::Error::new(io::ErrorKind::NotFound, "spawned Pi process has no pid"),
                )
                .await);
            };
            let Ok(pgid) = i32::try_from(pid) else {
                return Err(abandon_spawned_child(
                    child,
                    io::Error::other("spawned Pi pid is not a process group id"),
                )
                .await);
            };
            Ok(Self { child, pgid })
        }

        pub(crate) fn take_stdio(&mut self) -> io::Result<PiProcessStdio> {
            Ok(PiProcessStdio {
                stdin: self.child.stdin.take().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "Pi RPC stdin is unavailable")
                })?,
                stdout: self.child.stdout.take().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "Pi RPC stdout is unavailable")
                })?,
                stderr: self.child.stderr.take(),
            })
        }

        pub(crate) async fn terminate_and_confirm(
            mut self,
            allow_graceful_exit: bool,
            cleanup: CleanupBudget,
        ) -> io::Result<()> {
            let mut cleanup_errors = Vec::new();
            let mut child_reaped = false;
            if allow_graceful_exit {
                match tokio::time::timeout_at(cleanup.deadline(), self.child.wait()).await {
                    Ok(Ok(_)) => child_reaped = true,
                    Ok(Err(error)) => cleanup_errors.push(error),
                    Err(_) => {}
                }
            }

            if let Err(error) = signal_process_group(self.pgid, SIGKILL) {
                cleanup_errors.push(error);
            }
            if !child_reaped {
                if let Err(error) = self.child.start_kill() {
                    cleanup_errors.push(error);
                }
                match tokio::time::timeout_at(cleanup.deadline(), self.child.wait()).await {
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => cleanup_errors.push(error),
                    Err(_) => cleanup_errors.push(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Pi child did not exit",
                    )),
                }
            }
            if let Err(error) = confirm_process_group_exited(self.pgid, cleanup).await {
                cleanup_errors.push(error);
            }

            if cleanup_errors.is_empty() {
                Ok(())
            } else {
                Err(io::Error::other(
                    "Pi process cleanup could not be confirmed",
                ))
            }
        }
    }

    async fn abandon_spawned_child(mut child: Child, source: io::Error) -> PiProcessSpawnError {
        let cleanup = CleanupBudget::new();
        let _ = child.start_kill();
        let cleanup_confirmed = matches!(
            tokio::time::timeout_at(cleanup.deadline(), child.wait()).await,
            Ok(Ok(_))
        );
        PiProcessSpawnError::after_child(source, cleanup_confirmed, cleanup)
    }

    fn signal_process_group(pgid: i32, signal: i32) -> io::Result<()> {
        // SAFETY: pgid is the session leader we spawned; a negative pid is killpg.
        let result = unsafe { kill(-pgid, signal) };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ESRCH) {
            Ok(())
        } else {
            Err(error)
        }
    }

    fn process_group_has_members(pgid: i32) -> io::Result<bool> {
        // SAFETY: same session-leader pgid as spawn. Signal 0 is existence only.
        // Processes that already left the session are invisible here — the Job
        // Object gap this path must not paper over.
        let result = unsafe { kill(-pgid, 0) };
        if result == 0 {
            return Ok(true);
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ESRCH) {
            Ok(false)
        } else {
            Err(error)
        }
    }

    async fn confirm_process_group_exited(pgid: i32, cleanup: CleanupBudget) -> io::Result<()> {
        tokio::time::timeout_at(cleanup.deadline(), async {
            loop {
                if !process_group_has_members(pgid)? {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Pi process group did not empty"))?
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use std::io;

    use tokio::process::Command;

    use super::{PiProcessSpawnError, PiProcessStdio};

    pub(crate) struct PiProcessTree;

    impl PiProcessTree {
        pub(crate) async fn spawn(_command: &mut Command) -> Result<Self, PiProcessSpawnError> {
            Err(PiProcessSpawnError::before_child(io::Error::new(
                io::ErrorKind::Unsupported,
                "production Pi process containment is supported only on Windows and macOS",
            )))
        }

        pub(crate) fn take_stdio(&mut self) -> io::Result<PiProcessStdio> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "production Pi process containment is supported only on Windows and macOS",
            ))
        }

        pub(crate) async fn terminate_and_confirm(
            self,
            _allow_graceful: bool,
            _cleanup: super::CleanupBudget,
        ) -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "production Pi process containment is supported only on Windows and macOS",
            ))
        }
    }
}

pub(super) use platform::PiProcessTree;
