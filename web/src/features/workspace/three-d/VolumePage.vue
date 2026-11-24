<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, shallowRef, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { brick, slice, volumeMeta, type StoredImage, type Voxels } from "@/api/media";
import { projectContents, projectPage, type ProjectSummary } from "@/api/projects";
import type { Contents } from "@/features/gallery/project/project";
import { relistProjects } from "@/features/workspace/projects";
import { Bricks } from "@/features/workspace/three-d/bricks";
import { moved, rangeOf, START, type Point } from "@/features/workspace/three-d/transfer";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";
import Slider from "@/ui/Slider.vue";

type SceneModule = typeof import("@/features/workspace/three-d/volumeScene");
type Scene = ReturnType<SceneModule["volumeIn"]>;
type Axis = "x" | "y" | "z";

/**
 * A z-stack as a volume, and as slices through it (W7.8, D8): its bricks
 * fetched by level, ray cast in VTK.js's chunk, a channel at a time, through
 * a transfer function drawn on a chart; clipped; the three orthogonal slices
 * from the server, each with its slider, their planes outlined in the volume;
 * and another volume beside it, its deconvolution (P43) say, the two views
 * linked. Bricks are kept to a budget, the least recently used let go.
 */
const { t } = useI18n();
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));
const said = shallowRef("");
/** What the volume's brick may hold, and the bricks and slices kept, in bytes. */
const MOST = 128 * 128 * 128;
const bricks = new Bricks<Voxels>(256 * 2 ** 20);
/** What the cache holds, said after each fetch. */
const kept = shallowRef(bricks.size);

// The stacks: a project's acquisitions and results of several planes.
const projects = shallowRef<ProjectSummary[]>([]);
/** The projects read again as their list opens: one made since is offered. */
const relisted = () => relistProjects(projects);
const project = shallowRef<string>();
const contents = shallowRef<Contents>();
onMounted(async () => {
  projects.value = (await projectPage({ limit: 500 }).catch(() => ({ projects: [] }))).projects;
});
const stacks = computed(() => {
  const all = [
    ...(contents.value?.acquisitions ?? []).map((one) => ({ kind: "acquisitions" as const, one })),
    ...(contents.value?.results ?? []).map((one) => ({ kind: "results" as const, one })),
  ];
  return all
    .filter(({ one }) => (one.image?.planes ?? 1) > 1)
    .map(({ kind, one }) => ({
      value: `${kind}:${one.id}`,
      label: one.name,
      channels: one.image?.channels ?? 1,
    }));
});
const chosen = shallowRef<string>();
const beside = shallowRef("none");
watch(project, async (id) => {
  [contents.value, chosen.value, beside.value] = [undefined, undefined, "none"];
  if (!id) return;
  contents.value = (await projectContents(id)) as Contents;
  chosen.value = stacks.value[0]?.value;
});
const imageOf = (value: string | undefined): StoredImage | undefined => {
  const [kind, id] = (value ?? "").split(":") as ["acquisitions" | "results", string];
  return project.value && id ? { project: project.value, kind, id } : undefined;
};
const channels = computed(
  () => stacks.value.find((one) => one.value === chosen.value)?.channels ?? 1,
);
const channel = shallowRef("0");

/** One channel of a volume's brick: its size, its voxels' size, and its samples. */
async function read(image: StoredImage) {
  const key = `${image.kind}/${image.id}/brick`;
  const voxels = await bricks.of(key, () => brick(image, { max: MOST, format: "f32" }));
  kept.value = bricks.size;
  const [x, y, z] = voxels.size;
  const c = Number(channel.value);
  const samples = new Float32Array(x * y * z);
  for (let i = 0; i < samples.length; i++) samples[i] = voxels.samples[i * voxels.channels + c]!;
  const scale = 2 ** voxels.level;
  const voxelUm = (voxels.voxelUm ?? [scale, scale, 1]) as [number, number, number];
  return { size: [x, y, z] as [number, number, number], voxelUm, samples };
}

