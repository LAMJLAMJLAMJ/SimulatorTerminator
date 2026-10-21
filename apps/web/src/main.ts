import { createConnector } from './engine/connector';
import { testRoom } from './importers/test-room';

const engine = createConnector();
const out = document.querySelector<HTMLPreElement>('#out')!;
const caps = document.querySelector<HTMLParagraphElement>('#caps')!;
const testButton = document.querySelector<HTMLButtonElement>('#test-room')!;
const saveButton = document.querySelector<HTMLButtonElement>('#save')!;

const show = (x: unknown) => (out.textContent = JSON.stringify(x, null, 2));

engine
  .capabilities()
  .then((c) => {
    caps.textContent = `Engine ${c.engine_version} (${c.target}), project format ${c.format}`;
    testButton.disabled = false;
  })
  .catch((e) => (caps.textContent = `Engine failed to start: ${e}`));

testButton.onclick = async () => {
  show(await engine.setScene(testRoom()));
  saveButton.disabled = false;
};

saveButton.onclick = async () => {
  const blob = await engine.saveProject();
  const a = Object.assign(document.createElement('a'), {
    href: URL.createObjectURL(blob),
    download: 'test-room.raproj',
  });
  a.click();
  URL.revokeObjectURL(a.href);
};
