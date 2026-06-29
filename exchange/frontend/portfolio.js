/* ── Tredo Exchange - Portfolio Page ── */

async function loadPortfolio() {
  if (!STATE.apiKey) return;
  loadPortfolioBalances();
  loadPortfolioOrders();
}

// ── Balances ──
async function loadPortfolioBalances() {
  try {
    const data = await apiFetch(`/api/v1/balances/${STATE.user}`);
    const balances = data.balances || [];
    STATE.balances = balances;

    const container = document.getElementById('portfolio-balances');
    if (balances.length === 0) {
      container.innerHTML = '<div style="color:var(--text-muted);padding:8px;text-align:center">No balances</div>';
      return;
    }

    const assetColors = {
      'USD': '#1e80ff',
      'BTC': '#f7931a',
      'ETH': '#627eea',
      'SOL': '#9945ff',
      'ADA': '#0033ad',
    };

    container.innerHTML = `<table class="data-table">
      <thead><tr>
        <th>Asset</th><th>Available</th><th>Locked</th><th>Total</th><th>Actions</th>
      </tr></thead><tbody>
      ${balances.map(b => `
        <tr>
          <td>
            <span class="balance-asset">
              <span class="asset-icon" style="background:${assetColors[b.asset] || '#5e6673'}">
                ${b.asset[0]}
              </span>
              ${b.asset}
            </span>
          </td>
          <td class="green">${b.available.toFixed(b.asset === 'BTC' ? 6 : b.asset === 'USD' ? 2 : 4)}</td>
          <td style="color:var(--text-secondary)">${b.locked.toFixed(2)}</td>
          <td style="font-weight:600">${b.total.toFixed(b.asset === 'BTC' ? 6 : 2)}</td>
          <td>
            <div class="btn-group">
              <button class="btn btn-primary btn-sm" onclick="quickDeposit('${b.asset}')">Deposit</button>
              <button class="btn btn-danger btn-sm" onclick="quickWithdraw('${b.asset}')">Withdraw</button>
            </div>
          </td>
        </tr>
      `).join('')}
      </tbody></table>`;

    // Update balance hints on trading page
    updateBalanceHints();
  } catch (e) {}
}

function quickDeposit(asset) {
  document.getElementById('deposit-asset').value = asset;
  document.getElementById('deposit-amount').value = '10000';
  window.scrollTo(0, document.body.scrollHeight);
}

function quickWithdraw(asset) {
  document.getElementById('withdraw-asset').value = asset;
  document.getElementById('withdraw-amount').value = '100';
  window.scrollTo(0, document.body.scrollHeight);
}

// ── Deposit ──
async function deposit() {
  const asset = document.getElementById('deposit-asset').value;
  const amount = parseNum(document.getElementById('deposit-amount').value);
  if (amount <= 0) { toast('Invalid amount', 'error'); return; }

  const res = await apiFetch('/api/v1/deposit', {
    method: 'POST',
    body: { user_id: STATE.user, asset, amount },
  });

  if (res.deposited) {
    toast(`Deposited ${res.deposited} ${asset}`, 'success');
    loadPortfolioBalances();
  } else {
    toast(`Deposit failed: ${res.error || 'Error'}`, 'error');
  }
}

// ── Withdraw ──
async function withdraw() {
  const asset = document.getElementById('withdraw-asset').value;
  const amount = parseNum(document.getElementById('withdraw-amount').value);
  if (amount <= 0) { toast('Invalid amount', 'error'); return; }

  const res = await apiFetch('/api/v1/withdraw', {
    method: 'POST',
    body: { user_id: STATE.user, asset, amount },
  });

  if (res.withdrawn) {
    toast(`Withdrew ${res.withdrawn} ${asset}`, 'success');
    loadPortfolioBalances();
  } else {
    toast(`Withdrawal failed: ${res.error || 'Error'}`, 'error');
  }
}

// ── Portfolio Open Orders ──
async function loadPortfolioOrders() {
  try {
    const data = await apiFetch(`/api/v1/orders/open/${STATE.user}`);
    const orders = data.orders || [];

    const container = document.getElementById('portfolio-orders');
    if (orders.length === 0) {
      container.innerHTML = '<div style="color:var(--text-muted);padding:8px;text-align:center">No open orders</div>';
      return;
    }

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
          <td><button class="cancel-btn" onclick="cancelOrder('${o.id}');loadPortfolioOrders();">×</button></td>
        </tr>
      `).join('')}
      </tbody></table>`;
  } catch (e) {}
}
