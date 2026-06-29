const API_BASE = 'http://127.0.0.1:8080/api/v1';
const WS_URL = 'ws://127.0.0.1:8080/api/v1/ws';
const USER_ID = 'alice';

let currentMarketTab = 'crypto';
let currentTradeSymbol = 'BTC/USDT';
let allMarkets = [];
let chart = null;
let lineSeries = null;
let ws = null;

document.addEventListener('DOMContentLoaded', () => {
    switchPage('markets');
    initApp();
});

// Toast notification
function toast(message, type = 'info') {
    const container = document.getElementById('toastContainer');
    if(!container) return;
    const t = document.createElement('div');
    t.className = `toast toast-${type}`;
    t.innerHTML = message;
    container.appendChild(t);
    setTimeout(() => { t.remove(); }, 3000);
}

// Navigation
window.switchPage = function(pageId) {
    document.querySelectorAll('.exchange-page').forEach(p => p.classList.remove('active'));
    document.querySelectorAll('.top-nav-item').forEach(p => p.classList.remove('active'));
    
    const pageEl = document.getElementById(`page-${pageId}`);
    const navEl = document.querySelector(`.top-nav-item[data-page="${pageId}"]`);
    
    if(pageEl) pageEl.classList.add('active');
    if(navEl) navEl.classList.add('active');

    if(pageId === 'trade') {
        setTimeout(() => initChart(), 100);
    }
    if(pageId === 'options') renderOptionsChain();
}

async function initApp() {
    updateClock();
    setInterval(updateClock, 1000);
    
    await fetchMarkets();
    await fetchPortfolio();
    initWebSocket();
}

function updateClock() {
    const label = document.getElementById('sessionLabel');
    if(label) label.textContent = new Date().toISOString().substring(11, 19) + ' UTC';
}

// Markets Page
async function fetchMarkets() {
    try {
        const res = await fetch(`${API_BASE}/ticker/24hr`);
        if(res.ok) {
            allMarkets = await res.json();
            if(allMarkets.length > 0) {
                // Find BTC/USDT or fallback
                const btc = allMarkets.find(m => m.symbol === 'BTC/USDT');
                currentTradeSymbol = btc ? btc.symbol : allMarkets[0].symbol;
            }
            filterMarkets();
            populateTradeSymbolSelect();
        }
    } catch(e) {
        console.error('Error fetching markets:', e);
    }
}

window.switchMarket = function(market, btnEl) {
    currentMarketTab = market;
    document.querySelectorAll('.markets-tab').forEach(b => b.classList.remove('active'));
    if(btnEl) btnEl.classList.add('active');
    filterMarkets();
}

window.filterMarkets = function() {
    const search = document.getElementById('marketSearch')?.value.toLowerCase() || '';
    const tbody = document.getElementById('marketBody');
    if(!tbody) return;

    let filtered = allMarkets;
    if(currentMarketTab !== 'all') {
        if(currentMarketTab === 'crypto') {
            filtered = allMarkets.filter(m => m.symbol.endsWith('/USDT') || m.symbol.endsWith('/USD'));
        } else {
            filtered = []; // other markets not seeded
        }
    }
    
    if(search) {
        filtered = filtered.filter(m => m.symbol.toLowerCase().includes(search));
    }

    const countEl = document.getElementById('marketCount');
    if(countEl) countEl.textContent = `${filtered.length} assets`;

    tbody.innerHTML = filtered.map(m => {
        const color = m.price_change_percent >= 0 ? 'var(--trading-up)' : 'var(--trading-down)';
        const sign = m.price_change_percent >= 0 ? '+' : '';
        return `
            <div class="markets-table-row" style="display:flex; cursor:pointer" onclick="goToTrade('${m.symbol}')">
                <span style="width:80px;font-weight:600">${m.symbol}</span>
                <span style="flex:1;color:var(--muted)">${m.symbol.split('/')[0]}</span>
                <span style="width:50px;text-align:center">Spot</span>
                <span style="width:120px;text-align:right">$${m.last_price.toFixed(2)}</span>
                <span style="width:90px;text-align:right;color:${color}">${sign}${m.price_change_percent.toFixed(2)}%</span>
                <span style="width:100px;text-align:right">${m.volume.toFixed(2)}</span>
            </div>
        `;
    }).join('');
}

window.goToTrade = function(symbol) {
    currentTradeSymbol = symbol;
    const sel = document.getElementById('tradeSymbolSelect');
    if(sel) sel.value = symbol;
    switchPage('trade');
    switchTradeSymbol();
}

// Trade Page
function populateTradeSymbolSelect() {
    const sel = document.getElementById('tradeSymbolSelect');
    if(!sel) return;
    sel.innerHTML = allMarkets.map(m => `<option value="${m.symbol}">${m.symbol}</option>`).join('');
    sel.value = currentTradeSymbol;
    document.getElementById('tradePair').textContent = currentTradeSymbol;
}

