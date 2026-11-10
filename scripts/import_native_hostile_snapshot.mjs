#!/usr/bin/env node
/** Reviewed native supplement, independent of data.stfc.space API records.
 * node scripts/import_native_hostile_snapshot.mjs --cache <static-cache.bin> --write
 * node scripts/import_native_hostile_snapshot.mjs --write | --check
 * Reads only selected static HullSpecs(1), ComponentSpecs(12), ShipBonus(51)
 * and ActivatedAbilitySpecs(128). Never reads playerEntityGroups or battle logs.
 */
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";
import { fields } from "./inspect_stfc_combat_config.mjs";
import { extractNativeBuffSpec } from "./extract_stfc_research_buff_semantics.mjs";

export const NATIVE_HOSTILE_ID = 2978535446;
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SNAPSHOT = "data/native_hostile_static_snapshot.json";
const REVISION = "native-vger-gorn-l62-2026-11-10";
const HULL_SHA = "3fa46293f393ea412aa1f117715476bc34c7f617457a1a913791dea14f201b85";
const COMPONENT_IDS = [635429245,161618765,1545451774,2760464701,2821665303,409142410,2534309121,3804869128,3804869128,3804869128,3804869128,404883461,404883461,2498741777];
const BONUS_IDS = [3215268814,1761765323,4196543321,1029835177,1844469045,4173861882,4194583694,2026725568,1421423212];
const sha = bytes => crypto.createHash("sha256").update(bytes).digest("hex");
const nested = field => fields(field.value, field.dataOffset);
function one(rows, number, wire, required = false) {
  const matches = rows.filter(f=>f.number===number);
  if(matches.length>1 || (matches.length && matches[0].wire!==wire) || (required&&!matches.length)) throw Error(`Invalid native field ${number}`);
  return matches[0];
}
function integer(rows, number, fallback = 0) {
  const field=one(rows,number,0);
  if(!field)return fallback;
  const result=Number(BigInt.asIntN(64,field.value));
  if(!Number.isSafeInteger(result))throw Error(`Unsafe native integer ${number}`);
  return result;
}
function float(rows, number, required = true) {
  const field=one(rows,number,5,required);
  if(!field)return 0;
  const result=field.value.readFloatLE();
  if(!Number.isFinite(result)||result<0)throw Error(`Invalid native float ${number}`);
  return result;
}
function text(rows, number) {
  const field=one(rows,number,2,true);
  if(field.value.length>1024)throw Error("Native name too long");
  return new TextDecoder("utf-8",{fatal:true}).decode(field.value);
}
function packed(rows, number) {
  return rows.filter(f=>f.number===number).flatMap(field=>{
    if(field.wire===0)return [Number(BigInt.asIntN(64,field.value))];
    if(field.wire!==2)throw Error("Invalid packed wire type");
    const out=[];
    for(let i=0;i<field.value.length;){
      let value=0n,shift=0n,terminated=false;
      for(let n=0;n<10&&i<field.value.length;n++){
        const byte=field.value[i++];value|=BigInt(byte&127)<<shift;shift+=7n;
        if(!(byte&128)){terminated=true;break;}
      }
      const decoded=Number(BigInt.asIntN(64,value));
      if(!terminated||!Number.isSafeInteger(decoded))throw Error("Invalid packed native integer");
      out.push(decoded);
    }
    return out;
  });
}
function provenance(field) {
  return {sha256:sha(field.value),byte_offset:field.dataOffset,byte_length:field.value.length};
}
function only(rows, allowed, label) {
  if(rows.some(f=>!allowed.includes(f.number)))throw Error(`Unreviewed ${label} field`);
}
function component(payload) {
  const row=nested(payload);
  only(row,Array.from({length:23},(_,i)=>i+1),"component");
  const result={id:integer(row,2),native_type:integer(row,1),name:text(row,4),grade:integer(row,6),rarity:integer(row,7),subtype:integer(row,8),tier:integer(row,20),source:provenance(payload)};
  const dataField = new Map([[0,11],[3,14],[5,16],[8,19]]).get(result.native_type);
  if(dataField===undefined)return {...result,combat_data:null};
  const data=nested(one(row,dataField,2,true));
  if(result.native_type===0){
    only(data,[1,2,3],"armor");
    if(nested(one(data,3,2,true)).length)throw Error("Unreviewed armor modifiers");
    result.combat_data={tag:"Armor",hp:float(data,1),plating:float(data,2)};
  } else if(result.native_type===3){
    only(data,[1,2],"impulse");
    result.combat_data={tag:"Impulse",dodge:float(data,1),impulse:float(data,2)};
  } else if(result.native_type===5){
    only(data,[1,2,3,4,5],"shield");
    if(nested(one(data,3,2,true)).length)throw Error("Unreviewed shield modifiers");
    result.combat_data={tag:"Shield",absorption:float(data,1),hp:float(data,2),regeneration_time_seconds:integer(data,4),mitigation:float(data,5)};
  } else {
    only(data,[1,2,3,4,5],"weapon");
    if(integer(data,1)||float(data,2,false)||nested(one(data,3,2,true)).length||nested(one(data,4,2,true)).length)throw Error("Unreviewed weapon delay/debuff/modifier");
    const attack=nested(one(data,5,2,true));only(attack,Array.from({length:12},(_,i)=>i+1),"attack");
    if(integer(attack,4)||float(attack,5,false))throw Error("Unreviewed weapon damage-over-time");
    result.combat_data={tag:"Weapon",weapon_type:result.subtype,crit_chance:float(attack,1),warm_up:integer(attack,2),modulation:float(attack,3),cool_down:integer(attack,6),crit_modifier:float(attack,7),minimum_damage:integer(attack,8),penetration:integer(attack,9),shots:integer(attack,10),maximum_damage:integer(attack,11),accuracy:integer(attack,12)};
    if(![1,2].includes(result.subtype)||result.combat_data.shots<=0||result.combat_data.minimum_damage>result.combat_data.maximum_damage)throw Error("Invalid reviewed weapon");
  }
  return result;
}

