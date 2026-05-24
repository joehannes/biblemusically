import Jimp from 'jimp';
import path from 'path';

async function main() {
  const inputPath = path.resolve('app-icon.png');
  const outputPath = path.resolve('app-icon-transparent.png');

  console.log(`Loading image from: ${inputPath}`);
  const image = await Jimp.read(inputPath);

  const width = image.bitmap.width;
  const height = image.bitmap.height;
  console.log(`Image size: ${width}x${height}`);

  // Get color at top-left corner (0, 0)
  const targetColor = image.getPixelColor(0, 0);
  const targetRGBA = Jimp.intToRGBA(targetColor);
  console.log('Detected background color at (0,0):', targetRGBA);

  // We want to remove the greyish background. Let's use a threshold comparison with the corner color.
  const threshold = 40; // Color distance threshold (increased slightly)

  let transparentCount = 0;

  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const color = image.getPixelColor(x, y);
      const rgba = Jimp.intToRGBA(color);

      // Calculate distance to target background color
      const dist = Math.sqrt(
        Math.pow(rgba.r - targetRGBA.r, 2) +
        Math.pow(rgba.g - targetRGBA.g, 2) +
        Math.pow(rgba.b - targetRGBA.b, 2)
      );

      if (dist < threshold) {
        // Make it transparent
        image.setPixelColor(Jimp.rgbaToInt(0, 0, 0, 0), x, y);
        transparentCount++;
      }
    }
  }

  console.log(`Made ${transparentCount} pixels transparent.`);
  await image.writeAsync(outputPath);
  console.log(`Saved transparent image to: ${outputPath}`);
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
