#requires -Version 7.0

if (!$IsWindows) { throw 'Owned process execution requires Windows.' }

$nativeTypeName = 'MultiLauncher.Dev.OwnedProcessNative'
if ($null -eq ($nativeTypeName -as [type])) {
    $nativeSource = @'
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;

namespace MultiLauncher.Dev {
    [StructLayout(LayoutKind.Sequential)]
    internal struct SecurityAttributes {
        public int Length;
        public IntPtr SecurityDescriptor;
        public int InheritHandle;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    internal struct StartupInfo {
        public uint Size;
        public IntPtr Reserved;
        public IntPtr Desktop;
        public IntPtr Title;
        public uint X;
        public uint Y;
        public uint XSize;
        public uint YSize;
        public uint XCountChars;
        public uint YCountChars;
        public uint FillAttribute;
        public uint Flags;
        public ushort ShowWindow;
        public ushort Reserved2Size;
        public IntPtr Reserved2;
        public IntPtr StandardInput;
        public IntPtr StandardOutput;
        public IntPtr StandardError;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct StartupInfoEx {
        public StartupInfo StartupInfo;
        public IntPtr AttributeList;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ProcessInformation {
        public IntPtr Process;
        public IntPtr Thread;
        public uint ProcessId;
        public uint ThreadId;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct IoCounters {
        public ulong ReadOperationCount;
        public ulong WriteOperationCount;
        public ulong OtherOperationCount;
        public ulong ReadTransferCount;
        public ulong WriteTransferCount;
        public ulong OtherTransferCount;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ByHandleFileInformation {
        public uint FileAttributes;
        public FileTime CreationTime;
        public FileTime LastAccessTime;
        public FileTime LastWriteTime;
        public uint VolumeSerialNumber;
        public uint FileSizeHigh;
        public uint FileSizeLow;
        public uint NumberOfLinks;
        public uint FileIndexHigh;
        public uint FileIndexLow;
    }

    public sealed class OwnedFileDescriptor {
        public string CanonicalPath;
        public string FileId;
        public uint VolumeSerialNumber;
        public uint FileIndexHigh;
        public uint FileIndexLow;
    }

    public sealed class OwnedProcessImage {
        public uint ProcessId;
        public string CreationUtc;
        public bool InOwnedJob;
        public bool Running;
        public string CanonicalPath;
        public string FileId;
        public uint VolumeSerialNumber;
        public uint FileIndexHigh;
        public uint FileIndexLow;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct JobBasicLimitInformation {
        public long PerProcessUserTimeLimit;
        public long PerJobUserTimeLimit;
        public uint LimitFlags;
        public UIntPtr MinimumWorkingSetSize;
        public UIntPtr MaximumWorkingSetSize;
        public uint ActiveProcessLimit;
        public UIntPtr Affinity;
        public uint PriorityClass;
        public uint SchedulingClass;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct JobExtendedLimitInformation {
        public JobBasicLimitInformation BasicLimitInformation;
        public IoCounters IoInfo;
        public UIntPtr ProcessMemoryLimit;
        public UIntPtr JobMemoryLimit;
        public UIntPtr PeakProcessMemoryUsed;
        public UIntPtr PeakJobMemoryUsed;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct JobBasicAccountingInformation {
        public long TotalUserTime;
        public long TotalKernelTime;
        public long ThisPeriodTotalUserTime;
        public long ThisPeriodTotalKernelTime;
        public uint TotalPageFaultCount;
        public uint TotalProcesses;
        public uint ActiveProcesses;
        public uint TotalTerminatedProcesses;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct FileTime {
        public uint Low;
        public uint High;
    }

    public sealed class DrainState {
        private readonly object gate = new object();
        private long bytesRead;
        private long bytesRecorded;
        private string error;
        private bool completed;
        public long BytesRead { get { lock (gate) return bytesRead; } }
        public long BytesRecorded { get { lock (gate) return bytesRecorded; } }
        public string Error { get { lock (gate) return error; } }
        public bool Completed { get { lock (gate) return completed; } }
        internal void AddRead(long value) { lock (gate) bytesRead += value; }
        internal void AddRecorded(long value) { lock (gate) bytesRecorded += value; }
        internal void SetError(Exception value) { lock (gate) { if (error == null) error = value.GetType().FullName + ": " + value.Message; } }
        internal void Complete() { lock (gate) completed = true; }
    }

    public sealed class OwnedChild : IDisposable {
        public IntPtr JobHandle;
        public IntPtr ProcessHandle;
        public IntPtr ThreadHandle;
        public uint ProcessId;
        public FileStream StdoutPipe;
        public FileStream StderrPipe;
        public DrainState StdoutDrain;
        public DrainState StderrDrain;
        public Task StdoutTask;
        public Task StderrTask;
        private int disposed;

        public void StartDrain(FileStream stdoutTarget, FileStream stderrTarget) {
            StdoutDrain = new DrainState();
            StderrDrain = new DrainState();
            StdoutTask = StartOneDrain(StdoutPipe, stdoutTarget, StdoutDrain);
            StderrTask = StartOneDrain(StderrPipe, stderrTarget, StderrDrain);
        }

        private static Task StartOneDrain(FileStream input, FileStream output, DrainState state) {
            return Task.Factory.StartNew(() => {
                byte[] buffer = new byte[64 * 1024];
                bool outputWritable = true;
                try {
                    while (true) {
                        int read;
                        try { read = input.Read(buffer, 0, buffer.Length); }
                        catch (IOException ex) {
                            if (ex.HResult == unchecked((int)0x8007006D)) break; // ERROR_BROKEN_PIPE
                            throw;
                        }
                        if (read == 0) break;
                        state.AddRead(read);
                        if (outputWritable) {
                            try { output.Write(buffer, 0, read); state.AddRecorded(read); }
                            catch (Exception ex) { state.SetError(ex); outputWritable = false; }
                        }
                    }
                    if (outputWritable) {
                        try { output.Flush(); }
                        catch (Exception ex) { state.SetError(ex); }
                    }
                }
                catch (Exception ex) { state.SetError(ex); }
                finally {
                    try { input.Dispose(); } catch (Exception ex) { state.SetError(ex); }
                    try { output.Dispose(); } catch (Exception ex) { state.SetError(ex); }
                    state.Complete();
                }
            }, CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default);
        }

        public void Dispose() {
            if (Interlocked.Exchange(ref disposed, 1) != 0) return;
            // Closing a configured Job Object is the final fail-safe owner action.
            Close(ref JobHandle);
            Close(ref ThreadHandle);
            Close(ref ProcessHandle);
            try { if (StdoutPipe != null) StdoutPipe.Dispose(); } catch { }
            try { if (StderrPipe != null) StderrPipe.Dispose(); } catch { }
        }

        private static void Close(ref IntPtr handle) {
            IntPtr value = handle;
            handle = IntPtr.Zero;
            if (value != IntPtr.Zero) OwnedProcessNative.CloseHandle(value);
        }
    }

    public static class OwnedProcessNative {
        private const uint CREATE_SUSPENDED = 0x00000004;
        private const uint CREATE_NO_WINDOW = 0x08000000;
        private const uint EXTENDED_STARTUPINFO_PRESENT = 0x00080000;
        private const uint STARTF_USESTDHANDLES = 0x00000100;
        private const uint STARTF_USESHOWWINDOW = 0x00000001;
        private const uint HANDLE_FLAG_INHERIT = 0x00000001;
        private const uint JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000;
        private const uint JOB_OBJECT_LIMIT_BREAKAWAY_OK = 0x00000800;
        private const uint JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK = 0x00001000;
        private const int JobObjectExtendedLimitInformation = 9;
        private const int JobObjectBasicAccountingInformation = 1;
        private const int JobObjectBasicProcessIdList = 3;
        private const long PROC_THREAD_ATTRIBUTE_HANDLE_LIST = 0x00020002;
        private const uint GENERIC_READ = 0x80000000;
        private const uint PROCESS_QUERY_INFORMATION = 0x00000400;
        private const uint PROCESS_QUERY_LIMITED_INFORMATION = 0x00001000;
        private const uint FILE_SHARE_READ = 0x00000001;
        private const uint FILE_SHARE_WRITE = 0x00000002;
        private const uint FILE_READ_ATTRIBUTES = 0x00000080;
        private const uint OPEN_EXISTING = 3;
        private const uint FILE_ATTRIBUTE_NORMAL = 0x00000080;
        private const uint WAIT_OBJECT_0 = 0;
        private const uint WAIT_TIMEOUT = 258;
        private const uint WAIT_FAILED = 0xFFFFFFFF;

        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern IntPtr CreateJobObjectW(IntPtr attributes, string name);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool SetInformationJobObject(IntPtr job, int informationClass, IntPtr information, uint length);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool QueryInformationJobObject(IntPtr job, int informationClass, IntPtr information, uint length, out uint returnedLength);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool IsProcessInJob(IntPtr process, IntPtr job, out int result);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool TerminateJobObject(IntPtr job, uint exitCode);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool TerminateProcess(IntPtr process, uint exitCode);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern uint ResumeThread(IntPtr thread);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool GetExitCodeProcess(IntPtr process, out uint exitCode);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool GetProcessTimes(IntPtr process, out FileTime creation, out FileTime exit, out FileTime kernel, out FileTime user);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr OpenProcess(uint desiredAccess, bool inheritHandle, uint processId);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr GetCurrentProcess();
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool CreatePipe(out IntPtr readPipe, out IntPtr writePipe, ref SecurityAttributes attributes, uint size);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool SetHandleInformation(IntPtr handle, uint mask, uint flags);
        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern IntPtr CreateFileW(string name, uint access, uint share, ref SecurityAttributes attributes, uint creation, uint flags, IntPtr template);
        [DllImport("kernel32.dll", EntryPoint = "CreateFileW", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern IntPtr OpenFileForIdentity(string name, uint access, uint share, IntPtr attributes, uint creation, uint flags, IntPtr template);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool GetFileInformationByHandle(IntPtr file, out ByHandleFileInformation information);
        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern uint GetFinalPathNameByHandleW(IntPtr file, StringBuilder path, uint length, uint flags);
        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern bool QueryFullProcessImageNameW(IntPtr process, uint flags, StringBuilder imageName, ref uint size);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool InitializeProcThreadAttributeList(IntPtr list, int count, int flags, ref IntPtr size);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool UpdateProcThreadAttribute(IntPtr list, uint flags, IntPtr attribute, IntPtr value, IntPtr size, IntPtr previous, IntPtr returnedSize);
        [DllImport("kernel32.dll")]
        private static extern void DeleteProcThreadAttributeList(IntPtr list);
        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern bool CreateProcessW(string applicationName, StringBuilder commandLine, IntPtr processAttributes, IntPtr threadAttributes, bool inheritHandles, uint creationFlags, IntPtr environment, string currentDirectory, ref StartupInfoEx startupInfo, out ProcessInformation processInformation);

        private static Exception LastError(string operation) {
            int code = Marshal.GetLastWin32Error();
            return new Win32Exception(code, operation + " failed");
        }

        public static OwnedChild CreateSuspended(string executable, string commandLine, string workingDirectory, bool hiddenWindow) {
            OwnedChild child = new OwnedChild();
            IntPtr stdoutRead = IntPtr.Zero, stdoutWrite = IntPtr.Zero, stderrRead = IntPtr.Zero, stderrWrite = IntPtr.Zero, stdin = IntPtr.Zero;
            IntPtr attributes = IntPtr.Zero, handleArray = IntPtr.Zero;
            ProcessInformation process = new ProcessInformation();
            bool processCreated = false;
            try {
                child.JobHandle = CreateJobObjectW(IntPtr.Zero, null);
                if (child.JobHandle == IntPtr.Zero) throw LastError("CreateJobObjectW");
                JobExtendedLimitInformation limits = new JobExtendedLimitInformation();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                IntPtr limitBuffer = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(JobExtendedLimitInformation)));
                try {
                    Marshal.StructureToPtr(limits, limitBuffer, false);
                    if (!SetInformationJobObject(child.JobHandle, JobObjectExtendedLimitInformation, limitBuffer, (uint)Marshal.SizeOf(typeof(JobExtendedLimitInformation)))) throw LastError("SetInformationJobObject(KILL_ON_JOB_CLOSE)");
                    uint returned;
                    if (!QueryInformationJobObject(child.JobHandle, JobObjectExtendedLimitInformation, limitBuffer, (uint)Marshal.SizeOf(typeof(JobExtendedLimitInformation)), out returned)) throw LastError("QueryInformationJobObject(ownership limits)");
                    JobExtendedLimitInformation verified = (JobExtendedLimitInformation)Marshal.PtrToStructure(limitBuffer, typeof(JobExtendedLimitInformation));
                    if ((verified.BasicLimitInformation.LimitFlags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE) == 0 ||
                        (verified.BasicLimitInformation.LimitFlags & (JOB_OBJECT_LIMIT_BREAKAWAY_OK | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK)) != 0) {
                        throw new InvalidOperationException("Job Object ownership limits could not be verified.");
                    }
                }
                finally { Marshal.FreeHGlobal(limitBuffer); }

                SecurityAttributes security = new SecurityAttributes { Length = Marshal.SizeOf(typeof(SecurityAttributes)), InheritHandle = 1 };
                if (!CreatePipe(out stdoutRead, out stdoutWrite, ref security, 0)) throw LastError("CreatePipe(stdout)");
                if (!CreatePipe(out stderrRead, out stderrWrite, ref security, 0)) throw LastError("CreatePipe(stderr)");
                if (!SetHandleInformation(stdoutRead, HANDLE_FLAG_INHERIT, 0)) throw LastError("SetHandleInformation(stdout read)");
                if (!SetHandleInformation(stderrRead, HANDLE_FLAG_INHERIT, 0)) throw LastError("SetHandleInformation(stderr read)");
                stdin = CreateFileW("NUL", GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, ref security, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, IntPtr.Zero);
                if (stdin == IntPtr.Zero || stdin == new IntPtr(-1)) throw LastError("CreateFileW(NUL)");

                IntPtr attributeSize = IntPtr.Zero;
                InitializeProcThreadAttributeList(IntPtr.Zero, 1, 0, ref attributeSize);
                int attributeError = Marshal.GetLastWin32Error();
                if (attributeSize == IntPtr.Zero || attributeError != 122) throw new Win32Exception(attributeError, "InitializeProcThreadAttributeList(size) failed");
                attributes = Marshal.AllocHGlobal(attributeSize);
                if (!InitializeProcThreadAttributeList(attributes, 1, 0, ref attributeSize)) throw LastError("InitializeProcThreadAttributeList");
                IntPtr[] inherited = new IntPtr[] { stdin, stdoutWrite, stderrWrite };
                handleArray = Marshal.AllocHGlobal(IntPtr.Size * inherited.Length);
                for (int i = 0; i < inherited.Length; i++) Marshal.WriteIntPtr(handleArray, i * IntPtr.Size, inherited[i]);
                if (!UpdateProcThreadAttribute(attributes, 0, new IntPtr(PROC_THREAD_ATTRIBUTE_HANDLE_LIST), handleArray, new IntPtr(IntPtr.Size * inherited.Length), IntPtr.Zero, IntPtr.Zero)) throw LastError("UpdateProcThreadAttribute(handle list)");

                StartupInfoEx startup = new StartupInfoEx();
                startup.StartupInfo.Size = (uint)Marshal.SizeOf(typeof(StartupInfoEx));
                startup.StartupInfo.Flags = STARTF_USESTDHANDLES | STARTF_USESHOWWINDOW;
                startup.StartupInfo.ShowWindow = hiddenWindow ? (ushort)0 : (ushort)1;
                startup.StartupInfo.StandardInput = stdin;
                startup.StartupInfo.StandardOutput = stdoutWrite;
                startup.StartupInfo.StandardError = stderrWrite;
                startup.AttributeList = attributes;
                StringBuilder mutableCommand = new StringBuilder(commandLine, commandLine.Length + 1);
                uint flags = CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT | (hiddenWindow ? CREATE_NO_WINDOW : 0u);
                if (!CreateProcessW(executable, mutableCommand, IntPtr.Zero, IntPtr.Zero, true, flags, IntPtr.Zero, workingDirectory, ref startup, out process)) throw LastError("CreateProcessW(suspended)");
                processCreated = true;
                child.ProcessHandle = process.Process;
                child.ThreadHandle = process.Thread;
                child.ProcessId = process.ProcessId;
                process.Process = IntPtr.Zero;
                process.Thread = IntPtr.Zero;
                child.StdoutPipe = new FileStream(new SafeFileHandle(stdoutRead, true), FileAccess.Read, 65536, false);
                stdoutRead = IntPtr.Zero;
                child.StderrPipe = new FileStream(new SafeFileHandle(stderrRead, true), FileAccess.Read, 65536, false);
                stderrRead = IntPtr.Zero;
                return child;
            }
            catch {
                // The native handles may already have moved to child before
                // stream-wrapper construction fails. Retire that suspended
                // process through whichever handle still owns it.
                IntPtr failedProcess = child.ProcessHandle != IntPtr.Zero ? child.ProcessHandle : process.Process;
                if (processCreated && failedProcess != IntPtr.Zero) {
                    TerminateProcess(failedProcess, 1);
                    WaitForSingleObject(failedProcess, 3000);
                }
                child.Dispose();
                throw;
            }
            finally {
                if (attributes != IntPtr.Zero) { DeleteProcThreadAttributeList(attributes); Marshal.FreeHGlobal(attributes); }
                if (handleArray != IntPtr.Zero) Marshal.FreeHGlobal(handleArray);
                CloseHandle(stdin); CloseHandle(stdoutWrite); CloseHandle(stderrWrite); CloseHandle(stdoutRead); CloseHandle(stderrRead);
                if (process.Process != IntPtr.Zero) CloseHandle(process.Process);
                if (process.Thread != IntPtr.Zero) CloseHandle(process.Thread);
            }
        }

        public static void AssignToJob(OwnedChild child) {
            if (!AssignProcessToJobObject(child.JobHandle, child.ProcessHandle)) throw LastError("AssignProcessToJobObject");
        }

        public static bool VerifyAssignment(OwnedChild child) {
            int result;
            if (!IsProcessInJob(child.ProcessHandle, child.JobHandle, out result)) throw LastError("IsProcessInJob");
            if (result == 0) throw new InvalidOperationException("The suspended process was not assigned to its owned Job Object.");
            return true;
        }

        public static void Resume(OwnedChild child) {
            uint result = ResumeThread(child.ThreadHandle);
            if (result == 0xFFFFFFFF) throw LastError("ResumeThread");
        }

        public static int WaitForRoot(OwnedChild child, int milliseconds) {
            uint result = WaitForSingleObject(child.ProcessHandle, (uint)Math.Max(0, milliseconds));
            if (result == WAIT_OBJECT_0) return 0;
            if (result == WAIT_TIMEOUT) return 1;
            if (result == WAIT_FAILED) throw LastError("WaitForSingleObject(root)");
            throw new InvalidOperationException("WaitForSingleObject returned an unexpected result.");
        }

        public static int GetExitCode(OwnedChild child) {
            uint result;
            if (!GetExitCodeProcess(child.ProcessHandle, out result)) throw LastError("GetExitCodeProcess");
            return unchecked((int)result);
        }

        public static uint GetActiveProcessCount(OwnedChild child) {
            JobBasicAccountingInformation accounting = new JobBasicAccountingInformation();
            IntPtr buffer = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(JobBasicAccountingInformation)));
            try {
                uint returned;
                if (!QueryInformationJobObject(child.JobHandle, JobObjectBasicAccountingInformation, buffer, (uint)Marshal.SizeOf(typeof(JobBasicAccountingInformation)), out returned)) throw LastError("QueryInformationJobObject(accounting)");
                accounting = (JobBasicAccountingInformation)Marshal.PtrToStructure(buffer, typeof(JobBasicAccountingInformation));
                return accounting.ActiveProcesses;
            }
            finally { Marshal.FreeHGlobal(buffer); }
        }

        public static uint[] GetProcessIds(OwnedChild child) {
            int capacity = 64;
            while (capacity <= 65536) {
                int header = 8;
                IntPtr buffer = Marshal.AllocHGlobal(header + capacity * IntPtr.Size);
                try {
                    Marshal.WriteInt32(buffer, 0, 0);
                    Marshal.WriteInt32(buffer, 4, capacity);
                    uint returned;
                    bool ok = QueryInformationJobObject(child.JobHandle, JobObjectBasicProcessIdList, buffer, (uint)(header + capacity * IntPtr.Size), out returned);
                    int error = Marshal.GetLastWin32Error();
                    uint assigned = unchecked((uint)Marshal.ReadInt32(buffer, 0));
                    uint count = unchecked((uint)Marshal.ReadInt32(buffer, 4));
                    if (!ok) {
                        if (error == 122 && capacity < 65536) { capacity *= 2; continue; }
                        throw new Win32Exception(error, "QueryInformationJobObject(process IDs) failed");
                    }
                    if (assigned > count && capacity < 65536) { capacity = Math.Min(65536, Math.Max(capacity * 2, (int)assigned)); continue; }
                    if (assigned > count) throw new InvalidOperationException("Owned process identity list exceeded its bounded capacity.");
                    uint[] ids = new uint[count];
                    for (int i = 0; i < count; i++) {
                        long value = IntPtr.Size == 8 ? Marshal.ReadInt64(buffer, header + i * IntPtr.Size) : (uint)Marshal.ReadInt32(buffer, header + i * IntPtr.Size);
                        ids[i] = unchecked((uint)value);
                    }
                    return ids;
                }
                finally { Marshal.FreeHGlobal(buffer); }
            }
            throw new InvalidOperationException("Could not allocate a bounded owned process identity list.");
        }

        public static void TerminateJob(OwnedChild child) {
            if (!TerminateJobObject(child.JobHandle, 1)) throw LastError("TerminateJobObject");
        }

        public static void TerminateSuspendedRoot(OwnedChild child) {
            if (!TerminateProcess(child.ProcessHandle, 1)) {
                int error = Marshal.GetLastWin32Error();
                if (error != 5) throw new Win32Exception(error, "TerminateProcess(unassigned suspended root) failed");
            }
        }

        public static string GetRootCreationUtc(OwnedChild child) {
            FileTime creation, exit, kernel, user;
            if (!GetProcessTimes(child.ProcessHandle, out creation, out exit, out kernel, out user)) throw LastError("GetProcessTimes(root)");
            long ticks = ((long)creation.High << 32) | creation.Low;
            return DateTime.FromFileTimeUtc(ticks).ToString("o");
        }

        public static string GetOwnedProcessCreationUtc(OwnedChild child, uint processId) {
            IntPtr process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION, false, processId);
            if (process == IntPtr.Zero) {
                int error = Marshal.GetLastWin32Error();
                if (error == 87) return null; // ERROR_INVALID_PARAMETER: the inventoried process already exited.
                throw new Win32Exception(error, "OpenProcess(owned identity) failed");
            }
            try {
                int inJob;
                if (!IsProcessInJob(process, child.JobHandle, out inJob)) throw LastError("IsProcessInJob(identity)");
                if (inJob == 0) return null; // PID was reused after the Job Object inventory snapshot.
                FileTime creation, exit, kernel, user;
                if (!GetProcessTimes(process, out creation, out exit, out kernel, out user)) throw LastError("GetProcessTimes(owned identity)");
                long ticks = ((long)creation.High << 32) | creation.Low;
                return DateTime.FromFileTimeUtc(ticks).ToString("o");
            }
            finally { CloseHandle(process); }
        }

        private static string FinalPath(IntPtr file) {
            StringBuilder path = new StringBuilder(512);
            while (true) {
                uint length = GetFinalPathNameByHandleW(file, path, (uint)path.Capacity, 0);
                if (length == 0) throw LastError("GetFinalPathNameByHandleW");
                if (length < path.Capacity) return path.ToString();
                if (length > 32768) throw new InvalidOperationException("Canonical file path exceeded its bounded capacity.");
                path.EnsureCapacity((int)length + 1);
            }
        }

        private static OwnedFileDescriptor DescribeFile(IntPtr file) {
            ByHandleFileInformation information;
            if (!GetFileInformationByHandle(file, out information)) throw LastError("GetFileInformationByHandle");
            ulong index = ((ulong)information.FileIndexHigh << 32) | information.FileIndexLow;
            return new OwnedFileDescriptor {
                CanonicalPath = FinalPath(file),
                FileId = information.VolumeSerialNumber.ToString("X8") + ":" + index.ToString("X16"),
                VolumeSerialNumber = information.VolumeSerialNumber,
                FileIndexHigh = information.FileIndexHigh,
                FileIndexLow = information.FileIndexLow
            };
        }

        public static OwnedFileDescriptor GetOpenFileDescriptor(SafeFileHandle file) {
            if (file == null || file.IsInvalid || file.IsClosed) throw new InvalidOperationException("The trusted image lease is not open.");
            bool added = false;
            try {
                file.DangerousAddRef(ref added);
                return DescribeFile(file.DangerousGetHandle());
            }
            finally { if (added) file.DangerousRelease(); }
        }

        public static OwnedProcessImage GetOwnedProcessImage(OwnedChild child, uint processId) {
            IntPtr process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION, false, processId);
            if (process == IntPtr.Zero) {
                int error = Marshal.GetLastWin32Error();
                throw new Win32Exception(error, "OpenProcess(owned image identity) failed");
            }
            try {
                int inJob;
                if (!IsProcessInJob(process, child.JobHandle, out inJob)) throw LastError("IsProcessInJob(image identity)");
                if (inJob == 0) throw new InvalidOperationException("Process is not a member of the invocation's owned Job Object.");
                FileTime creation, exit, kernel, user;
                if (!GetProcessTimes(process, out creation, out exit, out kernel, out user)) throw LastError("GetProcessTimes(image identity)");
                long ticks = ((long)creation.High << 32) | creation.Low;
                uint exitCode;
                if (!GetExitCodeProcess(process, out exitCode)) throw LastError("GetExitCodeProcess(image identity)");
                StringBuilder image = new StringBuilder(32768);
                uint imageLength = (uint)image.Capacity;
                if (!QueryFullProcessImageNameW(process, 0, image, ref imageLength)) throw LastError("QueryFullProcessImageNameW");
                IntPtr imageFile = OpenFileForIdentity(image.ToString(), FILE_READ_ATTRIBUTES, FILE_SHARE_READ, IntPtr.Zero, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, IntPtr.Zero);
                if (imageFile == IntPtr.Zero || imageFile == new IntPtr(-1)) throw LastError("CreateFileW(process image identity)");
                try {
                    OwnedFileDescriptor descriptor = DescribeFile(imageFile);
                    int stillInJob;
                    if (!IsProcessInJob(process, child.JobHandle, out stillInJob)) throw LastError("IsProcessInJob(image recheck)");
                    if (stillInJob == 0) throw new InvalidOperationException("Process left the owned Job Object during image verification.");
                    FileTime creationAgain, exitAgain, kernelAgain, userAgain;
                    if (!GetProcessTimes(process, out creationAgain, out exitAgain, out kernelAgain, out userAgain)) throw LastError("GetProcessTimes(image recheck)");
                    long ticksAgain = ((long)creationAgain.High << 32) | creationAgain.Low;
                    if (ticksAgain != ticks) throw new InvalidOperationException("Process creation identity changed during image verification.");
                    if (!GetExitCodeProcess(process, out exitCode)) throw LastError("GetExitCodeProcess(image recheck)");
                    return new OwnedProcessImage {
                        ProcessId = processId,
                        CreationUtc = DateTime.FromFileTimeUtc(ticks).ToString("o"),
                        InOwnedJob = true,
                        Running = exitCode == 259,
                        CanonicalPath = descriptor.CanonicalPath,
                        FileId = descriptor.FileId,
                        VolumeSerialNumber = descriptor.VolumeSerialNumber,
                        FileIndexHigh = descriptor.FileIndexHigh,
                        FileIndexLow = descriptor.FileIndexLow
                    };
                }
                finally { CloseHandle(imageFile); }
            }
            finally { CloseHandle(process); }
        }

        public static string QuoteRawArgument(string value) {
            if (value == null) throw new ArgumentNullException("value");
            if (value.Length > 0 && value.IndexOfAny(new char[] { ' ', '\t', '\n', '\v', '"' }) < 0) return value;
            StringBuilder quoted = new StringBuilder();
            quoted.Append('"');
            int slashes = 0;
            foreach (char ch in value) {
                if (ch == '\\') { slashes++; continue; }
                if (ch == '"') {
                    quoted.Append('\\', slashes * 2 + 1);
                    quoted.Append('"');
                    slashes = 0;
                    continue;
                }
                quoted.Append('\\', slashes);
                slashes = 0;
                quoted.Append(ch);
            }
            quoted.Append('\\', slashes * 2);
            quoted.Append('"');
            return quoted.ToString();
        }

        public static string BuildCommandLine(string executable, string[] arguments, bool preEncoded) {
            StringBuilder command = new StringBuilder();
            command.Append(QuoteRawArgument(executable));
            if (arguments != null) {
                foreach (string argument in arguments) {
                    if (argument == null) throw new ArgumentException("Native argument arrays cannot contain null values.");
                    command.Append(' ');
                    command.Append(preEncoded ? argument : QuoteRawArgument(argument));
                }
            }
            if (command.Length > 32760) throw new ArgumentException("The Windows command line exceeds the supported length.");
            return command.ToString();
        }

        private static void ThrowIfWin32Error(string operation) { throw LastError(operation); }

        public static void CloseHandle(IntPtr handle) {
            if (handle != IntPtr.Zero) CloseHandleNative(handle);
        }
        [DllImport("kernel32.dll", EntryPoint = "CloseHandle", SetLastError = true)]
        private static extern bool CloseHandleNative(IntPtr handle);
    }
}
'@
    Add-Type -TypeDefinition $nativeSource -Language CSharp -ErrorAction Stop
}

function New-CheckedOwnedProcessCommandLine {
    param(
        [Parameter(Mandatory)][string]$Executable,
        [AllowEmptyCollection()][string[]]$Arguments = @(),
        [switch]$PreEncoded
    )
    return [MultiLauncher.Dev.OwnedProcessNative]::BuildCommandLine($Executable, $Arguments, $PreEncoded.IsPresent)
}

function Add-CheckedOwnedProcessToJob {
    param([Parameter(Mandatory)][MultiLauncher.Dev.OwnedChild]$Owner)
    [MultiLauncher.Dev.OwnedProcessNative]::AssignToJob($Owner)
    [void][MultiLauncher.Dev.OwnedProcessNative]::VerifyAssignment($Owner)
}

function Get-CheckedOwnedProcessIds {
    param([Parameter(Mandatory)][MultiLauncher.Dev.OwnedChild]$Owner)
    return [MultiLauncher.Dev.OwnedProcessNative]::GetProcessIds($Owner)
}

function Get-CheckedOwnedIdentity {
    param(
        [Parameter(Mandatory)][MultiLauncher.Dev.OwnedChild]$Owner,
        [Parameter(Mandatory)][uint32]$ProcessId
    )
    try {
        $creation = [MultiLauncher.Dev.OwnedProcessNative]::GetOwnedProcessCreationUtc($Owner, $ProcessId)
        if ([string]::IsNullOrWhiteSpace($creation)) { return $null }
        return [pscustomobject]@{
            ProcessId = [int]$ProcessId
            CreationDate = $creation
            Name = 'owned process'
        }
    }
    catch { throw "Could not verify owned process identity $ProcessId`: $($_.Exception.Message)" }
}

function Get-CheckedOwnedProcessImage {
    param(
        [Parameter(Mandatory)][MultiLauncher.Dev.OwnedChild]$Owner,
        [Parameter(Mandatory)][uint32]$ProcessId
    )
    try { return [MultiLauncher.Dev.OwnedProcessNative]::GetOwnedProcessImage($Owner, $ProcessId) }
    catch { throw "Could not verify owned process image $ProcessId`: $($_.Exception.Message)" }
}

function Test-CheckedOwnedIdentityAbsent {
    param([Parameter(Mandatory)][object]$Identity)
    try {
        $process = [Diagnostics.Process]::GetProcessById([int]$Identity.ProcessId)
        try {
            $creation = $process.StartTime.ToUniversalTime()
            $recorded = ([datetime]$Identity.CreationDate).ToUniversalTime()
            if ([Math]::Abs(($creation - $recorded).Ticks) -le 100) { return 'Alive' }
            return 'Reused'
        }
        finally { $process.Dispose() }
    }
    catch [ArgumentException] { return 'Absent' }
    catch { return 'Unknown' }
}

function Test-VerifiedMsvcCargoCommand {
    param(
        [Parameter(Mandatory)][string]$Stage,
        [Parameter(Mandatory)][string]$Executable,
        [AllowEmptyCollection()][string[]]$ArgumentList = @()
    )
    if ($Stage -cne 'compile' -or [IO.Path]::GetFileName($Executable) -ine 'cargo.exe' -or $ArgumentList.Count -eq 0) {
        return $false
    }
    if ([string]$ArgumentList[0] -in @('build', 'check', 'clippy')) { return $true }
    return $ArgumentList.Count -ge 2 -and [string]$ArgumentList[0] -ceq 'nextest' -and [string]$ArgumentList[1] -ceq 'list'
}

function Invoke-CheckedOwnedProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Executable,
        [AllowEmptyCollection()][string[]]$ArgumentList = @(),
        [AllowEmptyCollection()][string[]]$PreEncodedArguments,
        [string]$WorkingDirectory = (Get-Location).Path,
        [ValidateRange(1, 86400)][int]$TimeoutSeconds = 30,
        [ValidateRange(1, 60)][int]$CleanupTimeoutSeconds = 5,
        [string]$Stage = 'unspecified',
        [bool]$HiddenWindow = $true,
        [ValidateSet('none', 'verified-msvc-vctip')][string]$AuxiliaryProcessPolicy = 'none',
        [AllowEmptyCollection()][object[]]$VerifiedMsvcVctipImages = @(),
        [scriptblock]$TestOwnedProcessImageInspector,
        [Parameter(Mandatory)][string]$StdoutPath,
        [Parameter(Mandatory)][string]$StderrPath
    )

    $startedUtc = [DateTimeOffset]::UtcNow
    $deadlineUtc = $startedUtc.AddSeconds($TimeoutSeconds)
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $reason = 'completed'
    $timeoutReason = $null
    $timedOut = $false
    $processCreated = $false
    $processStarted = $false
    $processExited = $false
    $exitCode = $null
    $cleanupVerified = $false
    $outputDrainVerified = $false
    $stdoutFile = $null
    $stderrFile = $null
    $owner = $null
    $assigned = $false
    $rootIdentity = $null
    $identityMap = [ordered]@{}
    $postCleanup = @()
    $recorderErrors = [System.Collections.Generic.List[string]]::new()
    $cleanupErrors = [System.Collections.Generic.List[string]]::new()
    $stdoutState = $null
    $stderrState = $null
    $rootExitDeadline = $null
    $resolvedExecutable = $null
    $resolvedWorkingDirectory = $null
    $auxiliaryRetirementPending = $false
    $forcedJobRetirementAttempted = $false
    $auxiliaryDisposition = if ($AuxiliaryProcessPolicy -eq 'none') { 'not-enabled' } else { 'not-observed' }
    $auxiliaryClassifications = [System.Collections.Generic.List[object]]::new()
    $verifiedImageLeases = [System.Collections.Generic.List[object]]::new()
    $auxiliaryPreflightSummary = @()

    try {
        if ([string]::IsNullOrWhiteSpace($Executable)) { throw 'Executable must not be empty.' }
        if (!(Test-Path -LiteralPath $Executable -PathType Leaf)) {
            $application = Get-Command -Name $Executable -CommandType Application -ErrorAction Stop | Select-Object -First 1
            if ($null -eq $application) { throw "Executable was not found as a native application: $Executable" }
            $Executable = if ($application.Source) { $application.Source } else { $application.Path }
        }
        if ([IO.Path]::GetExtension($Executable) -in @('.ps1', '.cmd', '.bat')) { throw "Pass a native executable, not a shell script or command wrapper: $Executable" }
        if (!(Test-Path -LiteralPath $WorkingDirectory -PathType Container)) { throw "Working directory does not exist: $WorkingDirectory" }
        $resolvedExecutable = [IO.Path]::GetFullPath($Executable)
        $resolvedWorkingDirectory = [IO.Path]::GetFullPath($WorkingDirectory)
        if ($AuxiliaryProcessPolicy -eq 'verified-msvc-vctip') {
            $policyArguments = if ($null -ne $PreEncodedArguments) { @() } else { @($ArgumentList) }
            $testImageInspectorSupplied = $null -ne $TestOwnedProcessImageInspector
            $allowedCompilerCommand = Test-VerifiedMsvcCargoCommand -Stage $Stage -Executable $resolvedExecutable -ArgumentList $policyArguments
            if ($null -ne $PreEncodedArguments -or $Stage -cne 'compile' -or
                (!$testImageInspectorSupplied -and !$allowedCompilerCommand)) {
                throw 'Verified MSVC helper retirement is limited to an explicit Cargo compile invocation.'
            }
            if ($VerifiedMsvcVctipImages.Count -eq 0) { throw 'Verified MSVC helper retirement requires a successful trusted-image preflight.' }
            foreach ($image in $VerifiedMsvcVctipImages) {
                if ($null -eq $image -or $null -eq $image.Lease -or $null -eq $image.Descriptor) {
                    throw 'A verified MSVC image descriptor or held file lease is missing.'
                }
                $descriptor = $image.Descriptor
                $lease = $image.Lease
                if (!$lease.CanRead -or $lease.SafeFileHandle.IsClosed -or $lease.SafeFileHandle.IsInvalid) {
                    throw 'A verified MSVC image lease was closed before the Cargo invocation.'
                }
                if ([string]$descriptor.SignatureStatus -cne 'Valid' -or
                    [string]$descriptor.SignerOrganization -cne 'Microsoft Corporation' -or
                    [string]$descriptor.SignerThumbprint -notmatch '^[0-9A-Fa-f]{40,64}$' -or
                    [string]$descriptor.SignerCertificateSha256 -notmatch '^[0-9a-f]{64}$' -or
                    [string]$descriptor.Sha256 -notmatch '^[0-9a-f]{64}$' -or
                    [string]::IsNullOrWhiteSpace([string]$descriptor.FileId) -or
                    [string]::IsNullOrWhiteSpace([string]$descriptor.CanonicalPath)) {
                    throw 'The MSVC helper trust descriptor is malformed or is not a valid Microsoft-signed image.'
                }
                $heldDescriptor = [MultiLauncher.Dev.OwnedProcessNative]::GetOpenFileDescriptor($lease.SafeFileHandle)
                if ([string]$heldDescriptor.FileId -cne [string]$descriptor.FileId -or
                    [string]$heldDescriptor.CanonicalPath -ine [string]$descriptor.CanonicalPath) {
                    throw 'The held MSVC image lease no longer matches its trusted file identity.'
                }
                $verifiedImageLeases.Add($image)
                $auxiliaryPreflightSummary += [pscustomobject][ordered]@{
                    CanonicalPath = [string]$descriptor.CanonicalPath
                    FileId = [string]$descriptor.FileId
                    Sha256 = [string]$descriptor.Sha256
                    SignatureStatus = [string]$descriptor.SignatureStatus
                    SignerThumbprint = [string]$descriptor.SignerThumbprint
                    SignerCertificateSha256 = [string]$descriptor.SignerCertificateSha256
                    SignerOrganization = [string]$descriptor.SignerOrganization
                    VerificationSource = [string]$descriptor.VerificationSource
                }
            }
            $auxiliaryDisposition = 'eligible-image-preflight-verified'
        }
        elseif ($VerifiedMsvcVctipImages.Count -gt 0 -or $TestOwnedProcessImageInspector) {
            throw 'Trusted MSVC image descriptors require the explicit verified-msvc-vctip policy.'
        }
        try {
            $stdoutFile = [IO.File]::Open($StdoutPath, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::Read)
            $stderrFile = [IO.File]::Open($StderrPath, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        }
        catch {
            $reason = 'output recording failure: ' + $_.Exception.Message
            $recorderErrors.Add('Output log open: ' + $_.Exception.Message)
            throw
        }

        $isPreEncoded = $null -ne $PreEncodedArguments
        $arguments = if ($isPreEncoded) { @($PreEncodedArguments) } else { @($ArgumentList) }
        $commandLine = New-CheckedOwnedProcessCommandLine -Executable $resolvedExecutable -Arguments $arguments -PreEncoded:$isPreEncoded
        $owner = [MultiLauncher.Dev.OwnedProcessNative]::CreateSuspended(
            $resolvedExecutable, $commandLine, $resolvedWorkingDirectory, $HiddenWindow)
        $processCreated = $true
        $rootIdentity = [pscustomobject]@{
            ProcessId = [int]$owner.ProcessId
            CreationDate = [MultiLauncher.Dev.OwnedProcessNative]::GetRootCreationUtc($owner)
            Name = [IO.Path]::GetFileName($Executable)
        }
        $identityMap[[string]$rootIdentity.ProcessId] = $rootIdentity

        try {
            Add-CheckedOwnedProcessToJob -Owner $owner
            $assigned = $true
            $initialIds = @(Get-CheckedOwnedProcessIds -Owner $owner)
            if ($initialIds -notcontains [uint32]$owner.ProcessId) { throw 'The Job Object did not report the suspended root identity after assignment.' }
        }
        catch {
            $reason = 'ownership failure: ' + $_.Exception.Message
            throw
        }

        if ($watch.Elapsed.TotalSeconds -ge $TimeoutSeconds) {
            $timedOut = $true
            $timeoutReason = 'stage deadline elapsed before process resume'
            $reason = 'timeout'
        }
        else {
            $owner.StartDrain($stdoutFile, $stderrFile)
            $stdoutFile = $null
            $stderrFile = $null
            [MultiLauncher.Dev.OwnedProcessNative]::Resume($owner)
            $processStarted = $true

            while ($true) {
                try {
                    foreach ($processId in @(Get-CheckedOwnedProcessIds -Owner $owner)) {
                        $key = [string]$processId
                        if (!$identityMap.Contains($key)) {
                            $identity = Get-CheckedOwnedIdentity -Owner $owner -ProcessId $processId
                            if ($identity) { $identityMap[$key] = $identity }
                        }
                    }
                }
                catch {
                    if ($reason -eq 'completed') { $reason = 'containment monitor failure: ' + $_.Exception.Message }
                    break
                }

                foreach ($state in @($owner.StdoutDrain, $owner.StderrDrain)) {
                    if ($state -and $state.Error -and $reason -eq 'completed') {
                        $reason = 'output recording failure: ' + $state.Error
                    }
                }
                if ($reason -ne 'completed') { break }

                $rootWait = [MultiLauncher.Dev.OwnedProcessNative]::WaitForRoot($owner, 0)
                $activeCount = [MultiLauncher.Dev.OwnedProcessNative]::GetActiveProcessCount($owner)
                if ($rootWait -eq 0) {
                    $processExited = $true
                    $exitCode = [MultiLauncher.Dev.OwnedProcessNative]::GetExitCode($owner)
                    if ($activeCount -eq 0) { break }
                    if ($null -eq $rootExitDeadline) { $rootExitDeadline = [Diagnostics.Stopwatch]::StartNew() }
                    if ($rootExitDeadline.Elapsed.TotalSeconds -ge $CleanupTimeoutSeconds) {
                        if ($watch.Elapsed.TotalSeconds -ge $TimeoutSeconds) {
                            $timedOut = $true
                            $timeoutReason = 'stage deadline elapsed during owned descendant grace'
                            $reason = 'timeout'
                        }
                        elseif ($AuxiliaryProcessPolicy -eq 'verified-msvc-vctip' -and $exitCode -eq 0 -and
                            $recorderErrors.Count -eq 0 -and $cleanupErrors.Count -eq 0) {
                            try {
                                $currentIds = @(Get-CheckedOwnedProcessIds -Owner $owner)
                                $activeMembers = [System.Collections.Generic.List[object]]::new()
                                foreach ($memberId in $currentIds) {
                                    if ([uint32]$memberId -eq [uint32]$owner.ProcessId) { continue }
                                    $memberIdentity = Get-CheckedOwnedIdentity -Owner $owner -ProcessId $memberId
                                    if ($null -eq $memberIdentity) { throw "Owned process $memberId could not be positively identified." }
                                    $identityMap[[string]$memberId] = $memberIdentity
                                    $memberImage = if ($TestOwnedProcessImageInspector) {
                                        & $TestOwnedProcessImageInspector $owner $memberId
                                    } else {
                                        Get-CheckedOwnedProcessImage -Owner $owner -ProcessId $memberId
                                    }
                                    if (!$memberImage.InOwnedJob -or !$memberImage.Running -or
                                        [uint32]$memberImage.ProcessId -ne [uint32]$memberId -or
                                        ([datetime]$memberImage.CreationUtc).ToUniversalTime() -ne ([datetime]$memberIdentity.CreationDate).ToUniversalTime()) {
                                        throw "Owned process $memberId was not a live, exact Job/PID/creation identity."
                                    }
                                    $matches = @($verifiedImageLeases | Where-Object {
                                        [string]$_.Descriptor.FileId -ceq [string]$memberImage.FileId -and
                                        [string]$_.Descriptor.CanonicalPath -ieq [string]$memberImage.CanonicalPath
                                    })
                                    if ($matches.Count -ne 1) {
                                        throw "Owned process $memberId has an unknown or ambiguous executable image."
                                    }
                                    $activeMembers.Add([pscustomobject][ordered]@{
                                        ProcessId = [int]$memberId
                                        CreationDate = [string]$memberImage.CreationUtc
                                        CanonicalPath = [string]$memberImage.CanonicalPath
                                        FileId = [string]$memberImage.FileId
                                        Sha256 = [string]$matches[0].Descriptor.Sha256
                                        SignerThumbprint = [string]$matches[0].Descriptor.SignerThumbprint
                                        SignerCertificateSha256 = [string]$matches[0].Descriptor.SignerCertificateSha256
                                        Classification = 'verified-msvc-vctip-auxiliary'
                                        Disposition = 'pending-owned-job-retirement'
                                    })
                                }
                                if ($activeMembers.Count -eq 0 -or $activeMembers.Count -ne [int]$activeCount) {
                                    throw 'The active Job membership count did not match the verified auxiliary process identities.'
                                }
                                $secondIds = @(Get-CheckedOwnedProcessIds -Owner $owner | Where-Object { [uint32]$_ -ne [uint32]$owner.ProcessId })
                                $firstKey = (@($activeMembers | ForEach-Object { [string]$_.ProcessId } | Sort-Object) -join ',')
                                $secondKey = (@($secondIds | ForEach-Object { [string]$_ } | Sort-Object) -join ',')
                                $secondActiveCount = [MultiLauncher.Dev.OwnedProcessNative]::GetActiveProcessCount($owner)
                                if ($firstKey -cne $secondKey -or [int]$secondActiveCount -ne $activeMembers.Count -or
                                    $watch.Elapsed.TotalSeconds -ge $TimeoutSeconds) {
                                    throw 'The Job membership changed or the stage deadline expired during helper classification.'
                                }
                                foreach ($member in $activeMembers) {
                                    $recheckedIdentity = Get-CheckedOwnedIdentity -Owner $owner -ProcessId ([uint32]$member.ProcessId)
                                    $recheckedImage = if ($TestOwnedProcessImageInspector) {
                                        & $TestOwnedProcessImageInspector $owner ([uint32]$member.ProcessId)
                                    } else {
                                        Get-CheckedOwnedProcessImage -Owner $owner -ProcessId ([uint32]$member.ProcessId)
                                    }
                                    if ($null -eq $recheckedIdentity -or !$recheckedImage.InOwnedJob -or !$recheckedImage.Running -or
                                        [string]$recheckedIdentity.CreationDate -cne [string]$member.CreationDate -or
                                        [string]$recheckedImage.CreationUtc -cne [string]$member.CreationDate -or
                                        [string]$recheckedImage.FileId -cne [string]$member.FileId -or
                                        [string]$recheckedImage.CanonicalPath -ine [string]$member.CanonicalPath) {
                                        throw "Owned process $($member.ProcessId) changed identity during the final retirement check."
                                    }
                                }
                                foreach ($member in $activeMembers) { $auxiliaryClassifications.Add($member) }
                                $auxiliaryRetirementPending = $true
                                $auxiliaryDisposition = 'verified-helpers-pending-forced-job-retirement'
                                break
                            }
                            catch {
                                $reason = 'auxiliary process classification failure: ' + $_.Exception.Message
                                $auxiliaryDisposition = 'classification-failed'
                                break
                            }
                        }
                        else {
                            $reason = if ($exitCode -ne 0) { 'owned descendants survived a nonzero root exit' } else { 'owned descendants survived the root process' }
                        }
                        break
                    }
                }
                if ($watch.Elapsed.TotalSeconds -ge $TimeoutSeconds) {
                    $timedOut = $true
                    $timeoutReason = 'stage deadline elapsed'
                    $reason = 'timeout'
                    break
                }
                Start-Sleep -Milliseconds 20
            }
        }
    }
    catch {
        if ($reason -eq 'completed') {
            if ($processStarted) { $reason = 'containment monitor failure: ' + $_.Exception.Message }
            elseif ($processCreated -and !$assigned) { $reason = 'ownership failure: ' + $_.Exception.Message }
            else { $reason = 'start failure: ' + $_.Exception.Message }
        }
    }
    finally {
        foreach ($stream in @($stdoutFile, $stderrFile)) {
            if ($stream) {
                try { $stream.Dispose() }
                catch { $recorderErrors.Add('Output stream close: ' + $_.Exception.Message) }
            }
        }
        $stdoutFile = $null
        $stderrFile = $null

        if ($owner) {
            if (!$assigned) {
                try {
                    [MultiLauncher.Dev.OwnedProcessNative]::TerminateSuspendedRoot($owner)
                    $rootWait = [MultiLauncher.Dev.OwnedProcessNative]::WaitForRoot($owner, [Math]::Min(3000, $CleanupTimeoutSeconds * 1000))
                    $processExited = $rootWait -eq 0
                    if ($processExited) { $exitCode = [MultiLauncher.Dev.OwnedProcessNative]::GetExitCode($owner) }
                    else { $cleanupErrors.Add('Unassigned suspended root did not exit before its cleanup deadline.') }
                    $cleanupVerified = $processExited
                }
                catch { $cleanupErrors.Add('Suspended root cleanup: ' + $_.Exception.Message) }
            }
            else {
                try {
                    foreach ($processId in @(Get-CheckedOwnedProcessIds -Owner $owner)) {
                        $key = [string]$processId
                        if (!$identityMap.Contains($key)) {
                            $identity = Get-CheckedOwnedIdentity -Owner $owner -ProcessId $processId
                            if ($identity) { $identityMap[$key] = $identity }
                        }
                    }
                }
                catch { $cleanupErrors.Add('Owned identity snapshot: ' + $_.Exception.Message) }

                if ($reason -ne 'completed' -or $auxiliaryRetirementPending) {
                    if ($auxiliaryRetirementPending) { $forcedJobRetirementAttempted = $true }
                    try { [MultiLauncher.Dev.OwnedProcessNative]::TerminateJob($owner) }
                    catch { $cleanupErrors.Add('Job Object termination: ' + $_.Exception.Message) }
                }

                $cleanupWatch = [Diagnostics.Stopwatch]::StartNew()
                while ($true) {
                    try {
                        $activeCount = [MultiLauncher.Dev.OwnedProcessNative]::GetActiveProcessCount($owner)
                        if ($activeCount -eq 0) { $cleanupVerified = $true; break }
                    }
                    catch { $cleanupErrors.Add('Job Object empty check: ' + $_.Exception.Message); break }
                    if ($cleanupWatch.Elapsed.TotalSeconds -ge $CleanupTimeoutSeconds) {
                        $cleanupErrors.Add('Job Object did not become empty before the cleanup deadline.')
                        break
                    }
                    Start-Sleep -Milliseconds 20
                }
                if (!$processExited) {
                    try {
                        $rootWait = [MultiLauncher.Dev.OwnedProcessNative]::WaitForRoot($owner, [Math]::Max(0, ($CleanupTimeoutSeconds * 1000) - [int]$cleanupWatch.ElapsedMilliseconds))
                        $processExited = $rootWait -eq 0
                        if ($processExited) { $exitCode = [MultiLauncher.Dev.OwnedProcessNative]::GetExitCode($owner) }
                        else { $cleanupErrors.Add('Root process did not signal before the cleanup deadline.') }
                    }
                    catch { $cleanupErrors.Add('Root wait: ' + $_.Exception.Message) }
                }
            }

            $drainTasks = @($owner.StdoutTask, $owner.StderrTask) | Where-Object { $null -ne $_ }
            if ($drainTasks.Count -gt 0) {
                try {
                    $drainWatch = [Diagnostics.Stopwatch]::StartNew()
                    $remainingDrainMs = [Math]::Max(0, ($CleanupTimeoutSeconds * 1000) - [int]$drainWatch.ElapsedMilliseconds)
                    if (![Threading.Tasks.Task]::WaitAll([Threading.Tasks.Task[]]$drainTasks, $remainingDrainMs)) {
                        $cleanupErrors.Add('Output drain did not finish before the bounded drain deadline.')
                        try { if ($owner.StdoutPipe) { $owner.StdoutPipe.Dispose() } } catch { }
                        try { if ($owner.StderrPipe) { $owner.StderrPipe.Dispose() } } catch { }
                    }
                }
                catch { $cleanupErrors.Add('Output drain wait: ' + $_.Exception.Message) }
            }
            $stdoutState = $owner.StdoutDrain
            $stderrState = $owner.StderrDrain
            $outputDrainVerified = (!$owner.StdoutTask -or ($stdoutState -and $stdoutState.Completed)) -and
                (!$owner.StderrTask -or ($stderrState -and $stderrState.Completed))
            foreach ($state in @($stdoutState, $stderrState)) {
                if ($state -and $state.Error) { $recorderErrors.Add($state.Error) }
            }

            foreach ($identity in $identityMap.Values) {
                $state = Test-CheckedOwnedIdentityAbsent -Identity $identity
                $postCleanup += [pscustomobject]@{ ProcessId = $identity.ProcessId; CreationDate = $identity.CreationDate; State = $state }
            }
            if (@($postCleanup | Where-Object { $_.State -in @('Alive', 'Unknown') }).Count -gt 0) {
                $cleanupVerified = $false
                $cleanupErrors.Add('One or more owned process identities remain alive or could not be verified absent.')
            }
            if (!$outputDrainVerified) { $cleanupErrors.Add('Output drain completion is unverified.') }
            if ($stdoutState -and $stdoutState.BytesRead -ne $stdoutState.BytesRecorded -and !$stdoutState.Error) {
                $recorderErrors.Add('Standard output bytes were read but not recorded.')
            }
            if ($stderrState -and $stderrState.BytesRead -ne $stderrState.BytesRecorded -and !$stderrState.Error) {
                $recorderErrors.Add('Standard error bytes were read but not recorded.')
            }

            if ($reason -eq 'completed' -and (!$cleanupVerified -or !$outputDrainVerified)) { $reason = 'cleanup or output drain unverified' }
            if ($reason -eq 'completed' -and $recorderErrors.Count -gt 0) { $reason = 'output recording failure' }
            if ($reason -eq 'completed' -and $processExited -and $exitCode -ne 0) { $reason = 'failed exit' }
            if ($auxiliaryRetirementPending) {
                foreach ($classification in $auxiliaryClassifications) { $classification.Disposition = if ($cleanupVerified) { 'retired-and-cleanup-verified' } else { 'retirement-attempted-cleanup-unverified' } }
            }
            try { $owner.Dispose() } catch { $cleanupErrors.Add('Owned process handle disposal: ' + $_.Exception.Message) }
        }
        $watch.Stop()
    }

    if ($cleanupErrors.Count -gt 0 -and $reason -eq 'completed') { $reason = 'cleanup incomplete or uncertain' }
    if ($recorderErrors.Count -gt 0 -and $reason -eq 'completed') { $reason = 'output recording failure' }
    if ($auxiliaryRetirementPending) {
        $finalAuxiliaryChecksPassed = $reason -eq 'completed' -and $cleanupVerified -and $outputDrainVerified -and
            $recorderErrors.Count -eq 0 -and $cleanupErrors.Count -eq 0 -and
            @($postCleanup | Where-Object { $_.State -in @('Alive', 'Unknown') }).Count -eq 0
        foreach ($classification in $auxiliaryClassifications) {
            $classification.Disposition = if ($finalAuxiliaryChecksPassed) { 'retired-and-all-final-checks-verified' } else { 'retirement-attempted-but-final-checks-failed' }
        }
        $auxiliaryDisposition = if ($finalAuxiliaryChecksPassed) { 'verified-helpers-retired-and-all-final-checks-verified' } else { 'verified-helpers-retired-but-final-checks-failed' }
    }
    $endedUtc = [DateTimeOffset]::UtcNow
    $succeeded = $reason -eq 'completed' -and $processStarted -and $processExited -and $exitCode -eq 0 -and
        $cleanupVerified -and $outputDrainVerified -and $recorderErrors.Count -eq 0 -and $cleanupErrors.Count -eq 0
    [pscustomobject][ordered]@{
        Stage = $Stage
        StartedUtc = $startedUtc.ToString('o')
        EndedUtc = $endedUtc.ToString('o')
        DeadlineUtc = $deadlineUtc.ToString('o')
        TimeoutSeconds = $TimeoutSeconds
        TimeoutReason = $timeoutReason
        TimedOut = $timedOut
        ElapsedSeconds = [Math]::Round($watch.Elapsed.TotalSeconds, 6)
        Executable = if ($resolvedExecutable) { $resolvedExecutable } else { $Executable }
        ArgumentList = if ($null -ne $PreEncodedArguments) { @($PreEncodedArguments) } else { @($ArgumentList) }
        ArgumentsPreEncoded = $null -ne $PreEncodedArguments
        WorkingDirectory = if ($resolvedWorkingDirectory) { $resolvedWorkingDirectory } else { $WorkingDirectory }
        RunnerPid = if ($owner) { [int]$owner.ProcessId } else { $null }
        ProcessCreated = $processCreated
        ProcessStarted = $processStarted
        ProcessExited = $processExited
        ExitCode = if ($timedOut) { $null } else { $exitCode }
        Reason = $reason
        Succeeded = $succeeded
        CleanupVerified = $cleanupVerified
        CleanupErrors = @($cleanupErrors)
        OutputDrainVerified = $outputDrainVerified
        StdoutPath = $StdoutPath
        StderrPath = $StderrPath
        StdoutBytesRead = if ($stdoutState) { $stdoutState.BytesRead } else { 0 }
        StdoutBytesRecorded = if ($stdoutState) { $stdoutState.BytesRecorded } else { 0 }
        StderrBytesRead = if ($stderrState) { $stderrState.BytesRead } else { 0 }
        StderrBytesRecorded = if ($stderrState) { $stderrState.BytesRecorded } else { 0 }
        RecordingErrors = @($recorderErrors)
        OwnedProcesses = @($identityMap.Values)
        PostCleanup = @($postCleanup)
        RootAlive = if ($rootIdentity) { @($postCleanup | Where-Object { $_.ProcessId -eq $rootIdentity.ProcessId -and $_.State -eq 'Alive' }).Count -gt 0 } else { $false }
        AuxiliaryProcessPolicy = $AuxiliaryProcessPolicy
        AuxiliaryPreflight = @($auxiliaryPreflightSummary)
        AuxiliaryProcessClassifications = @($auxiliaryClassifications)
        ForcedJobRetirementAttempted = $forcedJobRetirementAttempted
        AuxiliaryDisposition = $auxiliaryDisposition
    }
}
