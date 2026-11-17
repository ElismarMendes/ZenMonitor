const test = require('node:test');
const assert = require('node:assert/strict');
const m = require('./navigation.js');
test('conversão painel/tiles preserva orientação e bordas', () => {
  for (const size of [256,768,1536]) for (const [x,y] of [[0,0],[122,110],[255,255],[160,197]]) {
    assert.deepEqual(m.tileAt((x+.5)*size/256,(y+.5)*size/256,{width:size,height:size}),{x,y});
  }
  assert.deepEqual(m.tileAt(-1,999,{width:256,height:256}),{x:0,y:255});
});
test('mesmo nome em PCs diferentes não compartilha observador', () => {
  const o=machine=>({machine,snapshot:{context:{char_name:'x9901'}}});
  assert.notEqual(m.key(o('PC1')),m.key(o('PC2')));
});
test('mapa não confirmado e observação velha não viram entidades válidas', () => {
  const s={context:{world:1,map_confirmed:true,age_ms:0},position:{world:1}};
  assert.ok(m.usable(s));s.context.age_ms=10001;assert.ok(!m.usable(s));
  s.context.age_ms=0;s.position.world=38;assert.ok(!m.usable(s));
  s.position.world=1;s.context.map_confirmed=false;assert.ok(!m.usable(s));
});
