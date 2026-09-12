// 在 node 里模拟 Worker 环境（提供同步 XMLHttpRequest），端到端验 wasm 的 HTTP。
// 用真的本地 HTTP 服务器，不是打桩的响应。
//
// 注意：本文件是 JS，标识符一律英文（见仓库 CLAUDE.md）。中文只留给面向人的文本。
//
// 服务器必须跑在**另一个进程**里。第一版把它开在本进程，然后用 execFileSync(curl)
// 去同步取 —— execFileSync 把事件循环一整个堵住，同一个进程里的 http 服务器
// 永远没机会应答，curl 就永远等着：死锁，一个字节都不打印。
//
// 用法：node 试_http.mjs <程序.wasm | 程序.qi> [端口]
//   给 .qi 就用原生编译器跑（QI 环境变量指到 qi 二进制），服务器同样由本脚本起 ——
//   这样 tests/wasm/http断言.sh 能拿同一台服务器给原生和 wasm 各跑一遍再对拍。
import { spawn, execFileSync } from 'node:child_process';
import { readFileSync, unlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { runQiWasm } from './wasi-shim.js';

const PORT = Number(process.argv[3] || 41573);

const SERVER_SRC = `
  const http = require('node:http');
  const server = http.createServer((req, res) => {
    let body = '';
    req.on('data', (c) => (body += c));
    req.on('end', () => {
      res.writeHead(req.url === '/big' ? 200 : 201, { 'Content-Type': 'text/plain; charset=utf-8' });
      // /big 回一个超过默认 64KB 缓冲的响应，逼出 wasm 侧的扩容重试那条路
      if (req.url === '/big') res.end('长'.repeat(50000));
      else res.end('方法=' + req.method + ' 路径=' + decodeURIComponent(req.url) + ' 收到=' + body);
    });
  });
  server.listen(${PORT}, '127.0.0.1', () => process.stdout.write('READY\\n'));
`;

const server = spawn(process.execPath, ['-e', SERVER_SRC], { stdio: ['ignore', 'pipe', 'inherit'] });
await new Promise((resolve, reject) => {
  server.stdout.on('data', (d) => { if (String(d).includes('READY')) resolve(); });
  server.on('exit', (code) => reject(new Error(`test server exited early: ${code}`)));
  setTimeout(() => reject(new Error('test server did not come up in 5s')), 5000);
});

// 同步 XHR 替身：用子进程里的 curl 阻塞取，语义与 Worker 里的同步 XHR 一致。
// -D 把响应头倒进临时文件，getAllResponseHeaders() 才有东西可回。
globalThis.XMLHttpRequest = class {
  open(method, url) { this._method = method; this._url = url; this._headers = []; this._respHeaders = ''; }
  setRequestHeader(k, v) { this._headers.push('-H', `${k}: ${v}`); }
  getAllResponseHeaders() { return this._respHeaders; }
  send(body) {
    const dump = join(tmpdir(), `qi-http-hdr-${process.pid}-${Date.now()}`);
    const args = ['-s', '-X', this._method, ...this._headers, '-D', dump, '-w', '\n__STATUS__%{http_code}'];
    if (body) args.push('--data-binary', body);
    args.push(this._url);
    const out = execFileSync('curl', args, { maxBuffer: 64 * 1024 * 1024, timeout: 10000 }).toString();
    const i = out.lastIndexOf('\n__STATUS__');
    this.responseText = out.slice(0, i);
    this.status = parseInt(out.slice(i + 11), 10);
    try {
      // 第一行是状态行 "HTTP/1.1 201 Created"，其余是 "名: 值"
      this._respHeaders = readFileSync(dump, 'utf8').split(/\r?\n/).slice(1).join('\r\n');
      unlinkSync(dump);
    } catch { this._respHeaders = ''; }
  }
};

let code = 1;
try {
  const target = process.argv[2];
  if (target.endsWith('.qi')) {
    const qi = process.env.QI || 'qi';
    // 事件循环被堵住没关系：服务器在另一个进程里
    const out = execFileSync(qi, ['run', target], { stdio: ['ignore', 'pipe', 'inherit'], timeout: 60000 });
    process.stdout.write(out);
    code = 0;
  } else {
    const chunks = [];
    code = await runQiWasm(readFileSync(target), (s) => chunks.push(s));
    process.stdout.write(chunks.join(''));
  }
} finally {
  server.kill();
}
process.exit(code);
