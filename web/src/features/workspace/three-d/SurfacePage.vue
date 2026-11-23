<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, shallowRef, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { brick, type Voxels } from "@/api/media";
import { projectContents, projectPage, type ProjectSummary } from "@/api/projects";
import type { Contents } from "@/features/gallery/project/project";
import {
  endsOf,
  meshOf,
  profileOf,
  type At,
  type HeightMap,
} from "@/features/workspace/three-d/surface";
import Button from "@/ui/Button.vue";
import Chart from "@/ui/charts/Chart.vue";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";
import Slider from "@/ui/Slider.vue";

type SceneModule = typeof import("@/features/workspace/three-d/scene");
type Scene = ReturnType<SceneModule["sceneIn"]>;

/**
 * A depth-from-focus result as a surface (W7.7, ADR-0054; P42): its heights
 * as a mesh, lit, its texture the all-in-focus image, or coloured by height;
 * turned, zoomed, stretched up and reset; and a profile along a line drawn
 * on its height map, charted in µm. Its data is the volume routes' bricks,
 * coarser from afar. Without WebGL2, the height map alone.
 */
const { t } = useI18n();
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));
const said = shallowRef("");

// The result: a depth from focus's heights, and its all-in-focus image.
const projects = shallowRef<ProjectSummary[]>([]);
const project = shallowRef<string>();
const contents = shallowRef<Contents>();
const chosen = shallowRef<string>();
onMounted(async () => {
  projects.value = (await projectPage({ limit: 500 }).catch(() => ({ projects: [] }))).projects;
});
watch(project, async (id) => {
  [contents.value, chosen.value] = [undefined, undefined];
  if (!id) return;
  contents.value = (await projectContents(id)) as Contents;
  chosen.value = surfaces.value[0]?.heights;
});
/** Depth from focus's results by their names, `… heights` and `… all in focus`: each pair. */
const surfaces = computed(() => {
  const results = contents.value?.results ?? [];
  return results
    .filter((one) => one.name.endsWith(" heights"))
    .map((one) => {
      const stem = one.name.slice(0, -" heights".length);
      const image = results.find((other) => other.name === `${stem} all in focus`);
      return { heights: one.id, image: image?.id, name: stem };
    });
});

// The data: the heights, and the picture, at the finest level a part of which holds no more than this.
const MOST = 512 * 512;
const map = shallowRef<HeightMap>();
const picture = shallowRef<{ width: number; height: number; rgb: Uint8Array }>();
const rgbOf = (voxels: Voxels) => {
  const [width, height] = voxels.size;
  const n = width * height;
  const rgb = new Uint8Array(n * 3);
  for (let i = 0; i < n; i++)
    for (let c = 0; c < 3; c++)
      rgb[i * 3 + c] = voxels.samples[i * voxels.channels + Math.min(c, voxels.channels - 1)]!;
  return { width, height, rgb };
};
watch(chosen, async (id) => {
  [map.value, picture.value, profile.value] = [undefined, undefined, undefined];
  const pair = surfaces.value.find((one) => one.heights === id);
  if (!pair || !project.value) return;
  const at = (record: string) => ({
    project: project.value!,
    kind: "results" as const,
    id: record,
  });
  try {
    const heights = await brick(at(pair.heights), { max: MOST, format: "f32" });
    const [width, height] = heights.size;
    const pixelUm = heights.voxelUm?.[0] ?? 2 ** heights.level;
    map.value = { width, height, heights: Float32Array.from(heights.samples), pixelUm };
    if (pair.image) picture.value = rgbOf(await brick(at(pair.image), { max: MOST, format: "u8" }));
  } catch (thrown) {
    said.value = t("workspace.surface.unread", { why: why(thrown) });
  }
});

