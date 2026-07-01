const API_BASE = 'http://127.0.0.1:8080/api/v1';
const WS_URL = 'ws://127.0.0.1:8080/api/v1/ws';
const USER_ID = 'alice';

// ── Validation Constants ────────────────────────────────────────────────────
const MIN_ORDER_VALUE = 10;        // Minimum order value in quote currency (e.g., $10 USDT)
const MAX_ORDER_VALUE = 1000000;   // Maximum order value
const MIN_QUANTITY_STEP = 0.00001; // Minimum quantity increment
const PRICE_PRECISION = 2;         // Decimal places for price display
const QTY_PRECISION = 6;           // Decimal places for quantity display
const MAX_RETRY_ATTEMPTS = 3;
const RETRY_DELAY_MS = 1000;

let currentMarketTab = 'crypto';
let currentTradeSymbol = 'BTC/USDT';
let allMarkets = [];
let chart = null;
let lineSeries = null;
let ws = null;
let wsRetryCount = 0;
let pendingOrders = new Set(); // Track in-flight order requests

// ── Network State ───────────────────────────────────────────────────────────
let networkStatus = 'online'; // 'online' | 'degraded' | 'offline'
let lastSuccessfulFetch = Date.now();

// ── Initialization ──────────────────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', () => {
    switchPage('markets');
    initApp();
    initNetworkMonitor();
});

function initNetworkMonitor() {
    window.addEventListener('online', () => {
        networkStatus = 'online';
        updateNetworkIndicator();
        toast('Network connection restored', 'success');
        // Retry failed fetches
        fetchMarkets();
        fetchPortfolio();
    });
    window.addEventListener('offline', () => {
        networkStatus = 'offline';
        updateNetworkIndicator();
        toast('Network connection lost. Working offline.', 'error');
    });
}

function updateNetworkIndicator() {
    const dot = document.getElementById('statusDot');
    if (!dot) return;
    switch (networkStatus) {
        case 'online':
            dot.style.color = 'var(--trading-up)';
            dot.title = 'Connected';
            break;
        case 'degraded':
            dot.style.color = 'var(--primary)';
            dot.title = 'Degraded connection';
            break;
        case 'offline':
            dot.style.color = 'var(--trading-down)';
            dot.title = 'Offline';
            break;
    }
}

// ── Toast Notifications ─────────────────────────────────────────────────────
function toast(message, type = 'info') {
    const container = document.getElementById('toastContainer');
    if (!container) return;
    const t = document.createElement('div');
    t.className = `toast toast-${type}`;
    t.innerHTML = `<span>${escapeHtml(message)}</span>`;
    container.appendChild(t);
    setTimeout(() => { t.remove(); }, 4000);
}

// ── HTML Escaping (XSS Prevention) ──────────────────────────────────────────
function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

// ── Safe Number Parsing ─────────────────────────────────────────────────────
function safeParseFloat(val, fallback = 0) {
    if (val === null || val === undefined || val === '') return fallback;
    const num = parseFloat(String(val).replace(/[^0-9.\-]/g, ''));
    return isNaN(num) ? fallback : num;
}

function sanitizeNumericInput(input) {
    // Remove everything except digits, dots, and leading minus
    let val = input.value;
    val = val.replace(/[^0-9.]/g, '');
    // Only allow one decimal point
    const parts = val.split('.');
    if (parts.length > 2) val = parts[0] + '.' + parts.slice(1).join('');
    input.value = val;
}

// ── Fetch with Retry ────────────────────────────────────────────────────────
async function fetchWithRetry(url, options = {}, retries = MAX_RETRY_ATTEMPTS) {
    for (let attempt = 1; attempt <= retries; attempt++) {
        try {
            const res = await fetch(url, options);
            if (res.ok) {
                lastSuccessfulFetch = Date.now();
                networkStatus = 'online';
                updateNetworkIndicator();
                return res;
            }
            // Don't retry client errors (4xx)
            if (res.status >= 400 && res.status < 500) {
                return res;
            }
            // Retry server errors (5xx)
            if (attempt < retries) {
                await new Promise(r => setTimeout(r, RETRY_DELAY_MS * attempt));
                continue;
            }
            return res;
        } catch (e) {
            if (attempt === retries) {
                networkStatus = 'offline';
                updateNetworkIndicator();
                throw e;
            }
            await new Promise(r => setTimeout(r, RETRY_DELAY_MS * attempt));
        }
    }
}

// ── Navigation ──────────────────────────────────────────────────────────────
window.switchPage = function (pageId) {
    document.querySelectorAll('.exchange-page').forEach(p => p.classList.remove('active'));
    document.querySelectorAll('.top-nav-item').forEach(p => p.classList.remove('active'));

    const pageEl = document.getElementById(`page-${pageId}`);
    const navEl = document.querySelector(`.top-nav-item[data-page="${pageId}"]`);

    if (pageEl) pageEl.classList.add('active');
    if (navEl) navEl.classList.add('active');

    if (pageId === 'trade') {
        setTimeout(() => initChart(), 100);
    }
    if (pageId === 'options') renderOptionsChain();
};

let portfolioRefreshInterval = null;
const PORTFOLIO_REFRESH_MS = 10000; // Refresh portfolio every 10s

