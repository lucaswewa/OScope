// Files dragged from the computer onto a project's card or its page (W6.3):
// a drag that carries files is taken, and said while it is over; what is
// dropped is handed on. In the browser; the desktop app's window lets the
// page have its drops (W9.1).
import { shallowRef } from "vue";

/** A drop target for files: whether a drag is over it, and its listeners, for `v-on`. */
export function useFileDrop(take: (files: File[]) => void) {
  const over = shallowRef(false);
  // Entering a child leaves its parent: counted, so the target stays lit.
  let depth = 0;
  const carriesFiles = (event: DragEvent) => event.dataTransfer?.types.includes("Files") ?? false;
  return {
    over,
    on: {
      dragenter(event: DragEvent) {
        if (!carriesFiles(event)) return;
        event.preventDefault();
        depth++;
        over.value = true;
      },
      dragover(event: DragEvent) {
        if (!carriesFiles(event)) return;
        event.preventDefault();
        if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
      },
      dragleave() {
        depth = Math.max(0, depth - 1);
        if (depth === 0) over.value = false;
      },
      drop(event: DragEvent) {
        if (!carriesFiles(event)) return;
        event.preventDefault();
        depth = 0;
        over.value = false;
        take([...(event.dataTransfer?.files ?? [])]);
      },
    },
  };
}
