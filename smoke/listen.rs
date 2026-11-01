//! Tiny wasm32-wasip1 probe: bind 0.0.0.0 via hostnet and answer HTTP.
#[link(wasm_import_module = "hostnet")]
extern "C" {
    fn host_listen(port: u32) -> i32;
    fn host_accept(fd: i32) -> i32;
    fn host_read(fd: i32, ptr: *mut u8, len: u32) -> i32;
    fn host_write(fd: i32, ptr: *const u8, len: u32) -> i32;
    fn host_close(fd: i32);
}

fn main() {
    unsafe {
        let listener = host_listen(4173);
        if listener < 0 {
            eprintln!("listen failed");
            return;
        }
        eprintln!("probe listening");
        loop {
            let conn = host_accept(listener);
            if conn < 0 {
                continue;
            }
            let mut buf = [0u8; 512];
            let _ = host_read(conn, buf.as_mut_ptr(), buf.len() as u32);
            let body = b"ok";
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = host_write(conn, head.as_ptr(), head.len() as u32);
            let _ = host_write(conn, body.as_ptr(), body.len() as u32);
            host_close(conn);
        }
    }
}