async function initApp() {
    updateClock();
    setInterval(updateClock, 1000);

    await fetchMarkets();
    await fetchPortfolio();
    fetchOpenOrders();
    initWebSocket();

    // Periodic portfolio refresh
    portfolioRefreshInterval = setInterval(() => {
        fetchPortfolio();
        fetchOpenOrders();
    }, PORTFOLIO_REFRESH_MS);

    // Cleanup on page unload
    window.addEventListener('beforeunload', () => {
        if (portfolioRefreshInterval) clearInterval(portfolioRefreshInterval);
    });
}

function updateClock() {
    const label = document.getElementById('sessionLabel');
    if (label) label.textContent = new Date().toISOString().substring(11, 19) + ' UTC';
}

// ══════════════════════════════════════════════════════════════════════════════
// MARKETS PAGE
// ══════════════════════════════════════════════════════════════════════════════
async function fetchMarkets() {
    try {
        const res = await fetchWithRetry(`${API_BASE}/ticker/24hr`);
        if (res.ok) {
            allMarkets = await res.json();
            if (allMarkets.length > 0) {
                const btc = allMarkets.find(m => m.symbol === 'BTC/USDT');
                currentTradeSymbol = btc ? btc.symbol : allMarkets[0].symbol;
            }
            filterMarkets();
            populateTradeSymbolSelect();
        } else {
            console.error('Markets fetch failed:', res.status);
        }
    } catch (e) {
        console.error('Error fetching markets:', e);
        // Show error state in markets table
        const tbody = document.getElementById('marketBody');
        if (tbody) {
            tbody.innerHTML = `
                <div class="empty-state" style="padding:32px">
                    <i class="fas fa-wifi" style="color:var(--trading-down)"></i>
                    <div class="empty-state-text">Unable to load markets</div>
                    <button class="exchange-btn exchange-btn-secondary exchange-btn-sm" onclick="fetchMarkets()" style="margin-top:8px">
                        <i class="fas fa-redo"></i> Retry
                    </button>
                </div>
            `;
        }
    }
}

window.switchMarket = function (market, btnEl) {
    currentMarketTab = market;
    document.querySelectorAll('.markets-tab').forEach(b => b.classList.remove('active'));
    if (btnEl) btnEl.classList.add('active');
    filterMarkets();
};

window.filterMarkets = function () {
    const search = document.getElementById('marketSearch')?.value.toLowerCase() || '';
    const tbody = document.getElementById('marketBody');
    if (!tbody) return;

    let filtered = allMarkets;
    if (currentMarketTab !== 'all') {
        if (currentMarketTab === 'crypto') {
            filtered = allMarkets.filter(m => m.symbol.endsWith('/USDT') || m.symbol.endsWith('/USD'));
        } else {
            filtered = [];
        }
    }

    if (search) {
        filtered = filtered.filter(m => m.symbol.toLowerCase().includes(search));
    }

    const countEl = document.getElementById('marketCount');
    if (countEl) countEl.textContent = `${filtered.length} assets`;

    if (filtered.length === 0) {
        tbody.innerHTML = `
            <div class="empty-state" style="padding:32px">
                <i class="fas fa-search"></i>
                <div class="empty-state-text">No markets found</div>
            </div>
        `;
        return;
    }

    tbody.innerHTML = filtered.map(m => {
        const color = m.price_change_percent >= 0 ? 'var(--trading-up)' : 'var(--trading-down)';
        const sign = m.price_change_percent >= 0 ? '+' : '';
        const symbol = escapeHtml(m.symbol);
        const name = escapeHtml(m.symbol.split('/')[0]);
        const safeSymbol = encodeURIComponent(m.symbol);
        return `
            <div class="markets-table-row" style="display:flex; cursor:pointer" data-symbol="${safeSymbol}" onclick="goToTrade(decodeURIComponent(this.dataset.symbol))">
                <span style="width:80px;font-weight:600">${symbol}</span>
                <span style="flex:1;color:var(--muted)">${name}</span>
                <span style="width:50px;text-align:center">Spot</span>
                <span style="width:120px;text-align:right">$${m.last_price.toFixed(2)}</span>
                <span style="width:90px;text-align:right;color:${color}">${sign}${m.price_change_percent.toFixed(2)}%</span>
                <span style="width:100px;text-align:right">${m.volume.toFixed(2)}</span>
            </div>
        `;
    }).join('');
};

window.goToTrade = function (symbol) {
    currentTradeSymbol = symbol;
    const sel = document.getElementById('tradeSymbolSelect');
    if (sel) sel.value = symbol;
    switchPage('trade');
    switchTradeSymbol();
};

// ══════════════════════════════════════════════════════════════════════════════
// TRADE PAGE
// ══════════════════════════════════════════════════════════════════════════════
function populateTradeSymbolSelect() {
    const sel = document.getElementById('tradeSymbolSelect');
    if (!sel) return;
    sel.innerHTML = allMarkets.map(m => `<option value="${escapeHtml(m.symbol)}">${escapeHtml(m.symbol)}</option>`).join('');
    sel.value = currentTradeSymbol;
    document.getElementById('tradePair').textContent = currentTradeSymbol;
}

window.switchTradeSymbol = function () {
    const sel = document.getElementById('tradeSymbolSelect');
    if (sel) currentTradeSymbol = sel.value;
    const pairLabel = document.getElementById('tradePair');
    if (pairLabel) pairLabel.textContent = currentTradeSymbol;
    clearValidationErrors();
    fetchOrderbook();
    fetchChartData();
    fetchPortfolio();
};

