//! Ownership boundary for the production Pi process and its containment handle.
//!
//! Pi is bundled and supported as a Windows sidecar. Windows Job Objects give
//! us a containment primitive whose emptiness can be confirmed. Other targets
//! deliberately fail closed instead of claiming process-tree cleanup that a
//! process group cannot provide for detached descendants.

use std::io;

use tokio::process::{ChildStderr, ChildStdin, ChildStdout};

use super::PROCESS_TREE_TERMINATION_TIMEOUT;

pub(super) struct PiProcessStdio {
    pub(super) stdin: ChildStdin,
    pub(super) stdout: ChildStdout,
    pub(super) stderr: Option<ChildStderr>,
}

pub(super) struct PiProcessSpawnError {
    _source: io::Error,
    cleanup_confirmed: bool,
}

impl PiProcessSpawnError {
    fn before_child(source: io::Error) -> Self {
        Self {
            _source: source,
            cleanup_confirmed: true,
        }
    }

    fn after_child(source: io::Error, cleanup_confirmed: bool) -> Self {
        Self {
            _source: source,
            cleanup_confirmed,
        }
    }

    pub(super) fn cleanup_confirmed(&self) -> bool {
        self.cleanup_confirmed
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
        sync::Arc,
    };

    use async_trait::async_trait;
    use tokio::process::{Child, Command};

