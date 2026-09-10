// capture-diskjanitor.mjs — Disk Janitor :1420 demo 数据三件套采集
// 按 capture-template.mjs 管线；CONFIG 针对还我磁盘 SPA（无路由 path，靠点击驱动）。
import puppeteer from 'puppeteer';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const CONFIG = {
  BASE: 'http://localhost:1420',
  OUT_DIR: '/Users/aaikyn1/Code/disk_janitor/video/assets/textures',
  LAYOUT_JSON: '/Users/aaikyn1/Code/disk_janitor/video/assets/layout.json',
  VIEWPORT: { width: 1920, height: 1080, deviceScaleFactor: 2 },
  SETTLE_MS: 700,
};

const here = path.dirname(fileURLToPath(import.meta.url));
const outDir = CONFIG.OUT_DIR;
fs.mkdirSync(outDir, { recursive: true });
fs.mkdirSync(path.dirname(CONFIG.LAYOUT_JSON), { recursive: true });

const browser = await puppeteer.launch({ headless: 'new' });
const page = await browser.newPage();
await page.setViewport(CONFIG.VIEWPORT);

const settle = async (ms = CONFIG.SETTLE_MS) => {
  await page.evaluate(() => document.fonts.ready);
  await new Promise((r) => setTimeout(r, ms));
};
const pageBox = (el) =>
  el.evaluate((e) => {
    const r = e.getBoundingClientRect();
    return { x: r.x + window.scrollX, y: r.y + window.scrollY, w: r.width, h: r.height };
  });

const layout = { pageW: CONFIG.VIEWPORT.width };

const clickNav = async (label) => {
  const did = await page.evaluate((lbl) => {
    let btn = [...document.querySelectorAll('button.nav-item, button.nav-btn')].find((b) => (b.textContent ?? '').includes(lbl));
    if (!btn) btn = document.querySelector(`button[aria-label="${lbl}"]`);
    if (btn) { btn.click(); return true; }
    return false;
  }, label);
  if (!did) throw new Error(`nav not found: ${label}`);
};

// ---- 状态 1: Dashboard 有数态 ----
await page.goto(CONFIG.BASE, { waitUntil: 'networkidle0' });
await settle(1200);
{
  const entry = { pageH: await page.evaluate(() => document.documentElement.scrollHeight) };
  layout.dashboard = entry;
  await page.screenshot({ path: `${outDir}/dashboard-full.png`, fullPage: true });
  console.log('captured dashboard-full');
  // orb 特写 4x
  const orb = await page.$('main .orb, main [class*="orb"], main canvas, main svg');
  // orb 是 div 球体：用包含大圆角的特征找
  const orbEl = await page.evaluateHandle(() => {
    const els = [...document.querySelectorAll('main *')];
    return els.find((e) => { const s = getComputedStyle(e); const r = e.getBoundingClientRect();
      return r.width > 80 && r.width < 220 && s.borderRadius && parseFloat(s.borderRadius) >= r.width * 0.45; });
  });
  if (orbEl && orbEl.asElement()) {
    const bb = await pageBox(orbEl.asElement());
    const cdp = await page.createCDPSession();
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1920, height: 1080, deviceScaleFactor: 4, mobile: false });
    await new Promise((r) => setTimeout(r, 400));
    try {
      await orbEl.asElement().screenshot({ path: `${outDir}/dashboard-orb.png` });
      entry.cutouts = [{ file: 'dashboard-orb.png', ...bb, scale: 4 }];
      console.log('captured dashboard-orb 4x', bb);
    } catch (e) { console.log('orb cutout miss:', e.message); }
    await cdp.send('Emulation.clearDeviceMetricsOverride');
  }
  // 一键清理按钮 cutout（供 finale 元素）
  const btn = await page.evaluateHandle(() => [...document.querySelectorAll('button')].find((b) => /一键清理/.test(b.textContent ?? '')));
  if (btn && btn.asElement()) {
    const bb = await pageBox(btn.asElement());
    await btn.asElement().screenshot({ path: `${outDir}/dashboard-btn-clean.png` });
    (entry.cutouts ??= []).push({ file: 'dashboard-btn-clean.png', ...bb });
    console.log('captured dashboard-btn-clean', bb);
  }
}

// ---- 状态 2: Scan 进行中 ----
await clickNav('仪表盘');
await settle(400);
await page.evaluate(() => {
  const b = [...document.querySelectorAll('button')].find((x) => /开始扫描/.test(x.textContent ?? ''));
  if (b) b.click();
});
await settle(2500); // 扫描进行中：速率与计数可见
{
  const entry = { pageH: await page.evaluate(() => document.documentElement.scrollHeight) };
  layout.scan = entry;
  await page.screenshot({ path: `${outDir}/scan-full.png`, fullPage: true });
  console.log('captured scan-full (running state)');
}

