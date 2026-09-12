use crate::protocol::MAX_FRAME;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::io;

pub const DATA_DIR: &str = "/data/adb/justlocation";

pub fn decode_request(encoded: &str) -> Result<String, String> {
    if encoded.len() > (MAX_FRAME as usize).div_ceil(3) * 4 {
        return Err("request exceeds 64 KiB".into());
    }
    let bytes = STANDARD.decode(encoded).map_err(|e| e.to_string())?;
    if bytes.len() >= MAX_FRAME as usize {
        return Err("request exceeds 64 KiB".into());
    }
    let request = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    if request.contains(['\n', '\r']) {
        return Err("only one request is allowed".into());
    }
    Ok(request)
}

#[cfg(unix)]
mod platform {
    use super::*;
    use crate::protocol::Control;
    use std::fs::{self, File, Permissions};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    };
    use std::path::Path;
    use std::time::Duration;

    #[cfg(target_os = "android")]
    fn acquire_lock(lock: &File) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        // This toolchain's std::fs file locks return Unsupported on Android.
        // SAFETY: the borrowed File keeps its descriptor open throughout flock.
        let result = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
    }

    #[cfg(not(target_os = "android"))]
    fn acquire_lock(lock: &File) -> io::Result<()> {
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::Error(error) => error,
            std::fs::TryLockError::WouldBlock => {
                io::Error::new(io::ErrorKind::WouldBlock, "another JustLocation service is running")
            }
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn service_lock_excludes_second_owner_and_releases_on_close() {
            let path =
                std::env::temp_dir().join(format!("justlocation-lock-{}", std::process::id()));
            let first = File::options().create_new(true).write(true).open(&path).unwrap();
            let second = File::options().write(true).open(&path).unwrap();
            let result = acquire_lock(&first);
            if let Err(error) = result {
                fs::remove_file(&path).unwrap();
                panic!("first service must acquire the lock: {error:?}");
            }
            assert_eq!(acquire_lock(&second).unwrap_err().kind(), io::ErrorKind::WouldBlock);
            drop(first);
            acquire_lock(&second).unwrap();
            drop(second);
            fs::remove_file(path).unwrap();
        }
    }

    pub fn serve(directory: &Path) -> io::Result<()> {
        fs::create_dir_all(directory)?;
        fs::set_permissions(directory, Permissions::from_mode(0o700))?;
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(directory.join("service.lock"))?;
        acquire_lock(&lock)?;
        let mut control = Control::open(directory.join("config.json"))?;
        let path = directory.join("control.sock");
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let listener = UnixListener::bind(&path)?;
        fs::set_permissions(&path, Permissions::from_mode(0o600))?;
        for stream in listener.incoming() {
            let stream = match stream {
                Ok(stream) => stream,
                Err(e) => {
                    eprintln!("accept: {e}");
                    continue;
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            stream.set_write_timeout(Some(Duration::from_secs(2)))?;
            // One frame per connection: a client cannot retain the single state owner's loop.
            let mut reader = BufReader::new(&stream);
            let mut line = String::new();
            match Read::take(&mut reader, MAX_FRAME + 1).read_line(&mut line) {
                Ok(0) => {}
                Ok(_) => {
                    if let Err(e) = control.serve(io::Cursor::new(line), &stream) {
                        eprintln!("request: {e}");
                    }
                    if control.is_shutdown() {
                        break;
                    }
                }
                Err(e) => eprintln!("read: {e}"),
            }
        }
        fs::remove_file(path)?;
        Ok(())
    }

    pub fn request(directory: &Path, request: &str) -> io::Result<String> {
        let mut stream = UnixStream::connect(directory.join("control.sock"))?;
        stream.set_read_timeout(Some(Duration::from_secs(4)))?;
        stream.set_write_timeout(Some(Duration::from_secs(4)))?;
        writeln!(stream, "{request}")?;
        let mut response = String::new();
        BufReader::new(stream).take(MAX_FRAME * 2).read_line(&mut response)?;
        if response.is_empty() || !response.ends_with('\n') {
            return Err(io::Error::other("incomplete response"));
        }
        Ok(response)
    }
}

pub fn serve() -> io::Result<()> {
    #[cfg(unix)]
    {
        platform::serve(std::path::Path::new(DATA_DIR))
    }
    #[cfg(not(unix))]
    {
        Err(io::Error::other("Unix sockets require Android/Linux; use stdio for host testing"))
    }
}

pub fn request(encoded: &str) -> Result<String, String> {
    let decoded = decode_request(encoded)?;
    #[cfg(unix)]
    {
        platform::request(std::path::Path::new(DATA_DIR), &decoded).map_err(|e| e.to_string())
    }
    #[cfg(not(unix))]
    {
        let _ = decoded;
        Err("Unix sockets require Android/Linux; use stdio for host testing".into())
    }
}
