use crate::core::error::{Error, Result};

use crate::core::config::models::ProcessPriority;

pub fn process_priority_supported() -> bool {
    cfg!(windows)
}

pub fn open_path(path: &std::path::Path) -> Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| Error::Terminal(format!("open path: {error}")))
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| Error::Terminal(format!("open path: {error}")))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| Error::Terminal(format!("open path: {error}")))
    }
}

/// Open a URL in the user's browser.
///
/// Separate from [`open_path`] because a URL is not a path: `explorer.exe`
/// mangles query strings, and the shell's `open` verb is what handles the
/// default-browser association.
pub fn open_url(url: &str) -> Result<()> {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::PCWSTR;

        let mut wide: Vec<u16> = OsStr::new(url).encode_wide().collect();
        wide.push(0);
        let opened =
            unsafe { ShellExecuteW(None, None, PCWSTR(wide.as_ptr()), None, None, SW_SHOWNORMAL) };
        // ShellExecuteW reports success as "greater than 32"; anything at or
        // below is a documented error code, not an HRESULT.
        if opened.0 as usize <= 32 {
            return Err(Error::Terminal(format!(
                "open url: Windows returned {}",
                opened.0 as usize
            )));
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| Error::Terminal(format!("open url: {error}")))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| Error::Terminal(format!("open url: {error}")))
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = url;
        Err(Error::Terminal(
            "opening a URL is not supported on this platform".into(),
        ))
    }
}

pub fn set_console_visibility(visible: bool) -> Result<()> {
    #[cfg(windows)]
    {
        use windows::Win32::System::Console::GetConsoleWindow;
        use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOW, ShowWindow};

        let window = unsafe { GetConsoleWindow() };
        if window.0.is_null() {
            return Ok(());
        }
        let _ = unsafe { ShowWindow(window, if visible { SW_SHOW } else { SW_HIDE }) };
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = visible;
        Err(Error::Terminal(
            "console visibility is not supported on this platform".into(),
        ))
    }
}

#[cfg(windows)]
pub fn set_process_priority(pid: u32, priority: ProcessPriority) -> Result<()> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        ABOVE_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, OpenProcess,
        PROCESS_ACCESS_RIGHTS, PROCESS_SET_INFORMATION, SetPriorityClass,
    };

    if priority == ProcessPriority::Default {
        return Ok(());
    }
    let access = PROCESS_ACCESS_RIGHTS(PROCESS_SET_INFORMATION.0);
    let handle = unsafe { OpenProcess(access, false, pid) }
        .map_err(|error| Error::Launch(format!("open game process {pid}: {error}")))?;
    let class = match priority {
        ProcessPriority::Default => NORMAL_PRIORITY_CLASS,
        ProcessPriority::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
        ProcessPriority::High => HIGH_PRIORITY_CLASS,
    };
    let result = unsafe { SetPriorityClass(handle, class) }
        .map_err(|error| Error::Launch(format!("set game process priority: {error}")));
    let _ = unsafe { CloseHandle(handle) };
    result
}

#[cfg(not(windows))]
pub fn set_process_priority(_pid: u32, priority: ProcessPriority) -> Result<()> {
    if priority == ProcessPriority::Default {
        Ok(())
    } else {
        Err(Error::Launch(
            "custom process priority is not supported on this platform".into(),
        ))
    }
}

/// When a process started, in seconds since the Unix epoch.
///
/// `None` when the process does not exist or cannot be queried. The value is a
/// fingerprint as much as a timestamp: it tells a live owner apart from a
/// different process that happens to reuse its id.
#[cfg(windows)]
pub fn process_started_at(pid: u32) -> Option<u64> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let queried =
        unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) };
    let _ = unsafe { CloseHandle(handle) };
    queried.ok()?;

    let ticks = ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64;
    // FILETIME counts 100 ns intervals since 1601-01-01.
    const EPOCH_DIFFERENCE_SECONDS: u64 = 11_644_473_600;
    (ticks / 10_000_000).checked_sub(EPOCH_DIFFERENCE_SECONDS)
}

#[cfg(not(windows))]
pub fn process_started_at(_pid: u32) -> Option<u64> {
    None
}

