#!/usr/bin/env python3
"""Run a wasm32-wasip1 program and give it a real TCP listener on 0.0.0.0.

Imports expected by the guest, module `hostnet`:
  host_listen(port) -> fd
  host_accept(fd) -> fd
  host_read(fd, ptr, len) -> n
  host_write(fd, ptr, len) -> n
  host_close(fd)
"""
import socket
import sys
from wasmtime import Config, Engine, FuncType, Linker, Module, Store, ValType, WasiConfig

HOST = "0.0.0.0"


class Net:
    def __init__(self):
        self.socks = {}
        self.next_fd = 3

    def listen(self, port: int) -> int:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        sock.bind((HOST, int(port)))
        sock.listen(32)
        fd = self.next_fd
        self.next_fd += 1
        self.socks[fd] = sock
        print(f"listening on {HOST}:{port}", flush=True)
        return fd

    def accept(self, fd: int) -> int:
        sock = self.socks.get(fd)
        if sock is None:
            return -1
        conn, addr = sock.accept()
        conn.settimeout(120)
        cfd = self.next_fd
        self.next_fd += 1
        self.socks[cfd] = conn
        print(f"accepted {addr[0]}:{addr[1]}", flush=True)
        return cfd

    def read(self, caller, fd: int, ptr: int, length: int) -> int:
        sock = self.socks.get(fd)
        if sock is None or length < 0 or ptr < 0:
            return -1
        try:
            data = sock.recv(length)
        except socket.timeout:
            return -1
        except Exception as exc:
            print(f"read: {exc}", flush=True)
            return -1
        if not data:
            return 0
        memory = caller["memory"]
        memory.write(caller, data, ptr)
        return len(data)

    def write(self, caller, fd: int, ptr: int, length: int) -> int:
        sock = self.socks.get(fd)
        if sock is None or length < 0 or ptr < 0:
            return -1
        memory = caller["memory"]
        data = bytes(memory.read(caller, ptr, ptr + length))
        try:
            return sock.send(data)
        except Exception as exc:
            print(f"write: {exc}", flush=True)
            return -1

    def close(self, fd: int) -> None:
        sock = self.socks.pop(int(fd), None)
        if sock is not None:
            try:
                sock.close()
            except Exception:
                pass


def main() -> None:
    if len(sys.argv) < 2:
        print("uso: host.py programa.wasm [args...]", file=sys.stderr)
        sys.exit(2)
    wasm = sys.argv[1]
    argv = ["convertetudo", *sys.argv[2:]]
    net = Net()

    config = Config()
    config.wasm_bulk_memory = True
    config.wasm_reference_types = True
    config.wasm_simd = True
    config.wasm_multi_value = True
    engine = Engine(config)
    module = Module.from_file(engine, wasm)
    linker = Linker(engine)
    linker.define_wasi()

    i32 = ValType.i32()
    linker.define_func("hostnet", "host_listen", FuncType([i32], [i32]), net.listen)
    linker.define_func("hostnet", "host_accept", FuncType([i32], [i32]), net.accept)
    linker.define_func(
        "hostnet", "host_read", FuncType([i32, i32, i32], [i32]), net.read, access_caller=True
    )
    linker.define_func(
        "hostnet", "host_write", FuncType([i32, i32, i32], [i32]), net.write, access_caller=True
    )
    linker.define_func("hostnet", "host_close", FuncType([i32], []), net.close)
    linker.define_unknown_imports_as_traps(module)

    store = Store(engine)
    wasi = WasiConfig()
    wasi.argv = argv
    wasi.env = [("RUST_BACKTRACE", "1")]
    wasi.inherit_stdout()
    wasi.inherit_stderr()
    wasi.preopen_dir("/tmp", "/tmp")
    wasi.preopen_dir("/home/user", "/home/user")
    store.set_wasi(wasi)

    inst = linker.instantiate(store, module)
    inst.exports(store)["_start"](store)


if __name__ == "__main__":
    main()