// The volumes: VTK.js's, in its chunk, where there is WebGL2.
const webgl2 = shallowRef(true);
onMounted(() => (webgl2.value = !!document.createElement("canvas").getContext("webgl2")));
const left = useTemplateRef<HTMLElement>("left");
const right = useTemplateRef<HTMLElement>("right");
const scenes: Scene[] = [];
const drawn = shallowRef(0);
const range = shallowRef<[number, number]>([0, 1]);
const size = shallowRef<[number, number, number]>();
/** The finest level the server slices, a raw mosaic's cells: its size, and its voxel's in µm, if known. */
const finest = shallowRef<{
  level: number;
  size: [number, number, number];
  voxel_um: [number, number, number] | null;
}>();
const points = shallowRef<Point[]>([...START]);
const map = shallowRef("Grayscale");
const MAPS = ["Grayscale", "Viridis (matplotlib)", "Cool to Warm"];
const cut = shallowRef({ x: 0, y: 0, z: 0 });
const at = shallowRef({ x: 0.5, y: 0.5, z: 0.5 });
/** Where each view's camera is, said for whoever checks they're linked. */
const cameras = shallowRef<string[]>([]);
const sayCameras = () =>
  (cameras.value = scenes.map((scene) =>
    scene
      .camera()
      .position.map((part) => part.toFixed(2))
      .join(" "),
  ));
function dispose(): void {
  for (const scene of scenes.splice(0)) scene.dispose();
  drawn.value = 0;
}
// Another stack: no slices until its levels are known.
watch(chosen, () => (finest.value = undefined));
/** The latest drawing: an earlier one, still fetching, draws nothing. */
let drawing = 0;
watch([chosen, beside, channel, () => left.value, () => right.value], async () => {
  const run = ++drawing;
  dispose();
  const image = imageOf(chosen.value);
  if (!image) return;
  try {
    const [main, meta] = await Promise.all([read(image), volumeMeta(image)]);
    if (run !== drawing) return;
    finest.value = meta.levels[0] ?? { level: 0, size: meta.size, voxel_um: meta.voxel_um };
    [range.value, size.value] = [rangeOf(main.samples), main.size];
    if (!webgl2.value) return;
    const other = imageOf(beside.value === "none" ? undefined : beside.value);
    const [{ volumeIn }, second] = await Promise.all([
      import("@/features/workspace/three-d/volumeScene") as Promise<SceneModule>,
      other && read(other),
    ]);
    if (run !== drawing || !left.value) return;
    const shown = [volumeIn(left.value, main)];
    if (second && right.value) shown.push(volumeIn(right.value, second));
    scenes.push(...shown);
    // The two views linked: each turns the other.
    if (shown.length === 2) {
      shown[0]!.onCamera((camera) => shown[1]!.follow(camera));
      shown[1]!.onCamera((camera) => shown[0]!.follow(camera));
    }
    for (const scene of shown) scene.onCamera(sayCameras);
    restyle();
    sayCameras();
    drawn.value = shown.length;
  } catch (thrown) {
    said.value = t("workspace.volume.unread", { why: why(thrown) });
  }
});
function restyle(): void {
  const box = { x: [cut.value.x, 1], y: [cut.value.y, 1], z: [cut.value.z, 1] } as Record<
    Axis,
    [number, number]
  >;
  for (const scene of scenes) {
    scene.look(points.value, range.value, map.value);
    scene.clip(box);
    scene.slices(at.value);
  }
}
watch([points, map, cut, at], restyle);
const resizing = new ResizeObserver(() => scenes.forEach((scene) => scene.resize()));
watch([left, right], (now) => now.forEach((element) => element && resizing.observe(element)));
onBeforeUnmount(() => {
  resizing.disconnect();
  dispose();
});

// The transfer function, drawn: its points dragged on a chart of 0 to 1.
const W = 240;
const H = 100;
const dragging = shallowRef<number>();
const placeOf = (event: PointerEvent): Point => {
  const box = (event.currentTarget as SVGElement).closest("svg")!.getBoundingClientRect();
  return {
    value: (event.clientX - box.left) / box.width,
    opacity: 1 - (event.clientY - box.top) / box.height,
  };
};
function pressPoint(event: PointerEvent, index: number): void {
  (event.currentTarget as Element).setPointerCapture(event.pointerId);
  dragging.value = index;
}
function movePoint(event: PointerEvent): void {
  if (dragging.value !== undefined)
    points.value = moved(points.value, dragging.value, placeOf(event));
}