/// Whether a process id still belongs to a running process.
///
/// Used only when the recorded start time is unavailable. Erring towards "alive"
/// is deliberate: a wrongly live owner leaves a stale record behind, while a
/// wrongly dead one would fail a task that is still running.
#[cfg(windows)]
pub fn process_is_alive(pid: u32) -> bool {
    process_started_at(pid).is_some()
}

#[cfg(target_os = "linux")]
pub fn process_is_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(all(not(windows), not(target_os = "linux")))]
pub fn process_is_alive(_pid: u32) -> bool {
    true
}

#[cfg(windows)]
pub fn pick_directory(initial: &std::path::Path) -> Result<Option<std::path::PathBuf>> {
    let initial = initial.to_path_buf();
    std::thread::spawn(move || pick_directory_sta(&initial))
        .join()
        .map_err(|_| Error::Terminal("directory picker thread panicked".into()))?
}

#[cfg(windows)]
fn pick_directory_sta(initial: &std::path::Path) -> Result<Option<std::path::PathBuf>> {
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog,
        IFileOpenDialog, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    use windows::core::{HRESULT, PCWSTR};

    struct ComApartment;
    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    struct TaskMemory(windows::core::PWSTR);
    impl Drop for TaskMemory {
        fn drop(&mut self) {
            unsafe { CoTaskMemFree(Some(self.0.0.cast())) };
        }
    }

    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
        .ok()
        .map_err(|error| Error::Terminal(format!("initialize folder picker: {error}")))?;
    let _apartment = ComApartment;

    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| Error::Terminal(format!("create folder picker: {error}")))?;
    unsafe {
        dialog
            .SetOptions(FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_NOCHANGEDIR)
    }
    .map_err(|error| Error::Terminal(format!("configure folder picker: {error}")))?;
    unsafe { dialog.SetTitle(windows::core::w!("Select Minecraft game directory")) }
        .map_err(|error| Error::Terminal(format!("title folder picker: {error}")))?;

    if initial.is_dir() {
        let mut wide = initial.as_os_str().encode_wide().collect::<Vec<_>>();
        wide.push(0);
        if let Ok(folder) = unsafe {
            SHCreateItemFromParsingName::<_, _, windows::Win32::UI::Shell::IShellItem>(
                PCWSTR(wide.as_ptr()),
                None,
            )
        } {
            let _ = unsafe { dialog.SetFolder(&folder) };
        }
    }

    if let Err(error) = unsafe { dialog.Show(None) } {
        if error.code() == HRESULT(0x8007_04c7_u32 as i32) {
            return Ok(None);
        }
        return Err(Error::Terminal(format!("show folder picker: {error}")));
    }
    let selected = unsafe { dialog.GetResult() }
        .map_err(|error| Error::Terminal(format!("read selected folder: {error}")))?;
    let path = TaskMemory(
        unsafe { selected.GetDisplayName(SIGDN_FILESYSPATH) }
            .map_err(|error| Error::Terminal(format!("read folder path: {error}")))?,
    );
    let path = unsafe { path.0.to_string() }
        .map_err(|error| Error::Terminal(format!("decode folder path: {error}")))?;
    Ok(Some(path.into()))
}

#[cfg(not(windows))]
pub fn pick_directory(_initial: &std::path::Path) -> Result<Option<std::path::PathBuf>> {
    Err(Error::Terminal(
        "native directory picker is only supported on Windows".into(),
    ))
}

#[cfg(windows)]
pub fn pick_save_file(
    initial_directory: &std::path::Path,
    suggested_name: &str,
) -> Result<Option<std::path::PathBuf>> {
    let initial = initial_directory.to_path_buf();
    let suggested_name = suggested_name.to_owned();
    std::thread::spawn(move || pick_save_file_sta(&initial, &suggested_name))
        .join()
        .map_err(|_| Error::Terminal("save-file picker thread panicked".into()))?
}

