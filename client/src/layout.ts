export type LayoutDirection =
  | "left-right"
  | "right-left"
  | "top-bottom"
  | "bottom-top"
  | "from-topleft"
  | "from-topright"
  | "from-bottomleft"
  | "from-bottomright";

export interface Vec2 {
  readonly x: number;
  readonly y: number;
}

export interface DirectionVectors {
  /** Dirección en la que avanza la profundidad del árbol (raíz -> hojas). */
  depth: Vec2;
  /** Dirección perpendicular en la que se separan los hermanos. */
  sibling: Vec2;
}

const D = Math.SQRT1_2;

/**
 * Un vector unitario de "profundidad" y uno de "hermanos" por cada
 * orientación. Los 4 cardinales se eligen a mano para que la lectura sea
 * natural (ej. top-bottom separa hermanos de izquierda a derecha, como un
 * organigrama); las 4 diagonales comparten una única convención de rotación
 * consistente entre sí.
 */
export const DIRECTION_VECTORS: Record<LayoutDirection, DirectionVectors> = {
  "left-right": { depth: { x: 1, y: 0 }, sibling: { x: 0, y: 1 } },
  "right-left": { depth: { x: -1, y: 0 }, sibling: { x: 0, y: 1 } },
  "top-bottom": { depth: { x: 0, y: 1 }, sibling: { x: 1, y: 0 } },
  "bottom-top": { depth: { x: 0, y: -1 }, sibling: { x: 1, y: 0 } },
  "from-topleft": { depth: { x: D, y: D }, sibling: { x: D, y: -D } },
  "from-topright": { depth: { x: -D, y: D }, sibling: { x: D, y: D } },
  "from-bottomleft": { depth: { x: D, y: -D }, sibling: { x: -D, y: -D } },
  "from-bottomright": { depth: { x: -D, y: -D }, sibling: { x: -D, y: D } },
};

export const LAYOUT_DIRECTIONS: LayoutDirection[] = [
  "from-topleft",
  "top-bottom",
  "from-topright",
  "left-right",
  "right-left",
  "from-bottomleft",
  "bottom-top",
  "from-bottomright",
];

/** Modo de layout: las 8 direcciones lineales, o "radial" (raíz al centro,
 * un anillo más separado por nivel, hijos repartidos en círculo alrededor
 * de su padre) — mejor aprovechamiento del espacio para árboles anchos y
 * poco profundos, donde cualquier dirección lineal termina viéndose como
 * una sola línea larga y delgada. */
export type LayoutMode = LayoutDirection | "radial";

export const LAYOUT_MODES: LayoutMode[] = [...LAYOUT_DIRECTIONS, "radial"];
