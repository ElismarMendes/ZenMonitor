// Testes de estado/edição em DOM simulado, sem navegador e sem acesso à rede.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

function editor(initialState={radar:[],results:[],navigation_execution:false},options={}) {
  const controls = new Map(), calls = [], timers = [];
  let saved = [], deferSave = false, resolveSave, clock = 0;
  const pendingRuns=[];
  const document = { activeElement: null, createElement: tag => new Element(tag) };
  const context = new Proxy({ createImageData: (w,h) => ({ data: new Uint8ClampedArray(w*h*4) }) }, {
    get: (o,k) => k in o ? o[k] : () => {},
  });
  class Element {
    constructor(tag = 'div') { this.tagName=tag;this.dataset={};this.style={};this._value='';this._html='';this._text='';this.checked=false;this.scrollTop=0;this.captures=new Set();this.writes=0; }
    set value(v) { this._value=String(v); } get value() { return this._value; }
    set textContent(v) { this._text=String(v);this._html=''; } get textContent() { return this._text; }
    set innerHTML(v) { this._html=String(v);this.writes++; } get innerHTML() { return this._html; }
    querySelectorAll() {
      return [...this._html.matchAll(/<button\b([^>]*)>/g)].map(m => {
        const b=new Element('button');for(const a of m[1].matchAll(/data-([\w-]+)="([^"]*)"/g)) b.dataset[a[1]]=a[2];return b;
      });
    }
    insertAdjacentHTML(_,html) { this._html+=html; }
    getContext() { return context; }
    getBoundingClientRect() { return { left:0,top:0,width:768,height:768 }; }
    setPointerCapture(id) { this.captures.add(id); }
    hasPointerCapture(id) { return this.captures.has(id); }
    releasePointerCapture(id) { this.captures.delete(id); }
    focus() { document.activeElement=this; }
  }
  const root=new Element();root.isConnected=true;root.dataset.navigation='mounted';
  Object.defineProperty(root,'innerHTML',{
    get() {return this._html;},
    set(html) {
      this._html=html;this.writes++;controls.clear();
      for(const m of html.matchAll(/<([a-z]+)\b([^>]*\bdata-n="([^"]+)"[^>]*)>/g)) {
        const el=new Element(m[1]);el.checked=/\bchecked\b/.test(m[2]);el.value=/\bvalue="([^"]*)"/.exec(m[2])?.[1]||'';controls.set(m[3],el);
      }
    },
  });
  root.querySelector=selector=>controls.get(/data-n="([^"]+)"/.exec(selector)?.[1])||null;
  const response=data=>({ok:true,json:async()=>structuredClone(data)});
  async function fetch(path,init={}) {
    const method=init.method||'GET',body=init.body?JSON.parse(init.body):null;calls.push({path,method,body});
    if(path==='/api/maps')return response([{world:1,name:'Lorencia',terrain:true},{world:2,name:'Dungeon',terrain:true},{world:5,name:'Lost Tower',terrain:true},{world:38,name:'Kanturu',terrain:true}]);
    if(/^\/api\/terrain\/(1|2|5|38)$/.test(path))return response({codes:Buffer.alloc(65536).toString('base64')});
    if(path==='/api/routes'&&method==='GET')return response(saved);
    if(path.startsWith('/api/navigation/runs?')) {
      if(options.deferRuns)return new Promise(resolve=>pendingRuns.push(resolve));
      return response(options.runs || []);
    }
    if(path==='/api/routes'&&method==='POST'){
      const route={...body,id:body.id||'route-test',revision:body.revision+1};saved=[route];
      if(deferSave)return new Promise(resolve=>{resolveSave=()=>resolve(response(route));});
      return response(route);
    }
    if(path==='/api/navigation/preview')return response({token:null,executable:false,origin:body.origin,plan:{segments:[body.route.waypoints],tiles:body.route.waypoints.length}});
    throw new Error('Chamada inesperada no teste: '+path);
  }
  class Clock extends Date { static now() {return clock;} }
  const sandbox={document,fetch,sessionStorage:{getItem:()=> 'test-token'},confirm:()=>true,Date:Clock,
    atob:s=>Buffer.from(s,'base64').toString('binary'),setInterval:fn=>timers.push(fn),URLSearchParams,console};
  sandbox.window=sandbox;
  vm.runInNewContext(fs.readFileSync(require.resolve('./navigation.js'),'utf8'),sandbox);
  const state=initialState;sandbox.ZenNavigation.mount(root,state);
  const $=name=>controls.get(name);
  function addPoint(x,y) {const e={button:0,pointerId:1,clientX:(x+.5)*3,clientY:(y+.5)*3};$('canvas').onpointerdown(e);$('canvas').onpointerup(e);}
  return {$,root,calls,state,api:sandbox.ZenNavigation,addPoint,
    resolveRuns(index,rows){pendingRuns[index](response(rows));},
    defer(){deferSave=true;},resolve(){deferSave=false;resolveSave();},advance(ms){clock+=ms;timers.forEach(fn=>fn());}};
}
const flush=()=>new Promise(resolve=>setImmediate(resolve));

