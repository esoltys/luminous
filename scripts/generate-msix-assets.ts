import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { read, Image, write } from "image-js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const projectRoot = path.resolve(__dirname, "..");
const srcTauriDir = path.join(projectRoot, "src-tauri");
const tauriIconsDir = path.join(srcTauriDir, "icons");
const assetsDir = path.join(srcTauriDir, "gen", "windows", "Assets");

const SCALE_FACTORS = [100, 125, 150, 200, 400];
const TARGET_SIZES = [16, 24, 32, 48, 256];
const MSIX_ASSETS = [
  { name: "StoreLogo.png", size: 50 },
  { name: "Square44x44Logo.png", size: 44 },
  { name: "Square150x150Logo.png", size: 150 },
  { name: "Wide310x150Logo.png", width: 310, height: 150, skipScaleVariants: true },
];

function variantFilename(baseName: string, suffix: string) {
  const stem = baseName.replace(/\.png$/i, "");
  return `${stem}.${suffix}.png`;
}

/**
 * Area-averaging downsampler with premultiplied alpha.
 * Integrates pixel overlap areas to ensure fine strokes, glow rings, and highlights
 * are preserved without point-sampling aliasing or dark alpha-fringe halos.
 */
function resizeAreaAverage(src: Image, dstW: number, dstH: number): Image {
  const dst = new Image(dstW, dstH, { colorModel: "RGBA" });
  const xRatio = src.width / dstW;
  const yRatio = src.height / dstH;

  for (let dy = 0; dy < dstH; dy++) {
    const sy0 = dy * yRatio;
    const sy1 = (dy + 1) * yRatio;
    const startY = Math.floor(sy0);
    const endY = Math.min(Math.ceil(sy1), src.height);

    for (let dx = 0; dx < dstW; dx++) {
      const sx0 = dx * xRatio;
      const sx1 = (dx + 1) * xRatio;
      const startX = Math.floor(sx0);
      const endX = Math.min(Math.ceil(sx1), src.width);

      let totalWeight = 0;
      let pR = 0, pG = 0, pB = 0, pA = 0;

      for (let sy = startY; sy < endY; sy++) {
        const yWeight = Math.min(sy + 1, sy1) - Math.max(sy, sy0);
        for (let sx = startX; sx < endX; sx++) {
          const xWeight = Math.min(sx + 1, sx1) - Math.max(sx, sx0);
          const weight = xWeight * yWeight;
          const [r, g, b, a] = src.getPixel(sx, sy);
          const alpha = a / 255;
          pR += r * alpha * weight;
          pG += g * alpha * weight;
          pB += b * alpha * weight;
          pA += a * weight;
          totalWeight += weight;
        }
      }

      if (totalWeight > 0 && pA > 0) {
        const finalA = pA / totalWeight;
        const normA = finalA / 255;
        const finalR = Math.round(pR / (totalWeight * normA));
        const finalG = Math.round(pG / (totalWeight * normA));
        const finalB = Math.round(pB / (totalWeight * normA));
        dst.setPixel(dx, dy, [finalR, finalG, finalB, Math.round(finalA)]);
      } else {
        dst.setPixel(dx, dy, [0, 0, 0, 0]);
      }
    }
  }
  return dst;
}

async function generateScaleVariants(sourcePath: string) {
  const source = await read(sourcePath);
  for (const asset of MSIX_ASSETS) {
    if (asset.skipScaleVariants) continue;
    const baseW = asset.width || asset.size || 50;
    const baseH = asset.height || asset.size || 50;
    for (const factor of SCALE_FACTORS) {
      const w = Math.round((baseW * factor) / 100);
      const h = Math.round((baseH * factor) / 100);
      const resized = resizeAreaAverage(source, w, h);
      const outPath = path.join(assetsDir, variantFilename(asset.name, `scale-${factor}`));
      await write(outPath, resized);
    }
  }
}

