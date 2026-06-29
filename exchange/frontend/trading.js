/* ── Tredo Exchange - Trading View ── */

// ── Symbol Select ──
function updateSubmitBtn() {
  const btn = document.getElementById('submit-order');
  const side = STATE.side.toLowerCase();
  btn.className = `submit-btn ${side}`;
  btn.textContent = `${STATE.side === 'Buy' ? 'Buy' : 'Sell'} ${STATE.symbol}`;
}

function setSide(s) {
  STATE.side = s;
  document.querySelectorAll('.side-tabs button').forEach(b => b.classList.remove('active'));
  document.getElementById(`side-${s}`).classList.add('active');
  updateSubmitBtn();
}

// ── Order Type ──
function setOrderType(t) {
  STATE.orderType = t;
  document.querySelectorAll('.order-type-tabs button').forEach(b => b.classList.remove('active'));
  document.querySelector(`.order-type-tabs button[data-otype="${t}"]`).classList.add('active');

  const showPrice = t === 'limit' || t === 'stoplimit' || t === 'takeprofit';
  const showTrigger = t === 'stop' || t === 'stoplimit' || t === 'takeprofit';
  const showTrailing = t === 'trailing';
  const showIceberg = t === 'limit';

  document.getElementById('order-price').closest('.form-row').style.display = showPrice ? '' : 'none';
  document.getElementById('trigger-row').style.display = showTrigger || showTrailing ? '' : 'none';
  document.getElementById('trailing-row').style.display = showTrailing ? '' : 'none';
  document.getElementById('iceberg-row').style.display = showIceberg ? '' : 'none';

  if (showTrailing) {
    document.querySelector('#trigger-row label').textContent = 'Initial Stop';
  } else if (showTrigger) {
    document.querySelector('#trigger-row label').textContent = 'Trigger';
  }
}

// ── Chart Intervals ──
function switchInterval(intv) {
  STATE.interval = intv;
  document.querySelectorAll('.chart-intervals button').forEach(b => b.classList.remove('active'));
  document.querySelector(`.chart-intervals button[data-int="${intv}"]`).classList.add('active');
  refreshChart();
}

// ── Order Book Grouping ──
function setObGrouping(btn, group) {
  document.querySelectorAll('.orderbook-panel .tabs button').forEach(b => b.classList.remove('active'));
  btn.classList.add('active');
  refreshOrderBook();
}

// ── Fill helpers ──
function fillBestPrice() {
  const book = STATE.orderBook;
  if (STATE.side === 'Buy') {
    const bestAsk = book.asks && book.asks.length > 0 ? book.asks[0].price : 0;
    document.getElementById('order-price').value = bestAsk;
  } else {
    const bestBid = book.bids && book.bids.length > 0 ? book.bids[0].price : 0;
    document.getElementById('order-price').value = bestBid;
  }
  updateTotal();
}

function fillMaxQty() {
  const base = STATE.symbol.split('/')[0];
  const bBal = STATE.balances.find(b => b.asset === base);
  if (bBal) {
    document.getElementById('order-qty').value = bBal.available.toFixed(6);
  }
}

function updateTotal() {
  const price = parseNum(document.getElementById('order-price').value);
  const qty = parseNum(document.getElementById('order-qty').value);
  document.getElementById('order-total').textContent = `Total: ${(price * qty).toFixed(2)} ${STATE.symbol.split('/')[1] || 'USD'}`;
}

// ── Leverage ──
let currentLeverage = 1;
async function onLeverageChange(val) {
  currentLeverage = parseInt(val);
  document.getElementById('leverage-val').textContent = `${currentLeverage}x`;
  if (STATE.apiKey) {
    await apiFetch('/api/v1/futures/leverage', {
      method: 'POST',
      body: { user_id: STATE.user, symbol: STATE.symbol, leverage: currentLeverage },
    });
  }
}