export function extractNativeHostileSnapshot(cache) {
  if(!cache.length||cache.length>128*1024*1024)throw Error("Invalid static cache size");
  const selected=new Map();
  for(const field of fields(cache).filter(f=>f.number===1)){
    const row=nested(field),code=integer(row,1);
    if(![1,12,51,128].includes(code))continue;
    if(selected.has(code))throw Error("Duplicate selected static group");
    selected.set(code,nested(one(row,2,2,true)));
  }
  const lookup=(group,id)=>{
    const matches=(selected.get(group)??[]).filter(f=>f.number===1&&f.wire===2).map(nested).filter(row=>integer(row,1)===id);
    if(matches.length!==1)throw Error(`Expected one native ${group}/${id}`);
    return one(matches[0],2,2,true);
  };
  const payload=lookup(1,NATIVE_HOSTILE_ID),hull=nested(payload);
  if(sha(payload.value)!==HULL_SHA)throw Error("Reviewed L62 native hull identity changed");
  const defaults=packed(hull,13),bonusIds=packed(hull,35);
  if(JSON.stringify(defaults)!==JSON.stringify(COMPONENT_IDS)||JSON.stringify(bonusIds)!==JSON.stringify(BONUS_IDS))throw Error("Native hull links changed");
  const components=[...new Set(defaults)].map(id=>component(lookup(12,id)));
  const skills=bonusIds.map(id=>{
    const p=lookup(51,id),spec=extractNativeBuffSpec(p),row=nested(p);
    return {id,source_group:51,source:provenance(p),loca_id:integer(nested(one(row,15,2,true)),2),...spec,raw_value_at_index_61:spec.ranked_values[Math.min(61,spec.ranked_values.length-1)]};
  });
  const activated=packed(hull,28).map(id=>{
    const rows=(selected.get(128)??[]).filter(f=>f.number===1&&f.wire===2).filter(f=>integer(nested(f),1)===id);
    if(rows.length!==1)throw Error("Expected one native immunity hook");
    const row=nested(rows[0]);
    return {id,id_str:text(row,2),ability_type:integer(row,4),buff_ids:packed(row,13),source:provenance(rows[0])};
  });
  return {schema_version:1,source_kind:"native-static-supplement",revision:REVISION,
    provenance:{cache_sha256:sha(cache),cache_byte_length:cache.length,selected_static_groups:[1,12,51,128],metadata_sha256:"abd0233dfb3c66eb6a8b70789e0c5d1e5a4b77b7977378768b9eeb497f62601f",metadata_build:274,field_binding:"IL2CPP31 field-default constants for HullSpec, ComponentSpec, ArmorSpec, ShieldSpec, ImpulseSpec and WeaponSpec.Types.Attack; primitive widths preserved.",limitation:"Client configuration is not the server combat implementation. Only the reviewed L62 identity is admitted; level does not generally establish a buff rank."},
    localization:{locale_sha256:"e68228ccde494b6f0713946ffb91ace7a938648e7fb800cab013df7edccaadcb",ship_name_key:"ship_name_86111",ship_name:"V'ger Gorn Hunter",skill_names:{86111:"Replicated Plasma Cannon",86112:"Replicated Counter-Intel"}},
    hull:{id:integer(hull,1),id_str:text(hull,2),level:62,hull_type:integer(hull,4),grade:integer(hull,6),hull_hp:integer(hull,10),xp_amount:integer(hull,8),rarity:integer(hull,25),faction_id:integer(hull,26),loca_id:integer(nested(one(hull,12,2,true)),2),component_defaults:defaults,component_indices:packed(hull,14),ship_bonus_ids:bonusIds,activated_ability_ids:packed(hull,28),source:provenance(payload)},components,skills,activated_abilities:activated};
}

