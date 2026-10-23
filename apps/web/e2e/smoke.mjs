// Loads the built app in headless Chromium, sends a test room through the wasm
// engine, and saves the project; then imports the .3dm and .skp fixtures.
// Usage: node e2e/smoke.mjs <out.raproj> [url]
// Set CHROMIUM_PATH to use a preinstalled browser.
import { chromium } from 'playwright';

const [out, url = 'http://localhost:4173/'] = process.argv.slice(2);
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH || undefined });
const page = await browser.newPage({ acceptDownloads: true });
page.on('pageerror', (e) => {
  console.error(e);
  process.exitCode = 1;
});
await page.goto(url);
await page.waitForFunction(() => !document.querySelector('#test-room').disabled, null, { timeout: 30000 });
console.log(await page.textContent('#caps'));
await page.click('#test-room');
await page.waitForFunction(() => document.querySelector('#out').textContent.length > 0);
const report = JSON.parse(await page.textContent('#out'));
if (report.enclosed_volume_m3 !== 7200) throw new Error(`unexpected report ${JSON.stringify(report)}`);
const [download] = await Promise.all([page.waitForEvent('download'), page.click('#save')]);
await download.saveAs(out);
console.log('saved', out);

// Then read the Rhino and SketchUp fixtures through the import worker.
for (const [file, importer] of [['hall.3dm', 'rhino3dm'], ['hall.skp', 'openskp']]) {
  await page.setInputFiles('#model', new URL(`../test/fixtures/${file}`, import.meta.url).pathname);
  await page.waitForFunction(() => document.querySelector('#out').textContent.startsWith('{'), null, { timeout: 30000 });
  const result = JSON.parse(await page.textContent('#out'));
  if (result.importer !== importer || !(result.report.triangles > 0)) throw new Error(`unexpected import ${JSON.stringify(result)}`);
  console.log(file, `${result.report.triangles} triangles, ${result.layers.length} layers`);
}
// Map layers from their names, then re-import: the mapping stays with the layers.
const isCarpet = (key) => document.querySelector(`select[data-key="${key}"]`)?.value === 'Carpet, heavy';
await page.click('#suggest');
await page.waitForFunction(isCarpet, 'Floor carpet', { timeout: 10000 });
await page.evaluate(() => (document.querySelector('#out').textContent = ''));
await page.setInputFiles('#model', new URL('../test/fixtures/hall.skp', import.meta.url).pathname);
await page.waitForFunction(() => document.querySelector('#out').textContent.startsWith('{'), null, { timeout: 30000 });
await page.waitForFunction(isCarpet, 'Floor carpet', { timeout: 10000 });
console.log('layer mapping kept across re-import');

// The reverberation estimate follows the mapping.
await page.waitForFunction(() => document.querySelector('#reverb-status').textContent.startsWith('Volume'), null, { timeout: 30000 });
const rt = await page.$$eval('#reverb tbody tr', (rows) => rows.map((r) => Number(r.children[1].textContent)));
if (!rt.every((t) => t > 0 && t < 20)) throw new Error(`unexpected reverberation ${rt}`);
console.log('Sabine s per octave', rt.join(' '), '|', (await page.textContent('#reverb-status')).split('\n')[0]);

// A material library from CSV joins the pickers.
await page.setInputFiles('#library', new URL('../test/fixtures/materials-sample.csv', import.meta.url).pathname);
await page.waitForFunction(() => document.querySelector('#library-status').textContent.startsWith('Imported 2'), null, { timeout: 10000 });
const names = await page.$$eval('select[data-key="Floor carpet"] option', (os) => os.map((o) => o.value));
if (!names.includes('Plaster on brick')) throw new Error(`library import missing: ${names}`);
console.log('material library imported from CSV');
await browser.close();