let tradeSide = 'buy';
window.switchTradeTab = function (side) {
    tradeSide = side;
    document.getElementById('tradeTabBuy').classList.toggle('active', side === 'buy');
    document.getElementById('tradeTabSell').classList.toggle('active', side === 'sell');
    // Remove sell active class from buy tab and vice versa
    document.getElementById('tradeTabBuy').classList.toggle('active-sell', side === 'sell');
    document.getElementById('tradeTabSell').classList.toggle('active-sell', side === 'sell');
    const btn = document.getElementById('tradeSubmitBtn');
    if (btn) {
        btn.className = `trade-submit-btn ${side}`;
        btn.textContent = `${side.toUpperCase()} ${currentTradeSymbol.split('/')[0]}`;
    }
    clearValidationErrors();
    fetchPortfolio();
};

// ── Input Sanitization ──────────────────────────────────────────────────────
window.updateTradeTotal = function () {
    const price = safeParseFloat(document.getElementById('tradePriceInput')?.value);
    const amount = safeParseFloat(document.getElementById('tradeAmount')?.value);
    const totalEl = document.getElementById('tradeTotal');
    if (totalEl) totalEl.textContent = `$${(price * amount).toFixed(2)}`;
    // Clear validation error when user types
    clearFieldError('tradeAmount');
    clearFieldError('tradePriceInput');
};

window.setTradePercent = function (pct) {
    const balStr = document.getElementById('tradeAvailable')?.textContent || "0";
    // Parse balance - remove currency label
    const avail = safeParseFloat(balStr.split(' ')[0]);
    const price = safeParseFloat(document.getElementById('tradePriceInput')?.value);
    const amountEl = document.getElementById('tradeAmount');
    if (!amountEl) return;

    if (price > 0 && tradeSide === 'buy') {
        amountEl.value = ((avail * (pct / 100)) / price).toFixed(QTY_PRECISION);
    } else {
        amountEl.value = (avail * (pct / 100)).toFixed(QTY_PRECISION);
    }
    updateTradeTotal();
};

// ══════════════════════════════════════════════════════════════════════════════
// ORDER FORM VALIDATION
// ══════════════════════════════════════════════════════════════════════════════

// Show inline validation error
function showFieldError(fieldId, message) {
    const field = document.getElementById(fieldId);
    if (!field) return;
    field.classList.add('trade-input-error');
    // Remove existing error message
    clearFieldError(fieldId);
    // Add error message
    const errorDiv = document.createElement('div');
    errorDiv.className = 'trade-field-error';
    errorDiv.id = `${fieldId}-error`;
    errorDiv.textContent = message;
    field.parentNode.appendChild(errorDiv);
}

function clearFieldError(fieldId) {
    const field = document.getElementById(fieldId);
    if (field) field.classList.remove('trade-input-error');
    const error = document.getElementById(`${fieldId}-error`);
    if (error) error.remove();
}

function clearValidationErrors() {
    document.querySelectorAll('.trade-input-error').forEach(el => el.classList.remove('trade-input-error'));
    document.querySelectorAll('.trade-field-error').forEach(el => el.remove());
}

// Validate the order form, returns { valid, order, errors }
function validateOrderForm() {
    const errors = [];
    const orderType = document.getElementById('tradeOrderType')?.value || 'market';
    const priceInput = document.getElementById('tradePriceInput');
    const amountInput = document.getElementById('tradeAmount');
    const price = safeParseFloat(priceInput?.value);
    const amount = safeParseFloat(amountInput?.value);

    // ── Validate Amount ──────────────────────────────────────────────────
    if (!amountInput?.value || amount <= 0) {
        showFieldError('tradeAmount', 'Please enter a valid amount greater than 0');
        errors.push('Invalid amount');
    } else if (amount < MIN_QUANTITY_STEP) {
        showFieldError('tradeAmount', `Minimum order quantity is ${MIN_QUANTITY_STEP}`);
        errors.push('Amount too small');
    }

    // ── Validate Price ───────────────────────────────────────────────────
    if (orderType !== 'market') {
        if (!priceInput?.value || price <= 0) {
            showFieldError('tradePriceInput', 'Please enter a valid price greater than 0');
            errors.push('Invalid price');
        } else if (orderType === 'stop' && price <= safeParseFloat(document.getElementById('tradePrice')?.textContent?.replace(/,/g, ''))) {
            // For stop orders, stop price should be below market for buy, above for sell
            // This is just a warning, not a hard error
        }
    }

    // ── Validate Order Value ─────────────────────────────────────────────
    const orderValue = price * amount;
    if (orderType !== 'market' && orderValue < MIN_ORDER_VALUE) {
        showFieldError('tradeAmount', `Minimum order value is $${MIN_ORDER_VALUE}`);
        errors.push('Order value too small');
    } else if (orderValue > MAX_ORDER_VALUE) {
        showFieldError('tradeAmount', `Maximum order value is $${MAX_ORDER_VALUE.toLocaleString()}`);
        errors.push('Order value too large');
    }

    // ── Validate Available Balance ───────────────────────────────────────
    const balStr = document.getElementById('tradeAvailable')?.textContent || "0";
    const avail = safeParseFloat(balStr.split(' ')[0]);
    if (tradeSide === 'buy' && orderType !== 'market') {
        if (orderValue > avail) {
            showFieldError('tradeAmount', `Insufficient balance. Available: $${avail.toFixed(2)}`);
            errors.push('Insufficient balance');
        }
    } else if (tradeSide === 'sell') {
        if (amount > avail) {
            showFieldError('tradeAmount', `Insufficient ${currentTradeSymbol.split('/')[0]} balance. Available: ${avail.toFixed(QTY_PRECISION)}`);
            errors.push('Insufficient balance');
        }
    }

    // ── Validate Symbol ──────────────────────────────────────────────────
    if (!currentTradeSymbol || !currentTradeSymbol.includes('/')) {
        errors.push('Invalid trading pair');
    }

    // ── Validate Order Type ──────────────────────────────────────────────
    if (!['market', 'limit', 'stop'].includes(orderType)) {
        errors.push('Invalid order type');
    }

    const isValid = errors.length === 0;

    const order = isValid ? {
        user_id: USER_ID,
        symbol: currentTradeSymbol,
        side: tradeSide.charAt(0).toUpperCase() + tradeSide.slice(1),
        order_type: orderType.charAt(0).toUpperCase() + orderType.slice(1),
        price: orderType === 'market' ? undefined : price,
        quantity: amount
    } : null;

    return { valid: isValid, order, errors };
}

