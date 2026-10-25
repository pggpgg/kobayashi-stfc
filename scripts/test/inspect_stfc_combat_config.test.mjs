import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { inspectCombatConfig, main } from "../inspect_stfc_combat_config.mjs";

const concat = (...parts) => Buffer.concat(parts);
function varint(value) {
  let n = BigInt(value);
  const out = [];
  do {
    out.push(Number(n & 127n) | (n > 127n ? 128 : 0));
    n >>= 7n;
  } while (n);
  return Buffer.from(out);
}
const integer = (number, value) => concat(varint(number * 8), varint(value));
const bytes = (number, value) => concat(varint(number * 8 + 2), varint(value.length), value);
const text = (number, value) => bytes(number, Buffer.from(value));
function float(number, value) {
  const data = Buffer.alloc(4);
  data.writeFloatLE(value);
  return concat(varint(number * 8 + 5), data);
}
const entity = (type, config) => bytes(1, concat(integer(1, type), bytes(2, bytes(1, config))));

test("extracts only static allowlisted groups with source hashes and exact spans", () => {
  const battle = concat(
    integer(5, 5),
    bytes(12, concat(text(1, "__proto__"), text(2, "throw new Error('must not run')"))),
    bytes(13, concat(text(1, "coefficient"), float(2, 0.25))),
  );
  const caps = bytes(1, concat(
    text(2, "armor"),
    bytes(1, concat(integer(1, 3), float(2, 0.75))),
    bytes(3, concat(integer(1, 3), float(2, 0.5))),
  ));
  const damage = concat(float(1, 0.1), float(2, 0.2), float(3, 0.3));
  const privateGroup = bytes(2, concat(integer(1, 38), bytes(2, Buffer.from("player-secret"))));
  const unrelatedGroup = bytes(1, concat(integer(1, 99), bytes(2, Buffer.from("other-secret"))));
  const input = concat(
    text(4, "user-secret"), privateGroup, unrelatedGroup,
    entity(38, battle), entity(52, caps), entity(53, damage),
  );
  const report = inspectCombatConfig(input, "/private/directory/cache.bin");
  assert.equal(report.provenance.source_filename, "cache.bin");
  assert.equal(report.provenance.source_sha256, crypto.createHash("sha256").update(input).digest("hex"));
  assert.deepEqual(report.groups.map((g) => g.type), [38, 52, 53]);
  assert.equal(report.groups[0].data.scalars.combatLengthSeconds, 5);
  assert.equal(report.groups[0].data.equations[0].key, "__proto__");
  assert.equal(report.groups[0].data.equations[0].value, "throw new Error('must not run')");
  assert.equal(report.groups[0].data.static[0].value, 0.25);
  assert.equal(report.groups[1].data.mitigationCaps[0].caps[0].value, 0.75);
  assert.equal(report.groups[1].data.mitigationCaps[0].officerBonusMultipliers[0].value, 0.5);
  assert.ok(Math.abs(report.groups[2].data.damageReductionOffset - 0.3) < 1e-6);
  for (const [i, expected] of [battle, caps, damage].entries()) {
    const group = report.groups[i];
    assert.deepEqual(input.subarray(group.config_offset, group.config_end_offset), expected);
    assert.equal(group.config_sha256, crypto.createHash("sha256").update(expected).digest("hex"));
    assert.ok(group.field_spans.every((f) =>
      f.offset >= group.config_offset && f.end_offset <= group.config_end_offset));
  }
  assert.doesNotMatch(JSON.stringify(report), /user-secret|player-secret|other-secret|private\/directory/);
});

test("empty equation maps stay empty, unknown payload contents are not exported", () => {
  const report = inspectCombatConfig(entity(38, concat(integer(5, 5), text(99, "unknown-secret"))));
  assert.deepEqual(report.groups[0].data.equations, []);
  assert.deepEqual(report.groups[0].data.static, []);
  assert.equal(report.groups[0].field_spans[1].field, 99);
  assert.equal(report.provenance.server_version, null);
  assert.equal(report.provenance.server_time, null);
  assert.doesNotMatch(JSON.stringify(report), /unknown-secret/);
});

