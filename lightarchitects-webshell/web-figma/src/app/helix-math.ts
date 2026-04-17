export const R_bundle = 1.05;
export const w_twist = 0.76;
export const tMin = -35;
export const tMax = 15;
export const fadeDist = 4.5;

export function getRailPoint(y: number, railIdx: number): [number, number, number] {
  const theta = y * w_twist + (railIdx === 0 ? 0 : Math.PI);
  return [R_bundle * Math.cos(theta), y, R_bundle * Math.sin(theta)];
}

export function generateRail(railIdx: number, samples = 400): [number, number, number][] {
  const points: [number, number, number][] = [];
  const range = tMax - tMin;
  const step = range / samples;
  for (let i = 0; i <= samples; i++) {
    points.push(getRailPoint(tMin + i * step, railIdx));
  }
  return points;
}
