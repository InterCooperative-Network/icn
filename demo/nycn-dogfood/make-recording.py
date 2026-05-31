#!/usr/bin/env python3
"""Render a NYCN dogfood transcript into a self-contained HTML terminal-replay.

Usage:
    make-recording.py <transcript.txt> <output.html>

The output is a single self-contained HTML file (no external assets) that replays
the captured run in a faux terminal, with a Replay button. Open it in any browser,
or screen-capture it to a GIF/video for slides.
"""
import json
import sys


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit("usage: make-recording.py <transcript.txt> <output.html>")
    src = open(sys.argv[1], encoding="utf-8").read()
    lines = [
        ln
        for ln in src.splitlines()
        if not ln.startswith("gateway up after")
        and not ln.startswith("RUN_EXIT=")
        and not ln.startswith("artifacts:")
        and not ln.startswith("recording:")
        and "CLEAN RUN" not in ln
    ]
    body = "\n".join(lines).strip()
    data = json.dumps({"cmd": "$ ICN_PASSPHRASE=•••• ./run.sh --fresh", "body": body})
    html = TEMPLATE.replace("__DATA__", data)
    open(sys.argv[2], "w", encoding="utf-8").write(html)
    print("wrote", sys.argv[2])


TEMPLATE = r'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>ICN dogfood loop &mdash; work &rarr; obligation &rarr; receipt</title>
<style>
:root{--bg:#0d1117;--fg:#c9d1d9;--dim:#8b949e;--hdr:#79c0ff;--ok:#7ee787;--bar:#161b22;--bd:#30363d}
*{box-sizing:border-box}
body{margin:0;background:#010409;color:var(--fg);font:15px/1.55 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;
display:flex;flex-direction:column;align-items:center;padding:28px 16px}
h1{font:600 15px/1.4 ui-sans-serif,system-ui,sans-serif;color:#adbac7;margin:0 0 4px;text-align:center}
.sub{font:13px/1.4 ui-sans-serif,system-ui,sans-serif;color:var(--dim);margin:0 0 16px;text-align:center;max-width:820px}
.term{width:min(900px,100%);background:var(--bg);border:1px solid var(--bd);border-radius:10px;overflow:hidden;box-shadow:0 16px 50px rgba(0,0,0,.5)}
.bar{display:flex;align-items:center;gap:8px;background:var(--bar);padding:10px 14px;border-bottom:1px solid var(--bd)}
.dot{width:12px;height:12px;border-radius:50%}.r{background:#ff5f56}.y{background:#ffbd2e}.g{background:#27c93f}
.bar .t{margin-left:8px;color:var(--dim);font-size:12.5px}
.screen{padding:16px 18px;min-height:420px;max-height:70vh;overflow:auto;white-space:pre-wrap;word-break:break-word}
.h{color:var(--hdr);font-weight:600}.ok{color:var(--ok)}.thesis{color:var(--ok);font-weight:600}.dim{color:var(--dim)}.cmd{color:#adbac7}
.cur{display:inline-block;width:8px;height:17px;background:var(--ok);vertical-align:-3px;animation:b 1s steps(1) infinite}
@keyframes b{50%{opacity:0}}
.foot{margin-top:14px;display:flex;gap:14px;align-items:center;font:13px/1.4 ui-sans-serif,system-ui,sans-serif;color:var(--dim)}
button{background:#21262d;color:#c9d1d9;border:1px solid var(--bd);border-radius:6px;padding:7px 14px;font:13px ui-sans-serif,system-ui,sans-serif;cursor:pointer}
button:hover{background:#30363d}
.label{background:#1f2d24;color:var(--ok);border:1px solid #2ea04326;border-radius:6px;padding:4px 9px;font-size:12px}
</style></head><body>
<h1>ICN turns cooperative work into legible obligations and verifiable receipts</h1>
<p class="sub">A live loop on a node the cooperative controls &mdash; work &rarr; obligation &rarr; <b>verifiable receipt</b>. Rehearsal, not production: test identities, local node, no real data.</p>
<div class="term"><div class="bar"><span class="dot r"></span><span class="dot y"></span><span class="dot g"></span><span class="t">demo/nycn-dogfood/run.sh &mdash; local ICN node</span></div>
<div class="screen" id="s"></div></div>
<div class="foot"><button id="replay">&#9654; Replay</button><span class="label">verifiable receipt: record_hash</span><span>Press Replay to watch it run.</span></div>
<script>
const DATA=__DATA__;
const s=document.getElementById('s'),btn=document.getElementById('replay');
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
function cls(l){if(l.startsWith('== '))return'h';if(l.startsWith('*'))return'thesis';
if(/record_hash|tamper-evident|"status": "completed"|open_cards|receipt|Obligation:|Recorded as|card cleared/.test(l))return'ok';
if(l.startsWith('   ')||l.startsWith('  -'))return'dim';return'';}
async function play(){
  btn.disabled=true;s.innerHTML='';
  const cur=document.createElement('span');cur.className='cur';
  const c=document.createElement('div');c.className='cmd';s.appendChild(c);s.appendChild(cur);
  for(const ch of DATA.cmd){c.textContent+=ch;await sleep(18);}
  await sleep(450);
  const lines=DATA.body.split('\n');
  for(const ln of lines){
    const d=document.createElement('div');const k=cls(ln);if(k)d.className=k;d.textContent=ln||'​';
    s.insertBefore(d,cur);s.scrollTop=s.scrollHeight;
    let delay=42;if(ln.startsWith('== '))delay=420;else if(ln.trim()==='')delay=180;else if(ln.startsWith('*'))delay=260;
    await sleep(delay);
  }
  s.scrollTop=s.scrollHeight;btn.disabled=false;
}
function renderStatic(){
  s.innerHTML='';
  const c=document.createElement('div');c.className='cmd';c.textContent=DATA.cmd;s.appendChild(c);
  for(const ln of DATA.body.split('\n')){const d=document.createElement('div');const k=cls(ln);if(k)d.className=k;d.textContent=ln||'​';s.appendChild(d);}
  s.scrollTop=0;
}
const reduceMotion=window.matchMedia&&window.matchMedia('(prefers-reduced-motion: reduce)').matches;
btn.onclick=play;
if(reduceMotion){renderStatic();}else{play();}
</script></body></html>'''


if __name__ == "__main__":
    main()
