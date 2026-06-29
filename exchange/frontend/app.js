/* ── Tredo Exchange - Core Application ── */

// ── State ──
const STATE = {
  user: 'alice',
  symbol: 'BTC/USD',
  side: 'Buy',
  orderType: 'limit',
  interval: '1m',
  apiKey: null,
  apiSecret: null,
  balances: [],
  ticker: null,
  orderBook: { bids: [], asks: [] },
  trades: [],
  candles: [],
  positions: [],
  openOrders: [],
  fundingRates: {},
};

// ── API Client ──
const API_BASE = window.location.origin;

async function apiFetch(path, options = {}) {
  const method = (options.method || 'GET').toUpperCase();
  const headers = { 'Content-Type': 'application/json', ...options.headers };

  // If we have API credentials, sign the request
  if (STATE.apiKey && STATE.apiSecret) {
    const nonce = Date.now().toString();
    const message = method + path + nonce;
    const sig = await hmacSha256(STATE.apiSecret, message);
    headers['X-API-Key'] = STATE.apiKey;
    headers['X-Nonce'] = nonce;
    headers['X-Signature'] = sig;
  }

  const res = await fetch(API_BASE + path, {
    method,
    headers,
    body: options.body ? JSON.stringify(options.body) : undefined,
  });
  return res.json();
}

// HMAC-SHA256 via Web Crypto API
async function hmacSha256(secret, message) {
  const enc = new TextEncoder();
  const key = await crypto.subtle.importKey(
    'raw', enc.encode(secret), { name: 'HMAC', hash: 'SHA-256' },
    false, ['sign']
  );
  const sig = await crypto.subtle.sign('HMAC', key, enc.encode(message));
  return Array.from(new Uint8Array(sig)).map(b => b.toString(16).padStart(2, '0')).join('');
}

// ── Toast Notifications ──
function toast(msg, type = 'info') {
  const c = document.getElementById('toast-container');
  const el = document.createElement('div');
  el.className = `toast ${type}`;
  el.textContent = msg;
  c.appendChild(el);
  setTimeout(() => { el.style.opacity = '0'; setTimeout(() => el.remove(), 300); }, 3000);
}

// ── Navigation ──
function switchPage(page) {
  document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));
  document.querySelectorAll('.nav a').forEach(a => a.classList.remove('active'));
  document.getElementById(`page-${page}`).classList.add('active');
  const navEl = document.querySelector(`.nav a[data-page="${page}"]`);
  if (navEl) navEl.classList.add('active');

  if (page === 'trade') {
    refreshOrderBook();
    refreshTrades();
    refreshChart();
    refreshOpenOrders();
    loadTicker();
    loadPositions();
    loadFundingRates();
  }
  if (page === 'futures') { loadFuturesPage(); }
  if (page === 'portfolio') { loadPortfolio(); }
}

function onUserChange(uid) {
  STATE.user = uid;
  // Get API key for this user
  generateApiKeyForUser(uid);
  loadBalances();
  refreshOpenOrders();
  loadPositions();
  loadPortfolio();
  toast(`Switched to user: ${uid}`, 'info');
}

// ── API Key Management ──
async function generateApiKeyForUser(uid) {
  // First try to load from localStorage
  const stored = localStorage.getItem(`apikey_${uid}`);
  if (stored) {
    const k = JSON.parse(stored);
    STATE.apiKey = k.api_key;
    STATE.apiSecret = k.secret_key;
    return;
  }
  // Generate new key via API
  try {
    const res = await apiFetch('/api/v1/auth/keys', {
      method: 'POST',
      body: { user_id: uid },
    });
    if (res.api_key) {
      STATE.apiKey = res.api_key;
      STATE.apiSecret = res.secret_key;
      localStorage.setItem(`apikey_${uid}`, JSON.stringify(res));
    }
  } catch (e) {
    console.warn('API key generation failed:', e);
  }
}

async function generateApiKey() {
  const uid = STATE.user;
  const res = await apiFetch('/api/v1/auth/keys', {
    method: 'POST',
    body: { user_id: uid },
  });
  if (res.api_key) {
    localStorage.setItem(`apikey_${uid}`, JSON.stringify(res));
    const div = document.getElementById('api-key-result');
    div.innerHTML = `
      <div class="api-key-display">
        <div class="key-label">API Key</div>
        <div class="key-value">${res.api_key}</div>
        <div class="key-label" style="margin-top:8px">Secret Key</div>
        <div class="key-value" style="color:var(--yellow)">${res.secret_key}</div>
        <div style="color:var(--red);margin-top:8px;font-size:11px">Store your secret key securely. It will not be shown again.</div>
      </div>
    `;
    STATE.apiKey = res.api_key;
    STATE.apiSecret = res.secret_key;
    toast('API key generated successfully', 'success');
  }
}

