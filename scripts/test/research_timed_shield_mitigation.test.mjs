import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import {resolveBuffStatMappings} from "../lib/research_buff_resolve.mjs";

test("native timed self/opponent shield mitigation cannot become flat own-profile rows",()=>{
  const ctx={researchBuffMapping:{1478241687:"shield_mitigation",1856737936:"shield_mitigation"},
    researchBuffById:{},commonBuffNormalization:{},locaIdToStat:{}};
  for(const id of[1478241687,1856737936])assert.deepEqual(resolveBuffStatMappings(ctx,{id},35012),[]);
  const catalog=JSON.parse(fs.readFileSync(new URL("../../data/research_catalog.json",import.meta.url)));
  assert.ok(!catalog.items.some(r=>[2392190200,4147903968].includes(r.rid)));
});

test("canonical Immortal retains opponent subtraction and two-round scope beside KSG self bonus",()=>{
  const canonical=JSON.parse(fs.readFileSync(new URL("../../data/research_canonical.json",import.meta.url)));
  const immortal=canonical.overrides.find(r=>r.rid===4147903968).effects[0];
  assert.equal(immortal.target,"defender_opponent");
  assert.equal(immortal.trigger,"combat_begin");
  assert.equal(immortal.snapshot_by_level,true);
  assert.deepEqual(immortal.by_level,[-.005,-.01,-.015,-.02,-.025]);
  assert.deepEqual(immortal.conditions,[{type:"round_range",min:1,max:2}]);
  const ksg=canonical.overrides.find(r=>r.rid===2392190200).effects[0];
  assert.equal(ksg.incoming_shield_mitigation_rounds,2);
  assert.deepEqual(ksg.by_level,[.005,.01,.015,.02,.025]);
  const upstream=JSON.parse(fs.readFileSync(new URL("../../data/upstream/data-stfc-space/research/4147903968.json",import.meta.url)));
  const buff=upstream.buffs.find(b=>b.id===1856737936);
  assert.deepEqual(buff.values.slice(0,5).map(r=>-r.value),immortal.by_level);
});
