import fs from 'fs-extra';
import path from 'path';
import sharp from 'sharp';
import { Command } from 'commander';

const program = new Command();
program.option("--event-dir <path>", "Path to event directory (contains .dzi files)").option("--tex-dir <path>", "Path to tex directory (contains tiles)").option("--output-dir <path>", "Path to output directory").option(
  "--enable-lower-layers <boolean>",
  "Enable lower layers (true/false)",
  "true"
).parse(process.argv);
const options = program.opts();
if (!options.eventDir || !options.texDir || !options.outputDir) {
  console.error("Missing required arguments.");
  program.help();
}
const ENABLE_LOWER_LAYERS = options.enableLowerLayers === "true";
const EVENT_DIR = path.resolve(process.cwd(), options.eventDir);
const TEX_DIR = path.resolve(process.cwd(), options.texDir);
const DIST_DIR = path.resolve(process.cwd(), options.outputDir);
const parseDzi = async (filePath) => {
  const content = await fs.readFile(filePath, "utf-8");
  const lines = content.trim().split(/\r?\n/);
  const [formatLine, sizeLine, ...restLines] = lines;
  const [imgWidth, imgHeight] = sizeLine.split(",").map(Number);
  const layers = [];
  let i = 0;
  while (i < restLines.length) {
    const [cols, rows] = restLines[i++].split(",").map(Number);
    const tileLines = [];
    for (let r = 0; r < rows; r++) {
      tileLines.push(restLines[i++].split(","));
    }
    layers.push({ tiles: tileLines, rows, cols });
  }
  return { width: imgWidth, height: imgHeight, layers };
};
const composeLayer = async (tiles, group, layerIndex, outputPath, finalWidth, finalHeight) => {
  if (!tiles || tiles.length === 0 || tiles[0].length === 0) {
    return;
  }
  const rows = tiles.length;
  const cols = tiles[0].length;
  const firstTilePath = path.join(TEX_DIR, tiles[0][0] + ".png");
  const { width: tileW, height: tileH } = await sharp(firstTilePath).metadata();
  if (!tileW || !tileH) {
    throw new Error(`Cannot get the tile size: ${firstTilePath}`);
  }
  const composedWidth = cols * tileW;
  const composedHeight = rows * tileH;
  const fullImg = sharp({
    create: {
      width: composedWidth,
      height: composedHeight,
      channels: 4,
      background: { r: 0, g: 0, b: 0, alpha: 0 }
    }
  });
  const composites = [];
  for (let y = 0; y < rows; y++) {
    for (let x = 0; x < cols; x++) {
      const tileRelPath = tiles[y][x];
      if (!tileRelPath) {
        continue;
      }
      const tileAbsPath = path.join(TEX_DIR, tileRelPath + ".png");
      const buffer = await fs.readFile(tileAbsPath);
      composites.push({
        input: buffer,
        left: x * tileW,
        top: y * tileH
      });
    }
  }
  const layerOutputDir = path.join(outputPath, group);
  await fs.ensureDir(layerOutputDir);
  const outFile = path.join(layerOutputDir, `layer_${layerIndex}.png`);
  const cropWidth = Math.min(finalWidth, composedWidth);
  const cropHeight = Math.min(finalHeight, composedHeight);
  await fullImg.composite(composites).extract({ left: 0, top: 0, width: cropWidth, height: cropHeight }).toFile(outFile);
  console.log(`Compose successfully: ${outFile}`);
};
const processAllDziFiles = async () => {
  const files = await fs.readdir(EVENT_DIR);
  if (fs.existsSync(DIST_DIR)) {
    await fs.rm(DIST_DIR, { recursive: true, force: true });
  }
  for (const file of files) {
    if (!file.endsWith(".dzi")) continue;
    const filePath = path.join(EVENT_DIR, file);
    const groupName = path.basename(file, ".dzi");
    console.log(`Handling ${groupName} ...`);
    const {
      width: imgWidth,
      height: imgHeight,
      layers
    } = await parseDzi(filePath);
    for (let i = 0; i < layers.length; i++) {
      if (i > 1 && !ENABLE_LOWER_LAYERS) {
        console.log(`Skip layer_${i} due to config`);
        continue;
      }
      const { tiles } = layers[i];
      const scaleFactor = Math.pow(0.5, i - 1);
      const targetWidth = Math.round(imgWidth * scaleFactor);
      const targetHeight = Math.round(imgHeight * scaleFactor);
      await composeLayer(
        tiles,
        groupName,
        i,
        DIST_DIR,
        targetWidth,
        targetHeight
      );
    }
  }
  console.log("Assemble all cgs successfully!");
};
if (require.main === module) {
  processAllDziFiles().catch((err) => {
    console.error("Error occurred:", err);
    process.exit(1);
  });
}
