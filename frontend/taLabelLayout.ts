export interface TaAnchor {
  key: string;
  faction?: string | null;
  groupingCategory?: 'unit-drop' | 'other';
  left: number;
  top: number;
}

export interface TaLabelGroup {
  key: string;
  left: number;
  top: number;
  width: number;
  height: number;
  members: string[];
}

export interface TaObstacle {
  left: number;
  top: number;
  radius?: number;
}

// Group competing card footprints after small local adjustments fail.
// Obstacles are normalized points with pixel radii; occupancy is a soft preference.
export function layoutTaLabels(
  anchors: TaAnchor[],
  width: number,
  height: number,
  obstacles: TaObstacle[] = [],
): TaLabelGroup[] {
  if (width < 24 || height < 54) {
    return [];
  }
  const w = Math.min(180, width - 8);
  const points = new Map(anchors.map((a) => [a.key, { x: a.left * width, y: a.top * height }]));
  const end = (p: { x: number; y: number }, box: TaLabelGroup) => ({
    x: Math.max(box.left, Math.min(box.left + box.width, p.x)),
    y: Math.max(box.top, Math.min(box.top + box.height, p.y)),
  });
  const cross = (
    a: { x: number; y: number },
    b: { x: number; y: number },
    c: { x: number; y: number },
  ) => (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
  const intersects = (
    a: { x: number; y: number },
    b: { x: number; y: number },
    c: { x: number; y: number },
    d: { x: number; y: number },
  ) => cross(a, b, c) * cross(a, b, d) < 0 && cross(c, d, a) * cross(c, d, b) < 0;

  const side = (a: TaAnchor) =>
    a.faction === 'USA' || a.faction === 'NATO' ? 'allied' : (a.faction ?? 'unknown');
  const category = (a: TaAnchor) => a.groupingCategory ?? 'other';
  const compatible = (a: TaAnchor, b: TaAnchor) =>
    side(a) === side(b) && category(a) === category(b);
  const gapFor = (a: TaAnchor) => (category(a) === 'unit-drop' ? 24 : 8);
  const occupancy = [...obstacles, ...anchors.map((a) => ({ ...a, radius: 6 }))];
  const clusters: TaAnchor[][] = [];
  const footprints: Array<{ left: number; top: number }> = [];
  const overlaps = (a: { left: number; top: number }, b: { left: number; top: number }) =>
    a.left < b.left + w + 6 && a.left + w + 6 > b.left && a.top < b.top + 52 && a.top + 52 > b.top;
  for (const anchor of anchors) {
    const preferred = (dx: number, dy: number) => ({
      left: Math.max(4, Math.min(width - w - 4, anchor.left * width - w / 2 + dx)),
      top: Math.max(4, Math.min(height - 50, anchor.top * height - 46 - gapFor(anchor) + dy)),
    });
    const closePositions = [
      [0, 0],
      [-24, 0],
      [24, 0],
      [0, -12],
      [0, 12],
    ].map(([dx, dy]) => preferred(dx!, dy!));
    const free = closePositions.find(
      (box) =>
        !footprints.some(
          (other, i) => compatible(clusters[i]![0]!, anchor) && overlaps(box, other),
        ),
    );
    if (free) {
      clusters.push([anchor]);
      footprints.push(free);
    } else {
      const box = preferred(0, 0);
      const competing = footprints
        .map((other, i) => ({ other, i }))
        .filter(({ other, i }) => compatible(clusters[i]![0]!, anchor) && overlaps(box, other));
      const nearest = competing.reduce((best, next) =>
        Math.hypot(next.other.left - box.left, next.other.top - box.top) <
        Math.hypot(best.other.left - box.left, best.other.top - box.top)
          ? next
          : best,
      );
      clusters[nearest.i]!.push(anchor);
    }
  }
  for (;;) {
    const groups: TaLabelGroup[] = [];
    let merged = false;
    for (const [index, members] of clusters.entries()) {
      const anchor = members[0]!;
      const x = anchor.left * width;
      const y = anchor.top * height;
      const h = Math.min(members.length * 46, 138, height - 8);
      const offsets: number[][] = [
        [0, -h / 2 - 8],
        [0, h / 2 + 8],
        [w / 2 + 8, 0],
        [-w / 2 - 8, 0],
        [w + 10, -h - 6],
        [-w - 10, -h - 6],
        [w + 10, h + 6],
        [-w - 10, h + 6],
        [0, -h * 2 - 12],
        [0, h * 2 + 12],
      ];
      const isStack = members.length > 1;
      const isDrop = category(anchor) === 'unit-drop';
      if (!isStack && isDrop) {
        offsets.splice(0, 4);
        for (const gap of [24, 16, 8]) {
          offsets.unshift([0, -h / 2 - gap], [0, h / 2 + gap], [w / 2 + gap, 0], [-w / 2 - gap, 0]);
        }
      }
      if (isStack) {
        const xs = members.map((a) => a.left * width),
          ys = members.map((a) => a.top * height);
        const minX = Math.min(...xs),
          maxX = Math.max(...xs),
          minY = Math.min(...ys),
          maxY = Math.max(...ys);
        offsets.splice(0);
        for (const gap of [32, 48, 72, 12]) {
          const tier = gap === 12 ? 1 : 0;
          for (const left of [(minX + maxX - w) / 2, minX, maxX - w]) {
            for (const top of [minY - gap - h, maxY + gap]) {
              offsets.push([left + w / 2 - x, top + h / 2 - y, tier]);
            }
          }
          for (const top of [(minY + maxY - h) / 2, minY, maxY - h]) {
            for (const left of [minX - gap - w, maxX + gap]) {
              offsets.push([left + w / 2 - x, top + h / 2 - y, tier]);
            }
          }
        }
        // Last-resort viewport-clamped candidates preserve readable rows on tiny maps.
        offsets.push([0, -h / 2 - 8, 2], [0, h / 2 + 8, 2], [w / 2 + 8, 0, 2], [-w / 2 - 8, 0, 2]);
      }
      // Once each side is consolidated, search the whole viewport before
      // allowing a mixed-side stack on an exceptionally cramped map.
      if (
        clusters.length <= 3 ||
        !clusters.some((cluster, i) =>
          clusters.some((other, j) => i !== j && compatible(cluster[0]!, other[0]!)),
        )
      ) {
        for (let top = 4; top + h <= height - 4; top += 52) {
          for (let left = 4; left + w <= width - 4; left += w + 6) {
            offsets.push([left + w / 2 - x, top + h / 2 - y, 2]);
          }
        }
      }
      let candidate: TaLabelGroup | undefined;
      let bestScore: number[] = [Infinity];
      let bestConnector = Infinity;
      for (const [dx, dy, tier = 0] of offsets) {
        if (
          isStack &&
          tier < 2 &&
          (x + dx! - w / 2 < 4 ||
            x + dx! + w / 2 > width - 4 ||
            y + dy! - h / 2 < 4 ||
            y + dy! + h / 2 > height - 4)
        ) {
          continue;
        }
        const left = Math.max(4, Math.min(width - w - 4, x + dx! - w / 2));
        const top = Math.max(4, Math.min(height - h - 4, y + dy! - h / 2));
        if (
          groups.some(
            (g) =>
              left < g.left + g.width + 6 &&
              left + w + 6 > g.left &&
              top < g.top + g.height + 6 &&
              top + h + 6 > g.top,
          )
        ) {
          continue;
        }
        const option = {
          key: anchor.key,
          left,
          top,
          width: w,
          height: h,
          members: members.map((a) => a.key),
        };
        const occupied = occupancy.reduce((total, obstacle) => {
          const ox = obstacle.left * width,
            oy = obstacle.top * height;
          const radius = obstacle.radius ?? 10;
          return (
            total +
            (ox + radius > left &&
            ox - radius < left + w &&
            oy + radius > top &&
            oy - radius < top + h
              ? 1
              : 0)
          );
        }, 0);
        const connector = Math.hypot(
          Math.max(left - x, 0, x - left - w),
          Math.max(top - y, 0, y - top - h),
        );
        // Edge-to-marker distance is the actual connector length. Do not trade
        // a short line for a distant empty patch just to avoid one map object.
        let crossings = 0,
          totalLength = 0;
        for (const member of members) {
          const p = points.get(member.key)!;
          const q = end(p, option);
          totalLength += Math.hypot(q.x - p.x, q.y - p.y);
          if (isStack) {
            for (const placed of groups) {
              for (const key of placed.members) {
                const r = points.get(key)!;
                if (intersects(p, q, r, end(r, placed))) {
                  crossings++;
                }
              }
            }
          }
        }
        const score = isStack
          ? [tier, occupied, crossings, totalLength]
          : [
              isDrop ? (connector >= 24 ? 0 : connector >= 16 ? 1 : 2) : 0,
              connector +
                Math.min(occupied, 3) * 12 +
                (connector < 4 ? 40 : 0) +
                Math.hypot(left + w / 2 - x, top + h / 2 - y) * 0.01,
            ];
        const different = score.findIndex((value, i) => value !== bestScore[i]);
        if (different >= 0 && score[different]! < (bestScore[different] ?? Infinity)) {
          candidate = option;
          bestScore = score;
          bestConnector = connector;
        }
      }
      if (
        candidate &&
        !isStack &&
        bestConnector > 40 &&
        clusters.some(
          (cluster, i) =>
            i !== index &&
            cluster.every((a) => compatible(a, anchor)) &&
            Math.hypot(
              (cluster[0]!.left - anchor.left) * width,
              (cluster[0]!.top - anchor.top) * height,
            ) <
              w * 1.5,
        )
      ) {
        candidate = undefined;
      }
      if (candidate) {
        groups.push(candidate);
      } else {
        // Exhaust category+side consolidation globally before relaxing category,
        // and preserve faction separation until no same-side merge remains.
        let source = index;
        let targets: TaAnchor[][] = [];
        for (const matches of [
          compatible,
          (a: TaAnchor, b: TaAnchor) => side(a) === side(b),
          () => true,
        ]) {
          targets = clusters.filter(
            (cluster, i) =>
              i !== source && cluster.every((a) => clusters[source]!.every((b) => matches(a, b))),
          );
          if (targets.length) {
            break;
          }
          const pair = clusters.findIndex((cluster, i) =>
            clusters.some(
              (other, j) => i !== j && cluster.every((a) => other.every((b) => matches(a, b))),
            ),
          );
          if (pair >= 0) {
            source = pair;
            targets = clusters.filter(
              (cluster, i) =>
                i !== source && cluster.every((a) => clusters[source]!.every((b) => matches(a, b))),
            );
            break;
          }
        }
        const origin = clusters[source]![0]!;
        const nearest = targets.reduce((best, cluster) =>
          Math.hypot(
            (cluster[0]!.left - origin.left) * width,
            (cluster[0]!.top - origin.top) * height,
          ) <
          Math.hypot((best[0]!.left - origin.left) * width, (best[0]!.top - origin.top) * height)
            ? cluster
            : best,
        );
        nearest.push(...clusters[source]!);
        clusters.splice(source, 1);
        merged = true;
        break;
      }
    }
    if (!merged) {
      return groups;
    }
  }
}