window.switchTradeSymbol = function() {
    const sel = document.getElementById('tradeSymbolSelect');
    if(sel) currentTradeSymbol = sel.value;
    const pairLabel = document.getElementById('tradePair');
    if(pairLabel) pairLabel.textContent = currentTradeSymbol;
    fetchOrderbook();
    fetchChartData();
    fetchPortfolio(); 
}

let tradeSide = 'buy';
window.switchTradeTab = function(side) {
    tradeSide = side;
    document.getElementById('tradeTabBuy').classList.toggle('active', side === 'buy');
    document.getElementById('tradeTabSell').classList.toggle('active', side === 'sell');
    const btn = document.getElementById('tradeSubmitBtn');
    if(btn) {
        btn.className = `trade-submit-btn ${side}`;
        btn.textContent = `${side.toUpperCase()} ${currentTradeSymbol.split('/')[0]}`;
    }
    fetchPortfolio(); 
}

window.updateTradeTotal = function() {
    const price = parseFloat(document.getElementById('tradePriceInput').value) || 0;
    const amount = parseFloat(document.getElementById('tradeAmount').value) || 0;
    const totalEl = document.getElementById('tradeTotal');
    if(totalEl) totalEl.textContent = `$${(price * amount).toFixed(2)}`;
}

window.setTradePercent = function(pct) {
    const balStr = document.getElementById('tradeAvailable')?.textContent || "0";
    const avail = parseFloat(balStr) || 0;
    const price = parseFloat(document.getElementById('tradePriceInput').value) || 0;
    const amountEl = document.getElementById('tradeAmount');
    if(!amountEl) return;

    if(price > 0 && tradeSide === 'buy') {
        amountEl.value = ((avail * (pct/100)) / price).toFixed(4);
    } else {
        amountEl.value = (avail * (pct/100)).toFixed(4);
    }
    updateTradeTotal();
}

window.submitTrade = async function() {
    const type = document.getElementById('tradeOrderType').value;
    const price = parseFloat(document.getElementById('tradePriceInput').value);
    const amount = parseFloat(document.getElementById('tradeAmount').value);
    
    if(!amount || amount <= 0) return toast('Invalid amount', 'error');

    const order = {
        user_id: USER_ID,
        symbol: currentTradeSymbol,
        side: tradeSide.charAt(0).toUpperCase() + tradeSide.slice(1),
        order_type: type.charAt(0).toUpperCase() + type.slice(1),
        price: type === 'market' ? undefined : price,
        quantity: amount
    };

    try {
        const res = await fetch(`${API_BASE}/orders`, {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify(order)
        });
        if(res.ok) {
            toast('Order placed successfully!', 'success');
            setTimeout(() => {
                fetchPortfolio();
                fetchOrderbook();
            }, 100);
        } else {
            const err = await res.text();
            toast(`Failed: ${err}`, 'error');
        }
    } catch(e) {
        toast(`Error: ${e.message}`, 'error');
    }
}

async function fetchOrderbook() {
    try {
        const res = await fetch(`${API_BASE}/orderbook?symbol=${currentTradeSymbol}&depth=10`);
        if(!res.ok) return;
        const ob = await res.json();
        
        const asksEl = document.getElementById('orderbookAsks');
        const bidsEl = document.getElementById('orderbookBids');
        
        if(asksEl) {
            asksEl.innerHTML = ob.asks.slice(0, 10).reverse().map(a => `
                <div style="display:flex; justify-content:space-between; position:relative; padding:2px 4px; font-family:'JetBrains Mono', monospace; font-size:12px; color:var(--trading-down); cursor:pointer;" onclick="document.getElementById('tradePriceInput').value='${a.price.toFixed(2)}'">
                    <span style="z-index:2; width:33%">${a.price.toFixed(2)}</span>
                    <span style="z-index:2; width:33%; text-align:right">${a.quantity.toFixed(4)}</span>
                    <span style="z-index:2; width:33%; text-align:right">${(a.price * a.quantity).toFixed(2)}</span>
                    <div style="position:absolute; top:0; right:0; height:100%; width: ${Math.min(100, (a.quantity/5)*100)}%; background:rgba(239,68,68,0.1); z-index:1;"></div>
                </div>
            `).join('');
        }
        
        if(bidsEl) {
            bidsEl.innerHTML = ob.bids.slice(0, 10).map(b => `
                <div style="display:flex; justify-content:space-between; position:relative; padding:2px 4px; font-family:'JetBrains Mono', monospace; font-size:12px; color:var(--trading-up); cursor:pointer;" onclick="document.getElementById('tradePriceInput').value='${b.price.toFixed(2)}'">
                    <span style="z-index:2; width:33%">${b.price.toFixed(2)}</span>
                    <span style="z-index:2; width:33%; text-align:right">${b.quantity.toFixed(4)}</span>
                    <span style="z-index:2; width:33%; text-align:right">${(b.price * b.quantity).toFixed(2)}</span>
                    <div style="position:absolute; top:0; right:0; height:100%; width: ${Math.min(100, (b.quantity/5)*100)}%; background:rgba(16,185,129,0.1); z-index:1;"></div>
                </div>
            `).join('');
        }
        
        const bestAsk = ob.asks[0]?.price || 0;
        const bestBid = ob.bids[0]?.price || 0;
        const spreadEl = document.getElementById('orderbookSpread');
        if(spreadEl && bestAsk && bestBid) {
            const spread = bestAsk - bestBid;
            const spreadPct = (spread / bestBid) * 100;
            spreadEl.textContent = `Spread: ${spread.toFixed(2)} (${spreadPct.toFixed(2)}%)`;
        }
    } catch(e) {
        console.error(e);
    }
}

