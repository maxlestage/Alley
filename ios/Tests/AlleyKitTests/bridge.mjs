// Tests sous Linux seulement : la WebSocket d'URLSession y manque (libcurl).
// Ce petit pont relaie une WebSocket ligne à ligne sur stdin/stdout.
const ws = new WebSocket(process.argv[2]);
ws.onmessage = (e) => process.stdout.write(JSON.stringify(String(e.data)) + "\n");
ws.onclose = (e) => { process.stdout.write(`CLOSE ${e.code}\n`); process.exit(0); };
let buf = "";
process.stdin.on("data", (d) => {
  buf += d;
  let i;
  while ((i = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, i); buf = buf.slice(i + 1);
    if (line === "CLOSE") ws.close(); else if (ws.readyState === 1) ws.send(JSON.parse(line));
  }
});
process.stdin.on("end", () => ws.close());