// ---- 状态 3: Review 展开态 ----
// 等扫描完成自动跳转（demo 数据秒级）；若已在 scan 页则等完成
await settle(6000);
{
  // 可能已经自动切到 review；不在就点 nav
  const onReview = await page.evaluate(() => [...document.querySelectorAll('button')].some((b) => b.className?.includes?.('active') && /审核/.test(b.textContent ?? '')));
  if (!onReview) { try { await clickNav('审核'); } catch {} }
  await settle(900);
  // 展开可安全回收折叠行（Row 主体是 ul.rows > li 内的 button）
  await page.evaluate(() => {
    const li = [...document.querySelectorAll('ul.rows > li')].find((e) => /可安全回收/.test(e.textContent ?? ''));
    const btn = li?.querySelector('button') ?? li;
    if (btn) btn.click();
  });
  await settle(700);
  const entry = { pageH: await page.evaluate(() => document.documentElement.scrollHeight) };
  layout.review = entry;
  await page.screenshot({ path: `${outDir}/review-full.png`, fullPage: true });
  console.log('captured review-full (expanded)');
  // 头部大数字 cutout 4x（MetricLine .value.num）
  const num = await page.evaluateHandle(() => document.querySelector('main .metric .value.num') ?? [...document.querySelectorAll('.num')].find((e) => /GB$/.test(e.textContent?.trim() ?? '')));
  if (num && num.asElement()) {
    const bb = await pageBox(num.asElement());
    const cdp = await page.createCDPSession();
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1920, height: 1080, deviceScaleFactor: 4, mobile: false });
    await new Promise((r) => setTimeout(r, 400));
    try {
      await num.asElement().screenshot({ path: `${outDir}/review-hero-num.png` });
      (entry.cutouts ??= []).push({ file: 'review-hero-num.png', ...bb, scale: 4 });
      console.log('captured review-hero-num 4x', bb);
    } catch (e) { console.log('num cutout miss:', e.message); }
    await cdp.send('Emulation.clearDeviceMetricsOverride');
  }
  // 证据行 cutouts（分级行）
  const rows = await page.$$('main li, main [class*="row"]');
  let i = 0;
  entry.rowCutouts = [];
  for (const r of rows.slice(0, 8)) {
    const bb = await pageBox(r);
    if (bb.h < 20 || bb.h > 160) continue;
    try {
      await r.screenshot({ path: `${outDir}/review-row${i + 1}.png` });
      entry.rowCutouts.push({ file: `review-row${i + 1}.png`, ...bb });
      i++;
    } catch {}
    if (i >= 6) break;
  }
  console.log('captured review rows:', i);
}

// ---- 状态 4: PurgeGate ----
// 审核→生成计划→应用→清理隔离区
await page.evaluate(() => {
  const b = [...document.querySelectorAll('button')].find((x) => /生成计划/.test(x.textContent ?? ''));
  if (b) b.click();
});
await settle(900);
await page.evaluate(() => {
  const b = [...document.querySelectorAll('button')].find((x) => /应用/.test(x.textContent ?? ''));
  if (b) b.click();
});
await settle(900);
await page.evaluate(() => {
  const b = [...document.querySelectorAll('button')].find((x) => /清理隔离区/.test(x.textContent ?? ''));
  if (b) b.click();
});
await settle(900);
{
  const entry = { pageH: await page.evaluate(() => document.documentElement.scrollHeight) };
  layout.purgegate = entry;
  await page.screenshot({ path: `${outDir}/purgegate-full.png`, fullPage: true });
  console.log('captured purgegate-full');
}

// ---- 状态 5: Ledger 记录态 ----
// PurgeGate 若完成了 purge 会有记录；先关闭 gate（取消），直接看台账
await page.evaluate(() => {
  const b = [...document.querySelectorAll('button')].find((x) => /^取消$/.test(x.textContent?.trim() ?? ''));
  if (b) b.click();
});
await settle(500);
try {
  await clickNav('台账');
} catch (e) {
  console.log('ledger nav miss:', e.message);
}
await settle(900);
{
  const where = await page.evaluate(() => document.querySelector('main h1')?.textContent ?? 'none');
  console.log('ledger page shows:', where);
  const entry = { pageH: await page.evaluate(() => document.documentElement.scrollHeight) };
  layout.ledger = entry;
  await page.screenshot({ path: `${outDir}/ledger-full.png`, fullPage: true });
  console.log('captured ledger-full', entry.pageH);
}

fs.writeFileSync(CONFIG.LAYOUT_JSON, JSON.stringify(layout, null, 2));
console.log('layout.json written');
await browser.close();