test("reports only envelope version and timestamp, preserves nanos, and excludes userId", () => {
  const input = concat(
    bytes(3, concat(integer(1, 1700000000), integer(2, 123456789))),
    text(4, "private-user-id"),
    text(5, "server-version-fixture"),
    entity(38, integer(5, 5)),
  );
  const report = inspectCombatConfig(input);
  assert.equal(report.provenance.server_version, "server-version-fixture");
  assert.deepEqual(report.provenance.server_time, {
    unix_seconds: "1700000000",
    nanos: 123456789,
    seconds_encoded: true,
    nanos_encoded: true,
    utc: "2023-11-14T22:13:20.123456789Z",
  });
  assert.doesNotMatch(JSON.stringify(report), /private-user-id/);
  assert.throws(() => inspectCombatConfig(concat(
    bytes(3, integer(2, 1000000000)), entity(38, integer(5, 5)),
  )), /Invalid server timestamp/);
});

test("distinguishes omitted map defaults and names from explicitly encoded zeros and empty strings", () => {
  const capWithDefaults = bytes(1, bytes(1, Buffer.alloc(0)));
  const capWithZeros = bytes(1, concat(
    text(2, ""), bytes(1, concat(integer(1, 0), float(2, 0))),
  ));
  const report = inspectCombatConfig(entity(52, concat(capWithDefaults, capWithZeros)));
  const [omitted, encoded] = report.groups[0].data.mitigationCaps;
  assert.equal(omitted.name, encoded.name);
  assert.equal(omitted.name_encoded, false);
  assert.equal(encoded.name_encoded, true);
  assert.equal(omitted.caps[0].key, 0);
  assert.equal(omitted.caps[0].value, 0);
  assert.equal(encoded.caps[0].key, 0);
  assert.equal(encoded.caps[0].value, 0);
  assert.equal(omitted.caps[0].key_encoded, false);
  assert.equal(omitted.caps[0].value_encoded, false);
  assert.equal(encoded.caps[0].key_encoded, true);
  assert.equal(encoded.caps[0].value_encoded, true);
});

test("fails closed on malformed framing, wrong known types, duplicate fields, and non-finite floats", () => {
  for (const invalid of [
    Buffer.alloc(0),
    Buffer.from([0]),
    Buffer.from([0x0a, 0x7f, 0]),
    Buffer.from([0x0b]),
    Buffer.from([0x08, ...Array(10).fill(0xff)]),
    entity(38, text(5, "wrong-wire")),
    entity(38, concat(integer(5, 1), integer(5, 2))),
    entity(53, float(1, Number.NaN)),
    entity(38, bytes(12, bytes(1, Buffer.from([0xff])))),
    entity(38, concat(
      bytes(12, concat(text(1, "same"), text(2, "a"))),
      bytes(12, concat(text(1, "same"), text(2, "b"))),
    )),
  ]) assert.throws(() => inspectCombatConfig(invalid));
});

test("does not search inside player groups or decode unrelated payload bytes", () => {
  const privateOnly = bytes(2, entity(38, integer(5, 5)));
  assert.throws(() => inspectCombatConfig(privateOnly), /No allowlisted/);
  const unknownPayload = bytes(1, concat(integer(1, 99), bytes(2, Buffer.from([0xff]))));
  assert.throws(() => inspectCombatConfig(unknownPayload), /No allowlisted/);
});

test("CLI preserves input and existing output, and writes no report for invalid input", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "stfc-combat-config-"));
  try {
    const input = path.join(dir, "cache.bin");
    const output = path.join(dir, "report.json");
    const original = entity(38, integer(5, 5));
    fs.writeFileSync(input, original);
    assert.throws(() => main([input, "--output", input]), /must not replace/);
    assert.deepEqual(fs.readFileSync(input), original);
    fs.writeFileSync(output, "existing");
    assert.throws(() => main([input, "--output", output]), /EEXIST/);
    assert.equal(fs.readFileSync(output, "utf8"), "existing");
    fs.writeFileSync(input, Buffer.from([0xff]));
    const absent = path.join(dir, "absent.json");
    assert.throws(() => main([input, "--output", absent]));
    assert.equal(fs.existsSync(absent), false);
  } finally {
    assert.equal(path.dirname(fs.realpathSync(dir)), fs.realpathSync(os.tmpdir()));
    assert.ok(path.basename(dir).startsWith("stfc-combat-config-"));
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