// ══════════════════════════════════════════════════════════════════════════════
// ORDER CONFIRMATION MODAL
// ══════════════════════════════════════════════════════════════════════════════

window.showOrderConfirmation = function (order) {
    const modal = document.getElementById('orderConfirmModal');
    if (!modal) return;

    const baseAsset = order.symbol.split('/')[0];
    const quoteAsset = order.symbol.split('/')[1];
    const isBuy = order.side === 'Buy';
    const total = (order.price || 0) * order.quantity;

    document.getElementById('confirmSide').textContent = order.side;
    document.getElementById('confirmSide').className = `confirm-value ${isBuy ? 'text-up' : 'text-down'}`;
    document.getElementById('confirmSymbol').textContent = order.symbol;
    document.getElementById('confirmType').textContent = order.order_type;
    document.getElementById('confirmPrice').textContent = order.price ? `$${order.price.toFixed(PRICE_PRECISION)}` : 'Market';
    document.getElementById('confirmAmount').textContent = `${order.quantity.toFixed(QTY_PRECISION)} ${baseAsset}`;
    document.getElementById('confirmTotal').textContent = order.order_type === 'Market' ? 'Market Price' : `$${total.toFixed(2)}`;

    modal.classList.add('active');

    // Store order for confirmation
    modal.dataset.pendingOrder = JSON.stringify(order);
};

window.closeOrderConfirmation = function () {
    const modal = document.getElementById('orderConfirmModal');
    if (modal) {
        modal.classList.remove('active');
        modal.dataset.pendingOrder = '';
    }
};

window.confirmAndSubmitOrder = async function () {
    const modal = document.getElementById('orderConfirmModal');
    const orderStr = modal?.dataset.pendingOrder;
    if (!orderStr) return;

    const order = JSON.parse(orderStr);
    closeOrderConfirmation();
    await executeOrder(order);
};

// ══════════════════════════════════════════════════════════════════════════════
// ORDER SUBMISSION
// ══════════════════════════════════════════════════════════════════════════════

async function executeOrder(order) {
    // Prevent duplicate submissions
    if (pendingOrders.has(order.symbol + order.side)) {
        toast('Order already being processed...', 'info');
        return;
    }

    pendingOrders.add(order.symbol + order.side);

    // Update button state
    const btn = document.getElementById('tradeSubmitBtn');
    if (btn) {
        btn.disabled = true;
        btn.innerHTML = '<div class="loading-spinner" style="width:16px;height:16px;border-width:2px;margin:0 auto"></div>';
    }

    try {
        const res = await fetchWithRetry(`${API_BASE}/orders`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(order)
        }, 1); // Don't retry order submissions (idempotency issues)

        if (res.ok) {
            const result = await res.json().catch(() => ({}));
            toast(`✓ ${order.side} order placed for ${order.quantity.toFixed(QTY_PRECISION)} ${order.symbol.split('/')[0]}`, 'success');
            // Reset form
            document.getElementById('tradeAmount').value = '';
            document.getElementById('tradeTotal').textContent = '$0.00';
            // Refresh data
            setTimeout(() => {
                fetchPortfolio();
                fetchOrderbook();
            }, 100);
        } else {
            let errMsg = 'Unknown error';
            try {
                const errText = await res.text();
                errMsg = errText || `HTTP ${res.status}`;
            } catch (_) {
                errMsg = `HTTP ${res.status}`;
            }
            toast(`✗ Order failed: ${errMsg}`, 'error');
        }
    } catch (e) {
        if (e.name === 'TypeError' && e.message.includes('fetch')) {
            toast('✗ Network error. Please check your connection and try again.', 'error');
        } else {
            toast(`✗ Error: ${e.message}`, 'error');
        }
    } finally {
        pendingOrders.delete(order.symbol + order.side);
        if (btn) {
            btn.disabled = false;
            btn.textContent = `${order.side.toUpperCase()} ${order.symbol.split('/')[0]}`;
        }
    }
}

window.submitTrade = async function () {
    // Clear previous errors
    clearValidationErrors();

    // Validate form
    const { valid, order, errors } = validateOrderForm();
    if (!valid) {
        if (errors.length > 0) {
            toast(errors[0], 'error');
        }
        return;
    }

    // Show confirmation modal
    showOrderConfirmation(order);
};

