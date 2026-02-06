import WebSocket from 'ws';

const BASE_URL = 'http://127.0.0.1:7000';
const cmd = process.argv[2];

async function rpc(endpoint, params = {}) {
  const res = await fetch(`${BASE_URL}${endpoint}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, params: [params] })
  });
  return res.json();
}

async function testPercentiles() {
  const data = await rpc('/', { levels: [5000, 7500, 9800] });
  console.log(JSON.stringify(data, null, 2));
}

async function testWindow() {
  const data = await rpc('/window', { processors: ['jito'] });
  console.log(JSON.stringify(data, null, 2));
}

function testWs() {
  const ws = new WebSocket(`${BASE_URL.replace('http', 'ws')}/ws`);
  ws.on('open', () => {
    console.log('Connected');
    ws.send(JSON.stringify({ levels: [5000, 7500, 9800], processors: ['jito'] }));
  });
  ws.on('message', (data) => console.log(JSON.parse(data.toString())));
  ws.on('close', () => console.log('Disconnected'));
  ws.on('error', (err) => console.error('Error:', err.message));
}

async function testFeePercentiles() {
  const data = await rpc('/fees', { levels: [5000, 7500, 9800] });
  console.log(JSON.stringify(data, null, 2));
}

async function testFeeWindow() {
  const data = await rpc('/fees/window');
  console.log(JSON.stringify(data, null, 2));
}

function testFeeWs() {
  const ws = new WebSocket(`${BASE_URL.replace('http', 'ws')}/fees/ws`);
  ws.on('open', () => {
    console.log('Connected');
    ws.send(JSON.stringify({ levels: [5000, 8500, 9000] }));
  });
  ws.on('message', (data) => console.log(JSON.parse(data.toString())));
  ws.on('close', () => console.log('Disconnected'));
  ws.on('error', (err) => console.error('Error:', err.message));
}

const tests = {
  percentiles: testPercentiles,
  window: testWindow,
  ws: testWs,
  'fee-percentiles': testFeePercentiles,
  'fee-window': testFeeWindow,
  'fee-ws': testFeeWs,
};

if (!cmd || !tests[cmd]) {
  console.log('Usage: node index.js <percentiles|window|ws|fee-percentiles|fee-window|fee-ws>');
  process.exit(1);
}

tests[cmd]();