#[cfg(windows)]
fn pick_save_file_sta(
    initial: &std::path::Path,
    suggested_name: &str,
) -> Result<Option<std::path::PathBuf>> {
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_OVERWRITEPROMPT, FileSaveDialog, IFileSaveDialog,
        SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    use windows::core::{HRESULT, PCWSTR};

    struct ComApartment;
    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    struct TaskMemory(windows::core::PWSTR);
    impl Drop for TaskMemory {
        fn drop(&mut self) {
            unsafe { CoTaskMemFree(Some(self.0.0.cast())) };
        }
    }

    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
        .ok()
        .map_err(|error| Error::Terminal(format!("initialize save-file picker: {error}")))?;
    let _apartment = ComApartment;
    let dialog: IFileSaveDialog =
        unsafe { CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| Error::Terminal(format!("create save-file picker: {error}")))?;
    unsafe { dialog.SetOptions(FOS_FORCEFILESYSTEM | FOS_NOCHANGEDIR | FOS_OVERWRITEPROMPT) }
        .map_err(|error| Error::Terminal(format!("configure save-file picker: {error}")))?;
    unsafe { dialog.SetTitle(windows::core::w!("Save downloaded world")) }
        .map_err(|error| Error::Terminal(format!("title save-file picker: {error}")))?;

    let mut name = suggested_name.encode_utf16().collect::<Vec<_>>();
    name.push(0);
    unsafe { dialog.SetFileName(PCWSTR(name.as_ptr())) }
        .map_err(|error| Error::Terminal(format!("set suggested file name: {error}")))?;
    if initial.is_dir() {
        let mut wide = initial.as_os_str().encode_wide().collect::<Vec<_>>();
        wide.push(0);
        if let Ok(folder) = unsafe {
            SHCreateItemFromParsingName::<_, _, windows::Win32::UI::Shell::IShellItem>(
                PCWSTR(wide.as_ptr()),
                None,
            )
        } {
            let _ = unsafe { dialog.SetFolder(&folder) };
        }
    }
    if let Err(error) = unsafe { dialog.Show(None) } {
        if error.code() == HRESULT(0x8007_04c7_u32 as i32) {
            return Ok(None);
        }
        return Err(Error::Terminal(format!("show save-file picker: {error}")));
    }
    let selected = unsafe { dialog.GetResult() }
        .map_err(|error| Error::Terminal(format!("read selected file: {error}")))?;
    let path = TaskMemory(
        unsafe { selected.GetDisplayName(SIGDN_FILESYSPATH) }
            .map_err(|error| Error::Terminal(format!("read file path: {error}")))?,
    );
    let path = unsafe { path.0.to_string() }
        .map_err(|error| Error::Terminal(format!("decode file path: {error}")))?;
    Ok(Some(path.into()))
}

#[cfg(not(windows))]
pub fn pick_save_file(
    _initial_directory: &std::path::Path,
    _suggested_name: &str,
) -> Result<Option<std::path::PathBuf>> {
    Err(Error::Terminal(
        "native save-file picker is only supported on Windows".into(),
    ))
}

/// Pick an existing modpack archive (`.zip` or `.mrpack`).
#[cfg(windows)]
pub fn pick_modpack_file(
    initial_directory: &std::path::Path,
) -> Result<Option<std::path::PathBuf>> {
    let initial = initial_directory.to_path_buf();
    std::thread::spawn(move || pick_modpack_file_sta(&initial))
        .join()
        .map_err(|_| Error::Terminal("modpack picker thread panicked".into()))?
}