test('WebSocket mantém rascunho, controles e rolagem do editor',async()=>{
  const e=editor();await flush();e.addPoint(122,108);
  const name=e.$('name'),points=e.$('points');name.value='Rota do Trainer';name.oninput();name.focus();points.scrollTop=73;
  const html=points.innerHTML,writes=e.root.writes;
  for(let i=0;i<10;i++)e.api.update({...e.state,now:i});
  assert.equal(e.root.writes,writes);assert.equal(e.$('name'),name);assert.equal(name.value,'Rota do Trainer');
  assert.equal(points.innerHTML,html);assert.equal(points.scrollTop,73);assert.match(e.$('draft').textContent,/não salvas/);
  assert.ok(!e.calls.some(c=>c.path==='/api/navigation/execute'));
});

test('resposta de salvar não apaga edição feita durante o envio',async()=>{
  const e=editor();await flush();e.addPoint(122,108);e.$('name').value='Antes';e.$('name').oninput();
  e.defer();const saving=e.$('save').onclick();await flush();
  e.$('name').value='Depois';e.$('name').oninput();e.resolve();await saving;
  assert.equal(e.$('name').value,'Depois');assert.match(e.$('draft').textContent,/não salvas/);
  await e.$('save').onclick();
  const posts=e.calls.filter(c=>c.path==='/api/routes'&&c.method==='POST');
  assert.equal(posts[0].body.name,'Antes');assert.equal(posts[1].body.name,'Depois');
  assert.equal(posts[1].body.revision,1);assert.equal(posts[1].body.waypoints[0].x,122);
});

test('prévia estática não dispara movimento nem habilita o envio',async()=>{
  const e=editor();await flush();e.addPoint(122,108);await e.$('save').onclick();await e.$('preview').onclick();
  assert.equal(e.$('execute').disabled,true);
  assert.ok(e.calls.some(c=>c.path==='/api/navigation/preview'&&c.body.origin.world===1));
  assert.ok(!e.calls.some(c=>c.path==='/api/navigation/execute'));
});