// ── Submit Order ──
async function submitOrder() {
  const price = parseNum(document.getElementById('order-price').value);
  const trigger = parseNum(document.getElementById('order-trigger').value);
  const qty = parseNum(document.getElementById('order-qty').value);
  const trail = parseNum(document.getElementById('order-trail').value);
  const visibleQty = parseNum(document.getElementById('order-iceberg').value);

  if (qty <= 0) { toast('Invalid quantity', 'error'); return; }

  let orderType;
  switch (STATE.orderType) {
    case 'limit': orderType = 'Limit'; break;
    case 'market': orderType = 'Market'; break;
    case 'stop': orderType = 'StopLoss'; break;
    case 'stoplimit': orderType = 'StopLimit'; break;
    case 'takeprofit': orderType = 'TakeProfit'; break;
    case 'trailing': orderType = 'TrailingStop'; break;
    default: orderType = 'Limit';
  }

  const body = {
    user_id: STATE.user,
    symbol: STATE.symbol,
    side: STATE.side,
    type: orderType,
    price: price > 0 ? price : null,
    trigger_price: trigger > 0 ? trigger : null,
    quantity: qty,
    time_in_force: 'Gtc',
  };

  if (trail > 0) body.trailing_delta = trail;
  if (visibleQty > 0 && STATE.orderType === 'limit') body.visible_quantity = visibleQty;

  const res = await apiFetch('/api/v1/orders', {
    method: 'POST',
    body,
  });

  if (res.order_id) {
    toast(`Order placed: ${res.status}`, 'success');
    refreshOpenOrders();
    loadBalances();
    refreshOrderBook();
    refreshTrades();
  } else {
    toast(`Order failed: ${res.msg || res.error || 'Unknown error'}`, 'error');
  }
}

// ── OCO Order ──
async function submitOco() {
  const side = document.getElementById('oco-side').value;
  const qty = parseNum(document.getElementById('oco-qty').value);
  const tpPrice = parseNum(document.getElementById('oco-tp-price').value);
  const slStop = parseNum(document.getElementById('oco-sl-stop').value);
  const slLimit = parseNum(document.getElementById('oco-sl-limit').value);

  if (qty <= 0 || tpPrice <= 0 || slStop <= 0) {
    toast('Fill in all OCO fields', 'error');
    return;
  }

  const res = await apiFetch('/api/v1/order/oco', {
    method: 'POST',
    body: {
      user_id: STATE.user,
      symbol: STATE.symbol,
      side,
      quantity: qty,
      price: tpPrice,
      stop_price: slStop,
      stop_limit_price: slLimit > 0 ? slLimit : null,
    },
  });

  if (res.oco_id) {
    toast(`OCO placed: ${res.orders ? res.orders.length : 0} legs`, 'success');
    refreshOpenOrders();
  } else {
    toast(`OCO failed: ${res.msg || 'Error'}`, 'error');
  }
}

// ── Cancel Order ──
async function cancelOrder(id) {
  const res = await apiFetch(`/api/v1/orders/${id}`, { method: 'DELETE' });
  if (res.id || res.order_id) {
    toast('Order cancelled', 'success');
    refreshOpenOrders();
    loadBalances();
  } else {
    toast('Cancel failed', 'error');
  }
}

// ── Refresh Order Book ──
async function refreshOrderBook() {
  try {
    const sym = STATE.symbol;
    const data = await apiFetch(`/api/v1/orderbook?symbol=${encodeURIComponent(sym)}&depth=15`);
    if (!data.bids || !data.asks) return;

    STATE.orderBook = data;

    const maxBidTotal = data.bids.reduce((m, l) => Math.max(m, l.price * l.quantity), 0);
    const maxAskTotal = data.asks.reduce((m, l) => Math.max(m, l.price * l.quantity), 0);
    const maxTotal = Math.max(maxBidTotal, maxAskTotal) || 1;

    const bestBid = data.bids.length > 0 ? data.bids[0].price : 0;
    const bestAsk = data.asks.length > 0 ? data.asks[0].price : 0;
    const spread = bestAsk - bestAsk > 0 ? bestAsk - bestBid : 0;
    const spreadPct = bestBid > 0 ? (spread / bestBid) * 100 : 0;

    // Render asks (reversed — lowest ask at bottom)
    const asksEl = document.getElementById('ob-asks');
    asksEl.innerHTML = data.asks.slice(0, 15).reverse().map(l =>
      `<div class="ob-row" onclick="document.getElementById('order-price').value=${l.price};updateTotal()">
        <span class="ob-price">${formatPrice(l.price)}</span>
        <span>${l.quantity.toFixed(4)}</span>
        <span>${(l.price * l.quantity).toFixed(2)}</span>
        <div class="ob-depth-bar" style="width:${(l.price * l.quantity / maxTotal * 100).toFixed(1)}%;background:var(--red)"></div>
      </div>`
    ).join('');

    // Spread
    document.getElementById('spread-price').textContent = `${formatPrice(spread)}`;
    document.getElementById('spread-pct').textContent = spreadPct > 0 ? `${spreadPct.toFixed(3)}%` : '—';

    // Render bids
    const bidsEl = document.getElementById('ob-bids');
    bidsEl.innerHTML = data.bids.slice(0, 15).map(l =>
      `<div class="ob-row" onclick="document.getElementById('order-price').value=${l.price};updateTotal()">
        <span class="ob-price">${formatPrice(l.price)}</span>
        <span>${l.quantity.toFixed(4)}</span>
        <span>${(l.price * l.quantity).toFixed(2)}</span>
        <div class="ob-depth-bar" style="width:${(l.price * l.quantity / maxTotal * 100).toFixed(1)}%;background:var(--green)"></div>
      </div>`
    ).join('');

    // Update chart price
    updateChartPrice(bestBid, bestAsk);
  } catch (e) { /* ignore */ }
}

