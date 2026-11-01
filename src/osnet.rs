#[cfg(not(target_os = "wasi"))]
use std::io::{Read, Write};
#[cfg(not(target_os = "wasi"))]
use std::net::{TcpListener, TcpStream};
#[cfg(not(target_os = "wasi"))]
use std::time::Duration;

#[cfg(target_os = "wasi")]
#[link(wasm_import_module = "hostnet")]
extern "C" {
    fn host_listen(port: u32) -> i32;
    fn host_accept(fd: i32) -> i32;
    fn host_read(fd: i32, ptr: *mut u8, len: u32) -> i32;
    fn host_write(fd: i32, ptr: *const u8, len: u32) -> i32;
    fn host_close(fd: i32);
}

pub struct Listener {
    #[cfg(not(target_os = "wasi"))]
    inner: TcpListener,
    #[cfg(target_os = "wasi")]
    fd: i32,
}

pub struct Conn {
    #[cfg(not(target_os = "wasi"))]
    inner: TcpStream,
    #[cfg(target_os = "wasi")]
    fd: i32,
}

impl Listener {
    pub fn bind(port: u16) -> Result<Self, String> {
        #[cfg(not(target_os = "wasi"))]
        {
            let listener = TcpListener::bind(("0.0.0.0", port))
                .map_err(|e| format!("não consegui abrir a porta {port}: {e}"))?;
            Ok(Self { inner: listener })
        }
        #[cfg(target_os = "wasi")]
        {
            let fd = unsafe { host_listen(port as u32) };
            if fd < 0 {
                return Err(format!("não consegui abrir a porta {port}"));
            }
            Ok(Self { fd })
        }
    }

    pub fn accept(&self) -> Result<Conn, String> {
        #[cfg(not(target_os = "wasi"))]
        {
            let (stream, _) = self.inner.accept().map_err(|e| e.to_string())?;
            let _ = stream.set_read_timeout(Some(Duration::from_secs(120)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(120)));
            Ok(Conn { inner: stream })
        }
        #[cfg(target_os = "wasi")]
        {
            let fd = unsafe { host_accept(self.fd) };
            if fd < 0 {
                return Err("falha ao aceitar conexão".into());
            }
            Ok(Conn { fd })
        }
    }
}

impl Conn {
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        #[cfg(not(target_os = "wasi"))]
        {
            self.inner.read(buf).map_err(|e| e.to_string())
        }
        #[cfg(target_os = "wasi")]
        {
            if buf.is_empty() {
                return Ok(0);
            }
            let n = unsafe { host_read(self.fd, buf.as_mut_ptr(), buf.len() as u32) };
            if n < 0 {
                return Err("falha de leitura".into());
            }
            Ok(n as usize)
        }
    }

    pub fn write_all(&mut self, mut data: &[u8]) -> Result<(), String> {
        while !data.is_empty() {
            #[cfg(not(target_os = "wasi"))]
            {
                let n = self.inner.write(data).map_err(|e| e.to_string())?;
                if n == 0 {
                    return Err("conexão fechou no meio da resposta".into());
                }
                data = &data[n..];
            }
            #[cfg(target_os = "wasi")]
            {
                let n = unsafe { host_write(self.fd, data.as_ptr(), data.len() as u32) };
                if n <= 0 {
                    return Err("falha de escrita".into());
                }
                data = &data[n as usize..];
            }
        }
        Ok(())
    }

    pub fn close(&mut self) {
        #[cfg(not(target_os = "wasi"))]
        {
            let _ = self.inner.shutdown(std::net::Shutdown::Both);
        }
        #[cfg(target_os = "wasi")]
        unsafe {
            host_close(self.fd);
        }
    }
}