function expand(values,count){return Array.from({length:count},(_,i)=>values[Math.min(i,values.length-1)]);}
export function normalizeNativeHostile(snapshot) {
  if(snapshot.schema_version!==1||snapshot.source_kind!=="native-static-supplement"||snapshot.hull.id!==NATIVE_HOSTILE_ID||snapshot.hull.id_str!=="Hull_L62_Explorer_M86_Gorn"||snapshot.hull.level!==62||snapshot.hull.source.sha256!==HULL_SHA||JSON.stringify(snapshot.hull.component_defaults)!==JSON.stringify(COMPONENT_IDS))throw Error("Unreviewed native hostile snapshot identity");
  const byId=new Map(snapshot.components.map(c=>[c.id,c]));
  if(byId.size!==snapshot.components.length)throw Error("Duplicate component snapshot");
  const components=snapshot.hull.component_defaults.map((id,slot)=>{
    const c=byId.get(id);if(!c)throw Error("Missing linked native component");
    return {id,order:slot,native_type:c.native_type,data:c.combat_data??{tag:"NativeNonCombatComponent"},native_source:c.source};
  });
  const single=tag=>{const found=components.filter(c=>c.data.tag===tag);if(found.length!==1)throw Error(`Expected one ${tag} component`);return found[0].data;};
  const armor=single("Armor"),shield=single("Shield"),impulse=single("Impulse"),weapons=components.filter(c=>c.data.tag==="Weapon").map(c=>c.data);
  if(weapons.length!==6||snapshot.hull.hull_type!==2||snapshot.hull.hull_hp!==0)throw Error("Unreviewed native base-stat layout");
  if([armor.hp,armor.plating,shield.hp,shield.absorption,impulse.dodge].some(v=>!Number.isFinite(v)||v<=0)||!Number.isFinite(shield.mitigation)||shield.mitigation<0||shield.mitigation>1)throw Error("Invalid native defensive component value");
  for(const w of weapons){
    if(![1,2].includes(w.weapon_type)||[w.minimum_damage,w.maximum_damage,w.penetration,w.modulation,w.accuracy,w.crit_chance,w.crit_modifier].some(v=>!Number.isFinite(v)||v<0)||w.minimum_damage>w.maximum_damage||!Number.isSafeInteger(w.shots)||w.shots<=0||w.shots>1024)throw Error("Invalid native weapon value");
  }
  if(JSON.stringify(snapshot.skills.map(s=>s.id))!==JSON.stringify(BONUS_IDS))throw Error("Native skill links changed");
  const supported=new Map([[4196543321,"Fixed L62 Isolytic Damage coefficient corroborated independently by recorded damage channels; no general level-to-rank rule."],[4173861882,"Opponent Shield Mitigation SUB at each round start; value .01 is constant across all source ranks."],[1761765323,"Exact Counter-Intel locale86112 supplies physical-only immunity semantics. Its dummy .01 AllDamage value is not applied."]]);
  const skill=(id)=>{const s=snapshot.skills.find(s=>s.id===id);if(!s)throw Error("Missing native skill");return s;};
  const iso=skill(4196543321),sm=skill(4173861882),carrier=skill(1761765323);
  for(const s of [iso,sm,carrier]){
    if(s.ranked_value_type!==0||s.attribute_fields.length||s.unknown_fields.length||!s.ranked_chance_count||!s.ranked_chances.length||s.ranked_chances.some(chance=>chance!==1))throw Error("Unreviewed native skill chance or attributes");
  }
  if(iso.modifier_code!==707||iso.operation_code!==1||iso.target_code!==1||iso.trigger_code!==25||iso.raw_value_at_index_61!==14.91||expand(iso.ranked_values,iso.ranked_value_count)[61]!==14.91||iso.condition_codes.length||sm.modifier_code!==76||sm.operation_code!==2||sm.target_code!==6||sm.trigger_code!==18||expand(sm.ranked_values,sm.ranked_value_count).some(v=>v!==.01)||sm.condition_codes.length||carrier.loca_id!==86112)throw Error("Reviewed native skill semantics changed");
  const ability=snapshot.skills.map(s=>({id:s.id,loca_id:s.loca_id,value_is_percentage:false,values:[{value:s.id===4196543321?14.91:s.id===4173861882?.01:0,chance:1}],modeling_status:supported.has(s.id)?"modeled":"unsupported",...(supported.has(s.id)?{modeling_note:supported.get(s.id)}:{unsupported_reason:s.id===3215268814?"Presentation damage stub; a dummy +1% damage boost is not modeled.":s.id===1421423212?"Conditional Apex source requires native predicate86001 and exact officer presence scope; not applied unconditionally.":"Native value/rank or modifier behavior has not been established for this combat consumer."}),native_source:{source_group:51,spec_sha256:s.sha256,modifier_code:s.modifier_code,operation_code:s.operation_code,target_code:s.target_code,trigger_code:s.trigger_code,condition_codes:s.condition_codes,source_snapshot:SNAPSHOT}}));
  for(const a of snapshot.activated_abilities)ability.push({id:a.id,modeling_status:"unsupported",unsupported_reason:`Native ${a.id_str} immunity hook is retained; this simulator does not execute Cutting Beam/Chain Shot targeting.`,native_source:{source_group:128,spec_sha256:a.source.sha256,source_snapshot:SNAPSHOT}});
  const sum=key=>weapons.reduce((total,w)=>total+w[key],0);
  return {id:String(snapshot.hull.id),hostile_name:snapshot.localization.ship_name,level:62,ship_class:"explorer",armor:armor.plating,shield_deflection:shield.absorption,dodge:impulse.dodge,hull_health:armor.hp,shield_health:shield.hp,shield_mitigation:shield.mitigation,apex_barrier:0,isolytic_defense:0,loca_id:snapshot.hull.loca_id,faction:{id:snapshot.hull.faction_id},upstream_ship_type:0,hull_type_raw:snapshot.hull.hull_type,rarity:snapshot.hull.rarity,xp_amount:snapshot.hull.xp_amount,accuracy:sum("accuracy"),armor_piercing:sum("penetration"),shield_piercing:sum("modulation"),crit_chance:weapons[0].crit_chance,crit_damage:weapons[0].crit_modifier,crit_damage_is_bonus:true,hostile_tags:["vger_gorn_hunter"],components,ability,native_source:{kind:"native-static-supplement",snapshot:SNAPSHOT,revision:snapshot.revision,hull_payload_sha256:snapshot.hull.source.sha256,base_stats_note:"Raw component stats, before passive buffs, research or any player profile. Repeated weapon slots are preserved. upstream_ship_type0 denotes unavailable API category, not native hull class."}};
}

