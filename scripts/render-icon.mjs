import sharp from "sharp";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const svg = readFileSync(fileURLToPath(new URL("../assets/brand/spindle-mark.svg", import.meta.url)));
const outPath = fileURLToPath(new URL("../assets/brand/spindle-mark-1024.png", import.meta.url));

await sharp(svg, { density: 384 })
  .resize(1024, 1024)
  .png()
  .toFile(outPath);

console.log("wrote assets/brand/spindle-mark-1024.png");
