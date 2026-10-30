//! Durable replacement of saved world files.
use std::io::{self, Write};
use std::path::Path;

/// Write and flush a sibling temporary file, then replace the destination.
pub fn save(path: &Path, data: &[u8]) -> Result<(), String> {
    let name = path.file_name().ok_or("invalid save path")?;
    for attempt in 0..1000u32 {
        let mut temp_name = name.to_os_string();
        temp_name.push(format!(".{}.{}.tmp", std::process::id(), attempt));
        let temp = path.with_file_name(temp_name);
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        };
        let result = file.write_all(data).and_then(|_| file.sync_all());
        drop(file);
        let result = result.and_then(|_| replace(&temp, path));
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        return result.map_err(|error| error.to_string());
    }
    Err("unable to reserve temporary save file".into())
}
#[cfg(not(windows))]
fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    std::fs::rename(source, destination)
}
#[cfg(windows)]
fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: Both NUL-terminated buffers stay alive for the duration of the OS call.
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