// ── Refresh Trades ──
async function refreshTrades() {
  try {
    const sym = STATE.symbol;
    const data = await apiFetch(`/api/v1/trades?symbol=${encodeURIComponent(sym)}&limit=50`);
    const trades = data.trades || [];
    STATE.trades = trades;

    const list = document.getElementById('trades-list');
    const prevTotal = trades.length > 0 ? trades[0].total : 0;

    list.innerHTML = trades.slice(0, 30).map(t => {
      const isUp = t.price > prevTotal || (t.taker_side === 'Buy');
      return `<div class="trade-row">
        <span class="trade-price ${isUp ? 'green' : 'red'}">${formatPrice(t.price)}</span>
        <span>${t.quantity.toFixed(4)}</span>
        <span class="trade-time">${formatTime(t.timestamp)}</span>
      </div>`;
    }).join('');
  } catch (e) { /* ignore */ }
}

// ── Chart ──
async function refreshChart() {
  const canvas = document.getElementById('chart-canvas');
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  const rect = canvas.parentElement.getBoundingClientRect();
  canvas.width = rect.width * devicePixelRatio;
  canvas.height = rect.height * devicePixelRatio;
  canvas.style.width = rect.width + 'px';
  canvas.style.height = rect.height + 'px';
  ctx.scale(devicePixelRatio, devicePixelRatio);

  const w = rect.width;
  const h = rect.height;

  // Background
  ctx.fillStyle = '#181a20';
  ctx.fillRect(0, 0, w, h);

  try {
    const sym = STATE.symbol;
    const data = await apiFetch(`/api/v1/candles?symbol=${encodeURIComponent(sym)}&interval=${STATE.interval}&limit=100`);
    const candles = data.candles || [];

    if (candles.length === 0) {
      ctx.fillStyle = '#5e6673';
      ctx.font = '14px sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText('No chart data — place some trades to see candles', w/2, h/2);
      return;
    }

    const high = Math.max(...candles.map(c => c.high));
    const low = Math.min(...candles.map(c => c.low));
    const range = high - low || 1;
    const padding = 20;
    const candleW = Math.max(2, (w - padding * 2) / candles.length - 1);
    const isUp = candles[candles.length - 1].close >= candles[0].open;

    // Grid lines
    ctx.strokeStyle = '#2b3139';
    ctx.lineWidth = 1;
    for (let i = 0; i < 5; i++) {
      const y = padding + (h - padding * 2) * (i / 5);
      ctx.beginPath(); ctx.moveTo(padding, y); ctx.lineTo(w - padding, y); ctx.stroke();
      ctx.fillStyle = '#5e6673';
      ctx.font = '10px sans-serif';
      ctx.textAlign = 'right';
      ctx.fillText((high - range * (i / 5)).toFixed(2), padding - 4, y + 3);
    }

    // Candles
    candles.forEach((c, i) => {
      const x = padding + i * (candleW + 1);
      const openY = padding + (h - padding * 2) * ((high - c.open) / range);
      const closeY = padding + (h - padding * 2) * ((high - c.close) / range);
      const highY = padding + (h - padding * 2) * ((high - c.high) / range);
      const lowY = padding + (h - padding * 2) * ((high - c.low) / range);
      const isGreen = c.close >= c.open;

      ctx.strokeStyle = isGreen ? '#0ecb81' : '#f6465d';
      ctx.fillStyle = isGreen ? '#0ecb81' : '#f6465d';
      ctx.lineWidth = 1;

      // Wick
      ctx.beginPath();
      ctx.moveTo(x + candleW / 2, highY);
      ctx.lineTo(x + candleW / 2, lowY);
      ctx.stroke();

      // Body
      const bodyTop = Math.min(openY, closeY);
      const bodyH = Math.max(Math.abs(closeY - openY), 1);
      ctx.fillRect(x, bodyTop, candleW, bodyH);
    });

    // Last price line
    const lastClose = candles[candles.length - 1].close;
    const lastY = padding + (h - padding * 2) * ((high - lastClose) / range);
    ctx.strokeStyle = isUp ? '#0ecb81' : '#f6465d';
    ctx.lineWidth = 1;
    ctx.setLineDash([4, 4]);
    ctx.beginPath(); ctx.moveTo(padding, lastY); ctx.lineTo(w - padding, lastY); ctx.stroke();
    ctx.setLineDash([]);

    // Last price label
    ctx.fillStyle = isUp ? '#0ecb81' : '#f6465d';
    ctx.font = 'bold 12px monospace';
    ctx.textAlign = 'left';
    ctx.fillText(lastClose.toFixed(2), w - padding - 70, lastY - 4);

  } catch (e) {
    ctx.fillStyle = '#5e6673';
    ctx.font = '14px sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText('Loading chart...', w/2, h/2);
  }
}