function initChart() {
    const container = document.getElementById('chartContainer');
    if(!container || chart) return;
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
        if(entries.length === 0 || entries[0].target !== container) return;
        const newRect = entries[0].contentRect;
        chart.applyOptions({ height: newRect.height, width: newRect.width });
    }).observe(container);

    fetchChartData();
}

async function fetchChartData() {
    if(!lineSeries) return;
    try {
        const res = await fetch(`${API_BASE}/candles?symbol=${currentTradeSymbol}&interval=1m&limit=100`);
        if(!res.ok) return;
        const data = await res.json();
        const chartData = data.map(c => ({
            time: Math.floor(new Date(c.open_time).getTime() / 1000),
            open: c.open, high: c.high, low: c.low, close: c.close
        })).sort((a,b) => a.time - b.time);
        
        if(chartData.length > 0) {
            lineSeries.setData(chartData);
            const priceInput = document.getElementById('tradePriceInput');
            if(priceInput) priceInput.value = chartData[chartData.length-1].close.toFixed(2);
            const tradePriceLabel = document.getElementById('tradePrice');
            if(tradePriceLabel) tradePriceLabel.textContent = chartData[chartData.length-1].close.toFixed(2);
            
            // Calculate 24h change approx from chart if available
            if(chartData.length > 1) {
                const first = chartData[0].close;
                const last = chartData[chartData.length-1].close;
                const pct = ((last - first) / first) * 100;
                const changeEl = document.getElementById('tradeChange');
                if(changeEl) {
                    changeEl.textContent = `${pct >= 0 ? '+' : ''}${pct.toFixed(2)}%`;
                    changeEl.style.color = pct >= 0 ? 'var(--trading-up)' : 'var(--trading-down)';
                }
            }
        }
    } catch(e) { console.error(e); }
}

window.switchTimeframe = function(tf, btn) {
    document.querySelectorAll('.chart-timeframe').forEach(b => b.classList.remove('active'));
    if(btn) btn.classList.add('active');
}

