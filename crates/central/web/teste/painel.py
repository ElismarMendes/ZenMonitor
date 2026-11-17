# Uso: python painel.py [v4.html] [estado.json] [prefixo-dos-prints]
# Requer: pip install playwright && playwright install chromium
# Testa o painel v4/v5 com estado real e WebSocket simulado (Playwright, Chromium).
import json, sys, time, copy
from playwright.sync_api import sync_playwright
import os
aqui = os.path.dirname(os.path.abspath(__file__))
html_path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(aqui, "..", "v4.html")
estado_path = sys.argv[2] if len(sys.argv) > 2 else os.path.join(aqui, "estado-exemplo.json")
out = sys.argv[3] if len(sys.argv) > 3 else os.path.join(aqui, "saida")
html = open(html_path, encoding='utf-8').read()
state = json.load(open(estado_path, encoding='utf-8-sig'))
errors = []
with sync_playwright() as p:
    b = p.chromium.launch()
    pg = b.new_page(viewport={'width': 1400, 'height': 900})
    pg.on('pageerror', lambda e: errors.append('pageerror: ' + str(e)))
    pg.on('console', lambda m: errors.append('console.' + m.type + ': ' + m.text) if m.type in ('error',) else None)
    def route(r):
        u = r.request.url
        if u.endswith('/api/panels'): return r.fulfill(json={'open': 1, 'boot': 1})
        if '/api/' in u: return r.fulfill(json={}) 
        return r.fulfill(body=html, content_type='text/html; charset=utf-8')
    pg.route('http://zen.test/**', route)
    sockets = []
    def ws_handler(ws):
        sockets.append(ws); ws.send(json.dumps(state))
    pg.route_web_socket('ws://zen.test/ws/panel', ws_handler)
    pg.goto('http://zen.test/#dash'); pg.wait_for_timeout(1200)
    pg.screenshot(path=f'{out}-dash.png')
    # mouse sobre um botão + nova atualização: o painel tem de refletir o Zen novo
    s2 = copy.deepcopy(state)
    if s2.get('chars'):
        s2['chars'][0]['zen'] = 1_234_567_890
    btn = pg.query_selector('main button')
    if btn: btn.hover()
    marker = pg.evaluate("() => { const e=document.querySelector('main .ch[data-c]'); if(e){e.__marca=1;} return !!e }")
    sockets[0].send(json.dumps(s2)); pg.wait_for_timeout(600)
    txt = pg.inner_text('main')
    vivo = '1,23 B' in txt
    mesmo_no = pg.evaluate("() => { const e=document.querySelector('main .ch[data-c]'); return !!(e && e.__marca) }")
    print('atualiza com mouse em cima:', vivo, '| nó preservado:', mesmo_no, '| marca inicial:', marker)
    for t in ['chars','maps','remote','alerts','pcs','cfg']:
        pg.click(f'nav a[data-t="{t}"]'); pg.wait_for_timeout(500)
        pg.screenshot(path=f'{out}-{t}.png')
    pg.click('nav a[data-t="chars"]'); pg.wait_for_timeout(500)
    row = pg.query_selector('tr[data-c]')
    if row:
        row.click(); pg.wait_for_timeout(500)
        sockets[-1].send(json.dumps(state)); pg.wait_for_timeout(500)
        pg.screenshot(path=f'{out}-gaveta.png')
        print('gaveta aberta:', pg.evaluate("() => document.getElementById('drawer').classList.contains('open')"))
    b.close()
print('erros:', errors[:10])