function updateChartPrice(bid, ask) {
  const mid = bid > 0 && ask > 0 ? (bid + ask) / 2 : 0;
  const el = document.getElementById('chart-last-price');
  const changeEl = document.getElementById('chart-change');
  if (mid > 0) {
    el.textContent = formatPrice(mid);
    // Color based on change from last ticker update
    const prev = parseFloat(el.dataset.prev || '0');
    const isUp = prev > 0 ? mid >= prev : true;
    el.className = `chart-last-price ${isUp ? 'green' : 'red'}`;
    el.dataset.prev = mid;
    if (changeEl) {
      const change = prev > 0 ? ((mid - prev) / prev * 100) : 0;
      const absChange = prev > 0 ? (mid - prev) : 0;
      changeEl.textContent = `${absChange >= 0 ? '+' : ''}${absChange.toFixed(2)} (${change >= 0 ? '+' : ''}${change.toFixed(2)}%)`;
      changeEl.className = `chart-change ${change >= 0 ? 'green' : 'red'}`;
    }
  }
}

// ── Open Orders ──
async function refreshOpenOrders() {
  if (!STATE.apiKey) return;
  try {
    const data = await apiFetch(`/api/v1/orders/open/${STATE.user}`);
    STATE.openOrders = data.orders || [];

    // Render in the open orders tab
    const container = document.getElementById('open-orders-content');
    const orders = STATE.openOrders;
    if (orders.length === 0) {
      container.innerHTML = '<div style="color:var(--text-muted);padding:8px;text-align:center">No open orders</div>';
    } else {
      container.innerHTML = `<table class="data-table">
        <thead><tr>
          <th>Symbol</th><th>Side</th><th>Type</th><th>Price</th><th>Qty</th><th>Filled</th><th>Status</th><th></th>
        </tr></thead><tbody>
        ${orders.map(o => `
          <tr>
            <td>${o.symbol}</td>
            <td class="${o.side === 'Buy' ? 'green' : 'red'}">${o.side}</td>
            <td>${o.order_type}</td>
            <td>${o.price ? formatPrice(o.price) : 'MKT'}</td>
            <td>${o.quantity.toFixed(4)}</td>
            <td>${o.filled_quantity.toFixed(4)}</td>
            <td><span class="status-badge ${o.status.toLowerCase()}">${o.status}</span></td>
            <td><button class="cancel-btn" onclick="cancelOrder('${o.id}')">×</button></td>
          </tr>
        `).join('')}
        </tbody></table>`;
    }
  } catch (e) {}
}

// ── Switch Tabs ──
function switchTradeTab(tab) {
  document.querySelectorAll('#trading-bottom .tab-bar button').forEach(b => b.classList.remove('active'));
  document.querySelector(`#trading-bottom .tab-bar button[data-tab="${tab}"]`).classList.add('active');
  document.querySelectorAll('#trading-bottom .tab-content').forEach(t => t.classList.add('hidden'));
  document.getElementById(`tab-${tab}`).classList.remove('hidden');

  if (tab === 'orders') refreshOpenOrders();
  if (tab === 'history') refreshOrderHistory();
}

function switchPosTab(tab) {
  document.querySelectorAll('.positions-panel .tab-bar button').forEach(b => b.classList.remove('active'));
  document.querySelector(`.positions-panel .tab-bar button[data-tab="${tab}"]`).classList.add('active');
  document.querySelectorAll('.positions-panel .tab-content').forEach(t => t.classList.add('hidden'));
  document.getElementById(`tab-${tab}`).classList.remove('hidden');
}

