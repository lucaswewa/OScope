// The preview's messages, read (W1.5): ADR-0014's header, little-endian,
// then the JPEG, then ADR-0035's blocks, the statistics and the saturation
// mask. A version this reader doesn't know is refused; a field added at the
// header's end, or a kind of block it doesn't know, is passed over.

/** A rectangle of the sensor's pixels, in full-sensor coordinates. */
export interface PixelRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A line across the sensor, in full-sensor coordinates. */
export interface Line {
  from: [number, number];
  to: [number, number];
}

/** A message's header (ADR-0014). Times are nanoseconds on the server's clock. */
export interface PreviewHeader {
  /** The JPEG's quality. */
  quality: number;
  /** The message's number in its session, from 0. */
  sequence: number;
  /** The frame's ID. */
  frame: bigint;
  /** When its exposure started. */
  exposureStart: bigint;
  /** When its last row's exposure ended. */
  exposureEnd: bigint;
  /** When the message was ready. */
  ready: bigint;
  /** The sensor's pixels it shows. */
  roi: PixelRect;
  /** The image's width, in pixels. */
  width: number;
  /** Its height. */
  height: number;
}

/** A region's statistics, channel by channel (ADR-0035). */
export interface RoiStats {
  rect: PixelRect;
  count: number[];
  mean: number[];
  std: number[];
  min: number[];
  max: number[];
}

/** A profile along a line, cell by cell: each sample's channels, or none off the frame. */
export interface Profile {
  line: Line;
  step: number;
  samples: (number[] | null)[];
}

/** P18's statistics of a frame, in the sensor's own units (ADR-0035). */
export interface LiveStats {
  frame: number;
  /** In order: `red`, `green` and `blue`, or `grey`. */
  channels: string[];
  /** The largest value a sample can have: 4095 for 12 bits. */
  full_scale: number;
  counted: number[];
  /** Each channel's bins, the first from 0, each `(full_scale + 1) / bins` wide. */
  histogram: number[][];
  /** Each channel's values at 0.1, 1, 50, 99 and 99.9 %. */
  percentiles: number[][];
  /** Each channel's share of samples at full scale, from 0 to 1. */
  saturated: number[];
  roi: RoiStats | null;
  profile: Profile | null;
  /** The focus value: higher when sharper. */
  focus: number;
}

/** Where the preview shows samples at full scale, a bit a pixel (ADR-0035). */
export interface SaturationMask {
  width: number;
  height: number;
  /** Bytes a row: ⌈width / 8⌉. */
  stride: number;
  /** Rows of bits, each pixel's from a byte's highest bit down. */
  bits: Uint8Array<ArrayBuffer>;
}

/** A message, read. */
export interface PreviewMessage {
  header: PreviewHeader;
  jpeg: Uint8Array<ArrayBuffer>;
  /** The statistics, when they were asked for and measured on this frame. */
  stats?: LiveStats | undefined;
  /** The mask, when anything was saturated. */
  mask?: SaturationMask | undefined;
}

/** A message this reader can't read: another version, or one cut short. */
export class PreviewError extends Error {
  override name = "PreviewError";
}

/** The kinds of message, and of block, as ADR-0014, ADR-0035 and ADR-0026 number them. */
export const KIND = { frame: 1, stats: 2, mask: 3, tiles: 4 } as const;

/** The version this reader knows. */
export const VERSION = 1;

/** ADR-0014's header, without ADR-0035's JPEG length. */
const SHORTEST = 72;

/** Reads one of the stream's binary messages. */
export function readMessage(buffer: ArrayBuffer): PreviewMessage {
  if (buffer.byteLength < SHORTEST) {
    throw new PreviewError(`a message of ${buffer.byteLength} bytes is shorter than a header`);
  }
  const view = new DataView(buffer);
  const u32 = (at: number) => view.getUint32(at, true);
  const u64 = (at: number) => view.getBigUint64(at, true);
  if (String.fromCharCode(...new Uint8Array(buffer, 0, 4)) !== "OSPV") {
    throw new PreviewError("not a preview message");
  }
  const [version, kind, codec] = [view.getUint8(4), view.getUint8(5), view.getUint8(6)];
  if (version !== VERSION) {
    throw new PreviewError(`a preview of version ${version}: this app reads version ${VERSION}`);
  }
  if (kind !== KIND.frame || codec !== 1) {
    throw new PreviewError(
      `a message of kind ${kind} and codec ${codec}, which this app doesn't read`,
    );
  }
  const length = u32(8);
  const jpegLength = length >= SHORTEST + 4 ? u32(SHORTEST) : buffer.byteLength - length;
  if (length < SHORTEST || length + jpegLength > buffer.byteLength) {
    throw new PreviewError("a message shorter than its header says");
  }
  const message: PreviewMessage = {
    header: {
      quality: view.getUint8(7),
      sequence: u32(12),
      frame: u64(16),
      exposureStart: u64(24),
      exposureEnd: u64(32),
      ready: u64(40),
      roi: { x: u32(48), y: u32(52), width: u32(56), height: u32(60) },
      width: u32(64),
      height: u32(68),
    },
    jpeg: new Uint8Array(buffer, length, jpegLength),
  };
  // Each block: its kind, three zero bytes, its length, then its bytes.
  for (let at = length + jpegLength; at < buffer.byteLength;) {
    const size = at + 8 <= buffer.byteLength ? u32(at + 4) : Infinity;
    if (at + 8 + size > buffer.byteLength) throw new PreviewError("a block runs past its message");
    const bytes = new Uint8Array(buffer, at + 8, size);
    const block = view.getUint8(at);
    if (block === KIND.stats) message.stats = JSON.parse(new TextDecoder().decode(bytes));
    else if (block === KIND.mask) message.mask = readMask(bytes);
    at += 8 + size;
  }
  return message;
}

/** A mask's block: its width and height, then its rows, copied out of the message. */
function readMask(bytes: Uint8Array): SaturationMask {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const [width, height] = [view.getUint32(0, true), view.getUint32(4, true)];
  const stride = Math.ceil(width / 8);
  if (bytes.byteLength < 8 + stride * height) {
    throw new PreviewError("a saturation mask shorter than its size");
  }
  return { width, height, stride, bits: bytes.slice(8, 8 + stride * height) };
}

/** Whether the mask's pixel at `x`, `y` is saturated. */
export function isSaturated(mask: SaturationMask, x: number, y: number): boolean {
  return ((mask.bits[y * mask.stride + (x >> 3)] ?? 0) & (0x80 >> (x & 7))) !== 0;
}

/** How many of the mask's pixels are saturated. */
export function saturatedPixels(mask: SaturationMask): number {
  let count = 0;
  for (let byte of mask.bits) {
    for (; byte; byte &= byte - 1) count++;
  }
  return count;
}
