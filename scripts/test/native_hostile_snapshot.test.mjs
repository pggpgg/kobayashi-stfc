import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import {extractNativeHostileSnapshot,normalizeNativeHostile,mergeNativeHostileSupplement,main} from "../import_native_hostile_snapshot.mjs";

const read=rel=>JSON.parse(fs.readFileSync(new URL(`../../${rel}`,import.meta.url)));
const source=()=>read("data/native_hostile_static_snapshot.json");
const index=()=>({data_version:"synthetic",source_note:"Synthetic fixture",hostiles:[{id:"keep",level:1,hostile_name:"Existing"}]});
const registry=()=>({hostiles:{source:"data-stfc-space",data_version:"synthetic",path:"hostiles/index.json"},ships:{source:"untouched"}});

test("reviewed native components preserve base HP and six ordered weapon slots",()=>{
  const row=normalizeNativeHostile(source());
  assert.equal(row.hull_health,12048861184);
  assert.equal(row.shield_health,22032201728);
  assert.equal(row.armor,516380);
  assert.equal(row.shield_deflection,1824542);
  assert.equal(row.dodge,206552);
  assert.equal(row.shield_mitigation,0.800000011920929);
  const weapons=row.components.filter(c=>c.data.tag==="Weapon");
  assert.deepEqual(weapons.map(c=>c.id),[3804869128,3804869128,3804869128,3804869128,404883461,404883461]);
  assert.deepEqual(weapons.map(c=>c.order),[7,8,9,10,11,12]);
  assert.deepEqual(weapons.map(c=>c.data.weapon_type),[2,2,2,2,1,1]);
  assert.equal(weapons[0].data.minimum_damage,14458633);
  assert.equal(weapons[4].data.maximum_damage,6885063);
  assert.equal(row.armor_piercing,351138);
  assert.equal(row.shield_piercing,2457966);
  assert.equal(row.accuracy,1404552);
  assert.equal(row.crit_damage_is_bonus,true);
  assert.equal(row.crit_damage,1.5);
  assert.ok(weapons.every(w=>w.data.crit_modifier===1.5&&w.data.shots===2));
  assert.equal(row.upstream_ship_type,0,"native hull class is not an invented API ship category");
});

test("only verified L62 mechanics are enabled; native rank and dummy rows remain explicit",()=>{
  const snapshot=source(),row=normalizeNativeHostile(snapshot);
  assert.deepEqual(row.ability.filter(a=>a.modeling_status==="modeled").map(a=>a.id),[1761765323,4196543321,4173861882]);
  assert.equal(row.ability.find(a=>a.id===4196543321).values[0].value,14.91);
  assert.equal(row.ability.find(a=>a.id===4173861882).values[0].value,.01);
  assert.equal(row.ability.find(a=>a.id===1761765323).values[0].value,0);
  assert.ok(row.ability.filter(a=>a.modeling_status==="unsupported").every(a=>a.unsupported_reason));
  assert.equal(row.ability.find(a=>a.id===1421423212).modeling_status,"unsupported");
  assert.equal(snapshot.skills.find(s=>s.id===1421423212).raw_value_at_index_61,10000000);
  assert.deepEqual(snapshot.skills.find(s=>s.id===1421423212).condition_codes,[86001]);
  snapshot.hull.level=61;
  assert.throws(()=>normalizeNativeHostile(snapshot),/identity/);
});

test("missing or malformed source fields fail closed instead of becoming usable stats",()=>{
  const missing=source();missing.components=missing.components.filter(c=>c.id!==1545451774);
  assert.throws(()=>normalizeNativeHostile(missing),/Missing linked/);
  const invalid=source();invalid.components.find(c=>c.id===3804869128).combat_data.minimum_damage=NaN;
  assert.throws(()=>normalizeNativeHostile(invalid),/Invalid native weapon/);
  const changed=source();changed.skills.find(s=>s.id===4173861882).target_code=1;
  assert.throws(()=>normalizeNativeHostile(changed),/semantics changed/);
  const chance=source();chance.skills.find(s=>s.id===4196543321).ranked_chances=[.5];
  assert.throws(()=>normalizeNativeHostile(chance),/chance or attributes/);
  assert.throws(()=>extractNativeHostileSnapshot(Buffer.from([0x0a,0x01,0x80])),/varint/);
  assert.throws(()=>extractNativeHostileSnapshot(Buffer.alloc(0)),/size/);
});

test("supplement merge is idempotent and preserves every unrelated index and registry entry",()=>{
  const base=index(),reg=registry(),a=mergeNativeHostileSupplement(source(),base,reg);
  const b=mergeNativeHostileSupplement(source(),a.index,a.registry);
  assert.deepEqual(a,b);
  assert.deepEqual(a.index.hostiles[0],base.hostiles[0]);
  assert.deepEqual(a.registry.ships,reg.ships);
  assert.equal(a.index.hostiles.length,2);
  assert.equal(base.hostiles.length,1,"pure merge does not mutate inputs");
  assert.match(a.registry.hostiles.source,/native-static-supplement/);
  assert.equal(a.registry.hostiles.data_version,a.index.data_version);
  const newer=registry();newer.hostiles.last_updated="2026-11-12";
  assert.equal(mergeNativeHostileSupplement(source(),base,newer).registry.hostiles.last_updated,"2026-11-12");
  const duplicate=index();duplicate.hostiles.push({id:"2978535446"},{id:"2978535446"});
  assert.throws(()=>mergeNativeHostileSupplement(source(),duplicate,reg),/Duplicate hostile/);
});

test("CLI check never writes; invalid snapshot leaves existing output bytes intact",()=>{
  const fixtureRoot=path.join(process.cwd(),"target");
  fs.mkdirSync(fixtureRoot,{recursive:true});
  const temp=fs.mkdtempSync(path.join(fixtureRoot,"kobayashi-native-hostile-"));
  try{
    fs.mkdirSync(path.join(temp,"data/hostiles"),{recursive:true});
    const put=(file,value)=>fs.writeFileSync(path.join(temp,file),JSON.stringify(value));
    put("data/native_hostile_static_snapshot.json",source());
    put("data/hostiles/index.json",index());put("data/registry.json",registry());
    main(["--root",temp,"--write"]);
    const before=fs.readFileSync(path.join(temp,"data/hostiles/2978535446.json"));
    main(["--root",temp,"--check"]);
    assert.deepEqual(fs.readFileSync(path.join(temp,"data/hostiles/2978535446.json")),before);
    const invalid=source();invalid.hull.id=1;put("data/native_hostile_static_snapshot.json",invalid);
    assert.throws(()=>main(["--root",temp,"--write"]),/identity/);
    assert.deepEqual(fs.readFileSync(path.join(temp,"data/hostiles/2978535446.json")),before);
  }finally{fs.rmSync(temp,{recursive:true,force:true});}
});

test("published supplement is exactly reproducible without private cache or player files",()=>{
  const row=normalizeNativeHostile(source());
  assert.deepEqual(row,read("data/hostiles/2978535446.json"));
  assert.doesNotMatch(JSON.stringify(source()),/pierr|captured_battle|journal|sync_token|owner_id|player_id/);
  main(["--check"]);
});
