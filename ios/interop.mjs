// Un navigateur (Chromium) et l'iPhone (AlleyKit, `swift test`) dans le même
// salon : la clé remise par le navigateur, les messages, les réponses et les
// réactions dans les deux sens, la même empreinte de clé.
// ALLEY_SERVER=http://localhost:3999 node ios/interop.mjs  (Playwright requis)
import { chromium } from "playwright";
import { spawn } from "node:child_process";
const base = process.env.ALLEY_SERVER || "http://localhost:3999";
const b = await chromium.launch();
const p = await (await b.newContext({ locale: "fr-FR" })).newPage();
let fails = 0; const check = (ok, w) => { console.log(ok ? "✓" : "ÉCHEC", w); if (!ok) fails++; };
await p.goto(base + "/app/"); await p.fill("#username", "Navigateur"); await p.click("#guest"); await p.waitForSelector(".lobby");
await p.click("#create-btn"); await p.fill("#new-room-name", "Interop iPhone"); await p.check(".switch-row input");
await p.click("dialog.sheet button[type=submit]");
await p.waitForFunction(() => document.querySelector(".lock")?.title.includes("-"));
const id = await p.evaluate(() => { let l; document.querySelector("#room-settings").click(); l = document.querySelector(".link-input").value; document.querySelector("dialog.sheet .close").click(); return l.split("salon=")[1]; });
const fp = (await p.getAttribute(".lock", "title")).match(/[0-9A-F]{4}(-[0-9A-F]{4}){3}/)[0];
const swift = spawn("swift", ["test", "--filter", "testTalksWithTheBrowser"], { cwd: new URL(".", import.meta.url).pathname, env: { ...process.env, ALLEY_SERVER: base, ALLEY_INTEROP_ROOM: id } });
let out = ""; swift.stdout.on("data", d => out += d); swift.stderr.on("data", d => out += d);
await p.waitForFunction(() => document.querySelector("#members")?.textContent.includes("iPhone"), null, { timeout: 120000 });
check(true, "l'iPhone est dans le salon, sous son pseudonyme");
await p.fill("#input", "Bonjour depuis le navigateur"); await p.keyboard.press("Enter");
await p.waitForSelector(".message .text:has-text('Bonjour depuis l\\'iPhone 📱')", { timeout: 30000 });
check(await p.isVisible(".message:has-text('Bonjour depuis l\\'iPhone') .reply-ref"), "sa réponse cite mon message");
await p.waitForSelector(".reaction:has-text('👍')", { timeout: 15000 });
check(true, "sa réaction 👍 s'affiche");
await p.fill("#input", "Bien reçu"); await p.keyboard.press("Enter");
const code = await new Promise(r => swift.on("close", r));
check(code === 0, `côté iPhone : swift test ${code === 0 ? "réussi" : "échoué\n" + out.split("\n").filter(l => /error|fail/i.test(l)).join("\n")}`);
const theirs = (out.match(/INTEROP-FINGERPRINT (\S+)/) || [])[1];
check(theirs === fp, `même empreinte de clé : ${fp} / ${theirs}`);
await p.screenshot({ path: process.env.SHOT || "/tmp/interop.png" });
await b.close(); console.log(fails ? `${fails} échec(s)` : "tout est bon"); process.exit(fails ? 1 : 0);