#[cfg(windows)]
fn pick_modpack_file_sta(initial: &std::path::Path) -> Result<Option<std::path::PathBuf>> {
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_PATHMUSTEXIST, FileOpenDialog,
        IFileOpenDialog, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    use windows::core::{HRESULT, PCWSTR};

    struct ComApartment;
    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    struct TaskMemory(windows::core::PWSTR);
    impl Drop for TaskMemory {
        fn drop(&mut self) {
            unsafe { CoTaskMemFree(Some(self.0.0.cast())) };
        }
    }

    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
        .ok()
        .map_err(|error| Error::Terminal(format!("initialize modpack picker: {error}")))?;
    let _apartment = ComApartment;
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| Error::Terminal(format!("create modpack picker: {error}")))?;
    unsafe {
        dialog.SetOptions(
            FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_FILEMUSTEXIST | FOS_NOCHANGEDIR,
        )
    }
    .map_err(|error| Error::Terminal(format!("configure modpack picker: {error}")))?;
    unsafe { dialog.SetTitle(windows::core::w!("Select a modpack archive")) }
        .map_err(|error| Error::Terminal(format!("title modpack picker: {error}")))?;

    // The filter strings must outlive the call that registers them.
    let label = "Modpack (*.zip;*.mrpack)";
    let pattern = "*.zip;*.mrpack";
    let mut label_wide = label
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut pattern_wide = pattern
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let filters = [COMDLG_FILTERSPEC {
        pszName: PCWSTR(label_wide.as_mut_ptr()),
        pszSpec: PCWSTR(pattern_wide.as_mut_ptr()),
    }];
    unsafe { dialog.SetFileTypes(&filters) }
        .map_err(|error| Error::Terminal(format!("filter modpack picker: {error}")))?;

    if initial.is_dir() {
        let mut wide = initial.as_os_str().encode_wide().collect::<Vec<_>>();
        wide.push(0);
        if let Ok(folder) = unsafe {
            SHCreateItemFromParsingName::<_, _, windows::Win32::UI::Shell::IShellItem>(
                PCWSTR(wide.as_ptr()),
                None,
            )
        } {
            let _ = unsafe { dialog.SetFolder(&folder) };
        }
    }
    if let Err(error) = unsafe { dialog.Show(None) } {
        if error.code() == HRESULT(0x8007_04c7_u32 as i32) {
            return Ok(None);
        }
        return Err(Error::Terminal(format!("show modpack picker: {error}")));
    }
    let selected = unsafe { dialog.GetResult() }
        .map_err(|error| Error::Terminal(format!("read selected modpack: {error}")))?;
    let path = TaskMemory(
        unsafe { selected.GetDisplayName(SIGDN_FILESYSPATH) }
            .map_err(|error| Error::Terminal(format!("read modpack path: {error}")))?,
    );
    let path = unsafe { path.0.to_string() }
        .map_err(|error| Error::Terminal(format!("decode modpack path: {error}")))?;
    Ok(Some(path.into()))
}

#[cfg(not(windows))]
pub fn pick_modpack_file(
    _initial_directory: &std::path::Path,
) -> Result<Option<std::path::PathBuf>> {
    Err(Error::Terminal(
        "native modpack picker is only supported on Windows".into(),
    ))
}

pub fn java_candidates() -> Vec<std::path::PathBuf> {
    #[cfg(windows)]
    {
        use std::path::PathBuf;

        let mut result = Vec::new();
        if let Ok(output) = std::process::Command::new("where").arg("java").output() {
            result.extend(
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(PathBuf::from),
            );
        }
        if let Some(java_home) = std::env::var_os("JAVA_HOME") {
            result.push(PathBuf::from(java_home));
        }
        for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(program_files) = std::env::var_os(variable) {
                let program_files = PathBuf::from(program_files);
                for vendor in [
                    "Java",
                    "Eclipse Adoptium",
                    "Microsoft",
                    "Amazon Corretto",
                    "Zulu",
                    "BellSoft",
                    "Oracle",
                ] {
                    result.extend(installed_java_homes(&program_files.join(vendor)));
                }
                result.push(program_files.join("Common Files/Oracle/Java/javapath"));
            }
        }
        result.extend(registry_java_candidates());
        result
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
fn registry_java_candidates() -> Vec<std::path::PathBuf> {
    let keys = [
        r"HKLM\SOFTWARE\JavaSoft",
        r"HKLM\SOFTWARE\Eclipse Adoptium",
        r"HKLM\SOFTWARE\Amazon Corretto",
        r"HKLM\SOFTWARE\Azul Systems",
        r"HKLM\SOFTWARE\Microsoft\JDK",
        r"HKLM\SOFTWARE\BellSoft",
        r"HKLM\SOFTWARE\WOW6432Node\JavaSoft",
        r"HKCU\SOFTWARE\JavaSoft",
    ];
    keys.into_iter()
        .filter_map(|key| {
            std::process::Command::new("reg.exe")
                .args(["query", key, "/s"])
                .output()
                .ok()
        })
        .flat_map(|output| parse_registry_java_homes(&String::from_utf8_lossy(&output.stdout)))
        .collect()
}

#[cfg(windows)]
fn parse_registry_java_homes(output: &str) -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;

    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let mut parts = trimmed.split_whitespace();
            let name = parts.next()?;
            let value_type = parts.next()?;
            if !name.eq_ignore_ascii_case("javahome") || !value_type.starts_with("REG_") {
                return None;
            }
            let value_start = trimmed.find(value_type)? + value_type.len();
            let value = trimmed[value_start..].trim();
            (!value.is_empty()).then(|| PathBuf::from(value))
        })
        .collect()
}