export function mergeNativeHostileSupplement(snapshot,index,registry) {
  const hostile=normalizeNativeHostile(snapshot),next=structuredClone(index),reg=structuredClone(registry);
  const matching=next.hostiles.filter(h=>h.id===hostile.id);
  if(matching.length>1)throw Error("Duplicate hostile index identity");
  const entry={id:hostile.id,hostile_name:hostile.hostile_name,level:hostile.level,ship_class:hostile.ship_class,rarity:hostile.rarity,upstream_ship_type:0,loca_id:hostile.loca_id,source:"native-static-supplement"};
  if(matching.length)next.hostiles[next.hostiles.findIndex(h=>h.id===hostile.id)]=entry;else next.hostiles.push(entry);
  const suffix=`+${REVISION}`;
  if(!next.data_version.endsWith(suffix))next.data_version+=suffix;
  const note="Includes reviewed native-static L62 V'ger Gorn Hunter supplement; no data.stfc.space detail record is claimed.";
  if(!next.source_note.includes(note))next.source_note+=` ${note}`;
  next.native_supplements=[...new Set([...(next.native_supplements??[]),SNAPSHOT])];
  reg.hostiles={...reg.hostiles,source:"data-stfc-space+native-static-supplement",data_version:next.data_version,last_updated:[reg.hostiles.last_updated??"","2026-11-10"].sort().at(-1)};
  return {hostile,index:next,registry:reg};
}

