// 在 node 里模拟 Worker 环境（提供同步 XMLHttpRequest），端到端验 wasm 的 HTTP。
// 用真的本地 HTTP 服务器，不是打桩的响应。
//
// 注意：本文件是 JS，标识符一律英文（见仓库 CLAUDE.md）。中文只留给面向人的文本。
import http from 'node:http';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { runQiWasm } from './wasi-shim.js';

const PORT = 41573;
const server = http.createServer((req, res) => {
  let body = '';
  req.on('data', (c) => (body += c));
  req.on('end', () => {
    res.writeHead(req.url === '/big' ? 200 : 201, { 'Content-Type': 'text/plain; charset=utf-8' });
    // /big 回一个超过默认 64KB 缓冲的响应，逼出 wasm 侧的扩容重试那条路
    if (req.url === '/big') res.end('长'.repeat(50000));
    else res.end(`方法=${req.method} 路径=${req.url} 收到=${body}`);
  });
});
await new Promise((r) => server.listen(PORT, r));

// 同步 XHR 替身：用子进程里的 curl 阻塞取，语义与 Worker 里的同步 XHR 一致
globalThis.XMLHttpRequest = class {
  open(method, url) { this._method = method; this._url = url; this._headers = []; }
  setRequestHeader(k, v) { this._headers.push('-H', `${k}: ${v}`); }
  send(body) {
    const args = ['-s', '-X', this._method, ...this._headers, '-w', '\n__STATUS__%{http_code}'];
    if (body) args.push('--data-binary', body);
    args.push(this._url);
    const out = execFileSync('curl', args, { maxBuffer: 64 * 1024 * 1024 }).toString();
    const i = out.lastIndexOf('\n__STATUS__');
    this.responseText = out.slice(0, i);
    this.status = parseInt(out.slice(i + 11), 10);
  }
};

const chunks = [];
const code = await runQiWasm(readFileSync(process.argv[2]), (s) => chunks.push(s));
process.stdout.write(chunks.join(''));
server.close();
process.exit(code);
