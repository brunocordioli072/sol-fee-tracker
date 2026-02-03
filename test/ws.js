import WebSocket from 'ws';

const ws = new WebSocket('ws://127.0.0.1:7000/ws');

ws.on('open', () => {
  console.log('Connected');
  // Subscribe to p50, p75, p98 for jito only
  ws.send(JSON.stringify({
    levels: [5000, 7500, 9800],
    processors: ['jito']
  }));
});

ws.on('message', (data) => {
  console.log(JSON.parse(data.toString()));
});

ws.on('close', () => {
  console.log('Disconnected');
});

ws.on('error', (err) => {
  console.error('Error:', err.message);
});