fn installed_java_homes(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut homes = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    homes.sort();
    homes
}

#[cfg(windows)]
pub fn protect(data: &[u8]) -> Result<Vec<u8>> {
    use std::slice;

    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    };
    use windows::core::PCWSTR;

    let cb_data = u32::try_from(data.len())
        .map_err(|_| Error::Account("secret is too large for DPAPI".into()))?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: cb_data,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    }
    .map_err(|error| Error::Account(format!("Windows DPAPI failed: {error}")))?;

    let result = unsafe { slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(Some(HLOCAL(output.pbData as _)));
    }
    Ok(result)
}

#[cfg(windows)]
pub fn unprotect(data: &[u8]) -> Result<Vec<u8>> {
    use std::slice;

    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
    };

    let cb_data = u32::try_from(data.len())
        .map_err(|_| Error::Account("secret is too large for DPAPI".into()))?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: cb_data,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    }
    .map_err(|error| Error::Account(format!("Windows DPAPI failed: {error}")))?;

    let result = unsafe { slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(Some(HLOCAL(output.pbData as _)));
    }
    Ok(result)
}

#[cfg(not(windows))]
pub fn protect(_data: &[u8]) -> Result<Vec<u8>> {
    Err(unsupported())
}

#[cfg(not(windows))]
pub fn unprotect(_data: &[u8]) -> Result<Vec<u8>> {
    Err(unsupported())
}

#[cfg(not(windows))]
fn unsupported() -> Error {
    Error::Account("DPAPI secret storage is only supported on Windows".into())
}

/// Local wall-clock time, broken into the fields a page header prints.
///
/// The `paper` preset prints a masthead date the way a newspaper does; the
/// clock is the OS's, so a daylight-saving change is already applied. `None`
/// means the platform cannot answer, and the caller leaves the field out rather
/// than inventing a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

impl LocalTime {
    /// `2026-11-02 13:37`: sortable, unambiguous, and the same in every locale.
    pub fn stamp(self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }
}

pub fn local_now() -> Option<LocalTime> {
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::GetLocalTime;

        // SAFETY: `GetLocalTime` takes no arguments and only writes into the
        // value it returns.
        let time = unsafe { GetLocalTime() };
        Some(LocalTime {
            year: time.wYear,
            month: time.wMonth as u8,
            day: time.wDay as u8,
            hour: time.wHour as u8,
            minute: time.wMinute as u8,
        })
    }
    #[cfg(not(windows))]
    {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs() as i64;
        let offset = std::env::var("TZ")
            .ok()
            .and_then(|value| parse_tz_offset_minutes(&value))
            .unwrap_or(0);
        let local = seconds + i64::from(offset) * 60;
        let (year, month, day) = civil_from_days(local.div_euclid(86_400));
        let clock = local.rem_euclid(86_400);
        Some(LocalTime {
            year: year as u16,
            month: month as u8,
            day: day as u8,
            hour: (clock / 3_600) as u8,
            minute: ((clock % 3_600) / 60) as u8,
        })
    }
}

/// `(year, month, day)` for a count of days since 1970-01-01, proleptic
/// Gregorian. Howard Hinnant's algorithm; only the non-Windows fallback needs it.
#[cfg(not(windows))]
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month as u32, day as u32)
}

/// Local wall-clock time as minutes since midnight.
///
/// The TUI only needs a clock for the `live` accent and the sidebar status
/// line, so this returns `None` rather than guessing when the platform cannot
/// tell us. Windows asks the OS, which already applies daylight saving; other
/// targets combine UTC with the fixed offset in `TZ`, enough for development
/// builds.
pub fn local_minutes_since_midnight() -> Option<u32> {
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::GetLocalTime;

        // SAFETY: `GetLocalTime` takes no arguments and only writes into the
        // value it returns.
        let time = unsafe { GetLocalTime() };
        Some(u32::from(time.wHour) * 60 + u32::from(time.wMinute))
    }
    #[cfg(not(windows))]
    {
        let since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        let utc_minutes = (since_epoch.as_secs() / 60) % 1440;
        let offset = std::env::var("TZ")
            .ok()
            .and_then(|value| parse_tz_offset_minutes(&value))?;
        Some((i64::from(utc_minutes) + i64::from(offset)).rem_euclid(1440) as u32)
    }
}