// The surface: VTK.js's scene, in its own chunk, where there is WebGL2.
const webgl2 = shallowRef(true);
onMounted(() => (webgl2.value = !!document.createElement("canvas").getContext("webgl2")));
const view = useTemplateRef<HTMLElement>("view");
let scene: Scene | undefined;
const loading = shallowRef(false);
const exaggeration = shallowRef(1);
const colouring = shallowRef<"image" | "height">("image");
const colourMap = shallowRef("Viridis (matplotlib)");
const MAPS = ["Viridis (matplotlib)", "Cool to Warm", "Grayscale"];
const drawn = shallowRef(false);
function draw(): void {
  if (!scene || !map.value) return;
  scene.show(meshOf(map.value, exaggeration.value), map.value.heights);
  scene.colour(picture.value ? colouring.value : "height", colourMap.value);
  drawn.value = true;
}
watch([map, () => view.value], async () => {
  scene?.dispose();
  [scene, drawn.value] = [undefined, false];
  if (!map.value || !webgl2.value || !view.value) return;
  loading.value = true;
  try {
    const { sceneIn } = (await import("@/features/workspace/three-d/scene")) as SceneModule;
    scene = sceneIn(view.value, picture.value);
    draw();
  } finally {
    loading.value = false;
  }
});
watch([exaggeration, colouring, colourMap], draw);
const resizing = new ResizeObserver(() => scene?.resize());
watch(view, (element) => element && resizing.observe(element));
onBeforeUnmount(() => {
  resizing.disconnect();
  scene?.dispose();
});

// The height map, drawn, and a profile along a line dragged on it.
const flat = useTemplateRef<HTMLCanvasElement>("flat");
watch([map, flat], () => {
  const canvas = flat.value;
  const at = map.value;
  if (!canvas || !at) return;
  [canvas.width, canvas.height] = [at.width, at.height];
  const context = canvas.getContext("2d")!;
  const pixels = context.createImageData(at.width, at.height);
  const finite = at.heights.filter(Number.isFinite);
  const range = finite.length ? [Math.min(...finite), Math.max(...finite)] : [0, 1];
  const span = range[1]! - range[0]! || 1;
  at.heights.forEach((h, i) => {
    const grey = Number.isFinite(h) ? Math.round((255 * (h - range[0]!)) / span) : 0;
    pixels.data.set([grey, grey, grey, 255], i * 4);
  });
  context.putImageData(pixels, 0, 0);
});
const line = shallowRef<{ from: At; to: At }>();
let dragging = false;
function placeOf(event: PointerEvent): At {
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const at = map.value!;
  return {
    x: ((event.clientX - box.left) / box.width) * at.width * at.pixelUm,
    y: ((event.clientY - box.top) / box.height) * at.height * at.pixelUm,
  };
}
function press(event: PointerEvent): void {
  if (!map.value) return;
  const at = placeOf(event);
  (event.currentTarget as Element).setPointerCapture(event.pointerId);
  [line.value, dragging] = [{ from: at, to: at }, true];
}
function move(event: PointerEvent): void {
  if (dragging && line.value) line.value = { from: line.value.from, to: placeOf(event) };
}
function release(): void {
  dragging = false;
}
const profile = shallowRef<{ distance: number[]; height: number[] }>();
watch(line, (now) => {
  // A band 2 µm wide: depth from focus's heights are noisy one by one.
  profile.value = now && map.value ? profileOf(map.value, now.from, now.to, 200, 2) : undefined;
});
const ends = computed(() => (profile.value ? endsOf(profile.value.height) : undefined));
const lineIn = computed(() => {
  const at = map.value;
  const now = line.value;
  if (!at || !now) return undefined;
  const px = (point: At) => ({ x: point.x / at.pixelUm, y: point.y / at.pixelUm });
  return { from: px(now.from), to: px(now.to) };
});
</script>

