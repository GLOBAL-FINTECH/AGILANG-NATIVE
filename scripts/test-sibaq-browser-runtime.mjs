import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";

const output = process.argv[2] ?? "target/sibaq-dashboard-browser";
const script = fs.readFileSync(path.join(output, "sibaq-dashboard.js"), "utf8");
const dependencies = JSON.parse(
  fs.readFileSync(path.join(output, "sibaq-dashboard.dependencies.json"), "utf8"),
);
const heightNode = dependencies["chain.height"][0];

const elements = new Map();
function element(id) {
  if (!elements.has(id)) {
    elements.set(id, {
      id,
      hidden: true,
      innerHTML: "",
      textContent: "",
      replaceChildren() { this.innerHTML = ""; },
    });
  }
  return elements.get(id);
}

const intervals = [];
const responses = [];
globalThis.document = {
  documentElement: { dataset: {} },
  getElementById: (id) => element(id),
};
globalThis.setInterval = (callback) => { intervals.push(callback); return intervals.length; };
globalThis.fetch = async () => {
  const next = responses.shift();
  if (next instanceof Error) throw next;
  return { ok: true, status: 200, json: async () => next };
};

vm.runInThisContext(script, { filename: "sibaq-dashboard.js" });
if (intervals.length !== 1) throw new Error(`expected one poller, got ${intervals.length}`);

const poll = intervals[0];
async function runPoll() {
  poll();
  await new Promise((resolve) => setTimeout(resolve, 0));
}
responses.push({ height: 1450901 });
await runPoll();
if (element(`node-${heightNode}`).textContent !== 1450901) throw new Error("changed height was not rendered");
if (document.documentElement.dataset.agschainState !== "Ready") throw new Error("source did not return to Ready");

responses.push(new Error("temporary outage"));
await runPoll();
if (document.documentElement.dataset.agschainState !== "Stale") throw new Error("failure did not enter Stale");
if (element("ags-state-chain").hidden) throw new Error("stale template was not shown");
if (element(`node-${heightNode}`).textContent !== 1450901) throw new Error("stale transition discarded valid data");

responses.push(new Error("continued outage"));
await runPoll();
if (document.documentElement.dataset.agschainState !== "Error") throw new Error("continued failure did not enter Error");
if (!element("ags-state-chain").innerHTML.includes("continued outage")) throw new Error("escaped error message was not rendered");

responses.push({ height: 1450902 });
await runPoll();
if (document.documentElement.dataset.agschainState !== "Ready") throw new Error("recovery did not return to Ready");
if (!element("ags-state-chain").hidden) throw new Error("state template remained visible after recovery");
if (element(`node-${heightNode}`).textContent !== 1450902) throw new Error("recovered value was not rendered");

console.log("AGS browser runtime harness passed: Ready -> Stale -> Error -> Ready");