function atomicWrite(file,text){fs.mkdirSync(path.dirname(file),{recursive:true});const tmp=`${file}.${process.pid}.${crypto.randomBytes(4).toString("hex")}.tmp`;try{fs.writeFileSync(tmp,text,{flag:"wx"});fs.renameSync(tmp,file);}finally{if(fs.existsSync(tmp))fs.unlinkSync(tmp);}}
export function main(args) {
  let root=ROOT,cachePath,check=false,write=false;
  for(let i=0;i<args.length;i++){
    if(args[i]==="--root")root=path.resolve(args[++i]);
    else if(args[i]==="--cache")cachePath=path.resolve(args[++i]);
    else if(args[i]==="--check")check=true;
    else if(args[i]==="--write")write=true;
    else throw Error(`Unknown argument ${args[i]}`);
  }
  if(check===write)throw Error("Select exactly one --write or --check mode");
  const read=f=>JSON.parse(fs.readFileSync(path.join(root,f),"utf8"));
  const snapshot=cachePath?extractNativeHostileSnapshot(fs.readFileSync(cachePath)):read(SNAPSHOT);
  const result=mergeNativeHostileSupplement(snapshot,read("data/hostiles/index.json"),read("data/registry.json"));
  const outputs=[[SNAPSHOT,snapshot],[`data/hostiles/${NATIVE_HOSTILE_ID}.json`,result.hostile],["data/hostiles/index.json",result.index],["data/registry.json",result.registry]];
  for(const[file,value]of outputs){
    const serialized=JSON.stringify(value,null,2)+"\n";
    if(check){if(JSON.stringify(read(file))!==JSON.stringify(value))throw Error(`Native supplement drift: ${file}`);}
    else atomicWrite(path.join(root,file),serialized);
  }
  console.log(JSON.stringify({mode:check?"check":"write",hostile_id:NATIVE_HOSTILE_ID,component_slots:result.hostile.components.length,weapon_slots:result.hostile.components.filter(c=>c.data.tag==="Weapon").length}));
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url))main(process.argv.slice(2));