// ── SSE Stream ──
let sseSource = null;
function startSse() {
  if (sseSource) sseSource.close();
  sseSource = new EventSource(`/api/v1/stream?symbols=${STATE.symbol}`);
  sseSource.onmessage = (e) => {
    try {
      const data = JSON.parse(e.data);
      if (data.type === 'Trade' || data.event === 'Trade') {
        // Refresh trades and orderbook
        refreshTrades();
        refreshOrderBook();
        loadTicker();
      } else if (data.type === 'OrderBookUpdate' || data.event === 'OrderBookUpdate') {
        refreshOrderBook();
      } else if (data.type === 'OrderUpdate') {
        refreshOpenOrders();
        loadBalances();
      }
    } catch (err) {}
  };
  sseSource.onerror = () => {
    // Reconnect after a delay
    setTimeout(startSse, 3000);
  };
}

// ── Market Ticker Bar ──
async function loadTicker() {
  // First, load exchange info for available markets
  const info = await apiFetch('/api/v1/exchange/info');
  const tickers = await apiFetch('/api/v1/ticker/24hr');
  const tickersArr = Array.isArray(tickers) ? tickers : [];

  const bar = document.getElementById('ticker-bar');
  bar.innerHTML = '';

  // Add all known symbols from exchange info
  const symbols = info.symbols || [];
  symbols.forEach(mkt => {
    const t = tickersArr.find(t => t.symbol === mkt.symbol) || {};
    const price = t.last_price || 0;
    const change = t.price_change_percent || 0;
    const isUp = change >= 0;
    const isActive = mkt.symbol === STATE.symbol;
    const el = document.createElement('div');
    el.className = `ticker-item${isActive ? ' active' : ''}`;
    el.innerHTML = `
      <span class="ticker-symbol">${mkt.symbol.replace('/', '')}</span>
      <span class="ticker-price ${isUp || change === 0 ? 'green' : 'red'}">${formatPrice(price)}</span>
      <span class="ticker-change ${isUp || change === 0 ? 'green' : 'red'}">${change >= 0 ? '+' : ''}${change.toFixed(2)}%</span>
    `;
    el.onclick = () => selectSymbol(mkt.symbol);
    bar.appendChild(el);
  });
}

function selectSymbol(sym) {
  STATE.symbol = sym;
  document.getElementById('chart-symbol').textContent = sym;
  updateSubmitBtn();
  refreshOrderBook();
  refreshTrades();
  refreshChart();
  loadTicker();
  loadFundingRates();
  loadPositions();
  // Restart SSE with new symbol
  startSse();
}

// ── Balances ──
async function loadBalances() {
  if (!STATE.apiKey) return;
  try {
    const data = await apiFetch(`/api/v1/balances/${STATE.user}`);
    STATE.balances = data.balances || [];
    // Update balance hints in the order form
    updateBalanceHints();
  } catch (e) {}
}

function updateBalanceHints() {
  const base = STATE.symbol.split('/')[0];
  const quote = STATE.symbol.split('/')[1];
  const qBal = STATE.balances.find(b => b.asset === quote);
  const bBal = STATE.balances.find(b => b.asset === base);
  const qtyInput = document.getElementById('order-qty');
  if (qtyInput) {
    qtyInput.placeholder = `0.00 (${bBal ? bBal.available.toFixed(4) : '—'} ${base})`;
  }
}

// ── Utility ──
function formatPrice(p) {
  if (!p || p === 0) return '—';
  if (p >= 1000) return p.toFixed(2);
  if (p >= 1) return p.toFixed(4);
  return p.toFixed(6);
}

function formatTime(ts) {
  if (!ts) return '—';
  const d = new Date(ts);
  return d.toLocaleTimeString();
}

function parseNum(s) {
  if (!s) return 0;
  return parseFloat(s.replace(/,/g, '')) || 0;
}

// ── Init ──
(async function init() {
  await generateApiKeyForUser(STATE.user);
  loadBalances();
  loadTicker();
  refreshOrderBook();
  refreshTrades();
  refreshChart();
  refreshOpenOrders();
  loadPositions();
  loadFundingRates();
  startSse();
  updateSubmitBtn();
})();
