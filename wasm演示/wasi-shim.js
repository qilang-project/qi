// 浏览器里跑 qi 编出来的 wasm32-wasip1 模块所需的最小 WASI 实现。
//
// 覆盖面按「qi 运行时实际会 import 什么」来定，不追求完整 WASI：
//   - 输出：fd_write（1=stdout 2=stderr，其余 fd 报 EBADF）
//   - 进程：proc_exit / args_* / environ_*（都是空）
//   - 时钟：clock_time_get（realtime 走 Date.now，monotonic 走 performance.now）
//   - 随机：random_get（crypto.getRandomValues）—— 标准库.随机 / uuid 用
//   - 睡眠：poll_oneoff 只处理 clock 订阅，用忙等实现（浏览器主线程没有同步 sleep）
//   - 文件系统：**没有**。fd_prestat_get 对 fd>=3 回 EBADF，让 wasi-libc 知道没有预打开
//     目录；path_open 回 ENOENT。读写文件的 qi 程序在浏览器里会拿到「文件不存在」，
//     跟 wasmtime 不给 --dir 时的行为一致。
//
// 面向用户的文本用中文；标识符一律英文（见仓库 CLAUDE.md 的「中文只写在 qi 里」）。

const ERRNO_SUCCESS = 0;
const ERRNO_BADF = 8;
const ERRNO_NOENT = 44;
const ERRNO_NOSYS = 52;
const ERRNO_NOTSUP = 58;

const CLOCK_REALTIME = 0;
const CLOCK_MONOTONIC = 1;

const EVENTTYPE_CLOCK = 0;

/**
 * 造一套 WASI 导入。
 * @param {() => WebAssembly.Memory} getMemory  实例化之后才有 memory，所以延迟取
 * @param {(text: string, fd: number) => void} onWrite  stdout/stderr 文本回调
 * @param {(code: number) => void} [onExit]  proc_exit 回调（默认抛异常终止执行）
 */