// ══════════════════════════════════════════════════════════════════════════════
// ORDER BOOK
// ══════════════════════════════════════════════════════════════════════════════
async function fetchOrderbook() {
    try {
        const res = await fetchWithRetry(`${API_BASE}/orderbook?symbol=${encodeURIComponent(currentTradeSymbol)}&depth=10`);
        if (!res.ok) return;
        const ob = await res.json();

        const asksEl = document.getElementById('orderbookAsks');
        const bidsEl = document.getElementById('orderbookBids');    if (asksEl && ob.asks) {
        asksEl.innerHTML = ob.asks.slice(0, 10).reverse().map(a => {
            const priceStr = a.price.toFixed(PRICE_PRECISION);
            return `
                <div style="display:flex; justify-content:space-between; position:relative; padding:2px 4px; font-family:'JetBrains Mono', monospace; font-size:12px; color:var(--trading-down); cursor:pointer;" onclick="document.getElementById('tradePriceInput').value='${priceStr}'">
                    <span style="z-index:2; width:33%">${priceStr}</span>
                    <span style="z-index:2; width:33%; text-align:right">${a.quantity.toFixed(QTY_PRECISION)}</span>
                    <span style="z-index:2; width:33%; text-align:right">${(a.price * a.quantity).toFixed(2)}</span>
                    <div style="position:absolute; top:0; right:0; height:100%; width: ${Math.min(100, (a.quantity / 5) * 100)}%; background:rgba(239,68,68,0.1); z-index:1;"></div>
                </div>
            `;
        }).join('');
    }

    if (bidsEl && ob.bids) {
        bidsEl.innerHTML = ob.bids.slice(0, 10).map(b => {
            const priceStr = b.price.toFixed(PRICE_PRECISION);
            return `
                <div style="display:flex; justify-content:space-between; position:relative; padding:2px 4px; font-family:'JetBrains Mono', monospace; font-size:12px; color:var(--trading-up); cursor:pointer;" onclick="document.getElementById('tradePriceInput').value='${priceStr}'">
                    <span style="z-index:2; width:33%">${priceStr}</span>
                    <span style="z-index:2; width:33%; text-align:right">${b.quantity.toFixed(QTY_PRECISION)}</span>
                    <span style="z-index:2; width:33%; text-align:right">${(b.price * b.quantity).toFixed(2)}</span>
                    <div style="position:absolute; top:0; right:0; height:100%; width: ${Math.min(100, (b.quantity / 5) * 100)}%; background:rgba(16,185,129,0.1); z-index:1;"></div>
                </div>
            `;
        }).join('');
    }

        const bestAsk = ob.asks?.[0]?.price || 0;
        const bestBid = ob.bids?.[0]?.price || 0;
        const spreadEl = document.getElementById('orderbookSpread');
        if (spreadEl && bestAsk && bestBid) {
            const spread = bestAsk - bestBid;
            const spreadPct = (spread / bestBid) * 100;
            spreadEl.textContent = `Spread: ${spread.toFixed(2)} (${spreadPct.toFixed(2)}%)`;
        }
    } catch (e) {
        console.error('Orderbook fetch error:', e);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// CHART
// ══════════════════════════════════════════════════════════════════════════════
function initChart() {
    const container = document.getElementById('chartContainer');
    if (!container || chart) return;
    container.innerHTML = '';
    chart = LightweightCharts.createChart(container, {
        width: container.clientWidth,
        height: container.clientHeight,
        layout: { background: { color: 'transparent' }, textColor: '#94a3b8' },
        grid: { vertLines: { color: '#1e293b' }, horzLines: { color: '#1e293b' } },
        rightPriceScale: { borderColor: '#1e293b' },
        timeScale: { borderColor: '#1e293b', timeVisible: true }
    });
    lineSeries = chart.addCandlestickSeries({
        upColor: '#10b981', downColor: '#ef4444',
        borderVisible: false, wickUpColor: '#10b981', wickDownColor: '#ef4444'
    });

    new ResizeObserver(entries => {
        if (entries.length === 0 || entries[0].target !== container) return;
        const newRect = entries[0].contentRect;
        chart.applyOptions({ height: newRect.height, width: newRect.width });
    }).observe(container);

    fetchChartData();
}

async function fetchChartData() {
    if (!lineSeries) return;
    try {
        const res = await fetchWithRetry(`${API_BASE}/candles?symbol=${encodeURIComponent(currentTradeSymbol)}&interval=1m&limit=100`);
        if (!res.ok) return;
        const data = await res.json();
        const chartData = data.map(c => ({
            time: Math.floor(new Date(c.open_time).getTime() / 1000),
            open: c.open, high: c.high, low: c.low, close: c.close
        })).sort((a, b) => a.time - b.time);

        if (chartData.length > 0) {
            lineSeries.setData(chartData);
            const priceInput = document.getElementById('tradePriceInput');
            if (priceInput) priceInput.value = chartData[chartData.length - 1].close.toFixed(PRICE_PRECISION);
            const tradePriceLabel = document.getElementById('tradePrice');
            if (tradePriceLabel) tradePriceLabel.textContent = chartData[chartData.length - 1].close.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });

            if (chartData.length > 1) {
                const first = chartData[0].close;
                const last = chartData[chartData.length - 1].close;
                const pct = ((last - first) / first) * 100;
                const changeEl = document.getElementById('tradeChange');
                if (changeEl) {
                    changeEl.textContent = `${pct >= 0 ? '+' : ''}${pct.toFixed(2)}%`;
                    changeEl.style.color = pct >= 0 ? 'var(--trading-up)' : 'var(--trading-down)';
                }
            }
        }
    } catch (e) {
        console.error('Chart data fetch error:', e);
    }
}

