/* Real-browser UI checks with a deliberately mocked API. No inference claims.
   PLAYWRIGHT_MODULE=/path/to/playwright-core node scripts/tests/ui-smoke.cjs
   Start scripts/serve-ui.py first. Chrome must be available locally. */
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright-core');
const origin = process.env.UI_URL || 'http://127.0.0.1:8000';
const state = page => page.evaluate(() => {
  const s = Alpine.$data(document.body);
  return { busy:s.busy, error:s.error, notice:s.notice, health:s.health, messages:s.activeChat?.messages || [], draft:s.draft, attachment:s.attachment, mode:s.mode };
});
(async () => {
  const browser = await chromium.launch({ executablePath:process.env.CHROME_PATH || '/usr/bin/google-chrome-stable', headless:true, args:['--no-sandbox'] });
  try {
  const page = await browser.newPage({ viewport:{ width:1440,height:1000 }, reducedMotion:'reduce' });
  const errors = [], foreign = [], calls = [];
  page.on('pageerror', e => errors.push(e.message));
  page.on('request', r => { if (!r.url().startsWith(origin) && !r.url().includes('/api/')) foreign.push(r.url()); });
  await page.route('**/api/ready', r => r.fulfill({ json:{ status:'ready',model_loaded:true } }));
  let handler = body => ({ response:'## A useful answer\n\n- Project Atlas launches on 12 November.\n- Maya owns the review.', model:'Qwen2.5-7B-Instruct-Q4_K_M', finish_reason:'stop', conversation_id:'31d170db-c6d7-4bd1-8b63-1fc66b05bf94' });
  await page.route('**/api/chat', async r => {
    const body = r.request().postDataJSON(); calls.push(body);
    const result = await handler(body);
    if (result.delay) await new Promise(resolve => setTimeout(resolve, result.delay));
    await r.fulfill({ status:result.status || 200, json:result.body || result }).catch(() => {});
  });
  const idle = () => page.waitForFunction(() => !Alpine.$data(document.body).busy && !Alpine.$data(document.body).reading);
  const fresh = async () => { await page.getByRole('button',{name:/New conversation/}).click(); };
  const send = async text => { await page.locator('#message-input').fill(text); await page.locator('#message-input').press('Enter'); await idle(); };
  await page.goto(origin); await page.waitForFunction(() => window.Alpine && Alpine.$data(document.body).health === 'ready');
  assert.equal((await state(page)).health, 'ready');
  await page.screenshot({path:'.cache/ui-check/desktop.png'});
  await page.locator('#message-input').fill('Hello'); await page.locator('#message-input').press('Shift+Enter');
  assert.equal(calls.length,0); assert.equal((await state(page)).draft,'Hello\n');
  await page.locator('#message-input').press('Enter'); await idle();
  assert.equal((await state(page)).messages.length,2);
  await send('Who owns the review?');
  assert.match(calls.at(-1).message,/Maya owns the review/); assert.match(calls.at(-1).message,/Who owns the review/);
  await fresh();
  const beforeLimit = calls.length;
  await send('a'.repeat(200001));
  assert.match((await state(page)).error, /200,000 characters/);
  assert.equal((await state(page)).draft.length, 200001);
  assert.equal(calls.length, beforeLimit);
  await fresh();
  const source = 'Section notes: The project keeps all details. '.repeat(370) + '\nFINAL_SENTINEL_729: Deadline 23 December. پایان سند';
  let rejected = false;
  handler = body => {
    if (!rejected && body.message.includes('<source>')) { rejected = true; return {status:500,body:{error:'Failed',details:'Prompt contains 5000 tokens; model context limit is 4096.'}}; }
    return {response:(body.message.includes('FINAL_SENTINEL') || body.message.includes('23 December')) ? 'Deadline 23 December. پایان سند' : 'The project keeps all details.', model:'Qwen2.5-7B-Instruct-Q4_K_M',finish_reason:'stop'};
  };
  const summaryStart = calls.length;
  await page.getByRole('button',{name:'Summarize',exact:true}).click();
  await send(source);
  assert.equal((await state(page)).error,''); assert.equal((await state(page)).messages.length,2); assert.equal(rejected,true);
  const sectionCalls = calls.slice(summaryStart).filter(c => c.message.includes('<source>'));
  // The first failed section is retried as smaller parts; successful sections
  // must reconstruct every character of the original, including the final tail.
  const reconstructed = sectionCalls.slice(1).map(c => c.message.split('<source>\n')[1].split('\n</source>')[0]).join('');
  assert.equal(reconstructed, source); assert.match((await state(page)).messages[1].contextNote,/all \d+ sections/);
  await page.getByRole('button',{name:'Chat',exact:true}).click();
  await send('What was the deadline?'); assert.ok(calls.at(-1).message.length < 10000); assert.match(calls.at(-1).message,/23 December/);
  await fresh();
  await page.locator('input[type=file]').setInputFiles({name:'notes.txt',mimeType:'text/plain',buffer:Buffer.from('Meeting notes: Maya owns the review. Deadline 12 November.')}); await idle();
  assert.equal((await state(page)).attachment.name,'notes.txt'); assert.equal((await state(page)).mode,'summary');
  await page.getByRole('button',{name:'Remove attachment'}).click();
  const docx = execFileSync('python3',['-c',`import io, zipfile, sys
b=io.BytesIO()
with zipfile.ZipFile(b,'w') as z:
 z.writestr('[Content_Types].xml','<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>')
 z.writestr('_rels/.rels','<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>')
 z.writestr('word/document.xml','<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Word document: Maya owns the review. متن فارسی</w:t></w:r></w:p></w:body></w:document>')
sys.stdout.buffer.write(b.getvalue())`]);
  await page.locator('input[type=file]').setInputFiles({name:'meeting.docx',mimeType:'application/vnd.openxmlformats-officedocument.wordprocessingml.document',buffer:docx}); await idle();
  assert.match((await state(page)).attachment.text,/Word document: Maya owns the review/); assert.match((await state(page)).attachment.text,/متن فارسی/);
  await page.getByRole('button',{name:'Remove attachment'}).click();
  const pdfPage = await browser.newPage(); await pdfPage.setContent('<p>PDF meeting notes: Review on 12 November.</p>'); const pdf = await pdfPage.pdf(); await pdfPage.close();
  await page.locator('input[type=file]').setInputFiles({name:'meeting.pdf',mimeType:'application/pdf',buffer:pdf}); await idle();
  assert.match((await state(page)).attachment?.text || (await state(page)).error,/PDF meeting notes/);
  await page.getByRole('button',{name:'Remove attachment'}).click();
  await page.locator('input[type=file]').setInputFiles({name:'program.exe',mimeType:'application/octet-stream',buffer:Buffer.from('hello')}); await idle(); assert.match((await state(page)).error,/Choose TXT/);
  await fresh();
  handler = () => ({response:'<img src="https://bad.invalid/x" onerror="window.pwned=1"><script>window.pwned=2</script>\n\n[click](javascript:alert(1)) **Safe**',model:'Qwen',finish_reason:'stop'});
  await send('Test rich text'); assert.equal(await page.evaluate(()=>window.pwned),undefined); assert.equal(await page.locator('.markdown script, .markdown img, .markdown [onerror], .markdown a[href^="javascript:"]').count(),0);
  handler = () => ({status:503,body:{error:'Service temporarily unavailable'}});
  await fresh(); await send('Please help'); assert.match((await state(page)).error,/temporarily unavailable/);
  handler = () => ({response:'Recovered answer',model:'Qwen',finish_reason:'stop'});
  await page.getByRole('button',{name:'Retry',exact:true}).click(); await idle(); assert.equal((await state(page)).messages.length,2); assert.equal(calls.at(-1).cache_bypass,true);
  handler = () => ({response:'Too late',model:'Qwen',finish_reason:'stop',delay:1500});
  await page.getByRole('button',{name:'Try again',exact:true}).click(); await page.getByRole('button',{name:'Stop waiting for response'}).click(); await idle();
  assert.equal((await state(page)).messages.at(-1).text,'Recovered answer'); assert.match((await state(page)).notice,/server may still/);
  await page.getByRole('button',{name:'Connection settings'}).click(); await page.locator('#server-url').fill('ftp://example.com');
  await page.getByRole('button',{name:'Save & connect'}).click(); assert.match(await page.locator('.settings-error').innerText(),/HTTP or HTTPS/);
  await page.keyboard.press('Escape');
  const downloadPromise = page.waitForEvent('download'); await page.getByRole('button',{name:'Download conversation'}).click(); assert.match((await downloadPromise).suggestedFilename(),/selfcare-.*\.md/);
  await fresh(); await page.setViewportSize({width:390,height:844});
  await page.screenshot({path:'.cache/ui-check/mobile.png'});
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth > innerWidth),false);
  assert.equal(await page.locator('.sidebar').evaluate(el=>el.getBoundingClientRect().right<=0),true);
  await page.getByRole('button',{name:'Open navigation'}).click(); assert.equal(await page.locator('.sidebar').evaluate(el=>el.getBoundingClientRect().left),0);
  await page.getByRole('button',{name:'Close navigation'}).click({position:{x:350,y:200}});
  assert.equal(await page.evaluate(()=>localStorage.length+sessionStorage.length),0);
  assert.deepEqual(foreign,[]); assert.deepEqual(errors,[]);
  assert.ok([403,404].includes((await page.request.get(origin + '/Cargo.toml')).status()));
  assert.ok([403,404].includes((await page.request.get(origin + '/.env')).status()));
  console.log(JSON.stringify({result:'PASS',checks:['local Alpine / no page errors','desktop and mobile layout','keyboard send / newline','follow-up context','all long-text sections / adaptive context split','TXT DOCX PDF extraction','unsupported files','safe Markdown','error / retry / stopped regeneration preserves reply','settings validation','export','session memory only','no CDN requests','private files not served'],apiRequests:calls.length}));
  } finally { await browser.close(); }
})().catch(error => {console.error(error);process.exit(1)});