export function makeWasi(getMemory, onWrite, onExit) {
  const decoder = new TextDecoder('utf-8');
  const view = () => new DataView(getMemory().buffer);
  const bytes = () => new Uint8Array(getMemory().buffer);

  // 每个 fd 各自攒半行：一次 fd_write 可能只送来半个 UTF-8 字符或半行，
  // 攒到换行再交出去，页面上才不会出现被切开的汉字
  const pending = { 1: [], 2: [] };
  function flushFd(fd, force) {
    const chunks = pending[fd];
    if (!chunks || chunks.length === 0) return;
    const total = chunks.reduce((n, c) => n + c.length, 0);
    const flat = new Uint8Array(total);
    let off = 0;
    for (const c of chunks) { flat.set(c, off); off += c.length; }
    let cut = flat.length;
    if (!force) {
      cut = flat.lastIndexOf(10) + 1;   // 最后一个换行之后的留到下次
      if (cut === 0) return;
    }
    onWrite(decoder.decode(flat.subarray(0, cut)), fd);
    pending[fd] = cut < flat.length ? [flat.slice(cut)] : [];
  }

  class ExitSignal extends Error {
    constructor(code) { super(`进程退出，代码 ${code}`); this.code = code; }
  }

  const wasi = {
    fd_write(fd, iovsPtr, iovsLen, nwrittenPtr) {
      if (fd !== 1 && fd !== 2) return ERRNO_BADF;
      const dv = view();
      const mem = bytes();
      let written = 0;
      for (let i = 0; i < iovsLen; i++) {
        const base = dv.getUint32(iovsPtr + i * 8, true);
        const len = dv.getUint32(iovsPtr + i * 8 + 4, true);
        // 必须 slice 拷一份：memory.grow 之后旧的 ArrayBuffer 会 detach
        pending[fd].push(mem.slice(base, base + len));
        written += len;
      }
      dv.setUint32(nwrittenPtr, written, true);
      flushFd(fd, false);
      return ERRNO_SUCCESS;
    },
    fd_read(fd, iovsPtr, iovsLen, nreadPtr) {
      // 没有 stdin：读到 EOF
      view().setUint32(nreadPtr, 0, true);
      return fd === 0 ? ERRNO_SUCCESS : ERRNO_BADF;
    },
    fd_close() { return ERRNO_SUCCESS; },
    fd_seek(fd, offset, whence, newOffsetPtr) {
      view().setBigUint64(newOffsetPtr, 0n, true);
      return ERRNO_BADF;
    },
    fd_fdstat_get(fd, statPtr) {
      // 只有 0/1/2 是合法 fd：filetype=character device(2)，flags=0，rights 全开
      if (fd > 2) return ERRNO_BADF;
      const dv = view();
      dv.setUint8(statPtr, 2);
      dv.setUint16(statPtr + 2, 0, true);
      dv.setBigUint64(statPtr + 8, 0xffffffffffffffffn, true);
      dv.setBigUint64(statPtr + 16, 0xffffffffffffffffn, true);
      return ERRNO_SUCCESS;
    },
    fd_fdstat_set_flags() { return ERRNO_SUCCESS; },
    fd_prestat_get() { return ERRNO_BADF; },          // 没有预打开目录
    fd_prestat_dir_name() { return ERRNO_BADF; },
    fd_filestat_get() { return ERRNO_BADF; },
    fd_readdir() { return ERRNO_BADF; },
    path_open() { return ERRNO_NOENT; },
    path_filestat_get() { return ERRNO_NOENT; },
    path_unlink_file() { return ERRNO_NOENT; },
    path_remove_directory() { return ERRNO_NOENT; },
    path_create_directory() { return ERRNO_NOTSUP; },
    path_rename() { return ERRNO_NOENT; },
    path_readlink() { return ERRNO_NOENT; },
    path_symlink() { return ERRNO_NOTSUP; },
    path_link() { return ERRNO_NOTSUP; },

    environ_sizes_get(countPtr, bufSizePtr) {
      const dv = view();
      dv.setUint32(countPtr, 0, true);
      dv.setUint32(bufSizePtr, 0, true);
      return ERRNO_SUCCESS;
    },
    environ_get() { return ERRNO_SUCCESS; },
    args_sizes_get(countPtr, bufSizePtr) {
      const dv = view();
      dv.setUint32(countPtr, 0, true);
      dv.setUint32(bufSizePtr, 0, true);
      return ERRNO_SUCCESS;
    },
    args_get() { return ERRNO_SUCCESS; },

    clock_time_get(id, precision, timePtr) {
      let ns;
      if (id === CLOCK_REALTIME) {
        ns = BigInt(Date.now()) * 1000000n;
      } else if (id === CLOCK_MONOTONIC) {
        ns = BigInt(Math.round(performance.now() * 1e6));
      } else {
        return ERRNO_NOSYS;
      }
      view().setBigUint64(timePtr, ns, true);
      return ERRNO_SUCCESS;
    },
    clock_res_get(id, resPtr) {
      view().setBigUint64(resPtr, 1000000n, true);
      return ERRNO_SUCCESS;
    },
    random_get(bufPtr, bufLen) {
      // getRandomValues 单次最多 65536 字节
      const mem = bytes();
      for (let off = 0; off < bufLen; off += 65536) {
        crypto.getRandomValues(mem.subarray(bufPtr + off, bufPtr + Math.min(bufLen, off + 65536)));
      }
      return ERRNO_SUCCESS;
    },
    sched_yield() { return ERRNO_SUCCESS; },
    poll_oneoff(inPtr, outPtr, nsubs, neventsPtr) {
      // 只支持 clock 订阅（thread::sleep 走这里）。主线程没有同步睡眠，忙等到点。
      const dv = view();
      let fired = 0;
      for (let i = 0; i < nsubs; i++) {
        const sub = inPtr + i * 48;
        const userdata = dv.getBigUint64(sub, true);
        const tag = dv.getUint8(sub + 8);
        if (tag === EVENTTYPE_CLOCK) {
          const timeoutNs = dv.getBigUint64(sub + 24, true);
          const flags = dv.getUint16(sub + 40, true);
          const ms = Number(timeoutNs / 1000000n);
          const deadline = performance.now() + (flags & 1 ? 0 : ms);
          while (performance.now() < deadline) { /* 忙等 */ }
        }
        const ev = outPtr + fired * 32;
        dv.setBigUint64(ev, userdata, true);
        dv.setUint16(ev + 8, ERRNO_SUCCESS, true);
        dv.setUint8(ev + 10, tag);
        fired++;
      }
      dv.setUint32(neventsPtr, fired, true);
      return ERRNO_SUCCESS;
    },
    proc_exit(code) {
      flushFd(1, true);
      flushFd(2, true);
      if (onExit) onExit(code);
      throw new ExitSignal(code);
    },
    proc_raise() { return ERRNO_NOSYS; },
  };

  return {
    imports: { wasi_snapshot_preview1: wasi },
    /** 程序正常从 _start 返回时调用，把没带换行的尾巴吐出来 */
    flush() { flushFd(1, true); flushFd(2, true); },
    ExitSignal,
  };
}

/**
 * 一步到位：取 wasm 字节、实例化、跑 _start，把输出交给回调。
 * 返回退出码（0 = 正常）。
 */
export async function runQiWasm(wasmBytes, onWrite) {
  let instance;
  let exitCode = 0;
  const wasi = makeWasi(() => instance.exports.memory, onWrite, (code) => { exitCode = code; });
  const result = await WebAssembly.instantiate(wasmBytes, wasi.imports);
  instance = result.instance;
  try {
    instance.exports._start();
    wasi.flush();
  } catch (err) {
    if (err instanceof wasi.ExitSignal) {
      // proc_exit 走异常出来是正常收尾
    } else {
      wasi.flush();
      throw err;
    }
  }
  return exitCode;
}
