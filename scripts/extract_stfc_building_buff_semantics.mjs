#!/usr/bin/env node
/** Extract only selected native StarbaseBuffs(68) static BuffSpecs, without player state. */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { extractNativeGlobalBuffSemantics } from "./extract_stfc_consumable_buff_semantics.mjs";

export const extractBuildingBuffSemantics = bytes => extractNativeGlobalBuffSemantics(bytes, 68);
export function main(args) {
  if (args.length !== 2) throw new Error("Usage: node scripts/extract_stfc_building_buff_semantics.mjs <cache.bin> <new-output.json>");
  const input = path.resolve(args[0]), output = path.resolve(args[1]);
  if (input === output) throw new Error("Output must differ from input");
  const stat = fs.statSync(input);
  if (!stat.isFile() || stat.size > 128 * 1024 * 1024) throw new Error("Invalid input");
  const result = extractBuildingBuffSemantics(fs.readFileSync(input));
  fs.writeFileSync(output, JSON.stringify(result, null, 2) + "\n", { flag: "wx" });
  console.log(JSON.stringify(result.coverage));
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main(process.argv.slice(2));