// The slices: the server's, at its finest level, each grey over its own range, at its slider's index.
/** The slices' canvases, by axis, as they are drawn. */
const canvases: Partial<Record<Axis, HTMLCanvasElement>> = {};
const index = (axis: Axis) => {
  const n = finest.value?.size[{ x: 0, y: 1, z: 2 }[axis]] ?? 1;
  return Math.round(at.value[axis] * (n - 1));
};
/** Each axis's latest slice asked for: one that comes after it is drawn, not one before. */
const asked = { x: 0, y: 0, z: 0 };
async function drawSlice(axis: Axis): Promise<void> {
  const image = imageOf(chosen.value);
  const canvas = canvases[axis];
  if (!image || !canvas || !finest.value) return;
  const run = ++asked[axis];
  const which = { axis, index: index(axis), level: finest.value.level, format: "f32" as const };
  const key = `${image.kind}/${image.id}/slice/${axis}/${which.index}`;
  const voxels = await bricks.of(key, () => slice(image, which));
  kept.value = bricks.size;
  if (run !== asked[axis]) return;
  const [a, b] = voxels.size.filter((_, i) => i !== { x: 0, y: 1, z: 2 }[axis]) as [number, number];
  const c = Number(channel.value);
  const values = Float32Array.from(
    { length: a * b },
    (_, i) => voxels.samples[i * voxels.channels + c]!,
  );
  const [least, most] = rangeOf(values);
  [canvas.width, canvas.height] = [a, b];
  // Its shape the plane's, in µm: a voxel one pixel, stretched as the voxels are.
  const [u, v] = (finest.value.voxel_um ?? [1, 1, 1]).filter(
    (_, i) => i !== { x: 0, y: 1, z: 2 }[axis],
  );
  canvas.style.setProperty("--aspect", String((a * u!) / (b * v!)));
  const context = canvas.getContext("2d")!;
  const pixels = context.createImageData(a, b);
  values.forEach((v, i) => {
    const grey = Math.round((255 * (v - least)) / (most - least));
    pixels.data.set([grey, grey, grey, 255], i * 4);
  });
  context.putImageData(pixels, 0, 0);
  canvas.dataset.index = String(which.index);
}
// After the page is drawn: the canvases are there only once a stack's size is known.
watch(
  [finest, at, channel],
  () =>
    void Promise.all((["x", "y", "z"] as Axis[]).map(drawSlice)).catch(
      (thrown: unknown) => (said.value = t("workspace.volume.unread", { why: why(thrown) })),
    ),
  { flush: "post" },
);
</script>

