import React from 'react';
import type { Polytope4DType } from './polytopes4d';

interface PolytopeIconProps {
  type: Polytope4DType;
  color: string;
  size?: number;
}

export const PolytopeIcon = ({ type, color, size = 48 }: PolytopeIconProps) => {
  return (
    <svg width={size} height={size} viewBox="0 0 100 100" fill="none">
      <rect x="25" y="25" width="50" height="50" stroke={color} strokeWidth="2" fill={`${color}20`} />
      <circle cx="50" cy="50" r="15" fill={color} opacity="0.8" />
    </svg>
  );
};