<template>
  <section class="surfaces" aria-labelledby="surfaces-title" data-said="surfaces">
    <h2 id="surfaces-title">{{ t("workspace.surface.title") }}</h2>
    <div class="choice">
      <Field :label="t('workspace.run.project')">
        <Select
          v-model="project"
          :options="projects.map((one) => ({ value: one.id, label: one.name }))"
          :placeholder="t('workspace.run.chooseProject')"
        />
      </Field>
      <Field :label="t('workspace.surface.result')">
        <Select
          v-model="chosen"
          :options="surfaces.map((one) => ({ value: one.heights, label: one.name }))"
          :placeholder="t('workspace.surface.choose')"
        />
      </Field>
    </div>
    <p v-if="!webgl2" class="said" role="status" data-said="no-webgl">
      {{ t("workspace.surface.noWebgl") }}
    </p>
    <template v-if="map">
      <div v-if="webgl2" class="controls">
        <Field class="scaled" :label="t('workspace.surface.exaggeration')">
          <Slider v-model="exaggeration" :min="1" :max="40" :step="1" />
        </Field>
        <Field v-if="picture" :label="t('workspace.surface.colouring')">
          <Select
            v-model="colouring"
            :options="[
              { value: 'image', label: t('workspace.surface.byImage') },
              { value: 'height', label: t('workspace.surface.byHeight') },
            ]"
          />
        </Field>
        <Field :label="t('workspace.surface.map')">
          <Select
            v-model="colourMap"
            :options="MAPS.map((name) => ({ value: name, label: name }))"
          />
        </Field>
        <Button compact @click="scene?.reset()">{{ t("workspace.surface.reset") }}</Button>
      </div>
      <div class="body">
        <div
          v-if="webgl2"
          ref="view"
          class="view"
          role="img"
          :aria-label="t('workspace.surface.label')"
          data-said="surface"
          :data-drawn="drawn"
        />
        <figure class="flat">
          <div class="map" @pointerdown="press" @pointermove="move" @pointerup="release">
            <canvas
              ref="flat"
              role="img"
              :aria-label="t('workspace.surface.mapLabel')"
              data-said="height-map"
            />
            <svg
              v-if="lineIn"
              class="line"
              :viewBox="`0 0 ${map.width} ${map.height}`"
              preserveAspectRatio="none"
            >
              <line :x1="lineIn.from.x" :y1="lineIn.from.y" :x2="lineIn.to.x" :y2="lineIn.to.y" />
            </svg>
          </div>
          <figcaption class="said">{{ t("workspace.surface.draw") }}</figcaption>
        </figure>
      </div>
      <template v-if="profile && ends">
        <p data-said="profile-ends">
          {{
            t("workspace.surface.ends", {
              from: ends[0].toFixed(2),
              to: ends[1].toFixed(2),
              rise: (ends[1] - ends[0]).toFixed(2),
            })
          }}
        </p>
        <Chart
          :title="t('workspace.surface.profile')"
          :series="[
            {
              name: t('workspace.surface.height'),
              kind: 'line',
              x: profile.distance,
              y: profile.height,
            },
          ]"
          :x="{ title: t('workspace.surface.along') }"
          :y="{ title: t('workspace.surface.heightAxis') }"
          :height="200"
          :level="3"
        />
      </template>
    </template>
    <p class="said" role="status">{{ loading ? t("workspace.surface.loading") : said }}</p>
  </section>
</template>

<style scoped>
.surfaces {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

h2 {
  margin: 0;
  font-size: var(--font-size-body);
}

.choice,
.controls {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2) var(--space-3);
  align-items: end;
}

/* A slider's field, wide enough for its scale beside its number. */
.scaled {
  inline-size: 16rem;
}

.body {
  display: grid;
  align-items: start;
  grid-template-columns: minmax(0, 2fr) minmax(0, 1fr);
  gap: var(--space-2);
}

.view {
  block-size: 24rem;
  background: var(--surface-pane);
  border: 1px solid var(--border);
}

.flat {
  display: grid;
  gap: var(--space-1);
  align-content: start;
  margin: 0;
}

.map {
  position: relative;
  cursor: crosshair;
  touch-action: none;
}

.map canvas {
  display: block;
  inline-size: 100%;
  image-rendering: pixelated;
}

.line {
  position: absolute;
  inset: 0;
  inline-size: 100%;
  block-size: 100%;
  pointer-events: none;
}

.line line {
  stroke: var(--accent);
  stroke-width: 2;
  vector-effect: non-scaling-stroke;
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
