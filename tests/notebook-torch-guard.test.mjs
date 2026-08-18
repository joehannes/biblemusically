// The CUDA-torch guard the engine notebooks carry, checked as structure rather than as prose.
//
// The failure it exists for was not a crash: a run that HAD a T4 installed a CPU-only torch wheel
// over Kaggle's CUDA one, the serve cell refused to start (it gates on torch, not on the driver),
// and the app reported an exhausted weekly quota to somebody with hours left. Nothing about that
// state is loud — pip prints no error, and the install cell's own "environment OK" line was true.
//
// None of this can be run here: there is no GPU, and the guard's whole job is to notice one. What
// CAN be held is the shape of the thing, and every property below is load-bearing rather than
// stylistic — each corresponds to a way the repair silently stops working while still looking right.
//
// Run with: npm run test:unit
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const ENGINES = ["acestep", "heartmula"];
const load = (e) => JSON.parse(readFileSync(new URL(`../src-tauri/kaggle_notebooks/${e}.ipynb`, import.meta.url)));
const codeOf = (nb) => nb.cells.filter((c) => c.cell_type === "code").map((c) => c.source.join(""));
const installCell = (nb) => codeOf(nb).find((s) => s.includes("_repair_torch_if_clobbered"));

for (const engine of ENGINES) {
  test(`${engine}: the repair probes torch out of process, never in it`, () => {
    const cell = installCell(load(engine));
    assert.ok(cell, "the install cell carries the repair");
    // The two facts that make an in-process check useless: torch caches CUDA availability for the
    // life of the process, and pip cannot replace a module that process has already imported. So a
    // bare `import torch` anywhere before the repair would quietly make it a no-op — it would run,
    // report success, and the serve cell would still find a blind torch.
    const repairAt = cell.indexOf("_torch_after = _repair_torch_if_clobbered()");
    const inProcessImport = cell.search(/^\s*(import torch|from torch)/m);
    assert.ok(repairAt > 0, "the repair is actually called");
    assert.ok(inProcessImport === -1 || inProcessImport > repairAt,
      "torch must not be imported in the kernel before the repair runs");
    assert.match(cell, /subprocess\.run\(\[sys\.executable, '-c'/, "the probe is a subprocess");
  });

  test(`${engine}: the restored wheel is derived, not guessed, and drags nothing with it`, () => {
    const cell = installCell(load(engine));
    // The version comes from what Kaggle actually shipped before this cell touched anything, and
    // the index comes from that build's own '+cuNNN' suffix. A hardcoded version or index would rot
    // the moment Kaggle bumped its image — silently, since the wrong wheel still installs.
    assert.match(cell, /'torch==' \+ prev/, "reinstalls the exact build recorded before the install");
    assert.match(cell, /'https:\/\/download\.pytorch\.org\/whl\/' \+ tag/, "index derived from the +cu suffix");
    assert.ok(!/download\.pytorch\.org\/whl\/cu\d/.test(cell), "no hardcoded CUDA index");
    // Without --no-deps, resolving torch's dependencies can pull the CPU wheel straight back.
    assert.match(cell, /'--no-deps'/, "the repair must not resolve dependencies");
  });

  test(`${engine}: the repair only fires when a GPU is present and torch is blind`, () => {
    const cell = installCell(load(engine));
    // Both halves matter. Without the nvidia-smi gate it would churn on every genuine denial; without
    // the cuda gate it would reinstall over a perfectly healthy environment on every single run.
    assert.match(cell, /nvidia-smi/, "gated on the driver being there");
    assert.match(cell, /if smi != 0 or after\.get\('cuda'\)/, "returns early unless GPU present and torch blind");
    assert.match(cell, /if '\+cu' not in prev/, "refuses to guess when there is no CUDA build to restore");
  });

  test(`${engine}: the notebook is stamped newer so the fix reaches existing kernels`, () => {
    // A start re-pushes the bundled notebook only when its revision outranks the copy on Kaggle.
    // Ship the fix without bumping this and it reaches precisely the people who have never run the
    // engine — which is the opposite of who needs it.
    const rev = load(engine).metadata?.bm_notebook_revision;
    assert.ok(Number.isInteger(rev), "carries a bm_notebook_revision stamp");
    assert.ok(rev >= (engine === "heartmula" ? 6 : 4), `revision ${rev} must not go backwards`);
  });
}
