import { createConnector } from './engine/connector';
import { importExtensions, importModel } from './importers';
import { testRoom } from './importers/test-room';
import { ReverberationPanel } from './acoustics/reverberation';
import { RunPanel } from './acoustics/run';
import { SetupPanel } from './acoustics/setup';
import { MaterialsPanel } from './materials/panel';

const engine = createConnector();
const out = document.querySelector<HTMLPreElement>('#out')!;
const caps = document.querySelector<HTMLParagraphElement>('#caps')!;
const testButton = document.querySelector<HTMLButtonElement>('#test-room')!;
const saveButton = document.querySelector<HTMLButtonElement>('#save')!;
const modelInput = document.querySelector<HTMLInputElement>('#model')!;
const reverbSection = document.querySelector<HTMLElement>('#reverb')!;
const reverb = new ReverberationPanel(engine, reverbSection);
const materials = new MaterialsPanel(engine, document.querySelector<HTMLElement>('#materials')!, () => reverb.update());
const setupSection = document.querySelector<HTMLElement>('#setup')!;
const setup = new SetupPanel(engine, setupSection);
const simulateSection = document.querySelector<HTMLElement>('#simulate')!;
new RunPanel(engine, simulateSection);
let projectName = 'test-room';

/** Shows the sources, receivers and run panels for the scene just loaded. */
async function showSetup() {
  setupSection.hidden = false;
  simulateSection.hidden = false;
  await setup.show();
}

const show = (x: unknown) => (out.textContent = JSON.stringify(x, null, 2));

engine
  .capabilities()
  .then((c) => {
    caps.textContent = `Engine ${c.engine_version} (${c.target}), project format ${c.format}`;
    testButton.disabled = false;
    modelInput.disabled = false;
  })
  .catch((e) => (caps.textContent = `Engine failed to start: ${e}`));

testButton.onclick = async () => {
  show(await engine.setScene(testRoom()));
  projectName = 'test-room';
  saveButton.disabled = false;
  await showSetup();
};

modelInput.accept = importExtensions.join(',');
modelInput.onchange = async () => {
  const file = modelInput.files?.[0];
  if (!file) return;
  // Clear the picker so choosing the same file again, after editing it, imports it again.
  modelInput.value = '';
  out.textContent = `Reading ${file.name}…`;
  try {
    const t0 = performance.now();
    const raw = await importModel(file);
    const seconds = +((performance.now() - t0) / 1000).toFixed(1);
    const report = await engine.setScene(raw);
    show({
      file: file.name,
      seconds,
      importer: raw.importer,
      layers: raw.groups.map((g) => g.layerPath),
      points: raw.points.map((p) => `${p.role} ${p.id}`),
      report,
      warnings: raw.warnings,
      skipped: raw.skipped,
    });
    projectName = file.name.replace(/\.[^.]+$/, '');
    saveButton.disabled = false;
    await materials.show();
    reverbSection.hidden = false;
    await reverb.update();
    await showSetup();
  } catch (e) {
    out.textContent = `Could not import ${file.name}: ${e instanceof Error ? e.message : e}`;
  }
};

saveButton.onclick = async () => {
  const blob = await engine.saveProject();
  const a = Object.assign(document.createElement('a'), {
    href: URL.createObjectURL(blob),
    download: `${projectName}.raproj`,
  });
  a.click();
  URL.revokeObjectURL(a.href);
};