// Portfolio & Wallet
async function fetchPortfolio() {
    try {
        const res = await fetch(`${API_BASE}/portfolio/${USER_ID}`);
        if(!res.ok) return;
        const p = await res.json();
        
        let totalVal = 0;
        
        const quoteAsset = currentTradeSymbol.split('/')[1] || 'USDT';
        const baseAsset = currentTradeSymbol.split('/')[0] || 'BTC';
        
        const qBal = p.balances.find(b => b.asset === quoteAsset);
        const bBal = p.balances.find(b => b.asset === baseAsset);
        
        const availEl = document.getElementById('tradeAvailable');
        if(availEl) {
            if(tradeSide === 'buy') {
                availEl.textContent = `${qBal ? qBal.available.toFixed(4) : '0.0000'} ${quoteAsset}`;
            } else {
                availEl.textContent = `${bBal ? bBal.available.toFixed(4) : '0.0000'} ${baseAsset}`;
            }
        }
        
        const usdtBal = document.getElementById('tradeUsdtBalance');
        if(usdtBal && qBal) usdtBal.textContent = `$${qBal.total.toFixed(2)}`;

        const wBody = document.getElementById('walletAssetsBody');
        if(wBody) {
            wBody.innerHTML = p.balances.map(b => {
                let estUsd = b.total;
                if(b.asset !== 'USD' && b.asset !== 'USDT') {
                    const m = allMarkets.find(m => m.symbol === `${b.asset}/USDT` || m.symbol === `${b.asset}/USD`);
                    if(m) estUsd = b.total * m.last_price;
                }
                totalVal += estUsd; 
                
                return `
                    <tr>
                        <td style="text-align:left"><strong>${b.asset}</strong></td>
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
        if(wTotal) wTotal.textContent = `$${totalVal.toLocaleString(undefined, {minimumFractionDigits:2, maximumFractionDigits:2})}`;
        const navBal = document.getElementById('navBalance');
        if(navBal) navBal.textContent = `$${totalVal.toLocaleString(undefined, {minimumFractionDigits:0, maximumFractionDigits:0})}`;

    } catch(e) { console.error(e); }
}

// Websocket
function initWebSocket() {
    ws = new WebSocket(WS_URL);
    ws.onopen = () => {
        const dot = document.getElementById('statusDot');
        if(dot) dot.style.color = 'var(--trading-up)';
    };
    ws.onclose = () => {
        const dot = document.getElementById('statusDot');
        if(dot) dot.style.color = 'var(--trading-down)';
        setTimeout(initWebSocket, 3000);
    };
    ws.onmessage = (e) => {
        try {
            const data = JSON.parse(e.data);
            if(data.type === 'trade' && data.symbol === currentTradeSymbol) {
                if(lineSeries) {
                    const time = Math.floor(new Date(data.timestamp).getTime() / 1000);
                    lineSeries.update({ time, open: data.price, high: data.price, low: data.price, close: data.price });
                }
                const tp = document.getElementById('tradePrice');
                if(tp) tp.textContent = data.price.toFixed(2);
            } else if (data.type === 'orderbook_update' && data.symbol === currentTradeSymbol) {
                fetchOrderbook(); 
            }
        } catch(err) {}
    };
}

// Mock Options
window.renderOptionsChain = function() {
    const calls = document.getElementById('optionsCallsBody');
    const puts = document.getElementById('optionsPutsBody');
    if(!calls || !puts) return;
    
    let callsHtml = '', putsHtml = '';
    const basePriceStr = document.getElementById('tradePrice')?.textContent || '68000';
    const basePrice = parseFloat(basePriceStr.replace(/,/g, ''));
    
    for(let i=-5; i<=5; i++) {
        const strike = (Math.round(basePrice/1000)*1000) + (i*1000);
        const cPrem = Math.max(10, 1500 - (i*200) + Math.random()*100);
        const pPrem = Math.max(10, 1500 + (i*200) + Math.random()*100);
        
        callsHtml += `
            <div style="display:flex;padding:6px 16px;font-size:12px;border-bottom:1px solid var(--border);cursor:pointer">
                <span class="oc-strike mono">${strike}</span>
                <span class="oc-premium mono" style="width:80px;text-align:right">$${cPrem.toFixed(1)}</span>
                <span class="oc-iv mono" style="width:60px;text-align:right">45%</span>
                <span class="oc-delta mono" style="width:60px;text-align:right">0.${(45 - i*8).toString().padStart(2,'0')}</span>
                <span class="oc-gamma mono" style="width:60px;text-align:right">0.05</span>
                <span class="oc-oi mono" style="width:70px;text-align:right">${Math.floor(Math.random()*500)}</span>
                <span class="oc-volume mono" style="width:70px;text-align:right">${Math.floor(Math.random()*100)}</span>
            </div>
        `;
        putsHtml += `
            <div style="display:flex;padding:6px 16px;font-size:12px;border-bottom:1px solid var(--border);cursor:pointer">
                <span class="oc-strike mono">${strike}</span>
                <span class="oc-premium mono" style="width:80px;text-align:right">$${pPrem.toFixed(1)}</span>
                <span class="oc-iv mono" style="width:60px;text-align:right">45%</span>
                <span class="oc-delta mono" style="width:60px;text-align:right">-0.${(45 + i*8).toString().padStart(2,'0')}</span>
                <span class="oc-gamma mono" style="width:60px;text-align:right">0.05</span>
                <span class="oc-oi mono" style="width:70px;text-align:right">${Math.floor(Math.random()*500)}</span>
                <span class="oc-volume mono" style="width:70px;text-align:right">${Math.floor(Math.random()*100)}</span>
            </div>
        `;
    }
    
    calls.innerHTML = callsHtml;
    puts.innerHTML = putsHtml;
}

// Miscellaneous UI toggles
window.showConnectionStatus = function() { toast('WebSocket Connection: ' + (ws?.readyState === 1 ? 'Connected' : 'Disconnected')); }
window.showWalletModal = function() { switchPage('wallet'); }
window.switchOrdersTab = function(tab, btn) {
    document.querySelectorAll('.orders-tab').forEach(b => b.classList.remove('active'));
    if(btn) btn.classList.add('active');
}
window.addWhitelistUser = function() { toast('Whitelist feature is mock only', 'info'); }