/// Parse a POSIX `TZ` fixed offset into "minutes to add to UTC".
///
/// POSIX negates the sign, so `CST-8` is eight hours *ahead* of UTC (China) and
/// `UTC+8` is eight hours behind it. Only the `std offset` form is supported;
/// DST rules and zone names that need a database are ignored.
pub fn parse_tz_offset_minutes(value: &str) -> Option<i32> {
    let trimmed = value.trim();
    let sign_at = trimmed.find(['+', '-'])?;
    let (name, offset) = trimmed.split_at(sign_at);
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let (sign, digits) = offset.split_at(1);
    let (hours, minutes) = match digits.split_once(':') {
        Some((hours, minutes)) => (hours, minutes),
        None => (digits, "0"),
    };
    if hours.is_empty() || hours.len() > 2 || minutes.is_empty() || minutes.len() > 2 {
        return None;
    }
    let hours: i32 = hours.parse().ok()?;
    let minutes: i32 = minutes.parse().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    let total = hours * 60 + minutes;
    Some(if sign == "-" { total } else { -total })
}

/// The current user's raw `Path` value, unexpanded.
///
/// `REG_EXPAND_SZ` entries such as `%USERPROFILE%\AppData\...` are returned
/// verbatim: expanding them would freeze one machine's paths into a value that
/// is rewritten for every future session.
#[cfg(windows)]
pub fn user_path_value() -> Result<String> {
    use std::ffi::c_void;

    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_ROUTINE_FLAGS, REG_SZ, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ,
        RRF_RT_REG_SZ, RegGetValueW,
    };
    use windows::core::w;

    let flags = REG_ROUTINE_FLAGS(RRF_RT_REG_SZ.0 | RRF_RT_REG_EXPAND_SZ.0 | RRF_NOEXPAND.0);
    let mut bytes: u32 = 0;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Environment"),
            w!("Path"),
            flags,
            None,
            None,
            Some(&mut bytes),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        // A user with no Path of their own; the machine value still applies.
        return Ok(String::new());
    }
    if status != ERROR_SUCCESS {
        return Err(Error::Config(format!(
            "read the user Path value: {status:?}"
        )));
    }
    let mut buffer = vec![0u16; (bytes as usize / 2) + 1];
    let mut kind = REG_SZ;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Environment"),
            w!("Path"),
            flags,
            Some(&mut kind),
            Some(buffer.as_mut_ptr().cast::<c_void>()),
            Some(&mut bytes),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(Error::Config(format!(
            "read the user Path value: {status:?}"
        )));
    }
    let length = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Ok(String::from_utf16_lossy(&buffer[..length]))
}

#[cfg(not(windows))]
pub fn user_path_value() -> Result<String> {
    Err(Error::Config(
        "editing PATH is only implemented on Windows".into(),
    ))
}

