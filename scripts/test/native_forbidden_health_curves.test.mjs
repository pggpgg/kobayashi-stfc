import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import {extractNativeForbiddenHealthCurve, applyNativeForbiddenHealthCurves} from "../apply_native_forbidden_health_curves.mjs";
const concat=(...parts)=>Buffer.concat(parts);
function varint(value){let n=BigInt(value);const out=[];do{out.push(Number(n&127n)|(n>127n?128:0));n>>=7n;}while(n);return Buffer.from(out);}
const integer=(field,value)=>concat(varint(field*8),varint(value));
const bytes=(field,value)=>concat(varint(field*8+2),varint(value.length),value);
function fixture(target=1){
  const details=[],maps=[];
  for(const[id,modifier,operation,recipient,trigger,tier,value]of[[331547130,60,3,target,24,8,19.5],[1746739507,9,1,1,24,3,0.18],[2885215064,76,2,6,18,6,0.17]]){
    const values=Array.from({length:99},(_,rank)=>rank===45||rank===48?value:0);
    const doubles=Buffer.alloc(values.length*8),chances=Buffer.alloc(values.length*4);
    values.forEach((v,i)=>{doubles.writeDoubleLE(v,i*8);chances.writeFloatLE(1,i*4);});
    const spec=concat(integer(1,id),integer(7,recipient),integer(8,trigger),integer(12,operation),bytes(13,chances),integer(18,modifier),bytes(21,doubles));
    maps.push(bytes(1,concat(integer(1,id),bytes(2,spec))));
    details.push({tier,buffs:[{id,values:values.map(value=>({value,chance:1}))}]});
  }
  const group=bytes(1,concat(integer(1,141),bytes(2,concat(...maps))));
  return {cache:concat(group,bytes(2,Buffer.from("private-player-state-must-not-escape"))),detail:Buffer.from(JSON.stringify({id:2439729135,buffs:details}))};
}
test("native HP curve keeps fractional units, tier and all ranks without player state",()=>{
  const {cache,detail}=fixture(),manifest=extractNativeForbiddenHealthCurve(cache,detail);
  assert.equal(manifest.items[0].values[48],19.5);
  assert.equal(manifest.items[0].values.length,99);
  assert.equal(manifest.items[0].minimum_tier,8);
  assert.doesNotMatch(JSON.stringify(manifest),/private-player/);
  assert.throws(()=>extractNativeForbiddenHealthCurve(fixture(6).cache,detail),/Unreviewed Quantum/);
  const drift=JSON.parse(detail);drift.buffs[0].tier=7;
  assert.throws(()=>extractNativeForbiddenHealthCurve(cache,Buffer.from(JSON.stringify(drift))),/binding drift/);
});
test("applying reviewed curve is idempotent and preserves unrelated tech effects",()=>{
  const {cache,detail}=fixture(),manifest=extractNativeForbiddenHealthCurve(cache,detail);
  const catalog={items:[{fid:2439729135,bonuses:[{stat:"shield_hp",value:0.195,operator:"add"},{stat:"crit_chance",value:0.16,operator:"add"},{stat:"shield_mitigation",value:0.15,operator:"add"},{stat:"weapon_damage",value:2,operator:"add"}]},{fid:1,bonuses:[]}]};
  const result=applyNativeForbiddenHealthCurves(catalog,manifest);
  assert.deepEqual(applyNativeForbiddenHealthCurves(result,manifest),result);
  assert.deepEqual(result.items[0].bonuses[3],catalog.items[0].bonuses[3]);
  assert.equal(result.items[0].bonuses[1].rank_curve.values[48],0.18);
  assert.equal(result.items[0].bonuses[2].rank_curve.values[48],0.17);
  assert.deepEqual(result.items[1],catalog.items[1]);
  assert.equal(result.items[0].bonuses[0].value,19.5);
  assert.equal(catalog.items[0].bonuses[0].value,0.195);
});
test("published native manifest and equipment catalog retain the same complete curve",()=>{
  const manifest=JSON.parse(fs.readFileSync(new URL("../../data/native_forbidden_health_curves.json",import.meta.url)));
  const catalog=JSON.parse(fs.readFileSync(new URL("../../data/forbidden_chaos_tech.json",import.meta.url)));
  assert.deepEqual(applyNativeForbiddenHealthCurves(catalog,manifest),catalog);
  assert.equal(manifest.items[0].values[48],19.5);
  assert.equal(manifest.provenance.cache_sha256,"b472e4460e3869cc8abb44e456a0873e3c5702ed6f3a9701b18d27938022f3cf");
});
