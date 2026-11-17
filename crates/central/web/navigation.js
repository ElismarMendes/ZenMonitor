/* Radar e editor declarativo. Nenhum comando de input arbitrário. */
(function (global) {
  'use strict';
  const clone = x => JSON.parse(JSON.stringify(x));
  const spot = (world, x, y) => ({ world, x, y });
  const tileAt = (px, py, rect) => ({ x: Math.max(0, Math.min(255, Math.floor(px * 256 / rect.width))), y: Math.max(0, Math.min(255, Math.floor(py * 256 / rect.height))) });
  const distance = (a, b) => Math.max(Math.abs(a.x - b.x), Math.abs(a.y - b.y));
  const key = o => JSON.stringify([o.machine, o.snapshot.context.char_name]);
  const usable = s => s && s.context.map_confirmed && s.context.age_ms <= 10000 && s.context.world === s.position.world;
  const newRoute = world => ({ id: '', revision: 0, name: 'Nova rota', world, waypoints: [], target: { kind: 'point' }, bindings: [], return_route: null });
  const model = { tileAt, distance, key, usable, newRoute };
  if (typeof module !== 'undefined') module.exports = model;
  if (!global.document) return;
  const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  let root, state, receivedAt = Date.now(), editGeneration = 0, draft = newRoute(1), routes = [], maps = [], selection = -1, dirty = false, preview = null, observer = '', terrains = new Map(), history = [], drag = null, seen = new Set(), prior = null, confirmedNpcs = [];
  let followObserver = true, followWorld = null, observerChosen = false;
  const terrainLoading = new Set(), terrainRetryAt = new Map();
  let runsAt = -Infinity, runRequest = 0;
  const $ = id => root.querySelector('[data-n="' + id + '"]');
  const notice = (s, bad) => { if(!$('message')) return; $('message').textContent = s; $('message').style.color = bad ? 'var(--bad)' : 'var(--mute)'; };
  function adminToken() { try { return sessionStorage.getItem('zm_admin') || ''; } catch { return ''; } }
  async function api(path, method = 'GET', data) {
    const token = adminToken();
    const r = await fetch(path, { method, headers: { 'content-type': 'application/json', authorization: 'Bearer ' + token }, body: data === undefined ? undefined : JSON.stringify(data) });
    if (!r.ok) throw new Error(await r.text());
    return r.json();
  }
  function action(f) { return async () => { try { await f(); } catch (e) { notice(e.message, true); } }; }
  function changed() { editGeneration++; dirty = true; preview = null; if(draft.target.kind==='npc' && draft.waypoints.length) draft.target.npc.interaction=clone(draft.waypoints.at(-1)); $('draft').textContent = 'Alterações não salvas'; draw(); }
  function observed() { return (state.radar || []).find(o => key(o) === observer); }
  function fresh() { const o = observed(); return o && usable(o.snapshot) && o.snapshot.context.age_ms + Date.now() - receivedAt <= 10000 ? o : null; }
  function displayedWorld() {
    const o = fresh();
    if (followObserver && o) followWorld = o.snapshot.context.world;
    return followObserver ? (followWorld ?? draft.world) : draft.world;
  }
  const worldName = world => maps.find(m => Number(m.world) === world)?.name || ({1:'Lorencia',2:'Dungeon',5:'Lost Tower',38:'Kanturu'}[world]) || `W${world}`;
  function syncMap() {
    const world = displayedWorld();
    // Only choose a useful default for an empty, unsaved route. Existing points,
    // names and saved revisions never change when the character changes maps.
    if (followObserver && fresh() && !draft.id && !draft.waypoints.length && draft.target.kind === 'point' && !draft.return_route) {
      draft.world = world; $('world').value = world;
    }
    $('follow').checked = followObserver;
    loadTerrain(world);
  }
  function fields() {
    if($('versions').dataset.route !== draft.id){$('versions').textContent='';$('versions').dataset.route=draft.id;}
    $('name').value = draft.name; $('world').value = draft.world; $('target').value = draft.target.kind;
    $('binding').checked = !!draft.bindings.length; $('back').value = draft.return_route || '';
    $('binding-info').textContent = draft.bindings.length ? 'Associação: ' + draft.bindings.map(b => `${b.machine} / ${b.char_name}`).join(', ') : 'Rota sem associação a personagem.';
    $('draft').textContent = dirty ? 'Alterações não salvas' : draft.id ? `Revisão ${draft.revision}` : 'Rota nova';
    points(); options(); draw(); loadTerrain(displayedWorld());
  }
  function points() {
    $('points').innerHTML = draft.waypoints.map((p, i) => `<div class="nr-point ${selection === i ? 'on' : ''}" data-index="${i}"><button data-pick="${i}">${i + 1}. (${p.x}, ${p.y})</button><button data-up="${i}" title="Subir">↑</button><button data-down="${i}" title="Descer">↓</button><button data-del="${i}" title="Excluir">×</button></div>`).join('');
    $('points').querySelectorAll('button').forEach(b => b.onclick = () => {
      const a = b.dataset; const i = Number(a.pick ?? a.up ?? a.down ?? a.del);
      if (a.pick !== undefined) { selection = i; points(); draw(); return; }
      if (a.del !== undefined) draft.waypoints.splice(i, 1);
      else { const j = i + (a.up !== undefined ? -1 : 1); if (j >= 0 && j < draft.waypoints.length) [draft.waypoints[i], draft.waypoints[j]] = [draft.waypoints[j], draft.waypoints[i]]; }
      selection = -1; changed(); points();
    });
  }
  function options() {
    $('saved').innerHTML = '<option value="">Rotas salvas…</option>' + routes.map(r => `<option value="${esc(r.id)}">${esc(r.name)} · W${r.world} · r${r.revision}</option>`).join('');
    $('saved').value = draft.id;
    $('back').innerHTML = '<option value="">Sem rota de retorno associada</option>' + routes.filter(r => r.world === draft.world && r.id !== draft.id).map(r => `<option value="${esc(r.id)}">${esc(r.name)}</option>`).join('');
    if(draft.return_route && !routes.some(r => r.id===draft.return_route)) $('back').insertAdjacentHTML('beforeend',`<option value="${esc(draft.return_route)}">Retorno indisponível — selecione outro ou remova</option>`);
    $('back').value = draft.return_route || '';
  }
  async function refresh() { routes = await api('/api/routes'); options(); }
  async function loadRuns(force = false) {
    const panel = $('runs'); if (!panel) return;
    const filter = JSON.stringify([observer,draft.id]);
    if (panel.dataset.filter !== filter) { panel.dataset.filter = filter; panel.textContent = 'Carregando execuções…'; delete panel.dataset.signature; force = true; }
    if (!force && Date.now() - runsAt < 15000) return;
    runsAt = Date.now(); const request = ++runRequest, host = root;
    if (!adminToken()) { panel.textContent = 'Informe o token de admin no cabeçalho para consultar as execuções.'; return; }
    const o = observed(), params = new URLSearchParams({limit:'30'});
    if (o) { params.set('machine',o.machine); params.set('char_name',o.snapshot.context.char_name); }
    if (draft.id) params.set('route_id',draft.id);
    try {
      const rows = await api('/api/navigation/runs?' + params);
      if (request !== runRequest || host !== root || !root.isConnected || panel.dataset.filter !== JSON.stringify([observer,draft.id])) return;
      const signature = JSON.stringify(rows); if (panel.dataset.signature === signature) return;
      panel.dataset.signature = signature; const scroll = panel.scrollTop;
      const labels = {enviada:'Enviada · aguardando chegada','concluída':'Chegada confirmada',falhou:'Falhou',interrompida:'Interrompida'};
      panel.innerHTML = rows.length ? '<table><thead><tr><th>Início</th><th>Personagem / PC</th><th>Rota</th><th>Resultado</th></tr></thead><tbody>' + rows.map(r => `<tr><td>${esc(new Date(r.started_at*1000).toLocaleString('pt-BR'))}</td><td>${esc(r.char_name || 'Personagem não registrado (execução antiga)')}<br>${esc(r.machine)}</td><td>${esc(r.route_name || r.route_id)} · r${r.revision}</td><td>${esc(labels[r.status] || r.status)}${r.finished_at ? ' · '+esc(new Date(r.finished_at*1000).toLocaleTimeString('pt-BR')) : ''}<div class="note">${esc(r.message || 'O envio ainda não confirma a chegada.')}</div></td></tr>`).join('') + '</tbody></table>' : 'Nenhuma execução registrada para esta seleção.';
      panel.scrollTop = scroll;
    } catch(e) { if (request === runRequest && host === root) panel.textContent = 'Não foi possível consultar as execuções: ' + e.message; }
  }
  async function save() {
    draft.name = $('name').value.trim();
    const o = fresh();
    if ($('binding').checked && !draft.bindings.length) {
      if (!o) throw new Error('Selecione um observador online para associar a rota.');
      draft.bindings = [{ machine: o.machine, char_name: o.snapshot.context.char_name }];
    } else if (!$('binding').checked) draft.bindings = [];
    const generation = editGeneration, sent = clone(draft);
    const saved = await api('/api/routes', 'POST', sent); preview = null;
    if(generation===editGeneration){draft=saved;dirty=false;await refresh();fields();loadRuns(true);notice('Rota salva. Salvar não movimenta o personagem.');}
    else {
      if(draft.id===sent.id){draft.id=saved.id;draft.revision=saved.revision;dirty=true;}
      await refresh();notice('Revisão salva; as alterações feitas durante o envio continuam no rascunho.');
    }
  }
  async function loadTerrain(world) {
    if (terrains.has(world)) { draw(); return; }
    if (terrainLoading.has(world) || Date.now() < (terrainRetryAt.get(world) || 0)) return;
    terrainLoading.add(world);
    try {
      const t = await api('/api/terrain/' + world); const bytes = Uint8Array.from(atob(t.codes), c => c.charCodeAt(0));
      const canvas = document.createElement('canvas'); canvas.width = canvas.height = 256;
      const c = canvas.getContext('2d'); const img = c.createImageData(256, 256);
      bytes.forEach((b, i) => { const color = b === 1 ? [57, 65, 76] : b === 2 ? [24, 57, 52] : [12, 23, 34]; img.data.set([...color, 255], i * 4); }); c.putImageData(img, 0, 0);
      terrains.set(world, { canvas, bytes }); draw();
      terrainRetryAt.delete(world);
    } catch (e) { terrainRetryAt.set(world, Date.now() + 30000); notice('Terreno indisponível: ' + e.message, true); }
    finally { terrainLoading.delete(world); }
  }
  function line(c, a, b, color, dashed) { c.strokeStyle = color; c.setLineDash(dashed ? [6, 5] : []); c.beginPath(); c.moveTo((a.x + .5) * 3, (a.y + .5) * 3); c.lineTo((b.x + .5) * 3, (b.y + .5) * 3); c.stroke(); c.setLineDash([]); }
  function marker(c, p, color, label, size = 4) { const x = (p.x + .5) * 3, y = (p.y + .5) * 3; c.fillStyle = color; c.beginPath(); c.arc(x, y, size, 0, Math.PI * 2); c.fill(); if (label) { c.font = '11px sans-serif'; c.fillText(label, x + 7, y - 4); } }
  function draw() {
    if (!root?.isConnected || root.dataset.navigation !== 'mounted') return;
    const canvas = $('canvas'), c = canvas.getContext('2d'); c.clearRect(0, 0, 768, 768);
    const world = displayedWorld(), routeVisible = world === draft.world;
    const terrain = terrains.get(world); if (terrain && $('obstacles').checked) { c.imageSmoothingEnabled = false; c.drawImage(terrain.canvas, 0, 0, 768, 768); }
    const o = fresh(), s = o?.snapshot; const sameWorld = s?.context.world === world;
    let last = sameWorld ? s.position : null;
    if (routeVisible) draft.waypoints.forEach((p, i) => { if (last) line(c, last, p, '#d8af57', true); marker(c, p, i === selection ? '#fff' : '#f6be50', String(i + 1), 5); last = p; });
    if (preview && preview.plan.world === world) preview.plan.segments.forEach(segment => { for (let i = 1; i < segment.length; i++) line(c, segment[i - 1], segment[i], '#59bfff', false); });
    if (routeVisible && draft.target.kind === 'npc') { marker(c, draft.target.npc.location, '#dc91ff', draft.target.npc.name, 6); marker(c, draft.target.npc.interaction, '#f4d6ff', 'Interação', 3); }
    if (sameWorld) {
      const range = Number($('range').value); let count = 0;
      (s.entities || []).forEach(e => {
        const d = distance(e, s.position); if (d > range || !$('kind-' + e.kind)?.checked) return;
        if (e.kind === 'player' && e.name.toLowerCase() === s.context.char_name.toLowerCase()) return;
        count++; const friendly=e.kind==='player'&&(state.spot_directory?.members || []).some(m => m.name?.toLowerCase()===e.name.toLowerCase());
        const color = friendly ? '#70bfff' : { player: '#ff8a91', npc: '#dc91ff', monster: '#73da9b' }[e.kind];
        if ($('directions').checked) line(c, s.position, e, color + '77', true);
        marker(c, e, color, $('names').checked ? `${e.name}${friendly ? ' (nosso)' : ''} · ${d}t` : '', 3);
      });
      marker(c, s.position, '#ffffff', s.context.char_name, 6);
      $('radar-status').textContent = `${o.machine} · ${s.context.char_name} · ${worldName(world)} (${s.position.x}, ${s.position.y}) · ${count} entidades · ${Math.round(s.context.age_ms / 1000)}s${s.truncated ? ' · lista limitada' : ''}${s.entities === null ? ' · leitura de entidades desconhecida' : ''}${routeVisible ? '' : ` · Rota em ${worldName(draft.world)} preservada; use “Ver mapa da rota” para editar.`}`;
    } else $('radar-status').textContent = o ? `Personagem em ${worldName(s.context.world)}; exibindo ${worldName(world)}. Marque “Acompanhar mapa do personagem” para vê-lo.` : (state.radar || []).length ? 'Selecione um personagem com leitura recente e mapa confirmado.' : 'Nenhum personagem com radar disponível neste servidor.';
    const installed = fresh()?.snapshot.installed_routes;
    $('installed-routes').textContent = !installed ? 'Rotas instaladas: aguardando agente e leitura recente.' : installed.length ? 'Rotas instaladas neste personagem: ' + installed.map(r => `${r.name} / r${r.revision} / ${worldName(r.destination.world)} (${r.destination.x}, ${r.destination.y})`).join(' | ') : 'Nenhuma rota instalada neste personagem.';
    $('npc-info').textContent = draft.target.kind === 'npc' ? `NPC ${draft.target.npc.name} · classe ${draft.target.npc.class} · (${draft.target.npc.location.x}, ${draft.target.npc.location.y}); interação (${draft.target.npc.interaction.x}, ${draft.target.npc.interaction.y}). Será reconfirmado por duas leituras e pela janela do NPC.` : 'NPC: selecione um NPC confirmado no radar e o último waypoint como posição de interação.';
    $('execute').disabled = !preview?.token || dirty || !state.navigation_execution || Date.now()>preview.expiresAt;
    $('return').disabled = !draft.return_route;
  }
  function update(next) {
    state = next; receivedAt = Date.now(); if (!root?.isConnected) return;
    const all = state.radar || [], signature = all.map(key).sort().join('|');
    if (!observerChosen && document.activeElement !== $('observer')) {
      const first = all.find(o => usable(o.snapshot));
      if (first) { observer = key(first); observerChosen = true; }
    }
    if ($('observer').dataset.signature !== signature && document.activeElement !== $('observer')) {
      $('observer').innerHTML = '<option value="">Selecione personagem / computador</option>' + all.map(o => `<option value="${esc(key(o))}">${esc(o.machine)} / ${esc(o.snapshot.context.char_name)}</option>`).join('');
      $('observer').dataset.signature = signature; $('observer').value = observer;
    }
    syncMap();
    // Só oferece NPCs presentes em duas amostras consecutivas do mesmo processo/mapa.
    const o = fresh(), s = o?.snapshot;
    if (s && (!prior || prior.context.sequence !== s.context.sequence)) {
      const same = prior && ['session', 'epoch', 'pid', 'world', 'char_name'].every(k => prior.context[k] === s.context[k]);
      if (prior && !same) preview = null;
      const npcs = same ? (s.entities || []).filter(e => e.kind === 'npc' && (prior.entities || []).some(p => p.kind === 'npc' && p.class === e.class && p.name === e.name && distance(p, e) <= 1)) : [];
      confirmedNpcs = npcs;
      const value = $('npcs').value, npcSignature = JSON.stringify(npcs);
      if($('npcs').dataset.signature !== npcSignature && document.activeElement !== $('npcs')) {
        $('npcs').innerHTML = '<option value="">NPCs confirmados neste observador</option>' + npcs.map(e => `<option value="${esc(JSON.stringify(e))}">${esc(e.name)} · classe ${e.class} · (${e.x}, ${e.y})</option>`).join(''); $('npcs').value = value; $('npcs').dataset.signature = npcSignature;
      }
      prior = clone(s);
    }
    const results = (state.results || []).filter(r => r.route_final);
    let completed = false;
    for (const r of results.slice().reverse()) { const k = JSON.stringify([r.machine, r.id, r.ts]); if (!seen.has(k)) { seen.add(k); notice(`${r.machine}: ${r.message}`, !r.ok); completed = true; } }
    while (seen.size > 100) seen.delete(seen.values().next().value);
    draw();
    loadRuns(completed);
  }
  function mount(el, next) {
    root = el; state = next;
    root.innerHTML = `<style>.nr-layout{display:grid;grid-template-columns:minmax(300px,1fr) 340px;gap:16px}.nr-map{overflow:auto;max-height:75vh;background:#080f19;border:1px solid var(--line);border-radius:10px}.nr-map canvas{display:block;touch-action:none;outline:none}.nr-controls{display:flex;gap:8px;flex-wrap:wrap;margin:10px 0;align-items:center}.nr-controls label{font-size:12px}.nr-side input:not([type=checkbox]),.nr-side select{width:100%;margin:5px 0}.nr-point{display:flex;gap:4px;margin:3px 0}.nr-point button:first-child{flex:1;text-align:left}.nr-point.on{outline:1px solid var(--acc)}.nr-points{max-height:235px;overflow:auto}.nr-history{max-height:180px;overflow:auto;font-size:12px}.nr-message{min-height:40px;white-space:pre-wrap}.nr-side{min-width:0}@media(max-width:1100px){.nr-layout{grid-template-columns:1fr}}</style>
      <h1>Radar e rotas</h1><div class="sub">Experimental · observador isolado · salvar não movimenta · clique para criar e arraste para mover waypoints.</div>
      <div class="nr-controls"><label>Personagem / computador <select data-n="observer" aria-label="Personagem / computador"></select></label><label><input data-n="follow" type="checkbox" checked> Acompanhar mapa do personagem</label><button data-n="route-map">Ver mapa da rota</button><label>Alcance <input data-n="range" type="range" min="1" max="25" value="25"></label><label><input type="checkbox" data-n="kind-player" checked> Jogadores</label><label><input type="checkbox" data-n="kind-monster" checked> Monstros</label><label><input type="checkbox" data-n="kind-npc" checked> NPCs</label><label><input type="checkbox" data-n="names" checked> Nomes/distâncias</label><label><input type="checkbox" data-n="directions"> Direções</label><label><input type="checkbox" data-n="obstacles" checked> Colisões</label><label>Zoom <input data-n="zoom" type="range" min="2" max="6" value="3"></label></div>
      <p class="note" data-n="radar-status"></p><div class="nr-layout"><div><div class="nr-map"><canvas data-n="canvas" width="768" height="768" tabindex="0" aria-label="Mapa e editor de waypoints"></canvas></div><p class="note">Cinza: bloqueado · verde escuro: seguro · amarelo tracejado: ligação direta · azul contínuo: A* · branco: observador · roxo: NPC.</p><div data-n="coords" class="note">X → direita · Y → baixo · coordenadas em tiles do terreno.</div></div>
      <div class="nr-side"><select data-n="saved"></select><div class="nr-controls"><button data-n="new">Nova</button><button data-n="duplicate">Duplicar</button><button data-n="delete">Excluir</button></div>
      <label>Nome da rota <input data-n="name" maxlength="120" aria-label="Nome da rota"></label><label>Mapa da rota <select data-n="world"></select></label>
      <p class="note" data-n="installed-routes"></p><p class="note" data-n="binding-info"></p><button data-n="bind-current">Associar ao personagem selecionado</button>
      <label>Destino <select data-n="target"><option value="point">Ponto</option><option value="spot">Spot (até 2 tiles)</option><option value="npc">NPC</option></select></label><select data-n="npcs"></select><button data-n="set-npc">Usar NPC e último waypoint</button><p class="note" data-n="npc-info"></p><label><input type="checkbox" data-n="binding"> Usar associação da rota</label><select data-n="back"></select><button data-n="return">Abrir rota de retorno</button><div class="nr-points" data-n="points"></div><p class="note" data-n="draft"></p><div class="nr-controls"><button data-n="save">Salvar</button><button data-n="preview">Validar / prévia A*</button><button data-n="history">Histórico</button></div><div class="nr-history" data-n="versions"></div><label><input type="checkbox" data-n="install"> Instalar esta revisão para retorno automático ao spot</label><div class="nr-controls"><button data-n="execute" disabled>Confirmar envio ao agente…</button></div><p class="note">Execução exige agente compatível, mapa confirmado e canal de teste habilitado. A prévia expira em 30 segundos.</p><div class="nr-message note" data-n="message"></div></div></div>
      <h2>Execuções de rotas</h2><p class="note">Histórico do personagem selecionado e, quando aberta, da rota atual. Envio não é confirmação de chegada.</p><button data-n="refresh-runs">Atualizar execuções</button><div data-n="runs" style="overflow:auto;max-height:280px;margin-top:10px"></div>`;
    $('refresh-runs').onclick = () => loadRuns(true);
    $('world').innerHTML = '<option value="1">Lorencia</option><option value="38">Kanturu</option>';
    $('name').placeholder = 'Nome da rota';
    $('name').title = 'Este nome identifica a rota. Escolha o personagem no seletor Personagem / computador.';
    $('observer').onchange = () => { observer = $('observer').value; observerChosen = true; followWorld = null; followObserver = true; prior = null; confirmedNpcs = []; preview = null; $('npcs').innerHTML='<option value="">Aguardando duas leituras do observador</option>'; delete $('npcs').dataset.signature; syncMap(); draw(); loadRuns(true); };
    $('follow').onchange = () => { followObserver = $('follow').checked; syncMap(); draw(); };
    $('route-map').onclick = () => { followObserver = false; syncMap(); draw(); };
    ['range', 'names', 'directions', 'obstacles', 'kind-player', 'kind-monster', 'kind-npc'].forEach(k => $(k).oninput = draw);
    $('zoom').oninput = () => { $('canvas').style.width = $('canvas').style.height = (256 * Number($('zoom').value)) + 'px'; };
    $('name').oninput = () => { draft.name = $('name').value; changed(); };
    $('world').onchange = () => { if (draft.waypoints.length && !confirm('Trocar de mapa remove os waypoints deste rascunho. Continuar?')) { $('world').value = draft.world; return; } followObserver = false; $('follow').checked = false; draft.world = Number($('world').value); draft.waypoints = []; draft.target = { kind: 'point' }; draft.return_route = null; selection = -1; changed(); fields(); };
    $('target').onchange = () => { const kind = $('target').value; if (kind === 'npc') { notice('Selecione um NPC confirmado e use o botão abaixo.'); $('target').value = draft.target.kind; return; } draft.target = { kind }; changed(); };
    $('binding').onchange = () => { if (!$('binding').checked) draft.bindings = []; changed(); $('binding-info').textContent = $('binding').checked ? 'Associação será mantida ao salvar; use o botão acima para escolher outro personagem.' : 'Rota sem associação a personagem.'; };
    $('bind-current').onclick = action(async () => { const o = fresh(); if (!o) throw new Error('Selecione um personagem com leitura confirmada.'); draft.bindings = [{machine:o.machine,char_name:o.snapshot.context.char_name}]; changed(); fields(); notice('Associação alterada no rascunho. Salve para confirmar.'); });
    $('back').onchange = () => { draft.return_route = $('back').value || null; changed(); };
    function load(r) { if (dirty && !confirm('Descartar as alterações não salvas?')) { $('saved').value=draft.id; return; } editGeneration++; draft = clone(r); if (draft.id) followObserver = false; $('follow').checked = followObserver; dirty = false; preview = null; selection = -1; fields(); loadRuns(true); }
    $('saved').onchange = () => { const r = routes.find(r => r.id === $('saved').value); if (r) load(r); };
    $('new').onclick = () => load(newRoute(displayedWorld()));
    $('duplicate').onclick = () => { draft = clone(draft); draft.id = ''; draft.revision = 0; draft.name += ' (cópia)'; changed(); fields(); };
    $('return').onclick = () => { const r = routes.find(r => r.id === draft.return_route); if (r) load(r); };
    $('delete').onclick = action(async () => { if (!draft.id || !confirm('Excluir esta rota e desativar sua instalação nos agentes conectados?')) return; await api('/api/routes/' + draft.id, 'DELETE'); draft = newRoute(draft.world); dirty = false; preview = null; await refresh(); fields(); });
    $('save').onclick = action(save);
    $('set-npc').onclick = action(async () => { const o = fresh(); if (!o || o.snapshot.context.world !== draft.world || !$('npcs').value || !draft.waypoints.length) throw new Error('NPC confirmado, observador neste mapa e waypoint de interação são obrigatórios.'); const npc = JSON.parse($('npcs').value), interaction = draft.waypoints.at(-1); if(!confirmedNpcs.some(n => n.class===npc.class && n.name===npc.name && distance(n,npc)<=1)) throw new Error('NPC mudou ou não foi reconfirmado neste observador.'); if (distance(npc, interaction) > 3) throw new Error('A interação precisa ficar a até 3 tiles do NPC.'); draft.target = { kind: 'npc', npc: { class: npc.class, name: npc.name, location: spot(draft.world, npc.x, npc.y), interaction: clone(interaction), radius: 3 } }; changed(); fields(); });
    $('preview').onclick = action(async () => { if (dirty || !draft.id) throw new Error('Salve a revisão antes de validar.'); const o = fresh(), live = o && o.snapshot.context.world===draft.world; preview = await api('/api/navigation/preview', 'POST', { machine: o?.machine || '', char_name: o?.snapshot.context.char_name || '', route: draft, origin: live ? null : draft.waypoints[0] }); preview.expiresAt=Date.now()+(preview.expires_in_s || 0)*1000; notice(`${live ? 'Caminho validado' : 'Prévia estática desde o primeiro waypoint'}: ${preview.plan.tiles} tiles, ${preview.plan.segments.length} segmentos. ${live ? 'Nenhum movimento enviado.' : 'Para executar, selecione um observador neste mapa e gere outra prévia.'}`); draw(); });
    $('execute').onclick = action(async () => {
      const o = fresh(); if (!o || !preview || dirty) throw new Error('Gere uma prévia válida.');
      const install = $('install').checked;
      if (install && draft.target.kind !== 'spot') throw new Error('Retorno automático exige destino do tipo Spot.');
      if (!confirm(`Enviar a rota “${draft.name}” r${draft.revision} para ${o.snapshot.context.char_name} em ${o.machine}, mapa ${draft.world}, destino (${draft.waypoints.at(-1).x}, ${draft.waypoints.at(-1).y})?${install ? '\nInstalar também para o retorno automático a este spot.' : ''}\nO agente movimentará esta janela do jogo.`)) return;
      const p = preview; preview = null; draw(); const r = await api('/api/navigation/execute', 'POST', { token: p.token, confirmed: true, install_for_recovery: install }); notice(r.status); loadRuns(true);
    });
    $('history').onclick = action(async () => { if (!draft.id) throw new Error('Salve a rota primeiro.'); history = await api('/api/routes/' + draft.id + '/history'); $('versions').innerHTML = history.map((h, i) => `<div>r${h.route.revision} · ${new Date(h.ts * 1000).toLocaleString('pt-BR')}${h.deleted ? ' · excluída' : ''} <button data-restore="${i}">Restaurar</button></div>`).join(''); $('versions').querySelectorAll('button').forEach(b => b.onclick = action(async () => { const h = history[Number(b.dataset.restore)]; if (!confirm('Restaurar a revisão ' + h.route.revision + ' como nova revisão?')) return; draft = await api(`/api/routes/${h.route.id}/restore/${h.route.revision}`, 'POST'); dirty = false; preview = null; await refresh(); fields(); notice('Revisão restaurada. Agentes mantêm a revisão instalada até novo envio confirmado.'); })); });
    const canvas = $('canvas');
    const atEvent = e => { const rect = canvas.getBoundingClientRect(); return tileAt(e.clientX - rect.left, e.clientY - rect.top, rect); };
    canvas.onpointerdown = e => { if (e.button !== 0) return; if (displayedWorld() !== draft.world) { notice('Use “Ver mapa da rota” para editar os pontos preservados, ou “Nova” para uma rota no mapa do personagem.', true); return; } const p = atEvent(e); selection = draft.waypoints.findIndex(w => distance(w, p) <= 2); drag = { index: selection, start: p, moved: false }; canvas.setPointerCapture(e.pointerId); canvas.focus(); points(); draw(); };
    canvas.onpointermove = e => { const p = atEvent(e), world = displayedWorld(); const code = terrains.get(world)?.bytes[p.y * 256 + p.x]; $('coords').textContent = `${worldName(world)} (${p.x}, ${p.y}) · ${code === 1 ? 'bloqueado' : code === 2 ? 'zona segura' : 'terreno acessível'} · X → direita / Y → baixo`; if (drag?.index >= 0 && world === draft.world && distance(p, drag.start) > 0) { draft.waypoints[drag.index] = spot(draft.world, p.x, p.y); drag.moved = true; changed(); points(); } };
    canvas.onpointerup = e => { if (!drag) return; if (displayedWorld() === draft.world && drag.index < 0 && draft.waypoints.length < 64) { const p = atEvent(e); draft.waypoints.push(spot(draft.world, p.x, p.y)); selection = draft.waypoints.length - 1; changed(); points(); } drag = null; if (canvas.hasPointerCapture(e.pointerId)) canvas.releasePointerCapture(e.pointerId); };
    canvas.onpointercancel = () => { drag = null; };
    canvas.onkeydown = e => { if (e.key === 'Delete' && selection >= 0) { draft.waypoints.splice(selection, 1); selection = -1; changed(); points(); e.preventDefault(); } };
    api('/api/maps').then(m => { maps = m; $('world').innerHTML = maps.filter(m => m.terrain).map(m => `<option value="${m.world}">${esc(m.name)} · W${m.world}</option>`).join(''); $('world').value = draft.world; draw(); }).catch(e => notice(e.message, true));
    refresh().catch(e => notice(e.message, true)); fields(); update(state);
  }
  global.ZenNavigation = { mount, update, model };
  setInterval(() => { if(root?.isConnected && root.dataset.navigation === 'mounted') draw(); }, 1000);
})(typeof window === 'undefined' ? globalThis : window);
