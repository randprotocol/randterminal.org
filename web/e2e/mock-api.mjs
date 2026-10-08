// Serves the recorded API responses for the hermetic Playwright suite (port 8799).
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
const F = (n) => JSON.parse(readFileSync(new URL(`./fixtures/${n}`, import.meta.url)));
const job = F('job.json'), run = F('run.json'), states = F('states.json'), memory = F('memory.json'), cycles0 = F('cycles-0.json');
const send = (res, code, body) => { res.writeHead(code, { 'content-type': 'application/json' }); res.end(JSON.stringify(body)); };
createServer((req, res) => {
  const u = new URL(req.url, 'http://x'); const p = u.pathname.replace(/^\/api\/v1/, '');
  if (p === '/samples') return send(res, 200, F('samples.json'));
  if (p === '/samples/erc20') return send(res, 200, F('sample-erc20.json'));
  if (req.method === 'POST' && p === '/jobs') return send(res, 201, { id: job.id });
  if (p === `/jobs/${job.id}`) return send(res, 200, job);
  if (p === `/jobs/${job.id}/events`) { res.writeHead(200, { 'content-type': 'text/event-stream' }); res.end(`event: job\ndata: ${JSON.stringify(job)}\n\nevent: done\ndata: {}\n\n`); return; }
  for (const s of ['compile', 'transpile', 'image']) if (p === `/jobs/${job.id}/${s}`) return send(res, 200, F(`${s}.json`));
  if (req.method === 'POST' && p === `/jobs/${job.id}/runs`) return send(res, 201, run);
  if (p === `/runs/${run.rid}`) return send(res, 200, run);
  if (p === `/runs/${run.rid}/cycles`) {
    const page = cycles0; const from = Number(u.searchParams.get('from') ?? 0), count = Number(u.searchParams.get('count') ?? 1024);
    if (!(from >= page.from && from < page.from + page.cycles.length)) return send(res, 404, { error: 'not_found', message: 'cycles not recorded there' });
    return send(res, 200, { ...page, from, cycles: page.cycles.slice(from - page.from, from - page.from + count) });
  }
  if (p === `/runs/${run.rid}/syscalls`) return send(res, 200, F('syscalls.json'));
  if (p === `/runs/${run.rid}/state`) { const s = states[u.searchParams.get('at')]; return s ? send(res, 200, s) : send(res, 404, { error: 'not_found', message: 'not recorded' }); }
  if (p === `/runs/${run.rid}/memory`) { const m = memory[u.searchParams.get('at')]; return m ? send(res, 200, m) : send(res, 404, { error: 'not_found', message: 'not recorded' }); }
  send(res, 404, { error: 'not_found', message: p });
}).listen(8799, '127.0.0.1');
