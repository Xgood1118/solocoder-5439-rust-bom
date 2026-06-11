// Custom R2 feishu write for 5439-rust-bom
// Per default-skip 4 automation (user prompt不含"执行自动化") — 4 fields 全部留空
// search-then-update, plain string, Windows GBK workaround via payload.json + curl
const fs = require('fs');
const path = require('path');
const https = require('https');
const { execFileSync } = require('child_process');

const FOLDER = '5439-rust-bom';
const GITHUB_URL = 'https://github.com/Xgood1118/solocoder-5439-rust-bom';
const COMMIT_SHA = '8811231e924c3703d9a2eae42bad97cadd513775';

const APP_ID = 'cli_a95fb8c174785cc9';
const APP_SECRET = 'gXVlL9GVkHPhMU90XCbwIgfpj5fMOYEK';
const APP_TOKEN = 'IkR2b7p55aXzHNst41AcxvoxnVb';
const TABLE_ID = 'tbldgCbC3v0MO8pv';

async function getToken() {
  return new Promise((resolve, reject) => {
    const data = JSON.stringify({ app_id: APP_ID, app_secret: APP_SECRET });
    const req = https.request({
      hostname: 'open.feishu.cn',
      path: '/open-apis/auth/v3/tenant_access_token/internal',
      method: 'POST',
      headers: { 'Content-Type': 'application/json; charset=utf-8', 'Content-Length': Buffer.byteLength(data) }
    }, res => {
      let body = ''; res.on('data', c => body += c);
      res.on('end', () => resolve(JSON.parse(body).tenant_access_token));
    });
    req.write(data); req.end();
  });
}

async function feishuCallSafe(token, method, p, body) {
  return new Promise((resolve, reject) => {
    const data = body ? JSON.stringify(body) : '';
    const req = https.request({
      hostname: 'open.feishu.cn',
      path: p, method: method,
      headers: {
        'Authorization': `Bearer ${token}`,
        'Content-Type': 'application/json; charset=utf-8',
        ...(data ? { 'Content-Length': Buffer.byteLength(data) } : {})
      }
    }, res => {
      let b = ''; res.on('data', c => b += b ? b + c : c);
      res.on('end', () => resolve(JSON.parse(b)));
    });
    if (data) req.write(data);
    req.end();
  });
}

(async () => {
  const token = await getToken();
  console.log('Token:', token.slice(0, 20) + '...');

  // R2 User Prompt = R1 NEXT_PROMPT (re-paste from Trae CN) per skill rule
  const nextPromptR1 = fs.readFileSync(`SoloCoder/${FOLDER}/NEXT_PROMPT_R1.txt`, 'utf8').trim();
  const dissatText = fs.readFileSync(`SoloCoder/${FOLDER}/tmp/5439-r2-dissat.txt`, 'utf8').trim();

  const fields = {
    "User Prompt": nextPromptR1,
    "轮次": "第二轮",
    "任务类型": "Bug修复",
    "业务领域": "工业软件",
    "修改范围": "全模块",
    "任务是否完成": "未完成任务",
    "产物及过程是否满意": "不满意",
    "不满意原因": dissatText,
    "github地址": GITHUB_URL,
    "commit id": COMMIT_SHA,
    "分支/文件夹": "main",
    // 4 automation fields all left empty per user default-skip
    "AI过程分析结果": null,
    "日志轨迹": null,
    "截图（userprompt附件/产物/运行结果/对话）": null,
    "Trae Session ID": null,
  };

  const filter = JSON.stringify({
    filter: {
      conjunction: "and",
      conditions: [
        { field_name: "github地址", operator: "contains", value: `solocoder-${FOLDER}` },
        { field_name: "轮次", operator: "is", value: "第二轮" }
      ]
    }
  });

  const searchRes = await feishuCallSafe(token, 'POST', `/open-apis/bitable/v1/apps/${APP_TOKEN}/tables/${TABLE_ID}/records/search`, { filter });
  console.log('Search result code:', searchRes.code, 'items:', searchRes.data && searchRes.data.items && searchRes.data.items.length);

  let recordId = null;
  let action = 'create';
  if (searchRes.code === 0 && searchRes.data && searchRes.data.items && searchRes.data.items.length > 0) {
    recordId = searchRes.data.items[0].record_id;
    action = 'update';
    console.log('Found existing R2 record, will UPDATE:', recordId);
  } else {
    console.log('No R2 record found, creating new...');
  }

  const url = action === 'update'
    ? `https://open.feishu.cn/open-apis/bitable/v1/apps/${APP_TOKEN}/tables/${TABLE_ID}/records/${recordId}`
    : `https://open.feishu.cn/open-apis/bitable/v1/apps/${APP_TOKEN}/tables/${TABLE_ID}/records`;
  const method = action === 'update' ? 'PUT' : 'POST';

  const body = JSON.stringify({ fields });
  const payloadFile = `SoloCoder/${FOLDER}/tmp/_5439-r2-feishu-payload.json`;
  fs.writeFileSync(payloadFile, body, 'utf8');
  console.log('Wrote payload to', payloadFile, 'size:', body.length);

  const curlArgs = [
    '-s', '-X', method, url,
    '-H', `Authorization: Bearer ${token}`,
    '-H', 'Content-Type: application/json; charset=utf-8',
    '-d', `@${payloadFile}`
  ];

  let curlOutput;
  try {
    curlOutput = execFileSync('curl', curlArgs, { encoding: 'utf8', maxBuffer: 10 * 1024 * 1024 });
  } catch (e) {
    console.log('curl err:', e.message);
    process.exit(1);
  }

  console.log(`${action} result:`, curlOutput.slice(0, 600));
  const parsed = JSON.parse(curlOutput);
  if (parsed.code !== 0) {
    console.log('FAILED. Full:', JSON.stringify(parsed).slice(0, 1000));
    process.exit(1);
  }
  if (action === 'create' && parsed.data && parsed.data.record) {
    recordId = parsed.data.record.record_id;
  }
  console.log('Final record_id:', recordId);
  console.log('Action:', action);
})();