/// Replaces the current user's `Path`, keeping it a `REG_EXPAND_SZ` value.
#[cfg(windows)]
pub fn set_user_path_value(value: &str) -> Result<()> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_EXPAND_SZ, RegCloseKey, RegOpenKeyExW,
        RegSetValueExW,
    };
    use windows::core::w;

    let mut key = HKEY::default();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Environment"),
            Some(0),
            KEY_SET_VALUE,
            &mut key,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(Error::Config(format!(
            "open the user environment key: {status:?}"
        )));
    }
    let wide = value
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let bytes = wide
        .iter()
        .flat_map(|unit| unit.to_le_bytes())
        .collect::<Vec<u8>>();
    let status = unsafe { RegSetValueExW(key, w!("Path"), None, REG_EXPAND_SZ, Some(&bytes)) };
    unsafe {
        let _ = RegCloseKey(key);
    }
    if status != ERROR_SUCCESS {
        return Err(Error::Config(format!(
            "write the user Path value: {status:?}"
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set_user_path_value(_value: &str) -> Result<()> {
    Err(Error::Config(
        "editing PATH is only implemented on Windows".into(),
    ))
}

/// Tells Explorer and anything else listening that the environment changed.
///
/// Without it a window opened from the Start menu keeps the environment block
/// it inherited at login, which is exactly the "I added it but it still says
/// command not found" report.
#[cfg(windows)]
pub fn broadcast_environment_change() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };

    let name = "Environment\0".encode_utf16().collect::<Vec<_>>();
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(name.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            2000,
            None,
        );
    }
}

#[cfg(not(windows))]
pub fn broadcast_environment_change() {}

/// Throws away input that arrived before the UI was ready to read it.
///
/// The console keeps input records typed while the process is still starting, such
/// as an extra Enter after the command that opened the launcher. Once the TUI
/// switches to raw mode those records are read as ordinary keys, and the first
/// Enter can activate whatever is under the cursor: on the overview that means
/// starting a game the user did not ask for. Nothing has been drawn yet when this
/// runs, so the keys cannot have been aimed at anything.
#[cfg(windows)]
pub fn discard_pending_console_input() {
    use windows::Win32::System::Console::{
        FlushConsoleInputBuffer, GetStdHandle, STD_INPUT_HANDLE,
    };

    unsafe {
        if let Ok(handle) = GetStdHandle(STD_INPUT_HANDLE) {
            let _ = FlushConsoleInputBuffer(handle);
        }
    }
}

#[cfg(not(windows))]
pub fn discard_pending_console_input() {}

#[cfg(test)]
mod tests {
    use super::installed_java_homes;

    #[test]
    fn the_masthead_stamp_is_zero_padded_and_sortable() {
        let time = super::LocalTime {
            year: 2026,
            month: 3,
            day: 7,
            hour: 9,
            minute: 5,
        };
        assert_eq!(time.stamp(), "2026-03-07 09:05");
    }

    #[test]
    fn the_local_clock_answers_with_plausible_fields() {
        let now = super::local_now().expect("the platform clock should answer");
        assert!((1970..=2200).contains(&now.year));
        assert!((1..=12).contains(&now.month));
        assert!((1..=31).contains(&now.day));
        assert!(now.hour < 24);
        assert!(now.minute < 60);
    }

    #[test]
    fn enumerates_jdks_installed_under_a_vendor_root() {
        let root = std::env::temp_dir().join(format!("tmcl-java-homes-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("jdk-17")).expect("create Java 17 home");
        std::fs::create_dir_all(root.join("jdk-21.0.10")).expect("create Java 21 home");
        std::fs::write(root.join("readme.txt"), "not a runtime").expect("write file");

        let homes = installed_java_homes(&root);

        assert_eq!(homes.len(), 2);
        assert!(homes.iter().any(|home| home.ends_with("jdk-17")));
        assert!(homes.iter().any(|home| home.ends_with("jdk-21.0.10")));
        std::fs::remove_dir_all(root).expect("clean test directories");
    }

    #[cfg(windows)]
    #[test]
    fn parses_java_home_registry_values_without_truncating_spaces() {
        let homes = super::parse_registry_java_homes(
            "HKEY_LOCAL_MACHINE\\SOFTWARE\\JavaSoft\\JDK\\21\n    JavaHome    REG_SZ    C:\\Program Files\\Java\\jdk-21\n    RuntimeLib    REG_SZ    ignored.dll\n",
        );
        assert_eq!(
            homes,
            vec![std::path::PathBuf::from(r"C:\Program Files\Java\jdk-21")]
        );
    }

    #[cfg(windows)]
    #[test]
    fn local_time_lands_inside_a_day() {
        let minutes = super::local_minutes_since_midnight().expect("GetLocalTime should answer");
        assert!(minutes < 1440, "minutes since midnight must wrap daily");
    }

    #[test]
    fn posix_offsets_are_negated_and_minutes_are_kept() {
        // POSIX: the sign is what you add to local time to reach UTC, so
        // `CST-8` is UTC+8 and `UTC+8` is UTC-8.
        assert_eq!(super::parse_tz_offset_minutes("CST-8"), Some(480));
        assert_eq!(super::parse_tz_offset_minutes("UTC+8"), Some(-480));
        assert_eq!(super::parse_tz_offset_minutes("IST-5:30"), Some(330));
        assert_eq!(super::parse_tz_offset_minutes("UTC+0"), Some(0));
    }

    #[test]
    fn malformed_offsets_are_rejected() {
        for value in ["", "UTC", "UTC+", "UTC+25", "UTC+8:70", "8", "+8", "UTC+8x"] {
            assert_eq!(
                super::parse_tz_offset_minutes(value),
                None,
                "{value:?} should not parse"
            );
        }
    }
}