window.switchTimeframe = function (tf, btn) {
    document.querySelectorAll('.chart-timeframe').forEach(b => b.classList.remove('active'));
    if (btn) btn.classList.add('active');
    // TODO: Re-fetch chart data with new timeframe
};

// ══════════════════════════════════════════════════════════════════════════════
// PORTFOLIO & WALLET
// ══════════════════════════════════════════════════════════════════════════════
async function fetchPortfolio() {
    try {
        const res = await fetchWithRetry(`${API_BASE}/portfolio/${USER_ID}`);
        if (!res.ok) return;
        const p = await res.json();

        let totalVal = 0;

        const quoteAsset = currentTradeSymbol.split('/')[1] || 'USDT';
        const baseAsset = currentTradeSymbol.split('/')[0] || 'BTC';

        const qBal = p.balances?.find(b => b.asset === quoteAsset);
        const bBal = p.balances?.find(b => b.asset === baseAsset);

        const availEl = document.getElementById('tradeAvailable');
        if (availEl) {
            if (tradeSide === 'buy') {
                availEl.textContent = `${qBal ? qBal.available.toFixed(QTY_PRECISION) : '0.0000'} ${quoteAsset}`;
            } else {
                availEl.textContent = `${bBal ? bBal.available.toFixed(QTY_PRECISION) : '0.0000'} ${baseAsset}`;
            }
        }

        const usdtBal = document.getElementById('tradeUsdtBalance');
        if (usdtBal && qBal) usdtBal.textContent = `$${qBal.total.toFixed(2)}`;

        const wBody = document.getElementById('walletAssetsBody');
        if (wBody) {
            wBody.innerHTML = (p.balances || []).map(b => {
                let estUsd = b.total;
                if (b.asset !== 'USD' && b.asset !== 'USDT') {
                    const m = allMarkets.find(m => m.symbol === `${b.asset}/USDT` || m.symbol === `${b.asset}/USD`);
                    if (m) estUsd = b.total * m.last_price;
                }
                totalVal += estUsd;

                return `
                    <tr>
                        <td style="text-align:left"><strong>${escapeHtml(b.asset)}</strong></td>
                        <td>${b.total.toFixed(4)}</td>
                        <td>${b.available.toFixed(4)}</td>
                        <td>${b.locked.toFixed(4)}</td>
                        <td>$${estUsd.toFixed(2)}</td>
                        <td class="text-up">+0.00%</td>
                    </tr>
                `;
            }).join('');
        }

        const wTotal = document.getElementById('walletTotal');
        if (wTotal) wTotal.textContent = `$${totalVal.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
        const navBal = document.getElementById('navBalance');
        if (navBal) navBal.textContent = `$${totalVal.toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits: 0 })}`;

    } catch (e) {
        console.error('Portfolio fetch error:', e);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// WEBSOCKET
// ══════════════════════════════════════════════════════════════════════════════
function initWebSocket() {
    try {
        ws = new WebSocket(WS_URL);
        ws.onopen = () => {
            wsRetryCount = 0;
            networkStatus = 'online';
            updateNetworkIndicator();
        };
        ws.onclose = (event) => {
            networkStatus = 'degraded';
            updateNetworkIndicator();
            // Exponential backoff with max 30s
            const delay = Math.min(3000 * Math.pow(2, wsRetryCount), 30000);
            wsRetryCount++;
            setTimeout(initWebSocket, delay);
        };
        ws.onerror = () => {
            // Error handler - onclose will handle reconnection
        };
        ws.onmessage = (e) => {
            try {
                const data = JSON.parse(e.data);
                if (data.type === 'trade' && data.symbol === currentTradeSymbol) {
                    if (lineSeries) {
                        const time = Math.floor(new Date(data.timestamp).getTime() / 1000);
                        lineSeries.update({
                            time,
                            open: data.price,
                            high: data.price,
                            low: data.price,
                            close: data.price
                        });
                    }
                    const tp = document.getElementById('tradePrice');
                    if (tp) tp.textContent = data.price.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
                } else if (data.type === 'orderbook_update' && data.symbol === currentTradeSymbol) {
                    fetchOrderbook();
                }
            } catch (err) {
                // Silently ignore malformed WS messages
            }
        };
    } catch (e) {
        console.error('WebSocket init error:', e);
        setTimeout(initWebSocket, 5000);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// OPTIONS (MOCK)
// ══════════════════════════════════════════════════════════════════════════════
window.renderOptionsChain = function () {
    const calls = document.getElementById('optionsCallsBody');
    const puts = document.getElementById('optionsPutsBody');
    if (!calls || !puts) return;

    let callsHtml = '', putsHtml = '';
    const basePriceStr = document.getElementById('tradePrice')?.textContent || '68000';
    const basePrice = safeParseFloat(basePriceStr.replace(/,/g, ''));

    for (let i = -5; i <= 5; i++) {
        const strike = (Math.round(basePrice / 1000) * 1000) + (i * 1000);
        const cPrem = Math.max(10, 1500 - (i * 200) + Math.random() * 100);
        const pPrem = Math.max(10, 1500 + (i * 200) + Math.random() * 100);

        callsHtml += `
            <div style="display:flex;padding:6px 16px;font-size:12px;border-bottom:1px solid var(--border);cursor:pointer">
                <span class="oc-strike mono">${strike}</span>
                <span class="oc-premium mono" style="width:80px;text-align:right">$${cPrem.toFixed(1)}</span>
                <span class="oc-iv mono" style="width:60px;text-align:right">45%</span>
                <span class="oc-delta mono" style="width:60px;text-align:right">0.${(45 - i * 8).toString().padStart(2, '0')}</span>
                <span class="oc-gamma mono" style="width:60px;text-align:right">0.05</span>
                <span class="oc-oi mono" style="width:70px;text-align:right">${Math.floor(Math.random() * 500)}</span>
                <span class="oc-volume mono" style="width:70px;text-align:right">${Math.floor(Math.random() * 100)}</span>
            </div>
        `;
        putsHtml += `
            <div style="display:flex;padding:6px 16px;font-size:12px;border-bottom:1px solid var(--border);cursor:pointer">
                <span class="oc-strike mono">${strike}</span>
                <span class="oc-premium mono" style="width:80px;text-align:right">$${pPrem.toFixed(1)}</span>
                <span class="oc-iv mono" style="width:60px;text-align:right">45%</span>
                <span class="oc-delta mono" style="width:60px;text-align:right">-0.${(45 + i * 8).toString().padStart(2, '0')}</span>
                <span class="oc-gamma mono" style="width:60px;text-align:right">0.05</span>
                <span class="oc-oi mono" style="width:70px;text-align:right">${Math.floor(Math.random() * 500)}</span>
                <span class="oc-volume mono" style="width:70px;text-align:right">${Math.floor(Math.random() * 100)}</span>
            </div>
        `;
    }

    calls.innerHTML = callsHtml;
    puts.innerHTML = putsHtml;
};

// ══════════════════════════════════════════════════════════════════════════════
// MISC UI
// ══════════════════════════════════════════════════════════════════════════════
window.showConnectionStatus = function () {
    const status = ws?.readyState === WebSocket.OPEN ? 'Connected' : 'Disconnected';
    toast(`WebSocket: ${status}`, status === 'Connected' ? 'success' : 'error');
};

window.showWalletModal = function () { switchPage('wallet'); };

window.switchOrdersTab = function (tab, btn) {
    document.querySelectorAll('.orders-tab').forEach(b => b.classList.remove('active'));
    if (btn) btn.classList.add('active');

    const body = document.getElementById('ordersBody');
    if (!body) return;

    if (tab === 'open') {
        fetchOpenOrders();
    } else if (tab === 'history') {
        fetchOrderHistory();
    } else if (tab === 'positions') {
        fetchPositions();
    }
};

// ══════════════════════════════════════════════════════════════════════════════
// ORDER HISTORY & OPEN ORDERS
// ══════════════════════════════════════════════════════════════════════════════

async function fetchOpenOrders() {
    const body = document.getElementById('ordersBody');
    // Only show loading if there's no existing content
    if (body && body.querySelector('.empty-state')) {
        body.innerHTML = '<div style="padding:16px;text-align:center;color:var(--muted)"><div class="loading-spinner" style="width:16px;height:16px"></div></div>';
    }
    try {
        const res = await fetchWithRetry(`${API_BASE}/orders/open/${USER_ID}`);
        if (!res.ok) {
            if (body) body.innerHTML = '<div class="empty-state" style="padding:16px"><div class="empty-state-text">No open orders</div></div>';
            return;
        }
        const data = await res.json();
        const orders = data.orders || [];

        if (!body) return;

        if (orders.length === 0) {
            body.innerHTML = '<div class="empty-state" style="padding:16px"><div class="empty-state-text">No open orders</div></div>';
            return;
        }

        body.innerHTML = renderOrdersTable(orders, false);
    } catch (e) {
        console.error('Open orders fetch error:', e);
        if (body) body.innerHTML = '<div class="empty-state" style="padding:16px"><div class="empty-state-text">No open orders</div></div>';
    }
}

async function fetchOrderHistory() {
    const body = document.getElementById('ordersBody');
    if (body) body.innerHTML = '<div style="padding:16px;text-align:center;color:var(--muted)"><div class="loading-spinner" style="width:16px;height:16px"></div></div>';
    try {
        const res = await fetchWithRetry(`${API_BASE}/orders/history/${USER_ID}?limit=50`);
        if (!res.ok) {
            if (body) body.innerHTML = '<div class="empty-state" style="padding:16px"><div class="empty-state-text">No order history yet</div></div>';
            return;
        }
        const data = await res.json();
        const orders = data.orders || [];

        if (!body) return;

        if (orders.length === 0) {
            body.innerHTML = '<div class="empty-state" style="padding:16px"><div class="empty-state-text">No order history yet</div></div>';
            return;
        }

        body.innerHTML = renderOrdersTable(orders, true);
    } catch (e) {
        console.error('Order history fetch error:', e);
        if (body) body.innerHTML = '<div class="empty-state" style="padding:16px"><div class="empty-state-text">No order history yet</div></div>';
    }
}

async function fetchPositions() {
    try {
        const res = await fetchWithRetry(`${API_BASE}/futures/positions/${USER_ID}`);
        if (!res.ok) {
            // Fallback: show empty positions
            const body = document.getElementById('ordersBody');
            if (body) body.innerHTML = '<div class="empty-state" style="padding:16px"><div class="empty-state-text">No open positions</div></div>';
            return;
        }
        const data = await res.json();
        const positions = data.positions || [];

        const body = document.getElementById('ordersBody');
        if (!body) return;

        if (positions.length === 0) {
            body.innerHTML = '<div class="empty-state" style="padding:16px"><div class="empty-state-text">No open positions</div></div>';
            return;
        }

        body.innerHTML = renderPositionsTable(positions);
    } catch (e) {
        console.error('Positions fetch error:', e);
        const body = document.getElementById('ordersBody');
        if (body) body.innerHTML = '<div class="empty-state" style="padding:16px"><div class="empty-state-text">No open positions</div></div>';
    }
}

function renderOrdersTable(orders, isHistory) {
    const header = `<div class="order-row order-header">
        <span style="width:80px;text-align:left">Time</span>
        <span style="width:90px;text-align:left">Symbol</span>
        <span style="width:50px;text-align:left">Side</span>
        <span style="width:70px;text-align:left">Type</span>
        <span style="width:70px;text-align:right">Price</span>
        <span style="width:70px;text-align:right">Qty</span>
        <span style="width:80px;text-align:right">Filled</span>
        <span style="width:70px;text-align:right">Total</span>
        <span style="width:70px;text-align:right">Status</span>
    </div>`;

    const rows = orders.map(o => {
        const side = (o.side || '').toLowerCase();
        const sideColor = side === 'buy' ? 'var(--trading-up)' : side === 'sell' ? 'var(--trading-down)' : 'var(--muted)';
        const status = (o.status || '').toLowerCase();
        let statusColor = 'var(--muted)';
        if (status.includes('filled') || status === 'new') statusColor = 'var(--trading-up)';
        else if (status.includes('cancel')) statusColor = 'var(--trading-down)';
        else if (status.includes('partial')) statusColor = 'var(--primary)';

        const price = o.price ? `$${(parseFloat(o.price) || 0).toFixed(2)}` : 'Market';
        const filled = o.filled_quantity || o.filled_qty || 0;
        const total = o.price ? `$${((parseFloat(o.price) || 0) * (parseFloat(o.quantity) || 0)).toFixed(2)}` : '—';

        let time = '';
        if (o.created_at || o.timestamp) {
            try {
                const d = new Date(o.created_at || o.timestamp);
                time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
            } catch (_) { time = '—'; }
        }

        return `<div class="order-row">
            <span style="width:80px;text-align:left;color:var(--muted)">${escapeHtml(time)}</span>
            <span style="width:90px;text-align:left;font-weight:600">${escapeHtml(o.symbol || '')}</span>
            <span style="width:50px;text-align:left;color:${sideColor};font-weight:600">${escapeHtml((o.side || '').toUpperCase())}</span>
            <span style="width:70px;text-align:left;color:var(--muted)">${escapeHtml(o.order_type || o.type || '')}</span>
            <span style="width:70px;text-align:right;font-family:var(--font-mono)">${escapeHtml(String(price))}</span>
            <span style="width:70px;text-align:right;font-family:var(--font-mono)">${parseFloat(o.quantity || 0).toFixed(4)}</span>
            <span style="width:80px;text-align:right;font-family:var(--font-mono)">${parseFloat(filled).toFixed(4)}</span>
            <span style="width:70px;text-align:right;font-family:var(--font-mono)">${escapeHtml(String(total))}</span>
            <span style="width:70px;text-align:right;color:${statusColor};font-size:10px;font-weight:600">${escapeHtml(o.status || '')}</span>
        </div>`;
    }).join('');

    return header + rows;
}

function renderPositionsTable(positions) {
    const header = `<div class="order-row order-header">
        <span style="width:90px;text-align:left">Symbol</span>
        <span style="width:50px;text-align:left">Side</span>
        <span style="width:70px;text-align:right">Size</span>
        <span style="width:80px;text-align:right">Entry</span>
        <span style="width:80px;text-align:right">Current</span>
        <span style="width:80px;text-align:right">P&L</span>
        <span style="width:70px;text-align:right">P&L %</span>
    </div>`;

    const rows = positions.map(p => {
        const side = (p.side || p.direction || '').toLowerCase();
        const sideColor = side === 'long' || side === 'buy' ? 'var(--trading-up)' : 'var(--trading-down)';
        const pnl = p.pnl || p.unrealized_pnl || 0;
        const pnlPct = p.pnl_pct || p.unrealized_pnl_pct || 0;
        const pnlColor = pnl >= 0 ? 'var(--trading-up)' : 'var(--trading-down)';

        return `<div class="order-row">
            <span style="width:90px;text-align:left;font-weight:600">${escapeHtml(p.symbol || '')}</span>
            <span style="width:50px;text-align:left;color:${sideColor};font-weight:600">${escapeHtml((side).toUpperCase())}</span>
            <span style="width:70px;text-align:right;font-family:var(--font-mono)">${parseFloat(p.quantity || p.size || 0).toFixed(4)}</span>
            <span style="width:80px;text-align:right;font-family:var(--font-mono)">$${parseFloat(p.entry_price || 0).toFixed(2)}</span>
            <span style="width:80px;text-align:right;font-family:var(--font-mono)">$${parseFloat(p.current_price || p.mark_price || 0).toFixed(2)}</span>
            <span style="width:80px;text-align:right;font-family:var(--font-mono);color:${pnlColor}">${pnl >= 0 ? '+' : ''}$${pnl.toFixed(2)}</span>
            <span style="width:70px;text-align:right;font-family:var(--font-mono);color:${pnlColor}">${pnlPct >= 0 ? '+' : ''}${pnlPct.toFixed(2)}%</span>
        </div>`;
    }).join('');

    return header + rows;
}

window.addWhitelistUser = function () { toast('Whitelist feature is mock only', 'info'); };
