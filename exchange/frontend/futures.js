/* ── Tredo Exchange - Futures Page ── */

async function loadFuturesPage() {
  if (!STATE.apiKey) return;

  // Load current settings
  try {
    const levData = await apiFetch(`/api/v1/futures/leverage/${STATE.user}/${encodeURIComponent(STATE.symbol)}`);
    const currentLev = levData.leverage || 1;
    document.getElementById('leverage-slider').value = currentLev;
    document.getElementById('leverage-val').textContent = `${currentLev}x`;
  } catch (e) {}

  try {
    const mmData = await apiFetch(`/api/v1/futures/margin_mode/${STATE.user}/${encodeURIComponent(STATE.symbol)}`);
    if (mmData.margin_mode) {
      document.getElementById('fut-margin-mode').value = mmData.margin_mode;
    }
  } catch (e) {}

  try {
    const pmData = await apiFetch(`/api/v1/futures/position_mode/${STATE.user}`);
    if (pmData.position_mode) {
      document.getElementById('fut-position-mode').value = pmData.position_mode;
    }
  } catch (e) {}

  // Load all positions
  loadAllPositionsTable();

  // Load all funding rates
  loadAllFundingRates();
}

// ── Set Margin Mode ──
async function setMarginMode(mode) {
  if (!STATE.apiKey) return;
  await apiFetch('/api/v1/futures/margin_mode', {
    method: 'POST',
    body: { user_id: STATE.user, symbol: STATE.symbol, margin_mode: mode },
  });
  toast(`Margin mode set to ${mode}`, 'success');
}

// ── Set Position Mode ──
async function setPositionMode(mode) {
  if (!STATE.apiKey) return;
  await apiFetch('/api/v1/futures/position_mode', {
    method: 'POST',
    body: { user_id: STATE.user, position_mode: mode },
  });
  toast(`Position mode set to ${mode}`, 'success');
}

// ── All Positions Table ──
async function loadAllPositionsTable() {
  try {
    const data = await apiFetch(`/api/v1/futures/positions/${STATE.user}`);
    const positions = data.positions || [];

    const table = document.getElementById('fut-positions-table');
    if (positions.length === 0) {
      table.innerHTML = '<div style="color:var(--text-muted);padding:12px;text-align:center">No open futures positions</div>';
      return;
    }

    table.innerHTML = `<table class="data-table">
      <thead><tr>
        <th>Symbol</th><th>Side</th><th>Size</th><th>Entry Price</th><th>Mark Price</th>
        <th>PnL</th><th>PnL%</th><th>Margin</th><th>Leverage</th><th>Liq. Price</th><th>Mode</th>
      </tr></thead><tbody>
      ${positions.map(p => {
        const pnlClass = p.unrealized_pnl >= 0 ? 'green' : 'red';
        const nearLiq = p.liquidation_price > 0 && p.mark_price > 0 &&
          Math.abs(p.mark_price - p.liquidation_price) / p.mark_price < 0.05;
        return `<tr>
          <td>${p.symbol}</td>
          <td class="${p.side === 'Long' ? 'green' : 'red'}">${p.side}</td>
          <td>${p.size.toFixed(4)}</td>
          <td>${formatPrice(p.entry_price)}</td>
          <td>${formatPrice(p.mark_price)}</td>
          <td class="${pnlClass}">${p.unrealized_pnl >= 0 ? '+' : ''}$${p.unrealized_pnl.toFixed(2)}</td>
          <td class="${pnlClass}">${p.pnl_percent >= 0 ? '+' : ''}${p.pnl_percent.toFixed(2)}%</td>
          <td>$${p.margin.toFixed(2)}</td>
          <td>${p.leverage}x</td>
          <td class="${nearLiq ? 'liq-warning' : ''}">${p.liquidation_price > 0 ? '$' + p.liquidation_price.toFixed(2) : '—'}</td>
          <td><span class="mode-badge ${p.margin_mode === 'Cross' ? 'cross' : 'isolated'}">${p.margin_mode}</span></td>
        </tr>`;
      }).join('')}
      </tbody></table>`;
  } catch (e) {}
}

// ── All Funding Rates ──
async function loadAllFundingRates() {
  try {
    const data = await apiFetch('/api/v1/exchange/info');
    const symbols = (data.symbols || []).map(s => s.symbol);

    const list = document.getElementById('fut-funding-list');
    if (symbols.length === 0) {
      list.innerHTML = '<div style="color:var(--text-muted);font-size:12px">No markets</div>';
      return;
    }

    let html = '<table class="data-table"><thead><tr><th>Symbol</th><th>Funding Rate</th><th>Next Settlement</th></tr></thead><tbody>';
    for (const sym of symbols) {
      try {
        await new Promise(r => setTimeout(r, 50)); // rate-limit burst
        const rateData = await apiFetch(`/api/v1/futures/funding/${encodeURIComponent(sym)}`);
        const rate = rateData.funding_rate || 0;
        const isPos = rate >= 0;
        html += `<tr>
          <td>${sym}</td>
          <td class="${isPos ? 'green' : 'red'}">${(rate * 100).toFixed(4)}%</td>
          <td style="color:var(--text-muted);font-family:var(--font-mono)">${rateData.next_funding_time ? new Date(rateData.next_funding_time).toLocaleTimeString() : '—'}</td>
        </tr>`;
      } catch (e) {
        html += `<tr><td>${sym}</td><td colspan="2" style="color:var(--text-muted)">—</td></tr>`;
      }
    }
    html += '</tbody></table>';
    list.innerHTML = html;
  } catch (e) {}
}