function observation(name,world,pid=11,sequence=2) {
  return {machine:'PC1',snapshot:{context:{char_name:name,world,pid,session:'test-'+pid,epoch:0,sequence,age_ms:0,map_confirmed:true},position:{world,x:160,y:197},entities:[{kind:'monster',name:'Mob '+name,class:1,x:160,y:198}],range:25}};
}
test('primeira abertura seleciona personagem e segue seu mapa em vez de Lorencia',async()=>{
  const e=editor({radar:[observation('x9901',38)],results:[],navigation_execution:false});await flush();
  assert.equal(e.$('observer').value,JSON.stringify(['PC1','x9901']));
  assert.equal(e.$('world').value,'38');
  assert.match(e.$('radar-status').textContent,/x9901 · Kanturu/);
  assert.match(e.$('radar-status').textContent,/1 entidades/);
  assert.ok(e.calls.some(c=>c.path==='/api/terrain/38'));
  assert.ok(!e.calls.some(c=>c.path==='/api/navigation/execute'));
});
test('trocar personagem e seguir teleporte preserva os waypoints do outro mapa',async()=>{
  const state={radar:[observation('x9901',38),observation('YouSee',2,12)],results:[],navigation_execution:false};
  const e=editor(state);await flush();e.addPoint(160,197);e.$('name').value='Minha rota de Kanturu';e.$('name').oninput();
  const points=e.$('points').innerHTML;
  e.$('observer').value=JSON.stringify(['PC1','YouSee']);e.$('observer').onchange();await flush();
  assert.match(e.$('radar-status').textContent,/YouSee · Dungeon/);
  assert.match(e.$('radar-status').textContent,/Rota em Kanturu preservada/);
  assert.equal(e.$('name').value,'Minha rota de Kanturu');assert.equal(e.$('world').value,'38');
  assert.equal(e.$('points').innerHTML,points);
  e.addPoint(20,20);assert.equal(e.$('points').innerHTML,points,'não cria tile de Dungeon na rota de Kanturu');
  e.api.update({...state,radar:[observation('YouSee',1,12,3)]});await flush();
  assert.match(e.$('radar-status').textContent,/YouSee · Lorencia/);
  assert.equal(e.$('points').innerHTML,points);
  e.$('route-map').onclick();assert.match(e.$('radar-status').textContent,/exibindo Kanturu/);
  await e.$('save').onclick();
  const saved=e.calls.find(c=>c.path==='/api/routes'&&c.method==='POST').body;
  assert.equal(saved.world,38);assert.deepEqual(saved.waypoints,[{world:38,x:160,y:197}]);
  assert.ok(!e.calls.some(c=>c.path==='/api/navigation/execute'));
});
test('observação sem mapa confirmado não é selecionada automaticamente',async()=>{
  const o=observation('x9901',38);o.snapshot.context.map_confirmed=false;
  const e=editor({radar:[o],results:[],navigation_execution:false});await flush();
  assert.equal(e.$('observer').value,'');
  assert.doesNotMatch(e.$('radar-status').textContent,/x9901 · Kanturu/);
  o.snapshot.context.map_confirmed=true;e.api.update({radar:[o],results:[],navigation_execution:false});await flush();
  assert.match(e.$('radar-status').textContent,/x9901 · Kanturu/);
});
test('observar outro personagem não reassocia uma rota salva ao salvar',async()=>{
  const state={radar:[observation('x9901',38),observation('YouSee',2,12)],results:[],navigation_execution:false};
  const e=editor(state);await flush();e.addPoint(160,197);
  await e.$('bind-current').onclick();await e.$('save').onclick();
  e.$('observer').value=JSON.stringify(['PC1','YouSee']);e.$('observer').onchange();
  e.$('name').value='Nome alterado';e.$('name').oninput();await e.$('save').onclick();
  const posts=()=>e.calls.filter(c=>c.path==='/api/routes'&&c.method==='POST');
  assert.deepEqual(posts().at(-1).body.bindings,[{machine:'PC1',char_name:'x9901'}]);
  assert.match(e.$('binding-info').textContent,/PC1 \/ x9901/);
  await e.$('bind-current').onclick();await e.$('save').onclick();
  assert.deepEqual(posts().at(-1).body.bindings,[{machine:'PC1',char_name:'YouSee'}]);
});
test('terreno pendente é reutilizado e histórico não é consultado a cada WebSocket',async()=>{
  const e=editor();for(let i=0;i<10;i++)e.api.update({...e.state,now:i});await flush();
  assert.equal(e.calls.filter(c=>c.path==='/api/terrain/1').length,1);
  assert.equal(e.calls.filter(c=>c.path.startsWith('/api/navigation/runs?')).length,1);
  e.advance(15001);e.api.update(e.state);await flush();
  assert.equal(e.calls.filter(c=>c.path.startsWith('/api/navigation/runs?')).length,2);
});
test('histórico atrasado de outro personagem não sobrescreve a seleção atual',async()=>{
  const state={radar:[observation('x9901',38),observation('YouSee',2,12)],results:[],navigation_execution:false};
  const e=editor(state,{deferRuns:true});await flush();
  e.$('observer').value=JSON.stringify(['PC1','YouSee']);e.$('observer').onchange();
  const row=(name)=>({id:1,machine:'PC1',char_name:name,route_name:'Rota',revision:1,status:'enviada',started_at:1,message:null});
  e.resolveRuns(1,[row('YouSee')]);await flush();
  assert.match(e.$('runs').innerHTML,/YouSee/);assert.match(e.$('runs').innerHTML,/aguardando chegada/);
  e.resolveRuns(0,[row('x9901')]);await flush();
  assert.match(e.$('runs').innerHTML,/YouSee/);assert.doesNotMatch(e.$('runs').innerHTML,/x9901/);
  const requests=e.calls.filter(c=>c.path.startsWith('/api/navigation/runs?'));
  assert.match(requests.at(-1).path,/machine=PC1&char_name=YouSee/);
});

test('installed revisions follow only the fresh selected character',async()=>{
  const a=observation('x9901',38),b=observation('YouSee',2,12);
  a.snapshot.installed_routes=[{id:'r1',name:'Home',revision:7,destination:{world:38,x:160,y:197}}];
  b.snapshot.installed_routes=[];
  const state={radar:[a,b],results:[],navigation_execution:false};
  const e=editor(state);await flush();
  assert.match(e.$('installed-routes').textContent,/Home \/ r7/);
  e.$('observer').value=JSON.stringify(['PC1','YouSee']);e.$('observer').onchange();
  assert.match(e.$('installed-routes').textContent,/Nenhuma rota/);
  delete b.snapshot.installed_routes;e.api.update(state);
  assert.match(e.$('installed-routes').textContent,/aguardando/);
});
