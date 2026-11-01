#!/usr/bin/env python3
"""Drive the persisted wasm32 rustc.

A real compile (not --version) imports wasi::thread-spawn and calls it
during startup. Trapping that import aborts rustc. This host spawns an OS
thread, instantiates the same module against the shared memory, and calls
the guest export wasi_thread_start(tid, arg).
"""
import sys
import threading
from wasmtime import (
    Config,
    Engine,
    FuncType,
    Linker,
    Limits,
    MemoryType,
    Module,
    SharedMemory,
    Store,
    ValType,
    WasiConfig,
)

ROOT = "/home/user/convertetudo"
RUSTC = ROOT + "/.toolchain/rustc/bin/rustc.wasm"
SYSROOT = ROOT + "/.toolchain/rustc"


def wasi_config() -> WasiConfig:
    wasi = WasiConfig()
    wasi.argv = sys.argv[1:] or ["rustc", "--version"]
    wasi.env = [
        ("RUST_MIN_STACK", "16777216"),
        ("RUST_BACKTRACE", "1"),
        ("RAYON_NUM_THREADS", "1"),
        ("TMPDIR", "/tmp"),
        ("HOME", "/tmp"),
        ("PATH", "/tmp"),
        ("PWD", ROOT),
        ("RUSTC_SYSROOT", SYSROOT),
    ]
    wasi.inherit_stdout()
    wasi.inherit_stderr()
    wasi.preopen_dir(ROOT, ".")
    wasi.preopen_dir("/tmp", "/tmp")
    wasi.preopen_dir("/home/user", "/home/user")
    wasi.preopen_dir("/", "/")
    return wasi


class ThreadHost:
    def __init__(self, engine: Engine, module: Module, memory: SharedMemory):
        self.engine = engine
        self.module = module
        self.memory = memory
        self.lock = threading.Lock()
        self.next_tid = 1
        self.workers = []

    def spawn(self, _caller, start_arg: int) -> int:
        with self.lock:
            tid = self.next_tid
            self.next_tid += 1
        print(f"thread-spawn tid={tid} arg={start_arg}", flush=True)
        worker = threading.Thread(target=self._run, args=(tid, start_arg), name=f"wasm-{tid}", daemon=True)
        worker.start()
        with self.lock:
            self.workers.append(worker)
        return tid

    def _run(self, tid: int, start_arg: int) -> None:
        try:
            store = Store(self.engine)
            store.set_wasi(wasi_config())
            linker = self._linker(store)
            instance = linker.instantiate(store, self.module)
            start = instance.exports(store)["wasi_thread_start"]
            start(store, tid, start_arg)
            print(f"thread {tid} returned", flush=True)
        except Exception as exc:
            print(f"thread {tid} failed: {exc}", flush=True)

    def _linker(self, store: Store) -> Linker:
        linker = Linker(self.engine)
        linker.define_wasi()
        linker.define(store, "env", "memory", self.memory)
        linker.define_func(
            "wasi",
            "thread-spawn",
            FuncType([ValType.i32()], [ValType.i32()]),
            self.spawn,
            access_caller=True,
        )
        linker.define_unknown_imports_as_traps(self.module)
        return linker

    def join(self) -> None:
        with self.lock:
            workers = list(self.workers)
        for worker in workers:
            worker.join(timeout=5)


def main() -> None:
    config = Config()
    config.wasm_threads = True
    config.wasm_shared_memory = True
    config.shared_memory = True
    config.wasm_simd = True
    config.wasm_bulk_memory = True
    config.wasm_reference_types = True
    config.wasm_multi_value = True
    try:
        config.wasm_relaxed_simd = True
    except Exception as exc:
        print("no relaxed simd", exc, flush=True)

    engine = Engine(config)
    print("loading rustc", flush=True)
    module = Module.from_file(engine, RUSTC)
    print("module ok", flush=True)

    limits = Limits(3072, 65536)
    memory = SharedMemory(engine, MemoryType(limits, shared=True))
    threads = ThreadHost(engine, module, memory)

    store = Store(engine)
    store.set_wasi(wasi_config())
    linker = threads._linker(store)
    print("argv", sys.argv[1:], flush=True)
    instance = linker.instantiate(store, module)
    instance.exports(store)["_start"](store)
    threads.join()
    print("rustc finished", flush=True)


if __name__ == "__main__":
    main()