<template>
  <section class="volumes" aria-labelledby="volumes-title" data-said="volumes">
    <h2 id="volumes-title">{{ t("workspace.volume.title") }}</h2>
    <div class="choice">
      <Field :label="t('workspace.run.project')">
        <Select
          v-model="project"
          :options="projects.map((one) => ({ value: one.id, label: one.name }))"
          :placeholder="t('workspace.run.chooseProject')"
          @opened="relisted"
        />
      </Field>
      <Field :label="t('workspace.volume.stack')">
        <Select v-model="chosen" :options="stacks" :placeholder="t('workspace.volume.choose')" />
      </Field>
      <Field :label="t('workspace.volume.beside')">
        <Select
          v-model="beside"
          :options="[
            { value: 'none', label: t('workspace.volume.none') },
            ...stacks.filter((one) => one.value !== chosen),
          ]"
        />
      </Field>
      <Field v-if="channels > 1" :label="t('workspace.volume.channel')">
        <Select
          v-model="channel"
          :options="
            Array.from({ length: channels }, (_, i) => ({ value: String(i), label: String(i + 1) }))
          "
        />
      </Field>
    </div>
    <p v-if="!webgl2" class="said" role="status" data-said="no-webgl">
      {{ t("workspace.volume.noWebgl") }}
    </p>
    <template v-if="size">
      <div v-if="webgl2" class="views" :class="{ two: beside !== 'none' }">
        <div
          ref="left"
          class="view"
          role="img"
          :aria-label="t('workspace.volume.label')"
          data-said="volume"
          :data-drawn="drawn"
          :data-camera="cameras[0]"
        />
        <div
          v-if="beside !== 'none'"
          ref="right"
          class="view"
          role="img"
          :aria-label="t('workspace.volume.besideLabel')"
          data-said="volume-beside"
          :data-camera="cameras[1]"
        />
      </div>
      <div class="controls">
        <figure class="transfer" data-said="transfer">
          <figcaption>{{ t("workspace.volume.transfer") }}</figcaption>
          <svg
            :viewBox="`0 0 ${W} ${H}`"
            role="img"
            :aria-label="t('workspace.volume.transferLabel')"
            @pointermove="movePoint"
            @pointerup="dragging = undefined"
          >
            <polyline
              :points="points.map((p) => `${p.value * W},${(1 - p.opacity) * H}`).join(' ')"
            />
            <circle
              v-for="(p, i) in points"
              :key="i"
              :cx="p.value * W"
              :cy="(1 - p.opacity) * H"
              r="5"
              :data-point="i"
              @pointerdown="pressPoint($event, i)"
            />
          </svg>
        </figure>
        <Field :label="t('workspace.surface.map')">
          <Select v-model="map" :options="MAPS.map((name) => ({ value: name, label: name }))" />
        </Field>
        <Field
          v-for="axis in ['x', 'y', 'z'] as const"
          :key="`cut-${axis}`"
          class="scaled"
          :label="t('workspace.volume.cut', { axis })"
        >
          <Slider
            :model-value="cut[axis]"
            :min="0"
            :max="0.9"
            :step="0.05"
            @update:model-value="(v: number) => (cut = { ...cut, [axis]: v })"
          />
        </Field>
        <Button compact @click="scenes.forEach((scene) => scene.reset())">{{
          t("workspace.surface.reset")
        }}</Button>
      </div>
      <div class="slices">
        <figure v-for="axis in ['z', 'y', 'x'] as const" :key="axis" class="slice">
          <div class="frame">
            <canvas
              :ref="(element) => (canvases[axis] = element as HTMLCanvasElement)"
              role="img"
              :aria-label="t('workspace.volume.sliceLabel', { axis })"
              :data-said="`slice-${axis}`"
            />
          </div>
          <Field :label="t('workspace.volume.slice', { axis, n: index(axis) + 1 })">
            <Slider
              :model-value="at[axis]"
              :min="0"
              :max="1"
              :step="0.01"
              @update:model-value="(v: number) => (at = { ...at, [axis]: v })"
            />
          </Field>
        </figure>
      </div>
      <p class="said" data-said="kept">
        {{ t("workspace.volume.kept", { mb: (kept.bytes / 2 ** 20).toFixed(1), n: kept.bricks }) }}
      </p>
    </template>
    <p class="said" role="status">{{ said }}</p>
  </section>
</template>

<style scoped>
.volumes {
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

.views {
  display: grid;
  gap: var(--space-2);
}

.views.two {
  grid-template-columns: 1fr 1fr;
}

.view {
  overflow: hidden;
  block-size: 22rem;
  border: 1px solid var(--border);
  background: var(--surface-pane);
}

.transfer {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  font-size: var(--font-size-small);
}

.transfer svg {
  inline-size: 15rem;
  block-size: 6.25rem;
  border: 1px solid var(--border);
  overflow: visible;
  background: var(--surface-raised);
  touch-action: none;
}

.transfer polyline {
  fill: none;
  stroke: var(--chart-1);
  stroke-width: 2;
  vector-effect: non-scaling-stroke;
}

.transfer circle {
  fill: var(--accent);
  cursor: grab;
}

.slices {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: var(--space-2);
}

.slice {
  display: grid;
  gap: var(--space-1);
  margin: 0;
}

/* A slice within a box of one height, so the sliders line up: as wide as its plane is in µm. */
.frame {
  display: grid;
  place-items: center;
  block-size: 12rem;
  background: var(--surface-image);
}

.frame canvas {
  inline-size: min(100%, 12rem * var(--aspect, 1));
  aspect-ratio: var(--aspect, 1);
  image-rendering: pixelated;
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
