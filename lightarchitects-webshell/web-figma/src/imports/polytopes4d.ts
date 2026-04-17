export type Polytope4DType = 'doubleHelix4D' | 'hexadecachoron' | 'tesseract' | 'duoprism64' | 'icositetrachoron' | 'duoprism34' | 'dualCompound';

export type Vec4 = [number, number, number, number];

export interface PolytopeData {
  vertices: Vec4[];
  edges: [number, number][];
}

export function getPolytope4D(type: Polytope4DType): PolytopeData {
  // Return a simple cube as a mock for all 4D polytopes
  const vertices: Vec4[] = [
    [-1, -1, -1, 0], [1, -1, -1, 0], [1, 1, -1, 0], [-1, 1, -1, 0],
    [-1, -1, 1, 0], [1, -1, 1, 0], [1, 1, 1, 0], [-1, 1, 1, 0]
  ];
  const edges: [number, number][] = [
    [0, 1], [1, 2], [2, 3], [3, 0],
    [4, 5], [5, 6], [6, 7], [7, 4],
    [0, 4], [1, 5], [2, 6], [3, 7]
  ];
  
  return { vertices, edges };
}
