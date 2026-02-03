const response = await fetch('http://127.0.0.1:7000', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    jsonrpc: '2.0',
    id: 1,
    params: [{ levels: [5000, 7500, 9800] }]
  })
});

const data = await response.json();
console.log(JSON.stringify(data, null, 2));