// ── Order History ──
async function refreshOrderHistory() {
  if (!STATE.apiKey) return;
  try {
    const data = await apiFetch(`/api/v1/orders/history/${STATE.user}?limit=50`);
    const orders = data.orders || [];
    const container = document.getElementById('order-history-content');
    if (orders.length === 0) {
      container.innerHTML = '<div style="color:var(--text-muted);padding:8px;text-align:center">No order history</div>';
    } else {
      container.innerHTML = `<table class="data-table">
        <thead><tr>
          <th>Symbol</th><th>Side</th><th>Type</th><th>Price</th><th>Qty</th><th>Filled</th><th>Status</th><th>Time</th>
        </tr></thead><tbody>
        ${orders.slice(0, 50).map(o => `
          <tr>
            <td>${o.symbol}</td>
            <td class="${o.side === 'Buy' ? 'green' : 'red'}">${o.side}</td>
            <td>${o.order_type}</td>
            <td>${o.price ? formatPrice(o.price) : 'MKT'}</td>
            <td>${o.quantity.toFixed(4)}</td>
            <td>${o.filled_quantity.toFixed(4)}</td>
            <td><span class="status-badge ${o.status.toLowerCase()}">${o.status}</span></td>
            <td>${o.created_at ? formatTime(o.created_at) : '—'}</td>
          </tr>
        `).join('')}
        </tbody></table>`;
    }
  } catch (e) {}
}

// ── Positions ──
async function loadPositions() {
  if (!STATE.apiKey) return;
  try {
    const data = await apiFetch(`/api/v1/futures/positions/${STATE.user}`);
    STATE.positions = data.positions || [];
    const container = document.getElementById('positions-content');
    const pos = STATE.positions;
    if (pos.length === 0) {
      container.innerHTML = '<div style="color:var(--text-muted);padding:8px;text-align:center">No open positions</div>';
    } else {
      container.innerHTML = `<table class="data-table">
        <thead><tr>
          <th>Symbol</th><th>Side</th><th>Size</th><th>Entry</th><th>Mark</th><th>PnL</th><th>PnL%</th><th>Liq Price</th><th>Margin</th><th>Lev</th>
        </tr></thead><tbody>
        ${pos.map(p => {
          const pnlClass = p.unrealized_pnl >= 0 ? 'green' : 'red';
          const nearLiq = p.liquidation_price > 0 && Math.abs(p.mark_price - p.liquidation_price) / p.mark_price < 0.05;
          return `<tr>
            <td>${p.symbol}</td>
            <td class="${p.side === 'Long' ? 'green' : 'red'}">${p.side}</td>
            <td>${p.size.toFixed(4)}</td>
            <td>${formatPrice(p.entry_price)}</td>
            <td>${formatPrice(p.mark_price)}</td>
            <td class="${pnlClass}">${p.unrealized_pnl >= 0 ? '+' : ''}${p.unrealized_pnl.toFixed(2)}</td>
            <td class="${pnlClass}">${p.pnl_percent >= 0 ? '+' : ''}${p.pnl_percent.toFixed(2)}%</td>
            <td class="${nearLiq ? 'liq-warning' : ''}">${p.liquidation_price > 0 ? formatPrice(p.liquidation_price) : '—'}</td>
            <td>${p.margin.toFixed(2)}</td>
            <td>${p.leverage}x</td>
          </tr>`;
        }).join('')}
        </tbody></table>`;
    }
  } catch (e) {}
}

// ── Funding Rate ──
async function loadFundingRates() {
  if (!STATE.apiKey) return;
  try {
    const sym = STATE.symbol;
    const data = await apiFetch(`/api/v1/futures/funding/${encodeURIComponent(sym)}`);
    const el = document.getElementById('funding-content');
    if (data.funding_rate !== undefined) {
      const rate = data.funding_rate;
      const isPos = rate >= 0;
      el.innerHTML = `
        <div class="funding-box">
          <span class="funding-label">${sym} Funding Rate:</span>
          <span class="funding-value ${isPos ? 'green' : 'red'}">${(rate * 100).toFixed(4)}%</span>
          <span class="funding-label">Next Funding:</span>
          <span class="funding-value">${data.next_funding_time ? new Date(data.next_funding_time).toLocaleTimeString() : '—'}</span>
        </div>`;
    } else {
      el.innerHTML = '<div class="funding-box"><span class="funding-label">No funding data</span></div>';
    }
  } catch (e) {}
}

// ── Event listeners for total calculation ──
document.addEventListener('DOMContentLoaded', () => {
  ['order-price', 'order-qty'].forEach(id => {
    const el = document.getElementById(id);
    if (el) el.addEventListener('input', updateTotal);
  });
});