    use super::{PROCESS_TREE_TERMINATION_TIMEOUT, PiProcessSpawnError, PiProcessStdio};

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
        async fn confirm_terminated(&self, processes: &[OwnedHandle]) -> io::Result<()>;
    }

    struct WindowsJobOperations {
        job: OwnedHandle,
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
            Ok(Self { job })
        }

        fn process_ids(&self) -> io::Result<Vec<u32>> {
            let mut word_capacity = 64_usize;
            loop {
                let mut buffer = vec![0_usize; word_capacity];
                let queried = unsafe {
                    query_information_job_object(
                        self.job.as_raw_handle(),
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

        fn active_processes(&self) -> io::Result<u32> {
            let mut information = JobObjectBasicAccountingInformation::default();
            let queried = unsafe {
                query_information_job_object(
                    self.job.as_raw_handle(),
                    JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION_CLASS,
                    ptr::from_mut(&mut information).cast(),
                    size_of::<JobObjectBasicAccountingInformation>() as u32,
                    ptr::null_mut(),
                )
            };
            if queried == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(information.active_processes)
        }
    }

    #[async_trait]
    impl ProcessTreeOperations for WindowsJobOperations {
        fn assign(&self, process: RawHandle) -> io::Result<()> {
            if unsafe { assign_process_to_job_object(self.job.as_raw_handle(), process) } == 0 {
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
            if unsafe { terminate_job_object(self.job.as_raw_handle(), 1) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        async fn confirm_terminated(&self, processes: &[OwnedHandle]) -> io::Result<()> {
            tokio::time::timeout(PROCESS_TREE_TERMINATION_TIMEOUT, async {
                loop {
                    let mut all_signaled = true;
                    for process in processes {
                        match unsafe { wait_for_single_object(process.as_raw_handle(), 0) } {
                            WAIT_OBJECT_0 => {}
                            WAIT_TIMEOUT => all_signaled = false,
                            _ => return Err(io::Error::last_os_error()),
                        }
                    }
                    if all_signaled && self.active_processes()? == 0 {
                        return Ok(());
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Pi Job did not empty"))?
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
            let mut tree = Self { child, operations };
            let Some(process) = tree.child.raw_handle() else {
                let error = io::Error::new(
                    io::ErrorKind::NotFound,
                    "spawned Pi process has no process handle",
                );
                return Err(match tree.kill_direct_child_and_wait().await {
                    Ok(()) => PiProcessSpawnError::after_child(error, true),
                    Err(cleanup_error) => PiProcessSpawnError::after_child(cleanup_error, false),
                });
            };
            if let Err(error) = tree.operations.assign(process) {
                return Err(match tree.kill_direct_child_and_wait().await {
                    Ok(()) => PiProcessSpawnError::after_child(error, true),
                    Err(cleanup_error) => PiProcessSpawnError::after_child(cleanup_error, false),
                });
            }
            if let Err(error) = tree.operations.resume(process) {
                return Err(match tree.kill_direct_child_and_wait().await {
                    Ok(()) => PiProcessSpawnError::after_child(error, true),
                    Err(cleanup_error) => PiProcessSpawnError::after_child(cleanup_error, false),
                });
            }
            Ok(tree)
        }

        async fn kill_direct_child_and_wait(&mut self) -> io::Result<()> {
            self.child.start_kill()?;
            tokio::time::timeout(PROCESS_TREE_TERMINATION_TIMEOUT, self.child.wait())
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Pi child did not exit"))??;
            Ok(())
        }

        pub(crate) fn take_stdio(&mut self) -> io::Result<PiProcessStdio> {
            self.child.take_stdio()
        }

        pub(crate) async fn terminate_and_confirm(
            mut self,
            allow_graceful_exit: bool,
        ) -> io::Result<()> {
            let mut child_reaped = false;
            if allow_graceful_exit
                && matches!(
                    tokio::time::timeout(PROCESS_TREE_TERMINATION_TIMEOUT, self.child.wait()).await,
                    Ok(Ok(()))
                )
            {
                child_reaped = true;
            }

            let processes = self.operations.snapshot_processes()?;
            self.operations.terminate()?;
            if !child_reaped {
                tokio::time::timeout(PROCESS_TREE_TERMINATION_TIMEOUT, self.child.wait())
                    .await
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::TimedOut, "Pi child did not exit")
                    })??;
            }
            self.operations.confirm_terminated(&processes).await
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

        use super::{ChildProcess, PiProcessStdio, PiProcessTree, ProcessTreeOperations};

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

            async fn confirm_terminated(&self, _processes: &[OwnedHandle]) -> io::Result<()> {
                self.calls.lock().unwrap().push("confirm");
                Ok(())
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
                &["assign", "kill", "wait", "drop_child", "drop_operations"]
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
                    "kill",
                    "wait",
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
                &["assign", "kill", "wait", "drop_child", "drop_operations"]
            );
        }

        #[tokio::test]
        async fn process_tree_reports_unconfirmed_when_terminate_fails() {
            let (child, operations, calls) = scripted_parts(Failure::Terminate);
            let tree = PiProcessTree { child, operations };
            assert!(tree.terminate_and_confirm(false).await.is_err());
            assert_eq!(
                &*calls.lock().unwrap(),
                &["query", "terminate", "drop_child", "drop_operations"]
            );
        }

        #[tokio::test]
        async fn process_tree_reports_unconfirmed_when_query_fails() {
            let (child, operations, calls) = scripted_parts(Failure::Query);
            let tree = PiProcessTree { child, operations };
            assert!(tree.terminate_and_confirm(false).await.is_err());
            assert_eq!(
                &*calls.lock().unwrap(),
                &["query", "drop_child", "drop_operations"]
            );
        }

        #[tokio::test]
        async fn process_tree_drops_all_handles_before_reporting_cleanup_success() {
            let (child, operations, calls) = scripted_parts(Failure::None);
            let tree = PiProcessTree { child, operations };
            tree.terminate_and_confirm(false).await.unwrap();
            assert_eq!(
                &*calls.lock().unwrap(),
                &[
                    "query",
                    "terminate",
                    "wait",
                    "confirm",
                    "drop_child",
                    "drop_operations"
                ]
            );
        }
    }
}
#[cfg(not(windows))]
mod platform {
    use std::io;

    use tokio::process::Command;

    use super::{PiProcessSpawnError, PiProcessStdio};

    pub(crate) struct PiProcessTree;

    impl PiProcessTree {
        pub(crate) async fn spawn(_command: &mut Command) -> Result<Self, PiProcessSpawnError> {
            Err(PiProcessSpawnError::before_child(io::Error::new(
                io::ErrorKind::Unsupported,
                "production Pi process containment is supported only on Windows",
            )))
        }

        pub(crate) fn take_stdio(&mut self) -> io::Result<PiProcessStdio> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "production Pi process containment is supported only on Windows",
            ))
        }

        pub(crate) async fn terminate_and_confirm(self, _allow_graceful: bool) -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "production Pi process containment is supported only on Windows",
            ))
        }
    }
}

pub(super) use platform::PiProcessTree;
