// Loads the built app in headless Chromium, sends a test room through the wasm
// engine, and saves the project. Usage: node e2e/smoke.mjs <out.raproj> [url]
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
await browser.close();