async function generateTargetSizeVariants(sourcePath: string, altform: string | null) {
  const source = await read(sourcePath);
  const suffixBase = "Square44x44Logo.png";
  for (const size of TARGET_SIZES) {
    const resized = resizeAreaAverage(source, size, size);
    const suffix = altform ? `targetsize-${size}_altform-${altform}` : `targetsize-${size}`;
    const outPath = path.join(assetsDir, variantFilename(suffixBase, suffix));
    await write(outPath, resized);
  }
}

async function generateWideTile(sourceIconsDir: string, outputPath: string) {
  const sourceIcons = ["Square150x150Logo.png", "Square142x142Logo.png", "icon.png", "128x128.png"];
  for (const iconName of sourceIcons) {
    const iconPath = path.join(sourceIconsDir, iconName);
    if (fs.existsSync(iconPath)) {
      try {
        const image = await read(iconPath);
        const iconSize = 150;
        const resized = image.resize({ width: iconSize, height: iconSize });
        const canvas = new Image(310, 150, { colorModel: "RGBA" });
        canvas.data.fill(0);
        const xOffset = Math.floor((310 - iconSize) / 2);
        for (let y = 0; y < iconSize; y++) {
          for (let x = 0; x < iconSize; x++) {
            const p = resized.getPixel(x, y);
            canvas.setPixel(x + xOffset, y, p);
          }
        }
        await write(outputPath, canvas);
        return true;
      } catch {
        // try next
      }
    }
  }
  return false;
}

export async function generateMsixAssets() {
  fs.mkdirSync(assetsDir, { recursive: true });
  console.log(`[MSIX Assets] Generating assets in ${assetsDir}...`);

  // 1. Copy or generate base assets
  const tauriIconMap: Record<string, string> = {
    "StoreLogo.png": "StoreLogo.png",
    "Square44x44Logo.png": "Square44x44Logo.png",
    "Square150x150Logo.png": "Square150x150Logo.png",
  };

  for (const asset of MSIX_ASSETS) {
    const assetPath = path.join(assetsDir, asset.name);
    const tauriIconName = tauriIconMap[asset.name];
    const tauriIconPath = tauriIconName ? path.join(tauriIconsDir, tauriIconName) : null;

    if (tauriIconPath && fs.existsSync(tauriIconPath)) {
      fs.copyFileSync(tauriIconPath, assetPath);
      console.log(`  [MSIX Assets] Copied ${asset.name}`);
    } else if (asset.name === "Wide310x150Logo.png") {
      const generated = await generateWideTile(tauriIconsDir, assetPath);
      if (generated) {
        console.log(`  [MSIX Assets] Generated Wide310x150Logo.png`);
      }
    }
  }

  // 2. Generate variants from high-res source icon
  const sourceCandidates = ["icon.png", "Square310x310Logo.png", "128x128.png"];
  let sourcePath: string | null = null;
  for (const candidate of sourceCandidates) {
    const p = path.join(tauriIconsDir, candidate);
    if (fs.existsSync(p)) {
      sourcePath = p;
      break;
    }
  }

  if (!sourcePath) {
    console.error(`[MSIX Assets] Error: No suitable source icon found in ${tauriIconsDir}`);
    process.exit(1);
  }

  console.log(`  [MSIX Assets] Generating variants from ${path.basename(sourcePath)}...`);
  await generateScaleVariants(sourcePath);
  console.log("    - Scale variants written (scale-100, 125, 150, 200, 400)");

  await generateTargetSizeVariants(sourcePath, null);
  console.log("    - Plated targetsize variants written (16, 24, 32, 48, 256)");

  await generateTargetSizeVariants(sourcePath, "unplated");
  console.log("    - Unplated targetsize variants written (16, 24, 32, 48, 256)");

  await generateTargetSizeVariants(sourcePath, "lightunplated");
  console.log("    - Light-unplated targetsize variants written (16, 24, 32, 48, 256)");

  console.log("[MSIX Assets] All MSIX asset variants successfully generated!");
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  generateMsixAssets().catch((err) => {
    console.error("[MSIX Assets] Failed:", err);
    process.exit(1);
  });
}
